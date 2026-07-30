#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

build_dir="target/cpp/fcitx5-ime-configured-pipewire-live"
stage_dir="target/tmp/fcitx-ime-configured-pipewire-live-smoke"
stage_abs="${repo_root}/${stage_dir}"
daemon_path="${stage_abs}/usr/local/bin/vinput-daemon"
config_path="${stage_abs}/usr/local/share/fcitx-vinput/e2e-configured-pipewire-live.json"
smoke_bin="${repo_root}/${build_dir}/vinput_fcitx_bridge_dbus_smoke"
addon_smoke_bin="${repo_root}/${build_dir}/vinput_fcitx_addon_dbus_smoke"
service_file="${stage_abs}/usr/local/share/dbus-1/services/org.fcitx.Vinput.service"
record_ms="${VINPUT_DBUS_SMOKE_RECORD_MS:-100}"
expected_text="live final: live pipewire command result"

rm -rf "${build_dir}" "${stage_dir}"
cargo build -q -p vinput-daemon --features pipewire-backend
install -Dm755 target/debug/vinput-daemon "${daemon_path}"
install -Dm644 data/e2e-configured-pipewire-live.json "${config_path}"

cmake -S cpp/fcitx5-addon -B "${build_dir}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DVINPUT_FCITX_BRIDGE_INSTALL_SYSTEMD_SERVICE=OFF \
  -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON \
  -DVINPUT_DAEMON_EXECUTABLE="${daemon_path}" \
  -DVINPUT_DAEMON_ARGS="--dbus --configured-backends --config ${config_path} --audio-backend pipewire"
cmake --build "${build_dir}" --target fcitx5_vinput_addon --parallel
cmake --build "${build_dir}" --target vinput_fcitx_bridge_dbus_smoke --parallel
cmake --build "${build_dir}" --target vinput_fcitx_addon_dbus_smoke --parallel
DESTDIR="${stage_abs}" cmake --install "${build_dir}"

test -x "${daemon_path}"
test -f "${config_path}"
test -f "${stage_abs}/usr/local/lib/fcitx5/fcitx5-vinput.so"
test -f "${stage_abs}/usr/local/share/fcitx5/addon/vinput.conf"
grep -qx "Name=org.fcitx.Vinput" "${service_file}"
! grep -q '^SystemdService=' "${service_file}"
grep -qx "Exec=${daemon_path} --dbus --configured-backends --config ${config_path} --audio-backend pipewire" "${service_file}"

echo "PipeWire audio diagnostics from staged configured daemon:"
"${daemon_path}" --config "${config_path}" audio-devices

XDG_DATA_DIRS="${stage_abs}/usr/local/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
VINPUT_DBUS_SMOKE_RECORD_MS="${record_ms}" \
VINPUT_DBUS_SMOKE_EXPECTED_NORMAL="${expected_text}" \
VINPUT_DBUS_SMOKE_EXPECTED_COMMAND="${expected_text}" \
VINPUT_DBUS_SMOKE_EXPECTED_ASR_PROVIDER="live-command-asr" \
VINPUT_DBUS_SMOKE_EXPECTED_TEXT_ADAPTER="live-text-adapter" \
VINPUT_DBUS_SMOKE_EXPECTED_ACTIVE_SCENE="live-postprocess" \
VINPUT_DBUS_SMOKE_EXPECT_SCENE_PERSISTED="1" \
  timeout 20s dbus-run-session -- bash -euo pipefail -c '"$1"; "$2"' \
    bash "${smoke_bin}" "${addon_smoke_bin}"
