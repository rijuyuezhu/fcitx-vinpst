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

build_dir="target/cpp/fcitx5-ime-configured-activation"
stage_dir="target/tmp/ime-act"
stage_abs="${repo_root}/${stage_dir}"
daemon_path="${stage_abs}/usr/local/bin/vinput-daemon"
daemon_wrapper="${stage_abs}/usr/local/bin/vinput-daemon-activation"
cargo_target_dir="${stage_abs}/cargo-target"
daemon_log="${stage_abs}/usr/local/bin/vinput-daemon-activation.log"
config_path="${stage_abs}/usr/local/share/fcitx-vinput/e2e-command-demo-config.json"
wav_path="${stage_abs}/usr/local/share/fcitx-vinput/e2e-command-demo.wav"
smoke_bin="${repo_root}/${build_dir}/vinput_fcitx_bridge_dbus_smoke"
addon_smoke_bin="${repo_root}/${build_dir}/vinput_fcitx_addon_dbus_smoke"
service_file="${stage_abs}/usr/local/share/dbus-1/services/org.fcitx.Vinput.service"

rm -rf "${build_dir}" "${stage_dir}"
CARGO_TARGET_DIR="${cargo_target_dir}" cargo build -q -p vinput-daemon --bin vinput-daemon
install -Dm755 "${cargo_target_dir}/debug/vinput-daemon" "${daemon_path}"
install -Dm644 data/e2e-command-demo-config.json "${config_path}"
python3 scripts/fixtures/write-demo-wav.py "${wav_path}"
cat >"${daemon_wrapper}" <<EOF
#!/usr/bin/env bash
set -euo pipefail
if [[ -z "\${DBUS_SESSION_BUS_ADDRESS:-}" && -n "\${DBUS_STARTER_ADDRESS:-}" ]]; then
  export DBUS_SESSION_BUS_ADDRESS="\${DBUS_STARTER_ADDRESS}"
fi
export RUST_LOG="\${RUST_LOG:-info}"
export VINPUT_DAEMON_TRACE_STARTUP=1
echo "DBUS_SESSION_BUS_ADDRESS=\${DBUS_SESSION_BUS_ADDRESS:-}" >"${daemon_log}"
echo "DBUS_STARTER_ADDRESS=\${DBUS_STARTER_ADDRESS:-}" >>"${daemon_log}"
echo "RUST_LOG=\${RUST_LOG}" >>"${daemon_log}"
echo "VINPUT_DAEMON_TRACE_STARTUP=\${VINPUT_DAEMON_TRACE_STARTUP}" >>"${daemon_log}"
echo "daemon_sha256=$(sha256sum "${daemon_path}" | awk '{print $1}')" >>"${daemon_log}"
echo "daemon_has_startup_marker=$(strings "${daemon_path}" | grep -F -c 'vinput-daemon-startup')" >>"${daemon_log}"
echo "daemon_argv=${daemon_path} --dbus --configured-backends --config ${config_path} --wav ${wav_path}" >>"${daemon_log}"
"${daemon_path}" --dbus --configured-backends --config "${config_path}" --wav "${wav_path}" >>"${daemon_log}" 2>&1
status=\$?
echo "daemon_exit_status=\${status}" >>"${daemon_log}"
exit "\${status}"
EOF
chmod +x "${daemon_wrapper}"
timeout 20s "${daemon_path}" --dbus --configured-backends --config "${config_path}" --wav "${wav_path}" runtime-status >/dev/null

cmake -S cpp/fcitx5-addon -B "${build_dir}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DVINPUT_FCITX_BRIDGE_INSTALL_SYSTEMD_SERVICE=OFF \
  -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON \
  -DVINPUT_DAEMON_EXECUTABLE="${daemon_wrapper}" \
  -DVINPUT_DAEMON_ARGS=""
cmake --build "${build_dir}" --target fcitx5_vinput_addon --parallel
cmake --build "${build_dir}" --target vinput_fcitx_bridge_dbus_smoke --parallel
cmake --build "${build_dir}" --target vinput_fcitx_addon_dbus_smoke --parallel
DESTDIR="${stage_abs}" cmake --install "${build_dir}"

test -x "${daemon_path}"
test -x "${daemon_wrapper}"
test -f "${config_path}"
test -f "${wav_path}"
test -f "${stage_abs}/usr/local/lib/fcitx5/fcitx5-vinput.so"
test -f "${stage_abs}/usr/local/share/fcitx5/addon/vinput.conf"
grep -qx "Name=org.fcitx.Vinput" "${service_file}"
! grep -q '^SystemdService=' "${service_file}"
grep -qx "Exec=${daemon_wrapper} --exit-when-executable-replaced" "${service_file}"

if XDG_DATA_DIRS="${stage_abs}/usr/local/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
  VINPUT_DBUS_SMOKE_EXPECTED_NORMAL="demo heard 16 bytes" \
  VINPUT_DBUS_SMOKE_EXPECTED_COMMAND="demo final: demo heard 16 bytes" \
  VINPUT_DBUS_SMOKE_EXPECTED_TAKEOVER="demo final: demo heard 16 bytes" \
  VINPUT_DBUS_SMOKE_EXPECTED_ASR_PROVIDER="demo-command-asr" \
  VINPUT_DBUS_SMOKE_EXPECTED_TEXT_ADAPTER="demo-text-adapter" \
  VINPUT_DBUS_SMOKE_EXPECTED_ACTIVE_SCENE="demo-postprocess" \
  VINPUT_DBUS_SMOKE_EXPECT_SCENE_PERSISTED="1" \
  timeout 120s dbus-run-session -- bash -euo pipefail -c '
    "${1}" &
    daemon_pid="$!"
    cleanup() {
      kill "${daemon_pid}" 2>/dev/null || true
      wait "${daemon_pid}" 2>/dev/null || true
    }
    trap cleanup EXIT
    has_owner() {
      dbus-send --session --dest=org.freedesktop.DBus --type=method_call --print-reply \
        /org/freedesktop/DBus org.freedesktop.DBus.NameHasOwner string:org.fcitx.Vinput \
        2>/dev/null | grep -q "boolean true"
    }
    for _ in $(seq 1 60); do
      if has_owner; then
        break
      fi
      if ! kill -0 "${daemon_pid}" 2>/dev/null; then
        daemon_status=0
        wait "${daemon_pid}" || daemon_status=$?
        echo "staged daemon exited before owning org.fcitx.Vinput with status ${daemon_status}" >&2
        exit 1
      fi
      sleep 0.5
    done
    if ! has_owner; then
      echo "staged daemon did not own org.fcitx.Vinput before smoke timeout" >&2
      exit 1
    fi
    "${2}"
    "${3}"
  ' bash "${daemon_wrapper}" "${smoke_bin}" "${addon_smoke_bin}"; then
  :
else
  status=$?
  echo "staged activation service:" >&2
  cat "${service_file}" >&2 || true
  echo "staged activation daemon log:" >&2
  cat "${daemon_log}" >&2 || true
  exit "${status}"
fi
