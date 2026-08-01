#!/usr/bin/env bash
# Shared certificate and HTTPS CONNECT fixture helpers.
# shellcheck shell=bash
# shellcheck disable=SC2034,SC2154

provider_network_generate_tls_material() {
  local prefix="$1"
  fixture_ca_key="${out_dir}/${prefix}-ca-key.pem"
  fixture_ca_cert="${out_dir}/${prefix}-ca-cert.pem"
  fixture_server_key="${out_dir}/${prefix}-server-key.pem"
  fixture_server_csr="${out_dir}/${prefix}-server.csr"
  fixture_server_cert="${out_dir}/${prefix}-server-cert.pem"
  fixture_ca_serial="${out_dir}/${prefix}-ca-cert.srl"

  openssl req -x509 -newkey rsa:2048 -nodes \
    -keyout "${fixture_ca_key}" \
    -out "${fixture_ca_cert}" \
    -days 1 \
    -subj '/CN=vinput fixture CA' \
    -addext 'basicConstraints=critical,CA:TRUE' \
    -addext 'keyUsage=critical,keyCertSign,cRLSign' \
    >"${out_dir}/${prefix}-ca.stdout" \
    2>"${out_dir}/${prefix}-ca.stderr"
  openssl req -new -newkey rsa:2048 -nodes \
    -keyout "${fixture_server_key}" \
    -out "${fixture_server_csr}" \
    -subj '/CN=127.0.0.1' \
    -addext 'basicConstraints=critical,CA:FALSE' \
    -addext 'keyUsage=critical,digitalSignature,keyEncipherment' \
    -addext 'extendedKeyUsage=serverAuth' \
    -addext 'subjectAltName=IP:127.0.0.1' \
    >"${out_dir}/${prefix}-server.stdout" \
    2>"${out_dir}/${prefix}-server.stderr"
  openssl x509 -req \
    -in "${fixture_server_csr}" \
    -CA "${fixture_ca_cert}" \
    -CAkey "${fixture_ca_key}" \
    -CAcreateserial \
    -out "${fixture_server_cert}" \
    -days 1 \
    -copy_extensions copy \
    >"${out_dir}/${prefix}-sign.stdout" \
    2>"${out_dir}/${prefix}-sign.stderr"
  chmod 600 "${fixture_ca_key}" "${fixture_server_key}"
}

provider_network_url_port() {
  python3 - "$1" <<'PY'
import sys
from urllib.parse import urlsplit

port = urlsplit(sys.argv[1]).port
if port is None:
    raise SystemExit("URL omitted its explicit fixture port")
print(port)
PY
}

provider_network_proxy_url_with_credentials() {
  python3 - "$1" "$2" "$3" <<'PY'
import sys
from urllib.parse import quote, urlsplit, urlunsplit

url, username, password = sys.argv[1:]
parsed = urlsplit(url)
userinfo = f"{quote(username, safe='')}:{quote(password, safe='')}@"
print(
    urlunsplit(
        (parsed.scheme, userinfo + parsed.netloc, parsed.path, parsed.query, parsed.fragment)
    )
)
PY
}

provider_network_start_connect_proxy() {
  local name="$1"
  local expected_host="$2"
  local expected_port="$3"
  local upstream_host="$4"
  local upstream_port="$5"
  local proxy_username="$6"
  local proxy_password="$7"
  local tls_cert="${8:-}"
  local tls_key="${9:-}"
  local tls_args=()
  if [[ -n "${tls_cert}" || -n "${tls_key}" ]]; then
    tls_args=(--tls-cert "${tls_cert}" --tls-key "${tls_key}")
  fi
  local ready_file="${out_dir}/${name}.ready.json"
  local trace_file="${out_dir}/${name}.trace.json"
  local error_file="${out_dir}/${name}.fixture-error.txt"
  local log_file="${out_dir}/${name}.fixture.log"

  python3 "${connect_proxy_fixture}" \
    --ready-file "${ready_file}" \
    --trace-file "${trace_file}" \
    --error-file "${error_file}" \
    --expected-host "${expected_host}" \
    --expected-port "${expected_port}" \
    --upstream-host "${upstream_host}" \
    --upstream-port "${upstream_port}" \
    --proxy-username "${proxy_username}" \
    --proxy-password "${proxy_password}" \
    "${tls_args[@]}" \
    >"${log_file}" 2>&1 &
  local pid=$!
  fixture_pids+=("${pid}")
  wait_ready "${pid}" "${ready_file}" "${log_file}"
  started_pid="${pid}"
  started_url="$(jq -r '.proxy_url' "${ready_file}")"
}

provider_network_start_intercept_proxy() {
  local name="$1"
  local expected_host="$2"
  local expected_port="$3"
  local upstream_host="$4"
  local upstream_port="$5"
  local proxy_username="$6"
  local proxy_password="$7"
  local intercept_cert="$8"
  local intercept_key="$9"
  local upstream_ca_cert="${10}"
  local ready_file="${out_dir}/${name}.ready.json"
  local trace_file="${out_dir}/${name}.trace.json"
  local error_file="${out_dir}/${name}.fixture-error.txt"
  local log_file="${out_dir}/${name}.fixture.log"

  python3 "${intercept_proxy_fixture}"     --ready-file "${ready_file}"     --trace-file "${trace_file}"     --error-file "${error_file}"     --expected-host "${expected_host}"     --expected-port "${expected_port}"     --upstream-host "${upstream_host}"     --upstream-port "${upstream_port}"     --proxy-username "${proxy_username}"     --proxy-password "${proxy_password}"     --intercept-cert "${intercept_cert}"     --intercept-key "${intercept_key}"     --upstream-ca-cert "${upstream_ca_cert}"     >"${log_file}" 2>&1 &
  local pid=$!
  fixture_pids+=("${pid}")
  wait_ready "${pid}" "${ready_file}" "${log_file}"
  started_pid="${pid}"
  started_url="$(jq -r '.proxy_url' "${ready_file}")"
}

provider_network_remove_tls_material() {
  rm -f \
    "${fixture_ca_key:-}" \
    "${fixture_ca_cert:-}" \
    "${fixture_server_key:-}" \
    "${fixture_server_csr:-}" \
    "${fixture_server_cert:-}" \
    "${fixture_ca_serial:-}"
}
