#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"
# shellcheck source=remote-text-common.sh
source "${script_dir}/remote-text-common.sh"
cd "${repo_root}"

for command in curl ip jq python3 ss; do
  vinpst_network_require_command "${command}"
done
for command in /usr/bin/ip /usr/bin/ss; do
  if [[ ! -x "${command}" ]]; then
    echo "external-device proof requires the system command: ${command}" >&2
    exit 1
  fi
done

confirm_physical_device="${VINPST_REMOTE_TEXT_CONFIRM_PHYSICAL_DEVICE:-0}"
if [[ "${confirm_physical_device}" != 1 ]]; then
  printf '%s\n' \
    'Set VINPST_REMOTE_TEXT_CONFIRM_PHYSICAL_DEVICE=1 only when the URL will be opened' \
    'on another physical phone, tablet, laptop, or computer (not a VM/container on this host).' >&2
  exit 1
fi
if ! { exec 3>/dev/tty; } 2>/dev/null; then
  echo "external-device proof requires a controlling terminal for the one-time URL" >&2
  exit 1
fi

lan_address="$(
  vinpst_network_select_lan_ipv4 "${VINPST_REMOTE_TEXT_LAN_ADDRESS:-}"
)"
out_dir="${VINPST_REMOTE_TEXT_EXTERNAL_OUT_DIR:-target/tmp/remote-text-external-device-live}"
timeout_seconds="${VINPST_REMOTE_TEXT_EXTERNAL_TIMEOUT:-180}"
if [[ ! "${timeout_seconds}" =~ ^[1-9][0-9]*$ ]]; then
  echo "VINPST_REMOTE_TEXT_EXTERNAL_TIMEOUT must be a positive integer" >&2
  exit 1
fi
rm -rf "${out_dir}"
mkdir -p "${out_dir}"

read -r port < <(vinpst_network_reserve_ports 1)
api_key="$(vinpst_network_random_token)"
challenge="vinpst-external-$(python3 - <<'PY'
import secrets
print(f"{secrets.randbelow(1_000_000):06d}")
PY
)"
config_path="${out_dir}/config.json"
server_log="${out_dir}/server.log"
vinpst_remote_text_write_config "${config_path}" "${port}" "${api_key}" 600000

cargo build -q -p vinpst-daemon --bin vinpst-daemon

target/debug/vinpst-daemon \
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

endpoint="http://${lan_address}:${port}"
vinpst_remote_text_wait_health \
  "${server_pid}" \
  "${endpoint}/health" \
  "${server_log}" \
  "${out_dir}/health.json"
rm -f "${config_path}"

printf '%s\n' \
  'Open this one-time URL on another physical device connected to the network:' \
  "${endpoint}/#key=${api_key}" \
  '' \
  'Enter this exact challenge in the page editor and press Send:' \
  "${challenge}" \
  '' \
  "Waiting up to ${timeout_seconds} seconds for an external peer..." >&3
exec 3>&-

VINPST_REMOTE_TEXT_API_KEY="${api_key}" \
  python3 scripts/live/network/remote-text-external-device-collector.py \
  --endpoint "${endpoint}" \
  --output-url "ws://127.0.0.1:${port}/v1/realtime" \
  --port "${port}" \
  --challenge "${challenge}" \
  --out-dir "${out_dir}" \
  --timeout-seconds "${timeout_seconds}" \
  --physical-device-confirmed \
  --ip-command /usr/bin/ip \
  --ss-command /usr/bin/ss \
  | tee "${out_dir}/collector.jsonl"

jq -e --arg endpoint "${endpoint}" --arg challenge "${challenge}" '
  .event == "summary" and
  .endpoint == $endpoint and
  .challenge == $challenge and
  .same_host_lan_proof == false and
  .distinct_network_peer_proof == true and
  .operator_confirmed_physical_device == true and
  .cross_device_proof == true and
  .loopback_output_connection == true and
  (.external_peer_addresses | length) >= 1 and
  .events[1].delta == $challenge and
  .events[2].transcript == $challenge and
  .api_key_recorded == false and
  .ip_command == "/usr/bin/ip" and
  .ss_command == "/usr/bin/ss"
' "${out_dir}/summary.json" >/dev/null

if grep -R -F -- "${api_key}" "${out_dir}" >/dev/null; then
  echo "external-device evidence retained the API key" >&2
  exit 1
fi

kill -INT "${server_pid}"
wait "${server_pid}"
server_stopped=1
vinpst_network_require_listener_released "${port}"

jq -n \
  --arg event wrapper_summary \
  --arg endpoint "${endpoint}" \
  --arg challenge "${challenge}" \
  --arg evidence "${out_dir}/summary.json" \
  '{
    event: $event,
    endpoint: $endpoint,
    challenge: $challenge,
    evidence: $evidence,
    same_host_lan_proof: false,
    distinct_network_peer_proof: true,
    operator_confirmed_physical_device: true,
    cross_device_proof: true,
    listener_released: true,
    api_key_recorded: false
  }' | tee "${out_dir}/wrapper-summary.json"
