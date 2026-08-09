#!/usr/bin/env bash
# shellcheck disable=SC1091,SC2154
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

out_dir="${VINPST_PROVIDER_CA_ROTATION_DIR:-target/tmp/provider-ca-rotation-smoke}"
common="scripts/tests/asr/provider-network-common.sh"
asr_fixture="scripts/fixtures/openai-compatible-asr-fixture.py"
text_fixture="scripts/fixtures/openai-compatible-text-provider-fixture.py"
daemon="target/debug/vinpst-daemon"
cli="target/debug/vinpst"
wav_file="${out_dir}/input.wav"
asr_config="${out_dir}/asr-config.json"
text_config="${out_dir}/text-config.json"
trusted_ca="${out_dir}/trusted-ca.pem"
api_key="rotation-secret-marker"
model="rotation-model"
language="zh"
prompt="rotation prompt marker"
selected_text="rotation selected text"

# shellcheck source=scripts/tests/asr/provider-network-common.sh
source "${common}"

rotation_files=()
cleanup() {
  local exit_code=$?
  for pid in $(jobs -pr); do
    kill -TERM "${pid}" 2>/dev/null || true
    wait "${pid}" 2>/dev/null || true
  done
  rm -f "${asr_config}" "${text_config}" "${trusted_ca}" "${trusted_ca}.next"
  if ((${#rotation_files[@]})); then
    rm -f "${rotation_files[@]}"
  fi
  find scripts -type d -name __pycache__ -prune -exec rm -rf {} +
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
ruff check "${asr_fixture}" "${text_fixture}"
ruff format --check "${asr_fixture}" "${text_fixture}"
cargo build -q -p vinpst-daemon -p vinpst-cli

python3 - "${wav_file}" <<'PY'
import math
import struct
import sys
import wave
from pathlib import Path

path = Path(sys.argv[1])
sample_rate = 16_000
samples = [
    int(8_000 * math.sin(2 * math.pi * 440 * index / sample_rate))
    for index in range(sample_rate // 4)
]
with wave.open(str(path), "wb") as wav:
    wav.setnchannels(1)
    wav.setsampwidth(2)
    wav.setframerate(sample_rate)
    wav.writeframes(b"".join(struct.pack("<h", sample) for sample in samples))
PY

provider_network_generate_tls_material rotation-a
ca_a_key="${fixture_ca_key}"
ca_a_cert="${fixture_ca_cert}"
server_a_key="${fixture_server_key}"
server_a_csr="${fixture_server_csr}"
server_a_cert="${fixture_server_cert}"
ca_a_serial="${fixture_ca_serial}"
rotation_files+=(
  "${ca_a_key}" "${ca_a_cert}" "${server_a_key}"
  "${server_a_csr}" "${server_a_cert}" "${ca_a_serial}"
)

provider_network_generate_tls_material rotation-b
ca_b_key="${fixture_ca_key}"
ca_b_cert="${fixture_ca_cert}"
server_b_key="${fixture_server_key}"
server_b_csr="${fixture_server_csr}"
server_b_cert="${fixture_server_cert}"
ca_b_serial="${fixture_ca_serial}"
rotation_files+=(
  "${ca_b_key}" "${ca_b_cert}" "${server_b_key}"
  "${server_b_csr}" "${server_b_cert}" "${ca_b_serial}"
)

read -r asr_port text_port < <(python3 - <<'PY'
import socket

sockets = []
ports = []
try:
    for _ in range(2):
        sock = socket.socket()
        sock.bind(("127.0.0.1", 0))
        sockets.append(sock)
        ports.append(sock.getsockname()[1])
    print(*ports)
finally:
    for sock in sockets:
        sock.close()
PY
)

jq -n \
  --arg endpoint "https://127.0.0.1:${asr_port}/v1" \
  --arg key "${api_key}" \
  --arg model "${model}" \
  --arg language "${language}" \
  --arg prompt "${prompt}" '
  {
    version: 1,
    asr: {
      active_provider: "rotation-remote",
      normalize_audio: false,
      input_gain: 1.0,
      providers: [{
        id: "rotation-remote",
        type: "remote",
        endpoint: $endpoint,
        model: $model,
        timeout_ms: 5000,
        env: {
          VINPST_ASR_API_KEY: $key,
          VINPST_ASR_LANGUAGE: $language,
          VINPST_ASR_PROMPT: $prompt
        }
      }]
    },
    scenes: {
      active_scene: "__raw__",
      definitions: [{id: "__raw__", label: "Raw", candidate_count: 0}]
    }
  }
' >"${asr_config}"

jq -n \
  --arg base_url "https://127.0.0.1:${text_port}/v1" \
  --arg key "${api_key}" \
  --arg model "${model}" '
  {
    version: 1,
    asr: {
      active_provider: "mock",
      normalize_audio: false,
      input_gain: 1.0,
      providers: [{id: "mock", type: "local", model: "fixture"}]
    },
    llm: {
      providers: [{
        id: "rotation-text",
        base_url: $base_url,
        api_key: $key,
        model: $model,
        extra_body: {}
      }],
      adapters: []
    },
    scenes: {
      active_scene: "__command__",
      definitions: [
        {id: "__raw__", label: "Raw", candidate_count: 0},
        {
          id: "__command__",
          label: "Command",
          prompt: "Apply the recognized command to the selected text.",
          provider_id: "rotation-text",
          model: $model,
          candidate_count: 1,
          timeout_ms: 5000,
          context_lines: 0
        }
      ]
    }
  }
' >"${text_config}"

export VINPST_ROTATION_OUT_DIR="${out_dir}"
export VINPST_ROTATION_ASR_FIXTURE="${asr_fixture}"
export VINPST_ROTATION_TEXT_FIXTURE="${text_fixture}"
export VINPST_ROTATION_DAEMON="${daemon}"
export VINPST_ROTATION_CLI="${cli}"
export VINPST_ROTATION_WAV="${wav_file}"
export VINPST_ROTATION_ASR_CONFIG="${asr_config}"
export VINPST_ROTATION_TEXT_CONFIG="${text_config}"
export VINPST_ROTATION_TRUSTED_CA="${trusted_ca}"
export VINPST_ROTATION_CA_A="${ca_a_cert}"
export VINPST_ROTATION_CA_B="${ca_b_cert}"
export VINPST_ROTATION_SERVER_A_CERT="${server_a_cert}"
export VINPST_ROTATION_SERVER_A_KEY="${server_a_key}"
export VINPST_ROTATION_SERVER_B_CERT="${server_b_cert}"
export VINPST_ROTATION_SERVER_B_KEY="${server_b_key}"
export VINPST_ROTATION_ASR_PORT="${asr_port}"
export VINPST_ROTATION_TEXT_PORT="${text_port}"
export VINPST_ROTATION_API_KEY="${api_key}"
export VINPST_ROTATION_MODEL="${model}"
export VINPST_ROTATION_LANGUAGE="${language}"
export VINPST_ROTATION_PROMPT="${prompt}"
export VINPST_ROTATION_SELECTED="${selected_text}"

install_ca() {
  local source="$1"
  cp "${source}" "${trusted_ca}.next"
  chmod 600 "${trusted_ca}.next"
  mv "${trusted_ca}.next" "${trusted_ca}"
}

install_ca "${ca_a_cert}"

mkdir -p "${out_dir}/xdg-asr"
XDG_DATA_HOME="${out_dir}/xdg-asr" timeout 90s dbus-run-session -- bash -euo pipefail <<'ASR_SESSION'
out_dir="${VINPST_ROTATION_OUT_DIR}"
origin_pid=""
daemon_pid=""

cleanup_session() {
  if [[ -n "${origin_pid}" ]] && kill -0 "${origin_pid}" 2>/dev/null; then
    kill -TERM "${origin_pid}" 2>/dev/null || true
    wait "${origin_pid}" 2>/dev/null || true
  fi
  if [[ -n "${daemon_pid}" ]] && kill -0 "${daemon_pid}" 2>/dev/null; then
    kill -TERM "${daemon_pid}" 2>/dev/null || true
    wait "${daemon_pid}" 2>/dev/null || true
  fi
}
trap cleanup_session EXIT INT TERM

wait_ready() {
  local pid="$1"
  local ready_file="$2"
  local log_file="$3"
  for _ in $(seq 1 100); do
    [[ -f "${ready_file}" ]] && return 0
    if ! kill -0 "${pid}" 2>/dev/null; then
      cat "${log_file}" >&2
      return 1
    fi
    sleep 0.05
  done
  echo "fixture did not publish readiness: ${ready_file}" >&2
  return 1
}

start_origin() {
  local name="$1"
  local cert="$2"
  local key="$3"
  local response="$4"
  local ready="${out_dir}/${name}.ready.json"
  local trace="${out_dir}/${name}.trace.json"
  local error="${out_dir}/${name}.fixture-error.txt"
  local log="${out_dir}/${name}.fixture.log"
  rm -f "${ready}" "${trace}" "${error}" "${log}"
  python3 "${VINPST_ROTATION_ASR_FIXTURE}" \
    --ready-file "${ready}" \
    --trace-file "${trace}" \
    --error-file "${error}" \
    --port "${VINPST_ROTATION_ASR_PORT}" \
    --api-key "${VINPST_ROTATION_API_KEY}" \
    --model "${VINPST_ROTATION_MODEL}" \
    --language "${VINPST_ROTATION_LANGUAGE}" \
    --prompt "${VINPST_ROTATION_PROMPT}" \
    --response-text "${response}" \
    --tls-cert "${cert}" \
    --tls-key "${key}" \
    >"${log}" 2>&1 &
  origin_pid=$!
  wait_ready "${origin_pid}" "${ready}" "${log}"
  jq -e --argjson port "${VINPST_ROTATION_ASR_PORT}" \
    '.tls == true and .port == $port' "${ready}" >/dev/null
}

wait_origin_success() {
  local pid="${origin_pid}"
  local log="$1"
  origin_pid=""
  if ! wait "${pid}"; then
    cat "${log}" >&2
    return 1
  fi
}

stop_origin() {
  if [[ -n "${origin_pid}" ]] && kill -0 "${origin_pid}" 2>/dev/null; then
    kill -TERM "${origin_pid}" 2>/dev/null || true
    wait "${origin_pid}" 2>/dev/null || true
  fi
  origin_pid=""
}

assert_idle_owner() {
  local expected_pid="$1"
  local output="$2"
  "${VINPST_ROTATION_CLI}" daemon status --json >"${output}"
  jq -e --argjson pid "${expected_pid}" '
    .status == "idle" and
    .runtime_status.active_session == false and
    .owner.ok == true and
    .owner.unix_process_id == $pid
  ' "${output}" >/dev/null
}

recognize_success() {
  local name="$1"
  local expected="$2"
  "${VINPST_ROTATION_CLI}" recording start --json \
    >"${out_dir}/${name}.start.json"
  "${VINPST_ROTATION_CLI}" recording stop --json \
    >"${out_dir}/${name}.stop.json"
  jq -e --arg expected "${expected}" '
    (.payload_json | fromjson | .commit_text) == $expected
  ' "${out_dir}/${name}.stop.json" >/dev/null
}

start_origin asr-ca-a "${VINPST_ROTATION_SERVER_A_CERT}" \
  "${VINPST_ROTATION_SERVER_A_KEY}" "rotation asr a"
env \
  -u ALL_PROXY -u all_proxy \
  -u HTTP_PROXY -u http_proxy \
  -u HTTPS_PROXY -u https_proxy \
  -u NO_PROXY -u no_proxy \
  SSL_CERT_FILE="${VINPST_ROTATION_TRUSTED_CA}" \
  "${VINPST_ROTATION_DAEMON}" \
    --dbus \
    --configured-backends \
    --config "${VINPST_ROTATION_ASR_CONFIG}" \
    --wav "${VINPST_ROTATION_WAV}" \
    >"${out_dir}/asr-daemon.log" 2>&1 &
daemon_pid=$!

ready=0
for _ in $(seq 1 100); do
  if busctl --user --no-pager --list | awk -v pid="${daemon_pid}" '
    $1 == "org.fcitx.Vinpst" && $2 == pid { found = 1 }
    END { exit !found }
  '; then
    "${VINPST_ROTATION_CLI}" daemon status --json \
      >"${out_dir}/asr-before.json"
    ready=1
    break
  fi
  if ! kill -0 "${daemon_pid}" 2>/dev/null; then
    cat "${out_dir}/asr-daemon.log" >&2
    exit 1
  fi
  sleep 0.05
done
if [[ "${ready}" != 1 ]]; then
  cat "${out_dir}/asr-daemon.log" >&2
  exit 1
fi
owner_pid="$(jq -r '.owner.unix_process_id' "${out_dir}/asr-before.json")"
test "${owner_pid}" = "${daemon_pid}"

recognize_success asr-ca-a "rotation asr a"
wait_origin_success "${out_dir}/asr-ca-a.fixture.log"
assert_idle_owner "${owner_pid}" "${out_dir}/asr-after-a.json"

cp "${VINPST_ROTATION_CA_B}" "${VINPST_ROTATION_TRUSTED_CA}.next"
chmod 600 "${VINPST_ROTATION_TRUSTED_CA}.next"
mv "${VINPST_ROTATION_TRUSTED_CA}.next" "${VINPST_ROTATION_TRUSTED_CA}"
start_origin asr-ca-mismatch "${VINPST_ROTATION_SERVER_A_CERT}" \
  "${VINPST_ROTATION_SERVER_A_KEY}" "unused"
"${VINPST_ROTATION_CLI}" recording start --json \
  >"${out_dir}/asr-mismatch.start.json"
set +e
"${VINPST_ROTATION_CLI}" recording stop --json \
  >"${out_dir}/asr-mismatch.stop.json" \
  2>"${out_dir}/asr-mismatch.stderr"
mismatch_status=$?
set -e
if ((mismatch_status == 0)); then
  echo "ASR request unexpectedly trusted the replaced CA mismatch" >&2
  exit 1
fi
grep -Fq 'remote ASR HTTP request failed' "${out_dir}/asr-mismatch.stderr"
test ! -e "${out_dir}/asr-ca-mismatch.trace.json"
stop_origin
assert_idle_owner "${owner_pid}" "${out_dir}/asr-after-mismatch.json"

start_origin asr-ca-b "${VINPST_ROTATION_SERVER_B_CERT}" \
  "${VINPST_ROTATION_SERVER_B_KEY}" "rotation asr b"
recognize_success asr-ca-b "rotation asr b"
wait_origin_success "${out_dir}/asr-ca-b.fixture.log"
assert_idle_owner "${owner_pid}" "${out_dir}/asr-after-b.json"
ASR_SESSION

install_ca "${ca_a_cert}"

mkdir -p "${out_dir}/xdg-text"
XDG_DATA_HOME="${out_dir}/xdg-text" timeout 90s dbus-run-session -- bash -euo pipefail <<'TEXT_SESSION'
out_dir="${VINPST_ROTATION_OUT_DIR}"
origin_pid=""
daemon_pid=""

cleanup_session() {
  if [[ -n "${origin_pid}" ]] && kill -0 "${origin_pid}" 2>/dev/null; then
    kill -TERM "${origin_pid}" 2>/dev/null || true
    wait "${origin_pid}" 2>/dev/null || true
  fi
  if [[ -n "${daemon_pid}" ]] && kill -0 "${daemon_pid}" 2>/dev/null; then
    kill -TERM "${daemon_pid}" 2>/dev/null || true
    wait "${daemon_pid}" 2>/dev/null || true
  fi
}
trap cleanup_session EXIT INT TERM

wait_ready() {
  local pid="$1"
  local ready_file="$2"
  local log_file="$3"
  for _ in $(seq 1 100); do
    [[ -f "${ready_file}" ]] && return 0
    if ! kill -0 "${pid}" 2>/dev/null; then
      cat "${log_file}" >&2
      return 1
    fi
    sleep 0.05
  done
  echo "fixture did not publish readiness: ${ready_file}" >&2
  return 1
}

start_origin() {
  local name="$1"
  local cert="$2"
  local key="$3"
  local prefix="$4"
  local ready="${out_dir}/${name}.ready.json"
  local trace="${out_dir}/${name}.trace.json"
  local error="${out_dir}/${name}.fixture-error.txt"
  local log="${out_dir}/${name}.fixture.log"
  rm -f "${ready}" "${trace}" "${error}" "${log}"
  python3 "${VINPST_ROTATION_TEXT_FIXTURE}" \
    --ready-file "${ready}" \
    --trace-file "${trace}" \
    --error-file "${error}" \
    --port "${VINPST_ROTATION_TEXT_PORT}" \
    --api-key "${VINPST_ROTATION_API_KEY}" \
    --model "${VINPST_ROTATION_MODEL}" \
    --response-prefix "${prefix}" \
    --tls-cert "${cert}" \
    --tls-key "${key}" \
    >"${log}" 2>&1 &
  origin_pid=$!
  wait_ready "${origin_pid}" "${ready}" "${log}"
  jq -e --argjson port "${VINPST_ROTATION_TEXT_PORT}" \
    '.tls == true and .port == $port' "${ready}" >/dev/null
}

wait_origin_success() {
  local pid="${origin_pid}"
  local log="$1"
  origin_pid=""
  if ! wait "${pid}"; then
    cat "${log}" >&2
    return 1
  fi
}

stop_origin() {
  if [[ -n "${origin_pid}" ]] && kill -0 "${origin_pid}" 2>/dev/null; then
    kill -TERM "${origin_pid}" 2>/dev/null || true
    wait "${origin_pid}" 2>/dev/null || true
  fi
  origin_pid=""
}

assert_idle_owner() {
  local expected_pid="$1"
  local output="$2"
  "${VINPST_ROTATION_CLI}" daemon status --json >"${output}"
  jq -e --argjson pid "${expected_pid}" '
    .status == "idle" and
    .runtime_status.active_session == false and
    .owner.ok == true and
    .owner.unix_process_id == $pid
  ' "${output}" >/dev/null
}

command_success() {
  local name="$1"
  local prefix="$2"
  local expected="${prefix}${VINPST_ROTATION_SELECTED} | command: mock recognition result"
  "${VINPST_ROTATION_CLI}" recording start \
    --selected-text "${VINPST_ROTATION_SELECTED}" \
    --json >"${out_dir}/${name}.start.json"
  "${VINPST_ROTATION_CLI}" recording stop \
    --scene __command__ \
    --json >"${out_dir}/${name}.stop.json"
  jq -e --arg expected "${expected}" '
    (.payload_json | fromjson | .commit_text) == $expected
  ' "${out_dir}/${name}.stop.json" >/dev/null
}

start_origin text-ca-a "${VINPST_ROTATION_SERVER_A_CERT}" \
  "${VINPST_ROTATION_SERVER_A_KEY}" "rotation text a: "
env \
  -u ALL_PROXY -u all_proxy \
  -u HTTP_PROXY -u http_proxy \
  -u HTTPS_PROXY -u https_proxy \
  -u NO_PROXY -u no_proxy \
  SSL_CERT_FILE="${VINPST_ROTATION_TRUSTED_CA}" \
  "${VINPST_ROTATION_DAEMON}" \
    --dbus \
    --configured-backends \
    --config "${VINPST_ROTATION_TEXT_CONFIG}" \
    --wav "${VINPST_ROTATION_WAV}" \
    >"${out_dir}/text-daemon.log" 2>&1 &
daemon_pid=$!

ready=0
for _ in $(seq 1 100); do
  if busctl --user --no-pager --list | awk -v pid="${daemon_pid}" '
    $1 == "org.fcitx.Vinpst" && $2 == pid { found = 1 }
    END { exit !found }
  '; then
    "${VINPST_ROTATION_CLI}" daemon status --json \
      >"${out_dir}/text-before.json"
    ready=1
    break
  fi
  if ! kill -0 "${daemon_pid}" 2>/dev/null; then
    cat "${out_dir}/text-daemon.log" >&2
    exit 1
  fi
  sleep 0.05
done
if [[ "${ready}" != 1 ]]; then
  cat "${out_dir}/text-daemon.log" >&2
  exit 1
fi
owner_pid="$(jq -r '.owner.unix_process_id' "${out_dir}/text-before.json")"
test "${owner_pid}" = "${daemon_pid}"

command_success text-ca-a "rotation text a: "
wait_origin_success "${out_dir}/text-ca-a.fixture.log"
assert_idle_owner "${owner_pid}" "${out_dir}/text-after-a.json"

cp "${VINPST_ROTATION_CA_B}" "${VINPST_ROTATION_TRUSTED_CA}.next"
chmod 600 "${VINPST_ROTATION_TRUSTED_CA}.next"
mv "${VINPST_ROTATION_TRUSTED_CA}.next" "${VINPST_ROTATION_TRUSTED_CA}"
start_origin text-ca-mismatch "${VINPST_ROTATION_SERVER_A_CERT}" \
  "${VINPST_ROTATION_SERVER_A_KEY}" "unused: "
"${VINPST_ROTATION_CLI}" recording start \
  --selected-text "${VINPST_ROTATION_SELECTED}" \
  --json >"${out_dir}/text-mismatch.start.json"
"${VINPST_ROTATION_CLI}" recording stop \
  --scene __command__ \
  --json >"${out_dir}/text-mismatch.stop.json"
jq -e \
  --arg selected "${VINPST_ROTATION_SELECTED}" \
  '.ok and ((.payload_json | fromjson) == {
    commit_text: $selected,
    candidates: [
      {text: $selected, source: "raw"},
      {text: "mock recognition result", source: "asr"}
    ]
  })' "${out_dir}/text-mismatch.stop.json" >/dev/null
test ! -e "${out_dir}/text-ca-mismatch.trace.json"
stop_origin
assert_idle_owner "${owner_pid}" "${out_dir}/text-after-mismatch.json"

start_origin text-ca-b "${VINPST_ROTATION_SERVER_B_CERT}" \
  "${VINPST_ROTATION_SERVER_B_KEY}" "rotation text b: "
command_success text-ca-b "rotation text b: "
wait_origin_success "${out_dir}/text-ca-b.fixture.log"
assert_idle_owner "${owner_pid}" "${out_dir}/text-after-b.json"
TEXT_SESSION

rm -f "${asr_config}" "${text_config}" "${trusted_ca}"
rm -f "${rotation_files[@]}"
rotation_files=()

for secret in "${api_key}" "${prompt}"; do
  if grep -R -F -- "${secret}" "${out_dir}" >/dev/null; then
    echo "CA rotation evidence retained provider credentials" >&2
    exit 1
  fi
done
if find "${out_dir}" -maxdepth 1 -type f \( \
  -name '*.pem' -o -name '*.csr' -o -name '*.srl' -o -name '*config.json' \
\) -print -quit | grep -q .; then
  echo "CA rotation evidence retained trust or config material" >&2
  exit 1
fi

jq -n \
  --argjson asr_port "${asr_port}" \
  --argjson text_port "${text_port}" '
  {
    event: "summary",
    same_asr_daemon: true,
    same_text_daemon: true,
    fixed_asr_port: $asr_port,
    fixed_text_port: $text_port,
    same_ca_path: true,
    atomic_ca_replacement: true,
    mismatch_rejected: true,
    idle_recovery: true,
    rotated_ca_success: true,
    credential_custody_proof: false,
    hosted_service_proof: false,
    ok: true
  }
' | tee "${out_dir}/summary.json"

echo "provider CA rotation smoke passed"
