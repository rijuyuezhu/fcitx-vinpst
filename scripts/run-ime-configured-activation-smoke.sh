#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

build_dir="target/cpp/fcitx5-ime-configured-activation"
stage_dir="target/tmp/ime-act"
stage_abs="${repo_root}/${stage_dir}"
daemon_path="${stage_abs}/usr/local/bin/vinput-daemon"
daemon_wrapper="${stage_abs}/usr/local/bin/vinput-daemon-activation"
config_path="${stage_abs}/usr/local/share/fcitx-vinput/e2e-command-demo-config.json"
wav_path="${stage_abs}/usr/local/share/fcitx-vinput/e2e-command-demo.wav"
smoke_bin="${repo_root}/${build_dir}/vinput_fcitx_bridge_dbus_smoke"
addon_smoke_bin="${repo_root}/${build_dir}/vinput_fcitx_addon_dbus_smoke"
service_file="${stage_abs}/share/dbus-1/services/org.fcitx.Vinput.service"

rm -rf "${build_dir}" "${stage_dir}"
cargo build -q -p vinput-daemon
install -Dm755 target/debug/vinput-daemon "${daemon_path}"
install -Dm644 data/e2e-command-demo-config.json "${config_path}"
python3 scripts/write-demo-wav.py "${wav_path}"
cat >"${daemon_wrapper}" <<EOF
#!/usr/bin/env bash
set -euo pipefail
exec "${daemon_path}" --dbus --configured-backends --config "${config_path}" --wav "${wav_path}"
EOF
chmod +x "${daemon_wrapper}"
timeout 20s "${daemon_path}" --dbus --configured-backends --config "${config_path}" --wav "${wav_path}" runtime-status >/dev/null

cmake -S cpp/fcitx5-addon -B "${build_dir}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON \
  -DVINPUT_DAEMON_EXECUTABLE="${daemon_wrapper}" \
  -DVINPUT_DAEMON_ARGS=""
cmake --build "${build_dir}" --target fcitx5_vinput_addon --parallel
cmake --build "${build_dir}" --target vinput_fcitx_bridge_dbus_smoke --parallel
cmake --build "${build_dir}" --target vinput_fcitx_addon_dbus_smoke --parallel
cmake --install "${build_dir}" --prefix "${stage_dir}"

test -x "${daemon_path}"
test -x "${daemon_wrapper}"
test -f "${config_path}"
test -f "${wav_path}"
test -f "${stage_abs}/usr/local/lib/fcitx5/fcitx5-vinput.so"
test -f "${stage_abs}/usr/local/share/fcitx5/addon/vinput.conf"
grep -qx "Name=org.fcitx.Vinput" "${service_file}"
grep -qx "Exec=${daemon_wrapper}" "${service_file}"

XDG_DATA_DIRS="${stage_abs}/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
VINPUT_DBUS_SMOKE_EXPECTED_NORMAL="demo heard 16 bytes" \
VINPUT_DBUS_SMOKE_EXPECTED_COMMAND="demo final: demo heard 16 bytes" \
VINPUT_DBUS_SMOKE_EXPECTED_ASR_PROVIDER="demo-command-asr" \
VINPUT_DBUS_SMOKE_EXPECTED_TEXT_ADAPTER="demo-text-adapter" \
  timeout 120s dbus-run-session -- bash -euo pipefail -c '"$1"; "$2"' \
    bash "${smoke_bin}" "${addon_smoke_bin}"
