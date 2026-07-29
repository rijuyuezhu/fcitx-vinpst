#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'required command not found: %s\n' "$1" >&2
    exit 1
  }
}

require_cmd curl
require_cmd dbus-run-session
require_cmd gdbus
require_cmd python3

cargo build -q -p vinput-daemon

daemon_bin="${repo_root}/target/debug/vinput-daemon"
root="${repo_root}/target/tmp/remote-text-daemon-lifecycle-smoke"
rm -rf "${root}"
mkdir -p "${root}"
config_path="${root}/config.json"
log_path="${root}/daemon.log"
port="$(python3 - <<'PY'
import socket
with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)"

python3 - "${config_path}" "${port}" <<'PY'
import json
import sys

config_path, port = sys.argv[1:]
config = {
    "version": 1,
    "asr": {
        "active_provider": "provider.vinput.remote.streaming",
        "providers": [
            {
                "id": "provider.vinput.remote.streaming",
                "type": "command",
                "command": "python3",
                "args": ["remote.py"],
                "env": {
                    "VINPUT_ASR_API_KEY": "fixture-key",
                    "VINPUT_ASR_PORT": port,
                    "VINPUT_ASR_DEBOUNCE_MS": "25",
                },
            }
        ],
    },
    "scenes": {
        "active_scene": "raw",
        "definitions": [{"id": "raw", "label": "Raw", "candidate_count": 0}],
    },
}
with open(config_path, "w", encoding="utf-8") as output:
    json.dump(config, output, indent=2)
PY

export VINPUT_REMOTE_LIFECYCLE_DAEMON_BIN="${daemon_bin}"
export VINPUT_REMOTE_LIFECYCLE_CONFIG="${config_path}"
export VINPUT_REMOTE_LIFECYCLE_LOG="${log_path}"
export VINPUT_REMOTE_LIFECYCLE_PORT="${port}"

timeout 30s dbus-run-session -- bash -euo pipefail <<'INNER'
daemon_bin="${VINPUT_REMOTE_LIFECYCLE_DAEMON_BIN}"
config_path="${VINPUT_REMOTE_LIFECYCLE_CONFIG}"
log_path="${VINPUT_REMOTE_LIFECYCLE_LOG}"
port="${VINPUT_REMOTE_LIFECYCLE_PORT}"
health_url="http://127.0.0.1:${port}/health"

"${daemon_bin}" --config "${config_path}" --dbus >"${log_path}" 2>&1 &
daemon_pid=$!
cleanup() {
  if kill -0 "${daemon_pid}" 2>/dev/null; then
    kill -KILL "${daemon_pid}" 2>/dev/null || true
    wait "${daemon_pid}" 2>/dev/null || true
  fi
}
trap cleanup EXIT

ready=false
for _ in $(seq 1 100); do
  if curl -fsS --max-time 0.2 "${health_url}" >/dev/null 2>&1; then
    ready=true
    break
  fi
  if ! kill -0 "${daemon_pid}" 2>/dev/null; then
    printf 'daemon exited before remote health became ready\n' >&2
    cat "${log_path}" >&2
    exit 1
  fi
  sleep 0.02
done
if [[ "${ready}" != true ]]; then
  printf 'remote health endpoint did not become ready\n' >&2
  cat "${log_path}" >&2
  exit 1
fi

gdbus call --session \
  --dest org.fcitx.Vinput \
  --object-path /org/fcitx/Vinput \
  --method org.fcitx.Vinput.Service.GetStatus \
  >/dev/null

kill -TERM "${daemon_pid}"
wait "${daemon_pid}"
trap - EXIT

if curl -fsS --max-time 0.2 "${health_url}" >/dev/null 2>&1; then
  printf 'remote health endpoint remained available after daemon shutdown\n' >&2
  exit 1
fi
INNER

printf 'remote text daemon lifecycle smoke passed\n'
