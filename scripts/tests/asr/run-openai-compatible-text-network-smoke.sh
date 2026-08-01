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

source scripts/tests/asr/provider-network-common.sh

for command in jq openssl python3 ruff; do
  command -v "${command}" >/dev/null 2>&1 || {
    printf 'required command not found: %s\n' "${command}" >&2
    exit 1
  }
done

origin_fixture="scripts/fixtures/openai-compatible-text-provider-fixture.py"
proxy_fixture="scripts/fixtures/http-chat-proxy-fixture.py"
connect_proxy_fixture="scripts/fixtures/https-connect-proxy-fixture.py"
intercept_proxy_fixture="scripts/fixtures/https-intercept-proxy-fixture.py"
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
  rm -f "${config_file}" "${out_dir}/tls-key.pem" "${out_dir}/tls-cert.pem"
  provider_network_remove_tls_material
  find scripts -type d -name __pycache__ -prune -exec rm -rf {} +
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
ruff check "${origin_fixture}" "${proxy_fixture}" "${connect_proxy_fixture}" "${intercept_proxy_fixture}"
ruff format --check "${origin_fixture}" "${proxy_fixture}" "${connect_proxy_fixture}" "${intercept_proxy_fixture}"
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
  -u SSL_CERT_FILE
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
  local response_body_delay_ms="$6"
  shift 6
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
    --response-body-delay-ms "${response_body_delay_ms}" \
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
  local proxy_username="${4:-}"
  local proxy_password="${5:-}"
  local proxy_auth_args=()
  if [[ -n "${proxy_username}" || -n "${proxy_password}" ]]; then
    proxy_auth_args=(
      --proxy-username "${proxy_username}"
      --proxy-password "${proxy_password}"
    )
  fi
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
    "${proxy_auth_args[@]}" \
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
    kill -TERM "${pid}" 2>/dev/null || true
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

# Basic credentials embedded in the proxy URL produce a redacted Proxy-Authorization header.
proxy_username="fixture-proxy-user"
proxy_password="fixture-proxy-password"
start_proxy authenticated-proxy remote-text-auth-proxy.invalid "authenticated proxy text final" \
  "${proxy_username}" "${proxy_password}"
authenticated_proxy_pid="${started_pid}"
authenticated_proxy_url="${started_url/http:\/\//http://${proxy_username}:${proxy_password}@}"
write_config "http://remote-text-auth-proxy.invalid/v1"
run_cli_success authenticated-proxy "authenticated proxy text final" 2000 \
  env \
  -u ALL_PROXY -u all_proxy -u HTTPS_PROXY -u https_proxy -u NO_PROXY -u no_proxy \
  HTTP_PROXY="${authenticated_proxy_url}" http_proxy="${authenticated_proxy_url}"
wait_fixture "${authenticated_proxy_pid}"
jq -e '
  .event == "proxy-request" and
  .request_count == 1 and
  .target_host == "remote-text-auth-proxy.invalid" and
  .target_path == "/v1/chat/completions" and
  .authorization_scheme == "Bearer" and
  .authorization_value_recorded == false and
  .proxy_authorization_scheme == "Basic" and
  .proxy_authorization_value_recorded == false and
  .proxy_authenticated == true
' "${out_dir}/authenticated-proxy.trace.json" >/dev/null

# An additional PEM root enables verified HTTPS through an authenticated CONNECT proxy.
provider_network_generate_tls_material custom-ca
start_origin custom-ca-origin "custom ca text final: " 200 "unused" 0 0 \
  --tls-cert "${fixture_server_cert}" \
  --tls-key "${fixture_server_key}"
custom_ca_origin_pid="${started_pid}"
custom_ca_origin_url="${started_url}"
custom_ca_origin_port="$(provider_network_url_port "${custom_ca_origin_url}")"
provider_network_start_connect_proxy \
  custom-ca-connect 127.0.0.1 "${custom_ca_origin_port}" \
  127.0.0.1 "${custom_ca_origin_port}" \
  "${proxy_username}" "${proxy_password}"
custom_ca_proxy_pid="${started_pid}"
custom_ca_proxy_url="${started_url/http:\/\//http://${proxy_username}:${proxy_password}@}"
write_config "${custom_ca_origin_url}"
run_cli_success \
  custom-ca-connect "custom ca text final: ${input_text}" 5000 \
  env \
  -u ALL_PROXY -u all_proxy -u HTTP_PROXY -u http_proxy \
  -u NO_PROXY -u no_proxy \
  HTTPS_PROXY="${custom_ca_proxy_url}" https_proxy="${custom_ca_proxy_url}" \
  SSL_CERT_FILE="${fixture_ca_cert}"
