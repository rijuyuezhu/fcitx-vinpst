#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 --bundle-dir DIRECTORY [--image IMAGE]" >&2
  exit 2
}

bundle_dir=""
image="ubuntu:24.04"
while (($#)); do
  case "$1" in
    --bundle-dir)
      (($# >= 2)) || usage
      bundle_dir="$2"
      shift 2
      ;;
    --image)
      (($# >= 2)) || usage
      image="$2"
      shift 2
      ;;
    *)
      usage
      ;;
  esac
done
[[ -n "${bundle_dir}" ]] || usage

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
bundle_dir="$(realpath "${bundle_dir}")"
[[ -d "${bundle_dir}" ]] || {
  echo "release bundle directory does not exist: ${bundle_dir}" >&2
  exit 1
}
for command in docker jq python3 sha256sum; do
  command -v "${command}" >/dev/null 2>&1 || {
    echo "required unrelated-environment release smoke command is missing: ${command}" >&2
    exit 1
  }
done

"${repo_root}/scripts/release/release_manifest.py" verify "${bundle_dir}"
(
  cd "${bundle_dir}"
  sha256sum -c SHA256SUMS
)

package_name="$(jq -r '.package.name' "${bundle_dir}/manifest.json")"
architecture="$(jq -r '.package.architecture' "${bundle_dir}/manifest.json")"
[[ "${package_name}" == fcitx-vinpst ]] || {
  echo "release bundle package name is not fcitx-vinpst" >&2
  exit 1
}
[[ "${architecture}" == x86_64 ]] || {
  echo "release bundle architecture is not x86_64" >&2
  exit 1
}
expected_roles=$'arch-x86_64\ndeb-debian12\ndeb-ubuntu24.04\nflatpak-x86_64\nsource-archive'
actual_roles="$(jq -r '.artifacts[].role' "${bundle_dir}/manifest.json" | LC_ALL=C sort)"
[[ "${actual_roles}" == "${expected_roles}" ]] || {
  echo "release bundle does not contain the exact selected 0.1.0 artifact roles" >&2
  diff -u <(printf '%s\n' "${expected_roles}") <(printf '%s\n' "${actual_roles}") || true
  exit 1
}

mapfile -t packages < <(
  find "${bundle_dir}" -mindepth 1 -maxdepth 1 -type f \
    -name 'fcitx-vinpst_*-1_ubuntu24.04_amd64.deb' \
    -printf '%f\n' | LC_ALL=C sort
)
((${#packages[@]} == 1)) || {
  echo "expected exactly one Ubuntu 24.04 release package, found ${#packages[@]}" >&2
  exit 1
}
package="${packages[0]}"
version="$(
  python3 - "${bundle_dir}/manifest.json" <<'PY'
import json
import pathlib
import sys

manifest = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
print(manifest["package"]["version"])
PY
)"
[[ -n "${version}" ]] || {
  echo "release manifest version is empty" >&2
  exit 1
}

docker run --rm \
  --env DEBIAN_FRONTEND=noninteractive \
  --env "VINPST_PACKAGE=${package}" \
  --env "VINPST_VERSION=${version}" \
  --volume "${bundle_dir}:/release:ro" \
  "${image}" \
  bash -lc '
    set -euo pipefail

    apt_options=(
      -o Acquire::Retries=3
      -o Acquire::http::Timeout=30
      -o Acquire::https::Timeout=30
    )
    apt-get "${apt_options[@]}" update
    apt-get "${apt_options[@]}" install -y --no-install-recommends \
      ca-certificates \
      jq \
      "/release/${VINPST_PACKAGE}"

    [[ "$(dpkg-query -W -f="\${Status}" fcitx-vinpst)" == "install ok installed" ]]
    [[ "$(dpkg-query -W -f="\${Version}" fcitx-vinpst)" == "${VINPST_VERSION}-1" ]]
    [[ "$(vinpst --version)" == *"${VINPST_VERSION}"* ]]
    [[ "$(vinpst-daemon --version)" == *"${VINPST_VERSION}"* ]]
    [[ "$(vinpst-gui --version)" == *"${VINPST_VERSION}"* ]]
    vinpst-gui --check --offline |
      jq -e ".ok and .application == \"vinpst-gui\" and .daemon.skipped" >/dev/null

    package_files="$(dpkg -L fcitx-vinpst)"
    for required_path in \
      /usr/bin/vinpst \
      /usr/bin/vinpst-daemon \
      /usr/bin/vinpst-gui; do
      grep -Fxq "${required_path}" <<<"${package_files}"
    done
    grep -Eq "/fcitx5/fcitx5-vinpst\\.so$" <<<"${package_files}"
    grep -Eq "/share/fcitx5/addon/vinpst\\.conf$" <<<"${package_files}"
    grep -Eq "/share/dbus-1/services/org\\.fcitx\\.Vinpst\\.service$" <<<"${package_files}"
    grep -Eq "/(lib|share)/systemd/user/vinpst-daemon\\.service$" <<<"${package_files}"
    grep -Eq "/share/applications/vinpst-gui\\.desktop$" <<<"${package_files}"

    useradd --create-home --user-group candidate
    runuser -u candidate -- env \
      HOME=/home/candidate \
      XDG_CONFIG_HOME=/home/candidate/.config \
      XDG_CACHE_HOME=/home/candidate/.cache \
      XDG_DATA_HOME=/home/candidate/.local/share \
      vinpst init
    config=/home/candidate/.config/fcitx-vinpst/config.json
    [[ -f "${config}" && ! -L "${config}" ]] || {
      echo "initialized config is missing, non-regular, or symlinked: ${config}" >&2
      exit 1
    }
    config_mode="$(stat -c "%a" "${config}")"
    [[ "${config_mode}" == 600 ]] || {
      echo "initialized config mode is ${config_mode}, expected 600" >&2
      exit 1
    }
    config_owner="$(stat -c "%U:%G" "${config}")"
    [[ "${config_owner}" == candidate:candidate ]] || {
      echo "initialized config owner is ${config_owner}, expected candidate:candidate" >&2
      exit 1
    }
    config_before="$(sha256sum "${config}" | cut -d " " -f1)"

    verification="$(dpkg -V fcitx-vinpst || true)"
    [[ -z "${verification}" ]] || {
      printf "%s\n" "${verification}" >&2
      exit 1
    }

    apt-get remove -y fcitx-vinpst
    ! dpkg-query -W -f="\${Status}" fcitx-vinpst 2>/dev/null |
      grep -q "install ok installed"
    for binary in vinpst vinpst-daemon vinpst-gui; do
      ! command -v "${binary}" >/dev/null 2>&1
    done
    [[ "$(sha256sum "${config}" | cut -d " " -f1)" == "${config_before}" ]]
    dpkg --purge fcitx-vinpst >/dev/null 2>&1 || true
    [[ "$(sha256sum "${config}" | cut -d " " -f1)" == "${config_before}" ]]
  '

printf 'Unrelated-environment release bundle install smoke passed: %s\n' "${package}"
