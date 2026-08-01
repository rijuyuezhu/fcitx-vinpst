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

cli_binary="${VINPUT_LIVE_CLI_BINARY:-target/debug/vinput}"
fixture="${repo_root}/scripts/fixtures/openai-compatible-asr-fixture.py"
selection_probe="${repo_root}/scripts/live/niri/probes/fcitx-live-asr-selection-probe.py"
virtual_runner="${repo_root}/scripts/live/niri/run-ime-fcitx-virtual-source-live.sh"
out_dir="${VINPUT_LIVE_REMOTE_ASR_OUT_DIR:-target/tmp/ime-fcitx-remote-asr-live}"
out_dir_abs="$(realpath -m "${out_dir}")"
service_path="${VINPUT_LIVE_DBUS_SERVICE:-${HOME}/.local/share/dbus-1/services/org.fcitx.Vinput.service}"
addon_config="${HOME}/.config/fcitx5/conf/vinput.conf"
recognition_wav="${VINPUT_LIVE_REMOTE_ASR_WAV:-${repo_root}/target/models/onnx-zf-ctc-zh-sm-int8-stream/test_wavs/0.wav}"
remote_provider="${VINPUT_LIVE_REMOTE_ASR_PROVIDER_ID:-remote-http}"
remote_model="${VINPUT_LIVE_REMOTE_ASR_MODEL:-fixture-remote-asr}"
remote_language="${VINPUT_LIVE_REMOTE_ASR_LANGUAGE:-zh}"
remote_prompt="${VINPUT_LIVE_REMOTE_ASR_PROMPT:-fixture remote names}"
remote_api_key="${VINPUT_LIVE_REMOTE_ASR_API_KEY:-live-remote-secret}"
remote_response="${VINPUT_LIVE_REMOTE_ASR_RESPONSE:-remote-http-final}"
remote_timeout_ms="${VINPUT_LIVE_REMOTE_ASR_TIMEOUT_MS:-10000}"
trigger_key="${VINPUT_LIVE_ASR_MENU_KEY:-F8}"
server_ready="${out_dir_abs}/server-ready.json"
server_trace="${out_dir_abs}/server-trace.json"
server_error="${out_dir_abs}/server-error.txt"
server_log="${out_dir_abs}/server.log"
config_path=""
original_provider=""
original_model=""
original_daemon_pid=""
profile_mutated=0
backup_existed=0
server_pid=""
remote_server_pid=""
fcitx_restart_needed=0

call_service() {
  gdbus call --session \
    --dest org.fcitx.Vinput \
    --object-path /org/fcitx/Vinput \
    --method "org.fcitx.Vinput.Service.$1" "${@:2}"
}

stop_server() {
  if [[ -n "${server_pid}" ]] && kill -0 "${server_pid}" 2>/dev/null; then
    kill -TERM "${server_pid}" 2>/dev/null || true
    wait "${server_pid}" 2>/dev/null || true
  fi
  server_pid=""
}

restart_fcitx() {
  local previous_pid pid
  previous_pid="$(pgrep -n -x fcitx5 || true)"
  fcitx5 -rd >/dev/null 2>&1
  for _ in $(seq 1 120); do
    pid="$(pgrep -n -x fcitx5 || true)"
    if [[ -n "${pid}" ]] && [[ "${pid}" != "${previous_pid}" ]] &&
      { [[ -z "${previous_pid}" ]] || [[ ! -e "/proc/${previous_pid}" ]]; } &&
      fcitx5-remote --check >/dev/null 2>&1 &&
      grep -q "${HOME}/.local/lib/fcitx5/fcitx5-vinput.so" "/proc/${pid}/maps"; then
      printf '%s\n' "${pid}"
      return 0
    fi
    sleep 0.1
  done
  echo "Fcitx did not restart with the user-installed addon" >&2
  return 1
}

