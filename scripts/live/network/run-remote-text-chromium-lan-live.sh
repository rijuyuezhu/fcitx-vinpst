#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"
# shellcheck source=remote-text-common.sh
source "${script_dir}/remote-text-common.sh"
cd "${repo_root}"

for command in curl ip jq python3 ss; do
  vinput_network_require_command "${command}"
done

browser="$(vinput_network_find_chromium "${VINPUT_REMOTE_TEXT_BROWSER:-}")"
lan_address="$(
  vinput_network_select_lan_ipv4 "${VINPUT_REMOTE_TEXT_LAN_ADDRESS:-}"
)"
out_dir="${VINPUT_REMOTE_TEXT_LIVE_OUT_DIR:-target/tmp/remote-text-chromium-lan-live}"
rm -rf "${out_dir}"
mkdir -p "${out_dir}"

read -r port debug_port < <(vinput_network_reserve_ports 2)
api_key="$(vinput_network_random_token)"
fixture_text="${VINPUT_REMOTE_TEXT_LIVE_TEXT:-remote text LAN browser fixture}"
config_path="${out_dir}/config.json"
server_log="${out_dir}/server.log"
vinput_remote_text_write_config "${config_path}" "${port}" "${api_key}" 5000

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

vinput_remote_text_wait_health \
  "${server_pid}" \
  "http://${lan_address}:${port}/health" \
  "${server_log}" \
  "${out_dir}/health.json"
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
vinput_network_require_listener_released "${port}"

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
