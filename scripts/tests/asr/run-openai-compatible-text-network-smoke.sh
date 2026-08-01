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

for command in jq openssl python3 ruff; do
  command -v "${command}" >/dev/null 2>&1 || {
    printf 'required command not found: %s\n' "${command}" >&2
    exit 1
  }
done

origin_fixture="scripts/fixtures/openai-compatible-text-provider-fixture.py"
proxy_fixture="scripts/fixtures/http-chat-proxy-fixture.py"
cli="target/debug/vinput"
out_dir="target/tmp/openai-compatible-text-network-smoke"
config_file="${out_dir}/config.json"
api_key="text-network-secret-marker"
model="text-network-model"
input_text="text network input marker"
fixture_pids=()
started_pid=""
started_url=""

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM
  set +e
  local pid
  for pid in "${fixture_pids[@]}"; do
    if [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null; then
      kill -TERM "${pid}" 2>/dev/null || true
      wait "${pid}" 2>/dev/null || true
    fi
  done
  rm -f "${config_file}"
  find scripts -type d -name __pycache__ -prune -exec rm -rf {} +
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
ruff check "${origin_fixture}" "${proxy_fixture}"
ruff format --check "${origin_fixture}" "${proxy_fixture}"
cargo build -q -p vinput-cli --bin vinput

write_config() {
  local base_url="$1"
  jq -n \
    --arg base_url "${base_url}" \
    --arg key "${api_key}" \
    --arg model "${model}" '
    {
      version: 1,
      llm: {
        providers: [{
          id: "network-text",
          base_url: $base_url,
          api_key: $key,
          model: $model,
          extra_body: {}
        }]
      },
      scenes: {
        active_scene: "raw",
        definitions: [{id: "raw", label: "Raw", candidate_count: 0}]
      }
    }
  ' >"${config_file}"
}

clear_proxy_env=(
  env
  -u ALL_PROXY -u all_proxy
  -u HTTP_PROXY -u http_proxy
  -u HTTPS_PROXY -u https_proxy
  -u NO_PROXY -u no_proxy
)

run_cli_success() {
  local name="$1"
  local expected="$2"
  local timeout_ms="$3"
  shift 3
  "$@" "${cli}" llm test network-text \
    --config "${config_file}" \
    --text "${input_text}" \
    --timeout-ms "${timeout_ms}" \
    --json >"${out_dir}/${name}.stdout" 2>"${out_dir}/${name}.stderr"
  jq -e --arg text "${expected}" --arg input "${input_text}" '
    .ok == true and
    .called == true and
    .will_call_http == true and
    .result.commit_text == $text and
    .result.candidate_count == 1 and
    (.request.headers | any(. == ["Authorization", "<redacted>"])) and
    (.request.body.messages[0].content | contains($input))
  ' "${out_dir}/${name}.stdout" >/dev/null
}

run_cli_failure() {
  local name="$1"
  local timeout_ms="$2"
  shift 2
  set +e
  "$@" "${cli}" llm test network-text \
    --config "${config_file}" \
    --text "${input_text}" \
    --timeout-ms "${timeout_ms}" \
    --json >"${out_dir}/${name}.stdout" 2>"${out_dir}/${name}.stderr"
  local status=$?
  set -e
  if ((status == 0)); then
    echo "OpenAI-compatible text network case unexpectedly succeeded: ${name}" >&2
    exit 1
  fi
}

wait_ready() {
  local pid="$1"
  local ready_file="$2"
  local log_file="$3"
  for _ in $(seq 1 100); do
    [[ -f "${ready_file}" ]] && return 0
    if ! kill -0 "${pid}" 2>/dev/null; then
      cat "${log_file}" >&2
      echo "fixture exited before readiness: ${log_file}" >&2
      return 1
    fi
    sleep 0.05
  done
  echo "fixture did not publish readiness: ${ready_file}" >&2
  return 1
}

start_origin() {
  local name="$1"
  local response_prefix="$2"
  local response_status="$3"
  local response_error="$4"
  local response_delay_ms="$5"
  shift 5
  local ready_file="${out_dir}/${name}.ready.json"
  local trace_file="${out_dir}/${name}.trace.json"
  local error_file="${out_dir}/${name}.fixture-error.txt"
  local log_file="${out_dir}/${name}.fixture.log"
  python3 "${origin_fixture}" \
    --ready-file "${ready_file}" \
    --trace-file "${trace_file}" \
    --error-file "${error_file}" \
    --api-key "${api_key}" \
    --model "${model}" \
    --response-prefix "${response_prefix}" \
    --response-status "${response_status}" \
    --response-error "${response_error}" \
    --response-delay-ms "${response_delay_ms}" \
    --allow-empty-selected \
    "$@" >"${log_file}" 2>&1 &
  local pid=$!
  fixture_pids+=("${pid}")
  wait_ready "${pid}" "${ready_file}" "${log_file}"
  started_pid="${pid}"
  started_url="$(jq -r '.base_url' "${ready_file}")"
}

start_proxy() {
  local name="$1"
  local expected_host="$2"
  local response_text="$3"
  local ready_file="${out_dir}/${name}.ready.json"
  local trace_file="${out_dir}/${name}.trace.json"
  local log_file="${out_dir}/${name}.fixture.log"
  python3 "${proxy_fixture}" \
    --ready-file "${ready_file}" \
    --trace-file "${trace_file}" \
    --api-key "${api_key}" \
    --expected-host "${expected_host}" \
    --model "${model}" \
    --input-text "${input_text}" \
    --response-text "${response_text}" \
    >"${log_file}" 2>&1 &
  local pid=$!
  fixture_pids+=("${pid}")
  wait_ready "${pid}" "${ready_file}" "${log_file}"
  started_pid="${pid}"
  started_url="$(jq -r '.proxy_url' "${ready_file}")"
}

forget_fixture() {
  local pid="$1"
  local kept=()
  local candidate
  for candidate in "${fixture_pids[@]}"; do
    [[ "${candidate}" == "${pid}" ]] || kept+=("${candidate}")
  done
  fixture_pids=("${kept[@]}")
}

wait_fixture() {
  local pid="$1"
  wait "${pid}"
  forget_fixture "${pid}"
}

stop_fixture() {
  local pid="$1"
  if kill -0 "${pid}" 2>/dev/null; then
    kill -TERM "${pid}"
  fi
  set +e
  wait "${pid}"
  set -e
  forget_fixture "${pid}"
}

# HTTP_PROXY routes a fake-host chat request through the configured proxy.
start_proxy proxy-route remote-text-proxy.invalid "proxy routed text final"
proxy_pid="${started_pid}"
proxy_url="${started_url}"
write_config "http://remote-text-proxy.invalid/v1"
run_cli_success proxy-route "proxy routed text final" 2000 \
  env \
  -u ALL_PROXY -u all_proxy -u HTTPS_PROXY -u https_proxy -u NO_PROXY -u no_proxy \
  HTTP_PROXY="${proxy_url}" http_proxy="${proxy_url}"
wait_fixture "${proxy_pid}"
jq -e '
  .event == "proxy-request" and
  .request_count == 1 and
  .method == "POST" and
  .target_scheme == "http" and
  .target_host == "remote-text-proxy.invalid" and
  .target_path == "/v1/chat/completions" and
  .authorization_scheme == "Bearer" and
  .authorization_value_recorded == false and
  .content_type == "application/json" and
  .model == "text-network-model" and
  .input_text_present == true and
  .input_text_recorded == false and
  .body_bytes > 0 and
  (.body_sha256 | length) == 64 and
  .response_text == "proxy routed text final"
' "${out_dir}/proxy-route.trace.json" >/dev/null

# NO_PROXY bypasses an available proxy for the loopback origin.
start_origin no-proxy-origin "no proxy text final: " 200 "unused" 0
origin_pid="${started_pid}"
origin_url="${started_url}"
start_proxy no-proxy-unused unused.invalid "must not be used"
bypass_proxy_pid="${started_pid}"
bypass_proxy_url="${started_url}"
write_config "${origin_url}"
run_cli_success no-proxy "no proxy text final: ${input_text}" 2000 \
  env \
  -u ALL_PROXY -u all_proxy -u HTTPS_PROXY -u https_proxy \
  HTTP_PROXY="${bypass_proxy_url}" http_proxy="${bypass_proxy_url}" \
  NO_PROXY="127.0.0.1" no_proxy="127.0.0.1"
wait_fixture "${origin_pid}"
test ! -e "${out_dir}/no-proxy-unused.trace.json"
stop_fixture "${bypass_proxy_pid}"
jq -e '
  .event == "request" and
  .request_count == 1 and
  .response_status == 200 and
  .selected_text == "" and
  .raw_asr_text == "text network input marker"
' "${out_dir}/no-proxy-origin.trace.json" >/dev/null

# Rate-limit and service-outage responses retain status and body diagnostics.
for case_name in rate-limit service-unavailable; do
  if [[ "${case_name}" == rate-limit ]]; then
    status=429
    marker="text-retry-later-marker"
  else
    status=503
    marker="text-service-offline-marker"
  fi
  start_origin "${case_name}" "unused: " "${status}" "${marker}" 0
  origin_pid="${started_pid}"
  origin_url="${started_url}"
  write_config "${origin_url}"
  run_cli_failure "${case_name}" 2000 "${clear_proxy_env[@]}"
  wait_fixture "${origin_pid}"
  grep -Fq "HTTP ${status}" "${out_dir}/${case_name}.stderr"
  grep -Fq "${marker}" "${out_dir}/${case_name}.stderr"
done

# Request deadlines fail explicitly after the origin accepts the request.
start_origin timeout "late text response: " 200 "unused" 250
timeout_pid="${started_pid}"
timeout_url="${started_url}"
write_config "${timeout_url}"
run_cli_failure timeout 25 "${clear_proxy_env[@]}"
wait_fixture "${timeout_pid}"
grep -Fq 'OpenAI-compatible HTTP request timed out' "${out_dir}/timeout.stderr"
jq -e '.event == "request" and .response_delay_ms == 250' \
  "${out_dir}/timeout.trace.json" >/dev/null

# A self-signed HTTPS endpoint is rejected by the production rustls trust policy.
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "${out_dir}/tls-key.pem" \
  -out "${out_dir}/tls-cert.pem" \
  -days 1 \
  -subj '/CN=127.0.0.1' \
  -addext 'subjectAltName=IP:127.0.0.1' \
  >"${out_dir}/openssl.stdout" 2>"${out_dir}/openssl.stderr"
start_origin tls "must not complete: " 200 "unused" 0 \
  --tls-cert "${out_dir}/tls-cert.pem" \
  --tls-key "${out_dir}/tls-key.pem"
tls_pid="${started_pid}"
tls_url="${started_url}"
write_config "${tls_url}"
run_cli_failure tls 2000 "${clear_proxy_env[@]}"
grep -Fq 'OpenAI-compatible HTTP request failed' "${out_dir}/tls.stderr"
if [[ -e "${out_dir}/tls.trace.json" ]]; then
  echo "self-signed text TLS fixture unexpectedly received an authenticated request" >&2
  exit 1
fi
stop_fixture "${tls_pid}"

# DNS resolution and a refused local connection remain distinct request failures.
write_config 'http://remote-text-dns-failure.invalid/v1'
run_cli_failure dns 2000 "${clear_proxy_env[@]}"
grep -Fq 'OpenAI-compatible HTTP request failed' "${out_dir}/dns.stderr"

refused_port="$(python3 - <<'PY'
import socket
with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)"
write_config "http://127.0.0.1:${refused_port}/v1"
run_cli_failure connection-refused 2000 "${clear_proxy_env[@]}"
grep -Fq 'OpenAI-compatible HTTP request failed' "${out_dir}/connection-refused.stderr"

rm -f "${config_file}" "${out_dir}/tls-key.pem"
if grep -R -F -- "${api_key}" "${out_dir}" >/dev/null; then
  echo "text provider network evidence retained the API key" >&2
  exit 1
fi

jq -n \
  --arg event summary '
  {
    event: $event,
    proxy_route: true,
    no_proxy_bypass: true,
    rate_limit_429: true,
    service_unavailable_503: true,
    request_timeout: true,
    self_signed_tls_rejected: true,
    dns_failure: true,
    connection_refused: true,
    credentials_redacted: true,
    hosted_service_proof: false,
    ok: true
  }
' | tee "${out_dir}/summary.json"

printf 'OpenAI-compatible text provider network semantics smoke passed\n'
