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

just addon-build
cargo build -q -p vinpst-daemon --features pipewire-backend

bus_runner="dbus-run-session"
log_file="target/tmp/vinpst-cpp-dbus-pipewire-live-smoke-daemon.log"
mkdir -p "$(dirname "${log_file}")"

${bus_runner} -- bash -euo pipefail <<'INNER'
log_file="target/tmp/vinpst-cpp-dbus-pipewire-live-smoke-daemon.log"
bridge_smoke_bin="target/cpp/fcitx5-addon/vinpst_fcitx_bridge_dbus_smoke"
addon_smoke_bin="target/cpp/fcitx5-addon/vinpst_fcitx_addon_dbus_smoke"
echo "PipeWire audio diagnostics from daemon build:"
target/debug/vinpst-daemon audio-devices
target/debug/vinpst-daemon --dbus --audio-backend pipewire >"${log_file}" 2>&1 &
daemon_pid=$!
cleanup() {
  kill "${daemon_pid}" >/dev/null 2>&1 || true
  wait "${daemon_pid}" >/dev/null 2>&1 || true
}
trap cleanup EXIT
sleep 0.5
export VINPST_DBUS_SMOKE_RECORD_MS=100

run_smokes() {
  "${bridge_smoke_bin}" || return 1
  if [[ -x "${addon_smoke_bin}" ]]; then
    "${addon_smoke_bin}" || return 1
  fi
}

for _ in $(seq 1 50); do
  if run_smokes; then
    exit 0
  fi
  if ! kill -0 "${daemon_pid}" >/dev/null 2>&1; then
    cat "${log_file}" >&2
    exit 1
  fi
  sleep 0.1
done

cat "${log_file}" >&2
exit 1
INNER
