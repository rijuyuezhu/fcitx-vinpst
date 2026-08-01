#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"
cd "${repo_root}"

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'required command not found: %s\n' "$1" >&2
    exit 1
  }
}

for command in curl ip jq python3 ss; do
  require_cmd "${command}"
done

browser="${VINPUT_REMOTE_TEXT_BROWSER:-}"
if [[ -z "${browser}" ]]; then
  for candidate in \
    google-chrome-unstable \
    google-chrome-stable \
    google-chrome \
    chromium \
    chromium-browser; do
    if command -v "${candidate}" >/dev/null 2>&1; then
      browser="$(command -v "${candidate}")"
      break
    fi
  done
fi
if [[ -z "${browser}" || ! -x "${browser}" ]]; then
  echo "a Chromium-family browser is required" >&2
  exit 1
fi

lan_address="${VINPUT_REMOTE_TEXT_LAN_ADDRESS:-}"
if [[ -z "${lan_address}" ]]; then
  lan_address="$(
    ip -j -4 route get 1.1.1.1 2>/dev/null |
      jq -r '.[0].prefsrc // .[0].src // empty'
  )"
fi
if [[ -z "${lan_address}" || "${lan_address}" == 127.* ]]; then
  echo "an operational non-loopback IPv4 address is required" >&2
  exit 1
fi
if ! ip -j -4 address show up scope global |
  jq -e --arg address "${lan_address}" \
    'any(.[]?.addr_info[]?; .local == $address)' >/dev/null; then
  echo "selected LAN address is not assigned to an up interface: ${lan_address}" >&2
  exit 1
fi

out_dir="${VINPUT_REMOTE_TEXT_LIVE_OUT_DIR:-target/tmp/remote-text-chromium-lan-live}"
rm -rf "${out_dir}"
mkdir -p "${out_dir}"

read -r port debug_port < <(
  python3 - <<'PY'
import socket

ports = []
while len(ports) < 2:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    if port not in ports:
        ports.append(port)
print(*ports)
PY
)
api_key="$(python3 - <<'PY'
import secrets
print(secrets.token_urlsafe(32))
PY
)"
fixture_text="${VINPUT_REMOTE_TEXT_LIVE_TEXT:-remote text LAN browser fixture}"
config_path="${out_dir}/config.json"
server_log="${out_dir}/server.log"

jq -n \
  --arg key "${api_key}" \
  --arg port "${port}" \
  '{
    version: 1,
    asr: {
      active_provider: "provider.vinput.remote.streaming",
      providers: [{
        id: "provider.vinput.remote.streaming",
        type: "command",
        command: "python3",
        args: ["unused-remote-text-provider.py"],
        env: {
          VINPUT_ASR_API_KEY: $key,
          VINPUT_ASR_PORT: $port,
          VINPUT_ASR_DEBOUNCE_MS: "5000"
        }
      }]
    },
    scenes: {
      active_scene: "raw",
      definitions: [{id: "raw", label: "Raw", candidate_count: 0}]
    }
  }' >"${config_path}"

cargo build -q -p vinput-daemon --bin vinput-daemon

target/debug/vinput-daemon \
  --config "${config_path}" \
  remote-text-server --bind 0.0.0.0 \
  >"${server_log}" 2>&1 &
server_pid=$!
server_stopped=0
cleanup() {
  local exit_code=$?
  set +e
  rm -f "${config_path}"
  if [[ "${server_stopped}" == 0 ]] && kill -0 "${server_pid}" 2>/dev/null; then
    kill -INT "${server_pid}" 2>/dev/null || true
    wait "${server_pid}" 2>/dev/null || true
  fi
  find scripts -type d -name __pycache__ -prune -exec rm -rf {} +
  trap - EXIT INT TERM
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

health_url="http://${lan_address}:${port}/health"
for _ in $(seq 1 100); do
  if ! kill -0 "${server_pid}" 2>/dev/null; then
    cat "${server_log}" >&2
    echo "remote text server exited before health became ready" >&2
    exit 1
  fi
  if curl --noproxy '*' --silent --show-error --fail \
    --max-time 1 "${health_url}" >"${out_dir}/health.json"; then
    break
  fi
  sleep 0.05
done
jq -e '.ok == true' "${out_dir}/health.json" >/dev/null
rm -f "${config_path}"

VINPUT_REMOTE_TEXT_API_KEY="${api_key}" \
  python3 scripts/live/network/remote-text-chromium-lan-probe.py \
  --browser "${browser}" \
  --endpoint "http://${lan_address}:${port}" \
  --output-url "ws://127.0.0.1:${port}/v1/realtime" \
  --text "${fixture_text}" \
  --out-dir "${out_dir}" \
  --debug-port "${debug_port}" \
  | tee "${out_dir}/probe.jsonl"

jq -e --arg text "${fixture_text}" --arg endpoint "http://${lan_address}:${port}" '
  .event == "summary" and
  .endpoint == $endpoint and
  .page_target_url == ($endpoint + "/") and
  .page_ready.input == "input connected" and
  .page_ready.output == "output connected" and
  .page_ready.disabled == false and
  .lan_browser_connection == true and
  .loopback_output_connection == true and
  .same_host_lan_proof == true and
  .cross_device_proof == false and
  .events[1].delta == $text and
  .events[2].transcript == $text and
  .renderer.no_new_privs == 1 and
  .renderer.seccomp == 2 and
  .renderer.cap_eff == "0000000000000000" and
  .api_key_recorded == false
' "${out_dir}/summary.json" >/dev/null

if grep -R -F -- "${api_key}" "${out_dir}" >/dev/null; then
  echo "remote text live evidence retained the API key" >&2
  exit 1
fi

test ! -e "${out_dir}/chrome-profile"
kill -INT "${server_pid}"
wait "${server_pid}"
server_stopped=1
if ss -Hln "( sport = :${port} )" | grep -q .; then
  echo "remote text listener remained after shutdown" >&2
  exit 1
fi

jq -n \
  --arg event wrapper_summary \
  --arg endpoint "http://${lan_address}:${port}" \
  --arg browser "${browser}" \
  --arg text "${fixture_text}" \
  --arg evidence "${out_dir}/summary.json" \
  '{
    event: $event,
    endpoint: $endpoint,
    browser: $browser,
    text: $text,
    evidence: $evidence,
    same_host_lan_proof: true,
    cross_device_proof: false,
    profile_removed: true,
    listener_released: true
  }' | tee "${out_dir}/wrapper-summary.json"
