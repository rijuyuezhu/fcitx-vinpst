#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

build_dir="target/cpp/fcitx5-addon-dbus-adapter-lifecycle"
stage_dir="target/tmp/vinput-cpp-dbus-adapter-lifecycle-smoke"
stage_abs="${repo_root}/${stage_dir}"
daemon_path="${stage_abs}/usr/local/bin/vinput-daemon"
cargo_target_dir="${stage_abs}/cargo-target"
source_config_path="${repo_root}/data/e2e-adapter-lifecycle-config.json"
config_path="${stage_abs}/usr/local/share/fcitx-vinput/e2e-adapter-lifecycle-config.json"
wav_path="${repo_root}/target/tmp/vinput-cpp-dbus-adapter-lifecycle-demo.wav"
smoke_bin="${build_dir}/vinput_fcitx_bridge_dbus_smoke"
service_file="${stage_abs}/usr/local/share/dbus-1/services/org.fcitx.Vinput.service"

rm -rf "${build_dir}" "${stage_dir}"
python3 scripts/write-demo-wav.py "${wav_path}"
CARGO_TARGET_DIR="${cargo_target_dir}" cargo build -q -p vinput-daemon --bin vinput-daemon
install -Dm755 "${cargo_target_dir}/debug/vinput-daemon" "${daemon_path}"
install -Dm644 "${source_config_path}" "${config_path}"

cmake -S cpp/fcitx5-addon -B "${build_dir}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DVINPUT_FCITX_BRIDGE_INSTALL_SYSTEMD_SERVICE=OFF \
  -DCMAKE_EXPORT_COMPILE_COMMANDS=ON \
  -DVINPUT_DAEMON_EXECUTABLE="${daemon_path}" \
  -DVINPUT_DAEMON_ARGS="--dbus --configured-backends --config ${config_path} --wav ${wav_path}"
cmake --build "${build_dir}" --target fcitx5_vinput_addon --parallel
cmake --build "${build_dir}" --target vinput_fcitx_bridge_dbus_smoke --parallel
DESTDIR="${stage_abs}" cmake --install "${build_dir}"

grep -qx "Name=org.fcitx.Vinput" "${service_file}"
! grep -q '^SystemdService=' "${service_file}"
grep -qx "Exec=${daemon_path} --dbus --configured-backends --config ${config_path} --wav ${wav_path} --exit-when-executable-replaced" "${service_file}"

XDG_DATA_DIRS="${stage_abs}/usr/local/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
VINPUT_DBUS_SMOKE_EXPECTED_NORMAL="lifecycle heard" \
VINPUT_DBUS_SMOKE_EXPECTED_COMMAND="lifecycle heard" \
VINPUT_DBUS_SMOKE_EXPECTED_ACTIVE_SCENE="raw" \
VINPUT_DBUS_SMOKE_EXPECT_SCENE_PERSISTED="1" \
VINPUT_DBUS_SMOKE_LIFECYCLE_ADAPTER="lifecycle-adapter" \
VINPUT_DBUS_SMOKE_LIFECYCLE_ONLY="1" \
  timeout 20s dbus-run-session -- "${smoke_bin}"
