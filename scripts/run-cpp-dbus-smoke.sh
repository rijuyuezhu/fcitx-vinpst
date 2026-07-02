#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

just addon-build
cargo build -q -p vinput-daemon

dbus-run-session -- bash -euo pipefail <<'INNER'
log_file="target/tmp/vinput-cpp-dbus-smoke-daemon.log"
bridge_smoke_bin="target/cpp/fcitx5-addon/vinput_fcitx_bridge_dbus_smoke"
addon_smoke_bin="target/cpp/fcitx5-addon/vinput_fcitx_addon_dbus_smoke"
mkdir -p "$(dirname "${log_file}")"

target/debug/vinput-daemon --dbus >"${log_file}" 2>&1 &
daemon_pid=$!
cleanup() {
  kill "${daemon_pid}" >/dev/null 2>&1 || true
  wait "${daemon_pid}" >/dev/null 2>&1 || true
}
trap cleanup EXIT
sleep 0.5

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
