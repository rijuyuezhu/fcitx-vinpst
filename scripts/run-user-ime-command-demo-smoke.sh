#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

build_dir="target/cpp/fcitx5-user-ime-command-demo-smoke"
install_root="${repo_root}/target/tmp/user-ime-command-demo-smoke"
bridge_smoke_bin="${repo_root}/${build_dir}/vinput_fcitx_bridge_dbus_smoke"
addon_smoke_bin="${repo_root}/${build_dir}/vinput_fcitx_addon_dbus_smoke"
service_file="${install_root}/share/dbus-1/services/org.fcitx.Vinput.service"

rm -rf "${build_dir}" "${install_root}"
XDG_DATA_HOME="${install_root}/share" \
VINPUT_USER_BIN_DIR="${install_root}/bin" \
VINPUT_USER_FCITX_LIB_DIR="${install_root}/lib/fcitx5" \
VINPUT_USER_PROFILE=command-demo \
  scripts/install-user-ime.sh >/tmp/vinput-user-ime-command-demo-install.log

cmake -S cpp/fcitx5-addon -B "${build_dir}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON
cmake --build "${build_dir}" --target vinput_fcitx_bridge_dbus_smoke --parallel
cmake --build "${build_dir}" --target vinput_fcitx_addon_dbus_smoke --parallel

test -x "${install_root}/bin/vinput-daemon"
test -f "${install_root}/lib/fcitx5/fcitx5-vinput.so"
test -f "${install_root}/share/fcitx5/addon/vinput.conf"
test -f "${install_root}/share/fcitx-vinput/e2e-command-demo-config.json"
test -f "${install_root}/share/fcitx-vinput/e2e-command-demo.wav"
grep -qx "Name=org.fcitx.Vinput" "${service_file}"
grep -q -- "--configured-backends" "${service_file}"
grep -q -- "--wav ${install_root}/share/fcitx-vinput/e2e-command-demo.wav" "${service_file}"

XDG_DATA_HOME="${install_root}/share" \
XDG_DATA_DIRS="${install_root}/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
VINPUT_DBUS_SMOKE_EXPECTED_NORMAL="demo heard 16 bytes" \
VINPUT_DBUS_SMOKE_EXPECTED_COMMAND="demo final: demo heard 16 bytes" \
VINPUT_DBUS_SMOKE_EXPECTED_ASR_PROVIDER="demo-command-asr" \
VINPUT_DBUS_SMOKE_EXPECTED_TEXT_ADAPTER="demo-text-adapter" \
  timeout 20s dbus-run-session -- bash -euo pipefail -c '"$1"; "$2"' \
    bash "${bridge_smoke_bin}" "${addon_smoke_bin}"
