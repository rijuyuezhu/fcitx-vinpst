#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

build_dir="target/cpp/fcitx5-addon-dbus-configured-activation"
stage_dir="target/tmp/vinput-cpp-dbus-configured-activation-smoke"
stage_abs="${repo_root}/${stage_dir}"
daemon_path="${repo_root}/target/debug/vinput-daemon"
source_config_path="${repo_root}/data/e2e-command-demo-config.json"
config_path="${repo_root}/target/tmp/vinput-cpp-dbus-configured-activation-config.json"
wav_path="${repo_root}/target/tmp/vinput-cpp-dbus-configured-activation-demo.wav"
smoke_bin="${repo_root}/${build_dir}/vinput_fcitx_bridge_dbus_smoke"
addon_smoke_bin="${repo_root}/${build_dir}/vinput_fcitx_addon_dbus_smoke"
service_file="${stage_abs}/usr/local/share/dbus-1/services/org.fcitx.Vinput.service"

cargo build -q -p vinput-daemon
install -Dm644 "${source_config_path}" "${config_path}"
python3 scripts/write-demo-wav.py "${wav_path}"
cmake -S cpp/fcitx5-addon -B "${build_dir}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DVINPUT_FCITX_BRIDGE_INSTALL_SYSTEMD_SERVICE=OFF \
  -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON \
  -DVINPUT_DAEMON_EXECUTABLE="${daemon_path}" \
  -DVINPUT_DAEMON_ARGS="--dbus --configured-backends --config ${config_path} --wav ${wav_path}"
cmake --build "${build_dir}" --target fcitx5_vinput_addon --parallel
cmake --build "${build_dir}" --target vinput_fcitx_bridge_dbus_smoke --parallel
cmake --build "${build_dir}" --target vinput_fcitx_addon_dbus_smoke --parallel
rm -rf "${stage_dir}"
DESTDIR="${stage_abs}" cmake --install "${build_dir}"

grep -qx "Name=org.fcitx.Vinput" "${service_file}"
! grep -q '^SystemdService=' "${service_file}"
grep -qx "Exec=${daemon_path} --dbus --configured-backends --config ${config_path} --wav ${wav_path}" "${service_file}"

XDG_DATA_DIRS="${stage_abs}/usr/local/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
VINPUT_DBUS_SMOKE_EXPECTED_NORMAL="demo heard 16 bytes" \
VINPUT_DBUS_SMOKE_EXPECTED_COMMAND="demo final: demo heard 16 bytes" \
VINPUT_DBUS_SMOKE_EXPECTED_TAKEOVER="demo final: demo heard 16 bytes" \
VINPUT_DBUS_SMOKE_EXPECTED_ASR_PROVIDER="demo-command-asr" \
VINPUT_DBUS_SMOKE_EXPECTED_TEXT_ADAPTER="demo-text-adapter" \
VINPUT_DBUS_SMOKE_EXPECTED_ACTIVE_SCENE="demo-postprocess" \
VINPUT_DBUS_SMOKE_EXPECT_SCENE_PERSISTED="1" \
  timeout 20s dbus-run-session -- bash -euo pipefail -c '"$1"; "$2"' \
    bash "${smoke_bin}" "${addon_smoke_bin}"