wait_fixture "${custom_ca_origin_pid}"
wait_fixture "${custom_ca_proxy_pid}"
jq -e --arg input "${input_text}" '
  .event == "request" and
  .request_count == 1 and
  .response_status == 200 and
  .raw_asr_text == $input and
  .candidate == ("custom ca text final: " + $input)
' "${out_dir}/custom-ca-origin.trace.json" >/dev/null
jq -e --argjson port "${custom_ca_origin_port}" '
  .event == "connect-tunnel" and
  .request_count == 1 and
  .method == "CONNECT" and
  .target_host == "127.0.0.1" and
  .target_port == $port and
  .proxy_authorization_scheme == "Basic" and
  .proxy_authorization_value_recorded == false and
  .proxy_authenticated == true and
  .client_to_upstream_bytes > 0 and
  .upstream_to_client_bytes > 0 and
  .tunnel_timeout == false and
  .payload_recorded == false
' "${out_dir}/custom-ca-connect.trace.json" >/dev/null

# A TLS-protected proxy endpoint can authenticate and establish the same HTTPS tunnel.
start_origin https-proxy-origin "https proxy text final: " 200 "unused" 0 0 \
  --tls-cert "${fixture_server_cert}" \
  --tls-key "${fixture_server_key}"
https_proxy_origin_pid="${started_pid}"
https_proxy_origin_url="${started_url}"
https_proxy_origin_port="$(provider_network_url_port "${https_proxy_origin_url}")"
provider_network_start_connect_proxy \
  https-proxy-endpoint 127.0.0.1 "${https_proxy_origin_port}" \
  127.0.0.1 "${https_proxy_origin_port}" \
  "${proxy_username}" "${proxy_password}" \
  "${fixture_server_cert}" "${fixture_server_key}"
https_proxy_endpoint_pid="${started_pid}"
https_proxy_endpoint_url="$(provider_network_proxy_url_with_credentials \
  "${started_url}" "${proxy_username}" "${proxy_password}")"
write_config "${https_proxy_origin_url}"
run_cli_success \
  https-proxy-endpoint "https proxy text final: ${input_text}" 5000 \
  env \
  -u ALL_PROXY -u all_proxy -u HTTP_PROXY -u http_proxy \
  -u NO_PROXY -u no_proxy \
  HTTPS_PROXY="${https_proxy_endpoint_url}" https_proxy="${https_proxy_endpoint_url}" \
  SSL_CERT_FILE="${fixture_ca_cert}"
wait_fixture "${https_proxy_origin_pid}"
wait_fixture "${https_proxy_endpoint_pid}"
jq -e --arg input "${input_text}" '
  .event == "request" and
  .request_count == 1 and
  .response_status == 200 and
  .raw_asr_text == $input and
  .candidate == ("https proxy text final: " + $input)
' "${out_dir}/https-proxy-origin.trace.json" >/dev/null
jq -e --argjson port "${https_proxy_origin_port}" '
  .event == "connect-tunnel" and
  .request_count == 1 and
  .method == "CONNECT" and
  .target_host == "127.0.0.1" and
  .target_port == $port and
  .proxy_authorization_scheme == "Basic" and
  .proxy_authorization_value_recorded == false and
  .proxy_authenticated == true and
  .proxy_tls == true and
  .client_to_upstream_bytes > 0 and
  .upstream_to_client_bytes > 0 and
  .tunnel_timeout == false and
  .payload_recorded == false
' "${out_dir}/https-proxy-endpoint.trace.json" >/dev/null

# A local interception proxy terminates and re-establishes verified TLS without retaining payloads.
start_origin tls-intercept-origin "tls intercept text final: " 200 "unused" 0 0   --tls-cert "${fixture_server_cert}"   --tls-key "${fixture_server_key}"
tls_intercept_origin_pid="${started_pid}"
tls_intercept_origin_url="${started_url}"
tls_intercept_origin_port="$(provider_network_url_port "${tls_intercept_origin_url}")"
provider_network_start_intercept_proxy   tls-intercept-proxy 127.0.0.1 "${tls_intercept_origin_port}"   127.0.0.1 "${tls_intercept_origin_port}"   "${proxy_username}" "${proxy_password}"   "${fixture_server_cert}" "${fixture_server_key}" "${fixture_ca_cert}"
tls_intercept_proxy_pid="${started_pid}"
tls_intercept_proxy_url="$(provider_network_proxy_url_with_credentials   "${started_url}" "${proxy_username}" "${proxy_password}")"
write_config "${tls_intercept_origin_url}"
run_cli_success   tls-intercept-proxy "tls intercept text final: ${input_text}" 5000   env   -u ALL_PROXY -u all_proxy -u HTTP_PROXY -u http_proxy   -u NO_PROXY -u no_proxy   HTTPS_PROXY="${tls_intercept_proxy_url}" https_proxy="${tls_intercept_proxy_url}"   SSL_CERT_FILE="${fixture_ca_cert}"
wait_fixture "${tls_intercept_origin_pid}"
wait_fixture "${tls_intercept_proxy_pid}"
jq -e --arg input "${input_text}" '
  .event == "request" and
  .request_count == 1 and
  .response_status == 200 and
  .raw_asr_text == $input and
  .candidate == ("tls intercept text final: " + $input)
