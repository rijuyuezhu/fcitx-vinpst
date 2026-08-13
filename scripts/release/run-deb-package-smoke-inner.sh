#!/usr/bin/env bash
set -euo pipefail

if (($# != 2)); then
  echo "usage: run-deb-package-smoke-inner.sh DISTRIBUTION OUTPUT_DIR" >&2
  exit 2
fi

distribution="$1"
output_dir="$2"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

rm -rf "${output_dir}"
mkdir -p "${output_dir}"
scripts/release/build-deb-package.sh \
  --distribution "${distribution}" \
  --output "${output_dir}" \
  --release 1 \
  --release 2
initial_package="$(find "${output_dir}" -maxdepth 1 -type f \
  -name "fcitx-vinpst_*-1_${distribution}_amd64.deb" -print -quit)"
upgrade_package="$(find "${output_dir}" -maxdepth 1 -type f \
  -name "fcitx-vinpst_*-2_${distribution}_amd64.deb" -print -quit)"
test -n "${initial_package}"
test -n "${upgrade_package}"
for package in "${initial_package}" "${upgrade_package}"; do
  test -s "${package}"
  dpkg-deb --info "${package}" >/dev/null
  dpkg-deb --contents "${package}" >/dev/null
done

version="$(dpkg-deb -f "${initial_package}" Version)"
upgrade_version="$(dpkg-deb -f "${upgrade_package}" Version)"
[[ "${version}" == *-1 ]]
[[ "${upgrade_version}" == *-2 ]]
[[ "$(dpkg-deb -f "${initial_package}" Package)" == "fcitx-vinpst" ]]
[[ "$(dpkg-deb -f "${initial_package}" Architecture)" == "amd64" ]]
[[ -z "$(dpkg-deb -f "${initial_package}" Provides)" ]]
[[ -z "$(dpkg-deb -f "${initial_package}" Conflicts)" ]]
[[ -z "$(dpkg-deb -f "${initial_package}" Replaces)" ]]

extract_root="${output_dir}/extract"
control_root="${output_dir}/control"
rm -rf "${extract_root}" "${control_root}"
mkdir -p "${extract_root}" "${control_root}"
dpkg-deb -x "${initial_package}" "${extract_root}"
dpkg-deb -e "${initial_package}" "${control_root}"

required_files=(
  usr/bin/vinpst
  usr/bin/vinpst-daemon
  usr/bin/vinpst-gui
  usr/lib/fcitx-vinpst/libsherpa-onnx-c-api.so
  usr/lib/fcitx-vinpst/libonnxruntime.so
  usr/lib/systemd/user/vinpst-daemon.service
  usr/share/applications/vinpst-gui.desktop
  usr/share/dbus-1/services/org.fcitx.Vinpst.service
  usr/share/fcitx5/addon/vinpst.conf
  usr/share/fcitx-vinpst/default-config.json
  usr/share/fcitx-vinpst/vad/silero_vad.onnx
  usr/share/doc/fcitx-vinpst/copyright
  usr/share/doc/fcitx-vinpst/LICENSE
  usr/share/icons/hicolor/256x256/apps/vinpst-gui.png
  usr/share/locale/zh_CN/LC_MESSAGES/fcitx5-vinpst.mo
)
for path in "${required_files[@]}"; do
  test -f "${extract_root}/${path}"
done
addon="$(find "${extract_root}/usr/lib" -path '*/fcitx5/fcitx5-vinpst.so' -type f -print -quit)"
test -n "${addon}"

for script in postinst postrm; do
  test -x "${control_root}/${script}"
  bash -n "${control_root}/${script}"
done
test ! -e "${control_root}/prerm"

for binary in vinpst vinpst-daemon vinpst-gui; do
  if ldd "${extract_root}/usr/bin/${binary}" | grep -q 'not found'; then
    ldd "${extract_root}/usr/bin/${binary}" >&2
    exit 1
  fi
done
if ldd "${addon}" | grep -q 'not found'; then
  ldd "${addon}" >&2
  exit 1
fi
readelf -d "${extract_root}/usr/bin/vinpst" | grep -Fq '$ORIGIN/../lib/fcitx-vinpst'
readelf -d "${extract_root}/usr/bin/vinpst-daemon" | grep -Fq '$ORIGIN/../lib/fcitx-vinpst'
readelf -d "${extract_root}/usr/lib/fcitx-vinpst/libsherpa-onnx-c-api.so" | grep -Fq '$ORIGIN'

"${extract_root}/usr/bin/vinpst" --version
"${extract_root}/usr/bin/vinpst-daemon" --version
"${extract_root}/usr/bin/vinpst-gui" --version
"${extract_root}/usr/bin/vinpst-gui" --check --offline \
  | jq -e '.ok and .application == "vinpst-gui" and .daemon.skipped' >/dev/null

for file in \
  "${extract_root}/usr/bin/vinpst" \
  "${extract_root}/usr/bin/vinpst-daemon" \
  "${extract_root}/usr/bin/vinpst-gui" \
  "${addon}"; do
  if strings "${file}" | grep -Fq "${repo_root}"; then
    echo "Debian package leaks the build path: ${file}" >&2
    exit 1
  fi
done

export DEBIAN_FRONTEND=noninteractive
rm -f /etc/dpkg/dpkg.cfg.d/excludes
dpkg -i "${initial_package}"
[[ "$(dpkg-query -W -f='${Status}' fcitx-vinpst)" == 'install ok installed' ]]
[[ "$(dpkg-query -W -f='${Version}' fcitx-vinpst)" == "${version}" ]]
for path in \
  /usr/bin/vinpst \
  /usr/bin/vinpst-daemon \
  /usr/bin/vinpst-gui \
  /usr/lib/systemd/user/vinpst-daemon.service \
  /usr/share/applications/vinpst-gui.desktop; do
  test -e "${path}"
done
vinpst-gui --check --offline \
  | jq -e '.ok and .application == "vinpst-gui" and .daemon.skipped' >/dev/null

future_config="/root/.config/fcitx-vinpst/config.json"
test ! -e "${future_config}"
mkdir -p "$(dirname "${future_config}")"
printf '%s\n' '{"version":999,"future":{"preserve":"exactly"}}' >"${future_config}"
future_before="$(sha256sum "${future_config}" | awk '{print $1}')"

if dpkg-query -S "${future_config}" >/dev/null 2>&1; then
  echo "Debian package unexpectedly owns user configuration" >&2
  exit 1
fi

dpkg -i "${upgrade_package}"
[[ "$(dpkg-query -W -f='${Version}' fcitx-vinpst)" == "${upgrade_version}" ]]
verification="$(dpkg -V fcitx-vinpst)"
if [[ -n "${verification}" ]]; then
  printf '%s\n' "${verification}" >&2
  echo "Debian package verification reported payload drift" >&2
  exit 1
fi
future_after_upgrade="$(sha256sum "${future_config}" | awk '{print $1}')"
[[ "${future_after_upgrade}" == "${future_before}" ]]

dpkg -r fcitx-vinpst
if dpkg-query -W -f='${Status}' fcitx-vinpst 2>/dev/null | grep -q 'install ok installed'; then
  echo "Debian package remained installed after removal" >&2
  exit 1
fi
for path in /usr/bin/vinpst /usr/bin/vinpst-daemon /usr/bin/vinpst-gui; do
  test ! -e "${path}"
done
future_after_remove="$(sha256sum "${future_config}" | awk '{print $1}')"
[[ "${future_after_remove}" == "${future_before}" ]]
dpkg -P fcitx-vinpst >/dev/null 2>&1 || true

echo "Debian ${distribution} package build and transaction smoke passed: ${initial_package}"
