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

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'required command not found: %s\n' "$1" >&2
    exit 1
  }
}

require_cmd curl
require_cmd dbus-run-session
require_cmd python3

cargo build -q -p vinpst-cli -p vinpst-daemon

daemon_bin="${repo_root}/target/debug/vinpst-daemon"
cli_bin="${repo_root}/target/debug/vinpst"
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
        "active_provider": "provider.vinpst.remote.streaming",
        "providers": [
            {
                "id": "provider.vinpst.remote.streaming",
                "type": "command",
                "command": "python3",
                "args": ["remote.py"],
                "env": {
                    "VINPST_ASR_API_KEY": "fixture-key",
                    "VINPST_ASR_PORT": port,
                    "VINPST_ASR_DEBOUNCE_MS": "25",
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

export VINPST_REMOTE_LIFECYCLE_DAEMON_BIN="${daemon_bin}"
export VINPST_REMOTE_LIFECYCLE_CLI_BIN="${cli_bin}"
export VINPST_REMOTE_LIFECYCLE_CONFIG="${config_path}"
export VINPST_REMOTE_LIFECYCLE_LOG="${log_path}"
export VINPST_REMOTE_LIFECYCLE_PORT="${port}"

timeout 30s dbus-run-session -- bash -euo pipefail <<'INNER'
daemon_bin="${VINPST_REMOTE_LIFECYCLE_DAEMON_BIN}"
cli_bin="${VINPST_REMOTE_LIFECYCLE_CLI_BIN}"
config_path="${VINPST_REMOTE_LIFECYCLE_CONFIG}"
log_path="${VINPST_REMOTE_LIFECYCLE_LOG}"
port="${VINPST_REMOTE_LIFECYCLE_PORT}"
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

status_json="$("${cli_bin}" daemon status --json)"
python3 - "${port}" "${status_json}" <<'PY'
import json
import sys

port, raw = sys.argv[1:]
status = json.loads(raw)
assert status["status"] == "idle", status
asr = status["asr_backend"]
remote = status["runtime_status"]["remote_text"]
assert asr["target_provider_id"] == "provider.vinpst.remote.streaming", asr
assert asr["remote_endpoints"] == [], asr
assert remote["running"] is True, remote
assert remote["listen_addr"] == f"0.0.0.0:{port}", remote
assert remote["endpoints"], remote
assert all(endpoint.startswith("http://") for endpoint in remote["endpoints"]), remote
assert all(endpoint.endswith(f":{port}") for endpoint in remote["endpoints"]), remote
assert "fixture-key" not in raw, raw
PY

kill -TERM "${daemon_pid}"
wait "${daemon_pid}"
trap - EXIT

if curl -fsS --max-time 0.2 "${health_url}" >/dev/null 2>&1; then
  printf 'remote health endpoint remained available after daemon shutdown\n' >&2
  exit 1
fi
INNER

printf 'remote text daemon lifecycle smoke passed\n'