' "${out_dir}/tls-intercept-origin.trace.json" >/dev/null
jq -e --argjson port "${tls_intercept_origin_port}" '
  .event == "tls-intercept" and
  .request_count == 1 and
  .method == "CONNECT" and
  .target_host == "127.0.0.1" and
  .target_port == $port and
  .proxy_authorization_scheme == "Basic" and
  .proxy_authorization_value_recorded == false and
  .proxy_authenticated == true and
  (.client_tls_version | startswith("TLSv")) and
  (.upstream_tls_version | startswith("TLSv")) and
  .request_header_bytes > 0 and
  .request_body_bytes > 0 and
  .response_header_bytes > 0 and
  .response_body_bytes > 0 and
  .payload_recorded == false
' "${out_dir}/tls-intercept-proxy.trace.json" >/dev/null

# NO_PROXY bypasses an available proxy for the loopback origin.
start_origin no-proxy-origin "no proxy text final: " 200 "unused" 0 0
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
  start_origin "${case_name}" "unused: " "${status}" "${marker}" 0 0
  origin_pid="${started_pid}"
  origin_url="${started_url}"
  write_config "${origin_url}"
  run_cli_failure "${case_name}" 2000 "${clear_proxy_env[@]}"
  wait_fixture "${origin_pid}"
  grep -Fq "HTTP ${status}" "${out_dir}/${case_name}.stderr"
  grep -Fq "${marker}" "${out_dir}/${case_name}.stderr"
done

# Provider redirects fail closed and never contact the advertised target.
start_origin redirect-target "redirect target must not run: " 200 "unused" 0 0
redirect_target_pid="${started_pid}"
redirect_target_url="${started_url}"
redirect_marker="text-redirect-denied-marker"
start_origin redirect-origin "unused: " 307 "${redirect_marker}" 0 0 \
  --response-location "${redirect_target_url}"
redirect_origin_pid="${started_pid}"
redirect_origin_url="${started_url}"
write_config "${redirect_origin_url}"
run_cli_failure redirect 2000 "${clear_proxy_env[@]}"
wait_fixture "${redirect_origin_pid}"
grep -Fq 'HTTP 307' "${out_dir}/redirect.stderr"
grep -Fq "${redirect_marker}" "${out_dir}/redirect.stderr"
jq -e '
  .event == "request" and
  .request_count == 1 and
  .response_status == 307 and
  .response_location_present == true
' "${out_dir}/redirect-origin.trace.json" >/dev/null
test ! -e "${out_dir}/redirect-target.trace.json"
test ! -e "${out_dir}/redirect-target.fixture-error.txt"
stop_fixture "${redirect_target_pid}"

# Success and error bodies larger than the shared safety limit are rejected.
oversized_padding_bytes=1100000
start_origin oversized-success "oversized success: " 200 "unused" 0 0 \
  --response-padding-bytes "${oversized_padding_bytes}"
oversized_success_pid="${started_pid}"
oversized_success_url="${started_url}"
write_config "${oversized_success_url}"
run_cli_failure oversized-success 2000 "${clear_proxy_env[@]}"
wait_fixture "${oversized_success_pid}"
grep -Fq 'OpenAI-compatible HTTP response body exceeds 1048576-byte limit'   "${out_dir}/oversized-success.stderr"
jq -e --argjson padding "${oversized_padding_bytes}" '
  .event == "request" and
  .response_status == 200 and
  .response_padding_bytes == $padding
' "${out_dir}/oversized-success.trace.json" >/dev/null

oversized_error_marker="oversized-text-error-marker"
start_origin oversized-error "unused: " 503 "${oversized_error_marker}" 0 0 \
  --response-padding-bytes "${oversized_padding_bytes}"
