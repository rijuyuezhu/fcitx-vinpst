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
# shellcheck source=../../live/network/remote-text-common.sh
source "${repo_root}/scripts/live/network/remote-text-common.sh"
cd "${repo_root}"

for command in curl ip jq python3 ss; do
  vinpst_network_require_command "${command}"
done
for command in /usr/bin/ip /usr/bin/ss; do
  [[ -x "${command}" ]] || {
    echo "required system network command is missing: ${command}" >&2
    exit 1
  }
done

mkdir -p target/tmp
confirmation_log="$(mktemp target/tmp/remote-text-external-confirmation.XXXXXX)"
if scripts/live/network/run-remote-text-external-device-live.sh \
  >/dev/null 2>"${confirmation_log}"; then
  echo "external-device wrapper unexpectedly ran without physical-device confirmation" >&2
  exit 1
fi
grep -Fq 'VINPST_REMOTE_TEXT_CONFIRM_PHYSICAL_DEVICE=1' "${confirmation_log}"
rm -f "${confirmation_log}"

cargo build -q -p vinpst-daemon --bin vinpst-daemon

root="${VINPST_REMOTE_TEXT_COLLECTOR_SMOKE_DIR:-target/tmp/remote-text-external-collector-smoke}"
rm -rf "${root}"
mkdir -p "${root}"
read -r port < <(vinpst_network_reserve_ports 1)
api_key="$(vinpst_network_random_token)"
challenge=vinpst-external-collector-smoke
config_path="${root}/config.json"
vinpst_remote_text_write_config "${config_path}" "${port}" "${api_key}" 600000

target/debug/vinpst-daemon \
  --config "${config_path}" \
  remote-text-server --bind 127.0.0.1 \
  >"${root}/server.log" 2>&1 &
server_pid=$!
input_pid=
cleanup() {
  local exit_code=$?
  set +e
  [[ -n "${input_pid}" ]] && kill "${input_pid}" 2>/dev/null || true
  kill -INT "${server_pid}" 2>/dev/null || true
  wait "${server_pid}" 2>/dev/null || true
  rm -rf "${root}"
  trap - EXIT INT TERM
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

vinpst_remote_text_wait_health \
  "${server_pid}" \
  "http://127.0.0.1:${port}/health" \
  "${root}/server.log" \
  "${root}/health.json"
rm -f "${config_path}"

# No input peer must time out without writing proof.
set +e
VINPST_REMOTE_TEXT_API_KEY="${api_key}" \
  python3 scripts/live/network/remote-text-external-device-collector.py \
  --endpoint "http://127.0.0.1:${port}" \
  --output-url "ws://127.0.0.1:${port}/v1/realtime" \
  --port "${port}" \
  --challenge "${challenge}" \
  --out-dir "${root}/timeout" \
  --timeout-seconds 1 \
  --ip-command /usr/bin/ip \
  --ss-command /usr/bin/ss \
  >"${root}/timeout.stdout" 2>"${root}/timeout.stderr"
timeout_status=$?
set -e
if ((timeout_status == 0)); then
  echo "collector unexpectedly succeeded without an input peer" >&2
  exit 1
fi
grep -Eq 'timed out|Timeout' "${root}/timeout.stderr"
test ! -e "${root}/timeout/summary.json"

# A completed challenge from this same host must still be rejected.
set +e
VINPST_REMOTE_TEXT_API_KEY="${api_key}" \
  python3 scripts/live/network/remote-text-external-device-collector.py \
  --endpoint "http://127.0.0.1:${port}" \
  --output-url "ws://127.0.0.1:${port}/v1/realtime" \
  --port "${port}" \
  --challenge "${challenge}" \
  --out-dir "${root}/same-host" \
  --timeout-seconds 10 \
  --ip-command /usr/bin/ip \
  --ss-command /usr/bin/ss \
  >"${root}/same-host.stdout" 2>"${root}/same-host.stderr" &
collector_pid=$!
set -e
sleep 0.1
VINPST_REMOTE_TEXT_API_KEY="${api_key}" \
  python3 scripts/fixtures/remote-text-input-client.py \
  --url "ws://127.0.0.1:${port}/ws" \
  --text "${challenge}" \
  --hold-seconds 1 \
  --require-output-connected &
input_pid=$!
set +e
wait "${collector_pid}"
collector_status=$?
set -e
wait "${input_pid}"
input_pid=
if ((collector_status == 0)); then
  echo "collector incorrectly accepted a same-host input peer" >&2
  exit 1
fi
grep -Fq 'no established remote-text peer differs from every local address' \
  "${root}/same-host.stderr"
test ! -e "${root}/same-host/summary.json"

# Distinct network evidence still requires explicit physical-device confirmation.
fake_network_dir="${root}/fake-network"
mkdir -p "${fake_network_dir}"
cat >"${fake_network_dir}/ip" <<'SH'
#!/usr/bin/env sh
cat <<'JSON'
[{"addr_info":[{"local":"127.0.0.1"}]}]
JSON
SH
cat >"${fake_network_dir}/ss" <<SH
#!/usr/bin/env sh
cat <<'EOF'
0 0 127.0.0.1:${port} 127.0.0.1:43100
0 0 127.0.0.1:${port} 198.51.100.44:53100
EOF
SH
chmod 755 "${fake_network_dir}/ip" "${fake_network_dir}/ss"

set +e
VINPST_REMOTE_TEXT_API_KEY="${api_key}" \
  python3 scripts/live/network/remote-text-external-device-collector.py \
  --endpoint "http://127.0.0.1:${port}" \
  --output-url "ws://127.0.0.1:${port}/v1/realtime" \
  --port "${port}" \
  --challenge "${challenge}" \
  --out-dir "${root}/missing-confirmation" \
  --timeout-seconds 15 \
  --ip-command "${fake_network_dir}/ip" \
  --ss-command "${fake_network_dir}/ss" \
  >"${root}/missing-confirmation.stdout" \
  2>"${root}/missing-confirmation.stderr" &
collector_pid=$!
set -e
sleep 0.1
VINPST_REMOTE_TEXT_API_KEY="${api_key}" \
  python3 scripts/fixtures/remote-text-input-client.py \
  --url "ws://127.0.0.1:${port}/ws" \
  --text "${challenge}" \
  --hold-seconds 1 \
  --require-output-connected &
input_pid=$!
set +e
wait "${collector_pid}"
collector_status=$?
set -e
wait "${input_pid}"
input_pid=
if ((collector_status == 0)); then
  echo "collector incorrectly accepted a peer without physical-device confirmation" >&2
  exit 1
fi
grep -Fq 'physical-device confirmation is missing' \
  "${root}/missing-confirmation.stderr"
test ! -e "${root}/missing-confirmation/summary.json"

if grep -R -F -- "${api_key}" "${root}" >/dev/null; then
  echo "collector smoke retained the API key" >&2
  exit 1
fi

kill -INT "${server_pid}"
wait "${server_pid}"
trap - EXIT INT TERM
vinpst_network_require_listener_released "${port}"
rm -rf "${root}"
echo "remote text external-device collector smoke passed"
