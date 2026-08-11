#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="${script_dir}"
while [[ ! -f "${repo_root}/Cargo.toml" || ! -d "${repo_root}/scripts" ]]; do
  parent="$(dirname "${repo_root}")"
  if [[ "${parent}" == "${repo_root}" ]]; then
    echo "cannot locate repository root from ${script_dir}" >&2
    exit 1
  fi
  repo_root="${parent}"
done
cd "${repo_root}"
source scripts/tests/dbus-session-common.sh

build_dir="target/cpp/fcitx5-addon-menu-responsiveness"
stage_dir="target/tmp/vinpst-cpp-menu-responsiveness-smoke"
stage_abs="${repo_root}/${stage_dir}"
service_dir="${stage_abs}/services"
bus_config="${stage_abs}/session.conf"
blocker="${stage_abs}/block-activation.sh"
marker="${stage_abs}/activation.pid"
smoke_bin="${repo_root}/${build_dir}/vinpst_fcitx_native_addon_dbus_smoke"

cmake -S cpp/fcitx5-addon -B "${build_dir}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DVINPST_FCITX_BRIDGE_INSTALL_SYSTEMD_SERVICE=OFF \
  -DVINPST_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON
cmake --build "${build_dir}" --target vinpst_fcitx_native_addon_dbus_smoke --parallel

rm -rf "${stage_dir}"
mkdir -p "${service_dir}" "${stage_abs}/xdg-config" "${stage_abs}/xdg-cache" \
  "${stage_abs}/xdg-data"
write_isolated_dbus_session_config "${bus_config}" "${service_dir}"

cat >"${blocker}" <<EOF
#!/bin/sh
echo \$\$ >"${marker}"
exec sleep 10
EOF
chmod +x "${blocker}"
cat >"${service_dir}/org.fcitx.Vinpst.service" <<EOF
[D-BUS Service]
Name=org.fcitx.Vinpst
Exec=${blocker}
EOF

cleanup_activation() {
  if [[ -s "${marker}" ]]; then
    kill "$(cat "${marker}")" 2>/dev/null || true
  fi
}
trap cleanup_activation EXIT

for menu in scene asr; do
  rm -f "${marker}"
  XDG_CONFIG_HOME="${stage_abs}/xdg-config" \
  XDG_CACHE_HOME="${stage_abs}/xdg-cache" \
  XDG_DATA_HOME="${stage_abs}/xdg-data" \
  VINPST_NATIVE_ADDON_MENU_PROBE="${menu}" \
    timeout 3s dbus-run-session --config-file="${bus_config}" -- "${smoke_bin}"

  if [[ ! -s "${marker}" ]]; then
    echo "${menu} menu probe did not start the deliberately blocked D-Bus activation" >&2
    exit 1
  fi
  cleanup_activation
  rm -f "${marker}"
done
