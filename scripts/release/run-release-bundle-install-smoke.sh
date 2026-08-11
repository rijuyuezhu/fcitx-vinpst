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
for command in docker jq python3 sha256sum tar; do
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
version="$(jq -r '.package.version' "${bundle_dir}/manifest.json")"
[[ "${package_name}" == fcitx-vinpst ]] || {
  echo "release bundle package name is not fcitx-vinpst" >&2
  exit 1
}
[[ "${architecture}" == x86_64 ]] || {
  echo "release bundle architecture is not x86_64" >&2
  exit 1
}
[[ -n "${version}" && "${version}" != null ]] || {
  echo "release manifest version is empty" >&2
  exit 1
}
expected_roles=$'arch-x86_64\ndeb-debian12\ndeb-ubuntu24.04\nflatpak-x86_64\nlinux-tarball-bundled\nrpm-fedora43-x86_64\nrpm-opensuse16.0-x86_64\nsource-archive'
actual_roles="$(jq -r '.artifacts[].role' "${bundle_dir}/manifest.json" | LC_ALL=C sort)"
[[ "${actual_roles}" == "${expected_roles}" ]] || {
  echo "release bundle does not contain the exact selected 0.1.0 artifact roles" >&2
  diff -u <(printf '%s\n' "${expected_roles}") <(printf '%s\n' "${actual_roles}") || true
  exit 1
}

mapfile -t tarballs < <(
  find "${bundle_dir}" -mindepth 1 -maxdepth 1 -type f \
    -name 'fcitx-vinpst_*-1_linux_x86_64_bundled.tar.gz' \
    -printf '%f\n' | LC_ALL=C sort
)
((${#tarballs[@]} == 1)) || {
  echo "expected exactly one bundled Linux tarball, found ${#tarballs[@]}" >&2
  exit 1
}
tarball="${tarballs[0]}"

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

docker run --rm \
  --env DEBIAN_FRONTEND=noninteractive \
  --env "VINPST_PACKAGE=${package}" \
  --env "VINPST_TARBALL=${tarball}" \
  --env "VINPST_VERSION=${version}" \
  --volume "${bundle_dir}:/release:ro" \
  "${image}" \
  bash -lc '
    set -euo pipefail

    # The official Ubuntu container excludes documentation and translations to
    # reduce image size. Re-include Vinpst release payload so `dpkg -V` verifies
    # the complete package rather than the container image policy.
    cat >/etc/dpkg/dpkg.cfg.d/zz-vinpst-release-smoke <<"EOF"
path-include=/usr/share/doc/fcitx-vinpst/*
path-include=/usr/share/locale/zh_CN/LC_MESSAGES/fcitx5-vinpst.mo
EOF

    apt_options=(
      -o Acquire::Retries=3
      -o Acquire::http::Timeout=30
      -o Acquire::https::Timeout=30
    )
    apt-get "${apt_options[@]}" update
    apt-get "${apt_options[@]}" install -y --no-install-recommends \
      ca-certificates \
      jq

    depends="$(dpkg-deb --field "/release/${VINPST_PACKAGE}" Depends)"
    [[ -n "${depends}" ]] || {
      echo "Ubuntu release package has no Depends metadata" >&2
      exit 1
    }
    apt-get "${apt_options[@]}" satisfy -y --no-install-recommends "${depends}"

    tarball_root="fcitx-vinpst_${VINPST_VERSION}-1_linux_x86_64_bundled"
    tar_stage="$(mktemp -d)"
    trap "rm -rf \"${tar_stage}\"" EXIT
    tar -xzf "/release/${VINPST_TARBALL}" -C "${tar_stage}"
    [[ -x "${tar_stage}/${tarball_root}/usr/bin/vinpst" ]]
    [[ -x "${tar_stage}/${tarball_root}/usr/bin/vinpst-daemon" ]]
    [[ -x "${tar_stage}/${tarball_root}/usr/bin/vinpst-gui" ]]
    "${tar_stage}/${tarball_root}/usr/bin/vinpst" --version >/dev/null
    "${tar_stage}/${tarball_root}/usr/bin/vinpst-daemon" --help >/dev/null
    XDG_CONFIG_HOME="${tar_stage}/config" \
      "${tar_stage}/${tarball_root}/usr/bin/vinpst-gui" --check --offline |
      jq -e ".ok and .application == \"vinpst-gui\" and .daemon.skipped" >/dev/null

    apt-get "${apt_options[@]}" install -y --no-install-recommends \
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
      /usr/bin/vinpst-gui \
      /usr/share/doc/fcitx-vinpst/LICENSE \
      /usr/share/doc/fcitx-vinpst/onnxruntime-LICENSE \
      /usr/share/doc/fcitx-vinpst/sherpa-onnx-LICENSE \
      /usr/share/doc/fcitx-vinpst/silero-vad-LICENSE \
      /usr/share/locale/zh_CN/LC_MESSAGES/fcitx5-vinpst.mo; do
      grep -Fxq "${required_path}" <<<"${package_files}"
      [[ -e "${required_path}" ]] || {
        echo "installed runtime payload is missing ${required_path}" >&2
        exit 1
      }
    done
    for required_pattern in \
      "/fcitx5/fcitx5-vinpst\\.so$" \
      "/share/fcitx5/addon/vinpst\\.conf$" \
      "/share/dbus-1/services/org\\.fcitx\\.Vinpst\\.service$" \
      "/(lib|share)/systemd/user/vinpst-daemon\\.service$" \
      "/share/applications/vinpst-gui\\.desktop$"; do
      required_path="$(grep -E "${required_pattern}" <<<"${package_files}" | head -n1)"
      [[ -n "${required_path}" && -e "${required_path}" ]] || {
        echo "installed runtime payload matching ${required_pattern} is missing" >&2
        exit 1
      }
    done

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
