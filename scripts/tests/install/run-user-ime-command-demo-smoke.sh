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

build_dir="target/cpp/fcitx5-user-ime-command-demo-smoke"
install_root="${repo_root}/target/tmp/user-ime-command-demo-smoke"
bridge_smoke_bin="${repo_root}/${build_dir}/vinpst_fcitx_bridge_dbus_smoke"
addon_smoke_bin="${repo_root}/${build_dir}/vinpst_fcitx_addon_dbus_smoke"
service_file="${install_root}/share/dbus-1/services/org.fcitx.Vinpst.service"
env_wrapper="${install_root}/share/fcitx-vinpst/fcitx5-with-vinpst-env.sh"
autostart_file="${install_root}/config/autostart/org.fcitx.Fcitx5.desktop"

rm -rf "${build_dir}" "${install_root}"
HOME="${install_root}/home" \
XDG_DATA_HOME="${install_root}/share" \
XDG_CONFIG_HOME="${install_root}/config" \
VINPST_USER_BIN_DIR="${install_root}/bin" \
VINPST_USER_FCITX_LIB_DIR="${install_root}/lib/fcitx5" \
VINPST_USER_PROFILE=command-demo \
  scripts/install/install-user-ime.sh >/tmp/vinpst-user-ime-command-demo-install.log

cmake -S cpp/fcitx5-addon -B "${build_dir}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DVINPST_FCITX_BRIDGE_INSTALL_SYSTEMD_SERVICE=OFF \
  -DVINPST_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON
cmake --build "${build_dir}" --target vinpst_fcitx_bridge_dbus_smoke --parallel
cmake --build "${build_dir}" --target vinpst_fcitx_addon_dbus_smoke --parallel

test -x "${install_root}/bin/vinpst-daemon"
test -f "${install_root}/lib/fcitx5/fcitx5-vinpst.so"
test -f "${install_root}/share/fcitx5/addon/vinpst.conf"
test -f "${install_root}/share/locale/zh_CN/LC_MESSAGES/fcitx5-vinpst.mo"
strings "${install_root}/lib/fcitx5/fcitx5-vinpst.so" >"${install_root}/module-strings.txt"
grep -Fxq "${install_root}/share/locale" "${install_root}/module-strings.txt"
if grep -Fq "${repo_root}/target/cpp/fcitx5-user-ime/locale" "${install_root}/module-strings.txt"; then
  echo "user-installed addon retained its build-tree locale fallback" >&2
  exit 1
fi
test -f "${install_root}/share/fcitx-vinpst/e2e-command-demo-config.json"
test -f "${install_root}/share/fcitx-vinpst/e2e-command-demo.wav"
test -x "${env_wrapper}"
test -f "${autostart_file}"
grep -qx "Name=org.fcitx.Vinpst" "${service_file}"
! grep -q '^SystemdService=' "${service_file}"
grep -qx "Exec=${env_wrapper}" "${autostart_file}"
grep -qx "X-fcitx-vinpst-managed=true" "${autostart_file}"
grep -q "FCITX_ADDON_DIRS=\"${install_root}/lib/fcitx5:" "${install_root}/share/fcitx-vinpst/fcitx-vinpst.env"
grep -q "XDG_DATA_HOME=\"${install_root}/share\"" "${install_root}/share/fcitx-vinpst/fcitx-vinpst.env"
grep -q -- "--configured-backends" "${service_file}"
grep -q -- "--wav ${install_root}/share/fcitx-vinpst/e2e-command-demo.wav" "${service_file}"

# shellcheck disable=SC2016
XDG_DATA_HOME="${install_root}/share" \
XDG_DATA_DIRS="${install_root}/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
VINPST_DBUS_SMOKE_EXPECTED_NORMAL="demo heard 16 bytes" \
VINPST_DBUS_SMOKE_EXPECTED_COMMAND="demo final: demo heard 16 bytes" \
VINPST_DBUS_SMOKE_EXPECTED_TAKEOVER="demo final: demo heard 16 bytes" \
VINPST_DBUS_SMOKE_EXPECTED_ACTIVE_SCENE="demo-postprocess" \
VINPST_DBUS_SMOKE_EXPECT_SCENE_PERSISTED="1" \
  timeout 20s dbus-run-session -- bash -euo pipefail -c '"$1"; "$2"' \
    bash "${bridge_smoke_bin}" "${addon_smoke_bin}"