wait_backend() {
  local output_path="$1"
  local provider="$2"
  local model="$3"
  local expected_endpoint="${4:-}"
  for _ in $(seq 1 400); do
    if "${cli_binary}" daemon status --json >"${output_path}" 2>/dev/null &&
      jq -e \
        --arg provider "${provider}" \
        --arg model "${model}" \
        --arg endpoint "${expected_endpoint}" '
          .status == "idle" and
          .owner.ok == true and
          .asr_backend.has_effective_backend == true and
          .asr_backend.reload_in_progress == false and
          .asr_backend.last_error == "" and
          .asr_backend.target_provider_id == $provider and
          .asr_backend.target_model_id == $model and
          .asr_backend.effective_provider_id == $provider and
          .asr_backend.effective_model_id == $model and
          ($endpoint == "" or (.asr_backend.remote_endpoints | index($endpoint)) != null)
        ' "${output_path}" >/dev/null; then
      return 0
    fi
    sleep 0.1
  done
  echo "ASR backend did not reach the expected provider/model" >&2
  cat "${output_path}" >&2 2>/dev/null || true
  return 1
}

restore_profile() {
  [[ "${profile_mutated}" == 0 ]] && return 0
  install -m 0644 "${out_dir_abs}/config-before.json" "${config_path}"
  if [[ "${backup_existed}" == 1 ]]; then
    install -m 0644 "${out_dir_abs}/config-backup-before.json" "${config_path}.bak"
  else
    rm -f "${config_path}.bak"
  fi
  "${cli_binary}" daemon reload-asr --json \
    >"${out_dir_abs}/restore-reload-call.json"
  wait_backend \
    "${out_dir_abs}/restored-status.json" \
    "${original_provider}" \
    "${original_model}"
  cmp "${out_dir_abs}/config-before.json" "${config_path}"
  if [[ "${backup_existed}" == 1 ]]; then
    cmp "${out_dir_abs}/config-backup-before.json" "${config_path}.bak"
  else
    test ! -e "${config_path}.bak"
  fi
  profile_mutated=0
}

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM
  set +e
  stop_server
  if ! restore_profile; then
    exit_code=1
  fi
  if [[ "${fcitx_restart_needed}" == 1 ]]; then
    if ! restart_fcitx >"${out_dir_abs}/fcitx-cleanup.pid"; then
      exit_code=1
    fi
    fcitx_restart_needed=0
  fi
  cmp "${out_dir_abs}/service-before.service" "${service_path}" || exit_code=1
  cmp "${out_dir_abs}/addon-before.conf" "${addon_config}" || exit_code=1
  find scripts -type d -name __pycache__ -prune -exec rm -rf {} +
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

for command in cmp fcitx5 fcitx5-remote gdbus install jq kill pgrep python3 readlink ruff; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required remote ASR live command is missing: ${command}" >&2
    exit 2
  fi
done
for path in \
  "${cli_binary}" \
  "${fixture}" \
  "${selection_probe}" \
  "${virtual_runner}" \
  "${service_path}" \
  "${addon_config}" \
  "${recognition_wav}"; do
  if [[ ! -e "${path}" ]]; then
    echo "remote ASR live input is missing: ${path}" >&2
    exit 2
  fi
done
if [[ ! "${remote_timeout_ms}" =~ ^[1-9][0-9]*$ ]]; then
  echo "VINPUT_LIVE_REMOTE_ASR_TIMEOUT_MS must be a positive integer" >&2
  exit 2
fi
if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  echo "DBUS_SESSION_BUS_ADDRESS is not set" >&2
  exit 2
fi
if ! fcitx5-remote --check >/dev/null 2>&1; then
  echo "Fcitx5 is not running in this session" >&2
  exit 2
fi

rm -rf "${out_dir_abs}"
mkdir -p "${out_dir_abs}"
ruff check "${fixture}" "${selection_probe}"
ruff format --check "${fixture}" "${selection_probe}"

"${cli_binary}" daemon status --json >"${out_dir_abs}/before.json"
if ! jq -e '
  .status == "idle" and
  .owner.ok == true and
  .asr_backend.has_effective_backend == true and
  .asr_backend.reload_in_progress == false and
  .asr_backend.last_error == ""