oversized_error_pid="${started_pid}"
oversized_error_url="${started_url}"
write_config "${oversized_error_url}"
run_cli_failure oversized-error 2000 "${clear_proxy_env[@]}"
wait_fixture "${oversized_error_pid}"
grep -Fq 'OpenAI-compatible HTTP response body exceeds 1048576-byte limit'   "${out_dir}/oversized-error.stderr"
if grep -Fq "${oversized_error_marker}" "${out_dir}/oversized-error.stderr"; then
  echo "oversized text-provider error body leaked into diagnostics" >&2
  exit 1
fi
jq -e --argjson padding "${oversized_padding_bytes}" '
  .event == "request" and
  .response_status == 503 and
  .response_padding_bytes == $padding
' "${out_dir}/oversized-error.trace.json" >/dev/null

# Request deadlines fail explicitly after the origin accepts the request.
start_origin timeout "late text response: " 200 "unused" 250 0
timeout_pid="${started_pid}"
timeout_url="${started_url}"
write_config "${timeout_url}"
run_cli_failure timeout 25 "${clear_proxy_env[@]}"
wait_fixture "${timeout_pid}"
grep -Fq 'OpenAI-compatible HTTP request timed out' "${out_dir}/timeout.stderr"
jq -e '.event == "request" and .response_delay_ms == 250' \
  "${out_dir}/timeout.trace.json" >/dev/null

# Response headers followed by a stalled body retain a distinct diagnostic.
start_origin response-body-timeout "late text body: " 200 "unused" 0 250
body_timeout_pid="${started_pid}"
body_timeout_url="${started_url}"
write_config "${body_timeout_url}"
run_cli_failure response-body-timeout 25 "${clear_proxy_env[@]}"
wait_fixture "${body_timeout_pid}"
grep -Fq 'OpenAI-compatible HTTP response body timed out' \
  "${out_dir}/response-body-timeout.stderr"
jq -e '.event == "request" and .response_body_delay_ms == 250' \
  "${out_dir}/response-body-timeout.trace.json" >/dev/null

# A self-signed HTTPS endpoint is rejected by the production rustls trust policy.
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "${out_dir}/tls-key.pem" \
  -out "${out_dir}/tls-cert.pem" \
  -days 1 \
  -subj '/CN=127.0.0.1' \
  -addext 'subjectAltName=IP:127.0.0.1' \
  >"${out_dir}/openssl.stdout" 2>"${out_dir}/openssl.stderr"
start_origin tls "must not complete: " 200 "unused" 0 0 \
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
write_config 'http://remote-text-dns-failure.invalid/v1?api-key=diagnostic-url-secret#fragment'
run_cli_failure dns 5000 "${clear_proxy_env[@]}" \
  RES_OPTIONS='attempts:1 timeout:1'
grep -Fq 'OpenAI-compatible HTTP request failed' "${out_dir}/dns.stderr"
grep -Fq 'api-key=REDACTED' "${out_dir}/dns.stderr"
if grep -Fq 'diagnostic-url-secret' "${out_dir}/dns.stderr"; then
  echo "DNS failure diagnostics retained the query secret" >&2
  exit 1
fi
if grep -Fq 'fragment' "${out_dir}/dns.stderr"; then
  echo "DNS failure diagnostics retained the URL fragment" >&2
  exit 1
fi

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

rm -f "${config_file}" "${out_dir}/tls-key.pem" "${out_dir}/tls-cert.pem"
provider_network_remove_tls_material
unset proxy_username proxy_password authenticated_proxy_url custom_ca_proxy_url
unset https_proxy_endpoint_url tls_intercept_proxy_url
for proxy_secret in fixture-proxy-user fixture-proxy-password; do
  if grep -R -F -- "${proxy_secret}" "${out_dir}" >/dev/null; then
    echo "network evidence retained proxy credentials" >&2
    exit 1
  fi
done
if grep -R -F -- "${api_key}" "${out_dir}" >/dev/null; then
  echo "text provider network evidence retained the API key" >&2
  exit 1
fi

jq -n \
  --arg event summary '
  {
    event: $event,
    proxy_route: true,
    basic_proxy_auth: true,
    custom_ca_bundle: true,
    https_connect_proxy: true,
    https_proxy_endpoint: true,
    tls_interception_proxy: true,
    no_proxy_bypass: true,
    rate_limit_429: true,
    service_unavailable_503: true,
    redirects_rejected: true,
    request_timeout: true,
    response_body_timeout: true,
    oversized_success_response_rejected: true,
    oversized_error_response_rejected: true,
    self_signed_tls_rejected: true,
    dns_failure: true,
    connection_refused: true,
    credentials_redacted: true,
    hosted_service_proof: false,
    ok: true
  }
' | tee "${out_dir}/summary.json"

printf 'OpenAI-compatible text provider network semantics smoke passed\n'
