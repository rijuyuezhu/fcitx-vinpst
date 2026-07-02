#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

build_dir="target/cpp/fcitx5-addon-dbus-adapter-lifecycle"
stage_dir="target/tmp/vinput-cpp-dbus-adapter-lifecycle-smoke"
daemon_path="target/debug/vinput-daemon"
config_path="data/e2e-adapter-lifecycle-config.json"
wav_path="${repo_root}/target/tmp/vinput-cpp-dbus-adapter-lifecycle-demo.wav"
smoke_bin="${build_dir}/vinput_fcitx_bridge_dbus_smoke"
service_file="${stage_dir}/share/dbus-1/services/org.fcitx.Vinput.service"

rm -rf "${build_dir}" "${stage_dir}"
python3 scripts/write-demo-wav.py "${wav_path}"
cargo build -q -p vinput-daemon

cmake -S cpp/fcitx5-addon -B "${build_dir}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DCMAKE_EXPORT_COMPILE_COMMANDS=ON \
  -DVINPUT_DAEMON_EXECUTABLE="${repo_root}/${daemon_path}" \
  -DVINPUT_DAEMON_ARGS="--dbus --configured-backends --config ${repo_root}/${config_path} --wav ${wav_path}"
cmake --build "${build_dir}" --target fcitx5_vinput_addon --parallel
cmake --build "${build_dir}" --target vinput_fcitx_bridge_dbus_smoke --parallel
cmake --install "${build_dir}" --prefix "${stage_dir}"

grep -qx "Name=org.fcitx.Vinput" "${service_file}"
grep -qx "Exec=${repo_root}/${daemon_path} --dbus --configured-backends --config ${repo_root}/${config_path} --wav ${wav_path}" "${service_file}"

XDG_DATA_DIRS="${repo_root}/${stage_dir}/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
VINPUT_DBUS_SMOKE_EXPECTED_NORMAL="lifecycle heard" \
VINPUT_DBUS_SMOKE_EXPECTED_COMMAND="lifecycle heard" \
VINPUT_DBUS_SMOKE_LIFECYCLE_ADAPTER="lifecycle-adapter" \
VINPUT_DBUS_SMOKE_LIFECYCLE_ONLY="1" \
  timeout 20s dbus-run-session -- "${smoke_bin}"