' "${out_dir_abs}/before.json" >/dev/null; then
  cat "${out_dir_abs}/before.json" >&2
  echo "daemon must be idle and healthy before remote ASR proof" >&2
  exit 2
fi
original_daemon_pid="$(jq -r '.owner.unix_process_id' "${out_dir_abs}/before.json")"
original_provider="$(jq -r '.asr_backend.effective_provider_id' "${out_dir_abs}/before.json")"
original_model="$(jq -r '.asr_backend.effective_model_id' "${out_dir_abs}/before.json")"
config_path="$(jq -r '
  .owner.process.cmdline as $args |
  ($args | index("--config")) as $index |
  if $index == null then empty else $args[$index + 1] end
' "${out_dir_abs}/before.json")"
if [[ -z "${config_path}" || ! -f "${config_path}" ]]; then
  echo "could not resolve the active daemon config path" >&2
  exit 2
fi
install -m 0644 "${config_path}" "${out_dir_abs}/config-before.json"
if [[ -f "${config_path}.bak" ]]; then
  backup_existed=1
  install -m 0644 "${config_path}.bak" "${out_dir_abs}/config-backup-before.json"
fi
install -m 0644 "${service_path}" "${out_dir_abs}/service-before.service"
install -m 0644 "${addon_config}" "${out_dir_abs}/addon-before.conf"

python3 "${fixture}" \
  --ready-file "${server_ready}" \
  --trace-file "${server_trace}" \
  --error-file "${server_error}" \
  --api-key "${remote_api_key}" \
  --model "${remote_model}" \
  --language "${remote_language}" \
  --prompt "${remote_prompt}" \
  --response-text "${remote_response}" \
  >"${server_log}" 2>&1 &
server_pid=$!
for _ in $(seq 1 100); do
  [[ -f "${server_ready}" ]] && break
  if ! kill -0 "${server_pid}" 2>/dev/null; then
    cat "${server_log}" >&2
    echo "remote ASR fixture exited before readiness" >&2
    exit 1
  fi
  sleep 0.05
done
if [[ ! -f "${server_ready}" ]]; then
  echo "remote ASR fixture did not publish readiness" >&2
  exit 1
fi
server_exe="$(readlink "/proc/${server_pid}/exe")"
server_cmdline="$(tr '\0' ' ' <"/proc/${server_pid}/cmdline")"
if [[ "${server_exe}" != *python* || "${server_cmdline}" != *"${fixture}"* ]]; then
  echo "remote ASR fixture process identity mismatch: pid=${server_pid}" >&2
  exit 1
fi
remote_server_pid="${server_pid}"
base_url="$(jq -r '.base_url' "${server_ready}")"

python3 - \
  "${out_dir_abs}/config-before.json" \
  "${out_dir_abs}/config-remote.json" \
  "${remote_provider}" \
  "${remote_model}" \
  "${base_url}" \
  "${remote_api_key}" \
  "${remote_language}" \
  "${remote_prompt}" \
  "${remote_timeout_ms}" <<'PY'
import json
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
provider_id, model, endpoint, api_key, language, prompt, timeout_ms = sys.argv[3:]
config = json.loads(source.read_text(encoding="utf-8"))
if any(provider.get("id") == provider_id for provider in config["asr"]["providers"]):
    raise SystemExit(f"temporary provider id already exists: {provider_id}")
config["asr"]["providers"].append(
    {
        "id": provider_id,
        "type": "remote",
        "endpoint": endpoint,
        "model": model,
        "timeout_ms": int(timeout_ms),
        "env": {
            "VINPUT_ASR_API_KEY": api_key,
            "VINPUT_ASR_LANGUAGE": language,
            "VINPUT_ASR_PROMPT": prompt,
        },
    }
)
target.write_text(
    json.dumps(config, ensure_ascii=False, indent=2) + "\n",
    encoding="utf-8",
)
PY
"${cli_binary}" config validate "${out_dir_abs}/config-remote.json" --json \
  | tee "${out_dir_abs}/config-validate.json"

