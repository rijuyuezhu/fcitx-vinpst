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

build_dir="target/cpp/fcitx5-addon-dbus-adapter-lifecycle"
stage_dir="target/tmp/vinput-cpp-dbus-adapter-lifecycle-smoke"
stage_abs="${repo_root}/${stage_dir}"
daemon_path="${stage_abs}/usr/local/bin/vinput-daemon"
daemon_wrapper="${stage_abs}/usr/local/bin/vinput-daemon-adapter-lifecycle"
daemon_pid_file="${stage_abs}/vinput-daemon.pid"
cargo_target_dir="${stage_abs}/cargo-target"
source_config_path="${repo_root}/data/e2e-adapter-lifecycle-config.json"
config_path="${stage_abs}/usr/local/share/fcitx-vinput/e2e-adapter-lifecycle-config.json"
wav_path="${repo_root}/target/tmp/vinput-cpp-dbus-adapter-lifecycle-demo.wav"
smoke_bin="${build_dir}/vinput_fcitx_bridge_dbus_smoke"
service_file="${stage_abs}/usr/local/share/dbus-1/services/org.fcitx.Vinput.service"

rm -rf "${build_dir}" "${stage_dir}"
python3 scripts/fixtures/write-demo-wav.py "${wav_path}"
CARGO_TARGET_DIR="${cargo_target_dir}" cargo build -q -p vinput-daemon --bin vinput-daemon
install -Dm755 "${cargo_target_dir}/debug/vinput-daemon" "${daemon_path}"
install -Dm644 "${source_config_path}" "${config_path}"
cat >"${daemon_wrapper}" <<EOF
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "\$\$" >"${daemon_pid_file}"
exec "${daemon_path}" --dbus --configured-backends --config "${config_path}" --wav "${wav_path}" "\$@"
EOF
chmod +x "${daemon_wrapper}"

stop_staged_daemon() {
  if [[ ! -f "${daemon_pid_file}" ]]; then
    return 0
  fi
  local daemon_pid
  daemon_pid="$(<"${daemon_pid_file}")"
  if [[ ! "${daemon_pid}" =~ ^[1-9][0-9]*$ ]]; then
    echo "invalid staged daemon pid: ${daemon_pid}" >&2
    return 1
  fi
  kill "${daemon_pid}" 2>/dev/null || true
  for _ in $(seq 1 100); do
    if ! kill -0 "${daemon_pid}" 2>/dev/null; then
      rm -f "${daemon_pid_file}"
      return 0
    fi
    sleep 0.05
  done
  kill -KILL "${daemon_pid}" 2>/dev/null || true
  for _ in $(seq 1 100); do
    if ! kill -0 "${daemon_pid}" 2>/dev/null; then
      rm -f "${daemon_pid_file}"
      return 0
    fi
    sleep 0.05
  done
  echo "staged daemon ${daemon_pid} did not exit" >&2
  return 1
}

trap 'stop_staged_daemon || true' EXIT

cmake -S cpp/fcitx5-addon -B "${build_dir}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DVINPUT_FCITX_BRIDGE_INSTALL_SYSTEMD_SERVICE=OFF \
  -DCMAKE_EXPORT_COMPILE_COMMANDS=ON \
  -DVINPUT_DAEMON_EXECUTABLE="${daemon_wrapper}" \
  -DVINPUT_DAEMON_ARGS=""
cmake --build "${build_dir}" --target fcitx5_vinput_addon --parallel
cmake --build "${build_dir}" --target vinput_fcitx_bridge_dbus_smoke --parallel
DESTDIR="${stage_abs}" cmake --install "${build_dir}"

grep -qx "Name=org.fcitx.Vinput" "${service_file}"
! grep -q '^SystemdService=' "${service_file}"
grep -qx "Exec=${daemon_wrapper} --exit-when-executable-replaced" "${service_file}"

mkdir -p "${stage_abs}/xdg-data-home"

smoke_status=0
XDG_DATA_HOME="${stage_abs}/xdg-data-home" \
XDG_DATA_DIRS="${stage_abs}/usr/local/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
VINPUT_DBUS_SMOKE_EXPECTED_NORMAL="lifecycle heard" \
VINPUT_DBUS_SMOKE_EXPECTED_COMMAND="lifecycle heard" \
VINPUT_DBUS_SMOKE_EXPECTED_ACTIVE_SCENE="raw" \
VINPUT_DBUS_SMOKE_EXPECT_SCENE_PERSISTED="1" \
VINPUT_DBUS_SMOKE_LIFECYCLE_ADAPTER="lifecycle-adapter" \
VINPUT_DBUS_SMOKE_LIFECYCLE_ONLY="1" \
  timeout 20s dbus-run-session -- "${smoke_bin}" || smoke_status=$?

cleanup_status=0
stop_staged_daemon || cleanup_status=$?
trap - EXIT
if ((smoke_status != 0)); then
  exit "${smoke_status}"
fi
exit "${cleanup_status}"