install -m 0644 "${out_dir_abs}/config-remote.json" "${config_path}"
profile_mutated=1
"${cli_binary}" daemon reload-asr --json \
  | tee "${out_dir_abs}/provider-list-reload-call.json"
wait_backend \
  "${out_dir_abs}/provider-list-ready.json" \
  "${original_provider}" \
  "${original_model}"

fcitx_pid_before="$(restart_fcitx)"
printf '%s\n' "${fcitx_pid_before}" | tee "${out_dir_abs}/fcitx-before-selection.pid"
fcitx_restart_needed=1

python3 "${selection_probe}" \
  --trigger-key "${trigger_key}" \
  --expected-provider "${remote_provider}" \
  --expected-model "${remote_model}" \
  | tee "${out_dir_abs}/selection.jsonl"
jq -s -e 'any(.[]; .event == "summary" and .ok == true and .selected == true and .filter_complete == true)' \
  "${out_dir_abs}/selection.jsonl" >/dev/null
wait_backend \
  "${out_dir_abs}/remote-status.json" \
  "${remote_provider}" \
  "${remote_model}" \
  "${base_url}"

VINPUT_LIVE_NATIVE_WAV="${recognition_wav}" \
VINPUT_LIVE_NATIVE_MODES=normal \
VINPUT_LIVE_REQUIRE_PARTIAL=0 \
VINPUT_LIVE_VIRTUAL_OUT_DIR="${out_dir_abs}/remote-recognition" \
  "${virtual_runner}"

wait "${server_pid}"
server_pid=""
test ! -e "${server_error}"
if [[ ! -f "${server_trace}" ]]; then
  cat "${server_log}" >&2
  echo "remote ASR fixture did not record a transcription request" >&2
  exit 1
fi

remote_jsonl="${out_dir_abs}/remote-recognition/fcitx/normal.jsonl"
remote_commit="$(jq -r 'select(.event == "summary") | .commit' "${remote_jsonl}")"
remote_partial_count="$(jq -s '[.[] | select(.event == "summary")][0].partial_count' "${remote_jsonl}")"
remote_require_partial="$(jq -s '[.[] | select(.event == "summary")][0].require_partial' "${remote_jsonl}")"
if [[ "${remote_commit}" != "${remote_response}" || "${remote_require_partial}" != "false" ]]; then
  echo "remote ASR final-only recognition did not produce the fixture response" >&2
  exit 1
fi
jq -e \
  --arg model "${remote_model}" \
  --arg language "${remote_language}" \
  --arg response "${remote_response}" '
    .event == "request" and
    .request_count == 1 and
    .method == "POST" and
    .path == "/v1/audio/transcriptions" and
    .authorization_scheme == "Bearer" and
    .authorization_value_recorded == false and
    .content_type == "multipart/form-data" and
    .file_field == "file" and
    .file_content_type == "audio/wav" and
    .model == $model and
    .language == $language and
    .prompt_matched == true and
    .prompt_value_recorded == false and
    .wav.sample_rate == 16000 and
    .wav.channels == 1 and
    .wav.sample_width_bits == 16 and
    .wav.frames > 0 and
    .wav.peak > 0 and
    (.wav.sha256 | length) == 64 and
    .response_text == $response
  ' "${server_trace}" >/dev/null
if grep -Fq "${remote_api_key}" "${server_trace}" "${server_ready}"; then
  echo "remote ASR live evidence leaked its API key" >&2
  exit 1
fi
if grep -Fq "${remote_prompt}" "${server_trace}" "${server_ready}"; then
  echo "remote ASR live evidence leaked its prompt" >&2
  exit 1
fi

restore_profile

VINPUT_LIVE_NATIVE_WAV="${recognition_wav}" \
VINPUT_LIVE_NATIVE_MODES=normal \
VINPUT_LIVE_REQUIRE_PARTIAL=1 \
VINPUT_LIVE_VIRTUAL_OUT_DIR="${out_dir_abs}/recovered-recognition" \
  "${virtual_runner}"
recovery_jsonl="${out_dir_abs}/recovered-recognition/fcitx/normal.jsonl"
recovery_partial_count="$(jq -s '[.[] | select(.event == "summary")][0].partial_count' "${recovery_jsonl}")"
recovery_commit="$(jq -r 'select(.event == "summary") | .commit' "${recovery_jsonl}")"
if [[ "${recovery_partial_count}" -le 0 || -z "${recovery_commit}" ]]; then
  echo "Zipformer did not recover with streaming recognition" >&2
  exit 1
fi

cmp "${out_dir_abs}/config-before.json" "${config_path}"
if [[ "${backup_existed}" == 1 ]]; then
  cmp "${out_dir_abs}/config-backup-before.json" "${config_path}.bak"
else
  test ! -e "${config_path}.bak"
fi
cmp "${out_dir_abs}/service-before.service" "${service_path}"
cmp "${out_dir_abs}/addon-before.conf" "${addon_config}"
restart_fcitx | tee "${out_dir_abs}/fcitx-restored.pid"
fcitx_restart_needed=0
wait_backend \
  "${out_dir_abs}/final-status.json" \
  "${original_provider}" \
  "${original_model}"

remote_wav_sha256="$(jq -r '.wav.sha256' "${server_trace}")"
remote_wav_frames="$(jq -r '.wav.frames' "${server_trace}")"
remote_wav_peak="$(jq -r '.wav.peak' "${server_trace}")"
remote_daemon_pid="$(jq -r '.owner.unix_process_id' "${out_dir_abs}/remote-status.json")"
restored_daemon_pid="$(jq -r '.owner.unix_process_id' "${out_dir_abs}/final-status.json")"

jq -n \
  --arg provider "${remote_provider}" \
  --arg model "${remote_model}" \
  --arg endpoint "${base_url}" \
  --arg language "${remote_language}" \
  --arg remote_commit "${remote_commit}" \
  --arg recovery_provider "${original_provider}" \
  --arg recovery_model "${original_model}" \
  --arg recovery_commit "${recovery_commit}" \
  --arg wav_sha256 "${remote_wav_sha256}" \
  --argjson server_pid "${remote_server_pid}" \
  --argjson original_daemon_pid "${original_daemon_pid}" \
  --argjson remote_daemon_pid "${remote_daemon_pid}" \
  --argjson restored_daemon_pid "${restored_daemon_pid}" \
  --argjson remote_partial_count "${remote_partial_count}" \
  --argjson wav_frames "${remote_wav_frames}" \
  --argjson wav_peak "${remote_wav_peak}" \
  --argjson recovery_partial_count "${recovery_partial_count}" '
  {
    event: "summary",
    remote_http_provider: true,
    local_fixture_not_hosted_service: true,
    target: {
      provider: $provider,
      model: $model,
      kind: "remote",
      endpoint: $endpoint,
      language: $language,
      request_path: "/v1/audio/transcriptions",
      authorization_scheme: "Bearer",
      authorization_value_recorded: false,
      prompt_value_recorded: false
    },
    request: {
      multipart: true,
      wav_sha256: $wav_sha256,
      wav_frames: $wav_frames,
      wav_peak: $wav_peak
    },
    recognition: {
      final_only: true,
      partial_count: $remote_partial_count,
      commit: $remote_commit
    },
    recovery: {
      provider: $recovery_provider,
      model: $recovery_model,
      partial_count: $recovery_partial_count,
      commit: $recovery_commit,
      streaming: ($recovery_partial_count > 0 and ($recovery_commit | length) > 0)
    },
    fixture_server_pid: $server_pid,
    original_daemon_pid: $original_daemon_pid,
    remote_daemon_pid: $remote_daemon_pid,
    restored_daemon_pid: $restored_daemon_pid,
    profile_restored: true,
    backup_restored: true,
    service_unchanged: true,
    addon_config_unchanged: true,
    fcitx_restored: true,
    backend_restored: true,
    ok: (
      ($remote_commit | length) > 0 and
      ($wav_frames > 0) and
      ($wav_peak > 0) and
      ($recovery_partial_count > 0) and
      (($recovery_commit | length) > 0)
    )
  }' | tee "${out_dir_abs}/summary.json"
