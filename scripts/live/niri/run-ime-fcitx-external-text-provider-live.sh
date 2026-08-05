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

cli_binary="${VINPST_LIVE_CLI_BINARY:-target/debug/vinpst}"
out_dir="${VINPST_LIVE_EXTERNAL_TEXT_OUT_DIR:-target/tmp/ime-fcitx-external-text-provider-live}"
if [[ "${out_dir}" == /* ]]; then
  out_dir_abs="${out_dir}"
else
  out_dir_abs="${repo_root}/${out_dir}"
fi
fixture="${repo_root}/scripts/fixtures/openai-compatible-text-provider-fixture.py"
service_path="${VINPST_LIVE_DBUS_SERVICE:-${HOME}/.local/share/dbus-1/services/org.fcitx.Vinpst.service}"
addon_config="${HOME}/.config/fcitx5/conf/vinpst.conf"
recognition_wav="${VINPST_LIVE_EXTERNAL_TEXT_WAV:-${repo_root}/target/models/onnx-zf-ctc-zh-sm-int8-stream/test_wavs/0.wav}"
selected_text="${VINPST_LIVE_EXTERNAL_SELECTED_TEXT:-This is live selected text.}"
provider_id="${VINPST_LIVE_EXTERNAL_TEXT_PROVIDER_ID:-external-http}"
provider_model="${VINPST_LIVE_EXTERNAL_TEXT_MODEL:-fixture-text-model}"
api_key="${VINPST_LIVE_EXTERNAL_TEXT_API_KEY:-live-fixture-token}"
response_prefix="${VINPST_LIVE_EXTERNAL_TEXT_RESPONSE_PREFIX:-external-http: }"
expected_prefix="${response_prefix}${selected_text} | command: "
config_path=""
original_provider=""
original_model=""
backup_existed=0
profile_mutated=0
server_pid=""
external_server_pid=""
failure_server_pid=""
server_ready="${out_dir_abs}/server-ready.json"
server_trace="${out_dir_abs}/server-trace.json"
server_error="${out_dir_abs}/server-error.txt"
server_log="${out_dir_abs}/server.log"

call_service() {
  gdbus call --session \
    --dest org.fcitx.Vinpst \
    --object-path /org/fcitx/Vinpst \
    --method "org.fcitx.Vinpst.Service.$1" "${@:2}"
}

stop_verified_owner() {
  local status pid exe cmdline proc_exe proc_cmdline
  status="$("${cli_binary}" daemon status --json 2>/dev/null || true)"
  pid="$(jq -r '.owner.unix_process_id // empty' <<<"${status}")"
  [[ -z "${pid}" ]] && return 0
  exe="$(jq -r '.owner.process.exe // empty' <<<"${status}")"
  cmdline="$(jq -r '.owner.process.cmdline | join(" ")' <<<"${status}")"
  if [[ "${exe}" != *vinpst-daemon* || "${cmdline}" != *"${config_path}"* ]]; then
    echo "refusing to stop unexpected org.fcitx.Vinpst owner: pid=${pid} exe=${exe}" >&2
    return 1
  fi
  proc_exe="$(readlink "/proc/${pid}/exe")"
  proc_cmdline="$(tr '\0' ' ' <"/proc/${pid}/cmdline")"
  if [[ "${proc_exe}" != *vinpst-daemon* || "${proc_cmdline}" != *"${config_path}"* ]]; then
    echo "owner process changed before stop: pid=${pid}" >&2
    return 1
  fi
  kill "${pid}"
  for _ in $(seq 1 100); do
    [[ ! -e "/proc/${pid}" ]] && return 0
    sleep 0.05
  done
  echo "verified daemon owner did not stop: ${pid}" >&2
  return 1
}

activate_and_wait() {
  local output_path="$1"
  for _ in $(seq 1 300); do
    call_service GetStatus >/dev/null 2>&1 || true
    if "${cli_binary}" daemon status --json >"${output_path}" 2>/dev/null &&
      jq -e \
        --arg config_path "${config_path}" \
        --arg provider "${original_provider}" \
        --arg model "${original_model}" '
          .status == "idle" and
          .owner.ok == true and
          (.owner.process.exe | endswith("vinpst-daemon")) and
          (.owner.process.cmdline | index($config_path)) != null and
          .asr_backend.reload_in_progress == false and
          .asr_backend.last_error == "" and
          .asr_backend.effective_provider_id == $provider and
          .asr_backend.effective_model_id == $model
        ' "${output_path}" >/dev/null; then
      return 0
    fi
    sleep 0.1
  done
  echo "D-Bus activation did not restore an idle configured daemon" >&2
  cat "${output_path}" >&2 2>/dev/null || true
  return 1
}

stop_server() {
  if [[ -n "${server_pid}" ]] && kill -0 "${server_pid}" 2>/dev/null; then
    kill "${server_pid}" 2>/dev/null || true
    wait "${server_pid}" 2>/dev/null || true
  fi
  server_pid=""
}

start_server() {
  local mode="${1:-success}"
  local -a fixture_args=(
    --ready-file "${server_ready}"
    --trace-file "${server_trace}"
    --error-file "${server_error}"
    --api-key "${api_key}"
    --model "${provider_model}"
    --response-prefix "${response_prefix}"
  )
  if [[ "${mode}" == error ]]; then
    fixture_args+=(--expect-error)
  elif [[ "${mode}" != success ]]; then
    echo "unknown external text fixture mode: ${mode}" >&2
    return 2
  fi
  rm -f "${server_ready}" "${server_trace}" "${server_error}" "${server_log}"
  python3 "${fixture}" "${fixture_args[@]}" \
    >"${server_log}" 2>&1 &
  server_pid=$!
  for _ in $(seq 1 100); do
    [[ -f "${server_ready}" ]] && break
    if ! kill -0 "${server_pid}" 2>/dev/null; then
      cat "${server_log}" >&2
      echo "external text provider exited before readiness" >&2
      return 1
    fi
    sleep 0.05
  done
  if [[ ! -f "${server_ready}" ]]; then
    echo "external text provider did not publish readiness" >&2
    return 1
  fi
  local server_exe server_cmdline
  server_exe="$(readlink "/proc/${server_pid}/exe")"
  server_cmdline="$(tr '\0' ' ' <"/proc/${server_pid}/cmdline")"
  if [[ "${server_exe}" != *python* || "${server_cmdline}" != *"${fixture}"* ]]; then
    echo "external text provider process identity mismatch: pid=${server_pid}" >&2
    return 1
  fi
  base_url="$(jq -r '.base_url' "${server_ready}")"
}

restore_profile() {
  [[ "${profile_mutated}" == 0 ]] && return 0
  stop_verified_owner
  install -m 0644 "${out_dir_abs}/config-before.json" "${config_path}"
  if [[ "${backup_existed}" == 1 ]]; then
    install -m 0644 "${out_dir_abs}/config-backup-before.json" "${config_path}.bak"
  else
    rm -f "${config_path}.bak"
  fi
  activate_and_wait "${out_dir_abs}/restored-status.json"
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
  cmp "${out_dir_abs}/service-before.service" "${service_path}" || exit_code=1
  cmp "${out_dir_abs}/addon-config-before.conf" "${addon_config}" || exit_code=1
  find scripts -type d -name __pycache__ -prune -exec rm -rf {} +
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

for command in cmp fcitx5-remote gdbus install jq kill pgrep python3 readlink ruff; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required external-text command is missing: ${command}" >&2
    exit 2
  fi
done
for path in \
  "${cli_binary}" \
  "${fixture}" \
  "${service_path}" \
  "${addon_config}" \
  "${recognition_wav}"; do
  if [[ ! -e "${path}" ]]; then
    echo "external-text input is missing: ${path}" >&2
    exit 2
  fi
done
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
"${cli_binary}" daemon status --json >"${out_dir_abs}/before.json"
if ! jq -e '
  .status == "idle" and
  .owner.ok == true and
  .asr_backend.has_effective_backend == true and
  .asr_backend.reload_in_progress == false and
  .asr_backend.last_error == ""
' "${out_dir_abs}/before.json" >/dev/null; then
  cat "${out_dir_abs}/before.json" >&2
  echo "daemon must be idle and healthy before external-text proof" >&2
  exit 2
fi
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
install -m 0644 "${addon_config}" "${out_dir_abs}/addon-config-before.conf"

ruff check "${fixture}"
ruff format --check "${fixture}"
python3 -m py_compile "${fixture}"

start_server error
failure_base_url="${base_url}/unavailable"

python3 - \
  "${out_dir_abs}/config-before.json" \
  "${out_dir_abs}/config-external-text-failure.json" \
  "${provider_id}" \
  "${failure_base_url}" \
  "${api_key}" \
  "${provider_model}" <<'PY'
import json
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
provider_id, base_url, api_key, model = sys.argv[3:]
config = json.loads(source.read_text(encoding="utf-8"))
config["llm"]["providers"] = [
    {
        "id": provider_id,
        "base_url": base_url,
        "api_key": api_key,
        "model": model,
        "extra_body": {"fixture_mode": "live"},
    }
]
config["llm"]["adapters"] = []
command_scenes = [
    scene
    for scene in config["scenes"]["definitions"]
    if scene.get("id") == "__command__"
]
if len(command_scenes) != 1:
    raise SystemExit(f"expected one __command__ scene, found {len(command_scenes)}")
command = command_scenes[0]
command["prompt"] = "Apply the recognized command to the selected text."
command["provider_id"] = provider_id
command["model"] = model
command["candidate_count"] = 1
command["timeout_ms"] = 10000
command["context_lines"] = 0
target.write_text(json.dumps(config, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY
"${cli_binary}" config validate "${out_dir_abs}/config-external-text-failure.json" --json \
  | tee "${out_dir_abs}/config-failure-validate.json"

install -m 0644 "${out_dir_abs}/config-external-text-failure.json" "${config_path}"
profile_mutated=1
stop_verified_owner
activate_and_wait "${out_dir_abs}/external-text-failure-status.json"
failure_daemon_pid="$(jq -r '.owner.unix_process_id' "${out_dir_abs}/external-text-failure-status.json")"
original_daemon_pid="$(jq -r '.owner.unix_process_id' "${out_dir_abs}/before.json")"
if [[ "${failure_daemon_pid}" == "${original_daemon_pid}" ]]; then
  echo "daemon PID did not change after enabling the failing external text provider" >&2
  exit 1
fi

runtime_status="$(call_service GetRuntimeStatus)"
python3 - "${runtime_status}" "${provider_id}" "${provider_model}" <<'PY'
import ast
import json
import sys

runtime = json.loads(ast.literal_eval(sys.argv[1])[0])
provider_id = sys.argv[2]
model = sys.argv[3]
if runtime.get("text_adapters", {}).get("adapter_ids"):
    raise SystemExit("external HTTP text runtime unexpectedly retained command adapters")
config = runtime.get("config", {})
if config.get("active_scene") != "raw":
    raise SystemExit(f"external text setup changed active scene: {config.get('active_scene')!r}")
print(json.dumps({"provider": provider_id, "model": model, "ok": True}))
PY

VINPST_LIVE_NATIVE_WAV="${recognition_wav}" \
VINPST_LIVE_NATIVE_MODES=command \
VINPST_LIVE_SELECTED_TEXT="${selected_text}" \
VINPST_LIVE_EXPECT_UNCHANGED_ON_ERROR=1 \
VINPST_LIVE_VIRTUAL_OUT_DIR="${out_dir_abs}/external-text-failure" \
  scripts/live/niri/run-ime-fcitx-virtual-source-live.sh

failure_server_pid="${server_pid}"
wait "${server_pid}"
server_pid=""
test -f "${server_error}"
grep -Fq 'unexpected request path:' "${server_error}"
install -m 0644 "${server_ready}" "${out_dir_abs}/failure-server-ready.json"
install -m 0644 "${server_error}" "${out_dir_abs}/failure-server-error.txt"
install -m 0644 "${server_log}" "${out_dir_abs}/failure-server.log"

failure_command_summary="${out_dir_abs}/external-text-failure/fcitx/command.jsonl"
jq -s -e \
  --arg selected "${selected_text}" '
    any(.[];
      .event == "summary" and
      .ok == true and
      .expect_unchanged_on_error == true and
      .selection_source == "surrounding" and
      .selected_text == $selected and
      .commit == "" and
      .delete_count == 0 and
      .final_buffer == $selected
    )
  ' "${failure_command_summary}" >/dev/null
"${cli_binary}" daemon status --json >"${out_dir_abs}/after-failure-status.json"
jq -e \
  --arg config_path "${config_path}" '
    .status == "idle" and
    .owner.ok == true and
    (.owner.process.cmdline | index($config_path)) != null and
    .runtime_status.active_session == false and
    (.runtime_status.text_adapters.adapter_ids | length) == 0
  ' "${out_dir_abs}/after-failure-status.json" >/dev/null
post_failure_daemon_pid="$(jq -r '.owner.unix_process_id' "${out_dir_abs}/after-failure-status.json")"

start_server success
success_base_url="${base_url}"
python3 - \
  "${out_dir_abs}/config-external-text-failure.json" \
  "${out_dir_abs}/config-external-text.json" \
  "${success_base_url}" <<'PY'
import json
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
base_url = sys.argv[3]
config = json.loads(source.read_text(encoding="utf-8"))
providers = config["llm"]["providers"]
if len(providers) != 1:
    raise SystemExit(f"expected one HTTP text provider, found {len(providers)}")
providers[0]["base_url"] = base_url
target.write_text(json.dumps(config, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY
"${cli_binary}" config validate "${out_dir_abs}/config-external-text.json" --json \
  | tee "${out_dir_abs}/config-success-validate.json"
install -m 0644 "${out_dir_abs}/config-external-text.json" "${config_path}"
stop_verified_owner
activate_and_wait "${out_dir_abs}/external-text-status.json"
external_daemon_pid="$(jq -r '.owner.unix_process_id' "${out_dir_abs}/external-text-status.json")"
if [[ "${external_daemon_pid}" == "${post_failure_daemon_pid}" ]]; then
  echo "daemon PID did not change while recovering the external text provider" >&2
  exit 1
fi

VINPST_LIVE_NATIVE_WAV="${recognition_wav}" \
VINPST_LIVE_NATIVE_MODES=command \
VINPST_LIVE_SELECTED_TEXT="${selected_text}" \
VINPST_LIVE_EXPECTED_COMMIT_PREFIX="${expected_prefix}" \
VINPST_LIVE_CANDIDATE_DELAY_MS=0 \
VINPST_LIVE_VIRTUAL_OUT_DIR="${out_dir_abs}/external-text-recognition" \
  scripts/live/niri/run-ime-fcitx-virtual-source-live.sh

external_server_pid="${server_pid}"
wait "${server_pid}"
server_pid=""
test ! -e "${server_error}"
if [[ ! -f "${server_trace}" ]]; then
  cat "${server_log}" >&2
  echo "external text provider did not record an HTTP request" >&2
  exit 1
fi

command_summary="${out_dir_abs}/external-text-recognition/fcitx/command.jsonl"
final_commit="$(jq -r 'select(.event == "summary") | .commit' "${command_summary}")"
raw_asr="$(jq -r '.raw_asr_text' "${server_trace}")"
trace_candidate="$(jq -r '.candidate' "${server_trace}")"
if [[ "${final_commit}" != "${trace_candidate}" ]]; then
  echo "Fcitx final commit did not equal the external HTTP candidate" >&2
  printf 'commit=%s\ntrace=%s\n' "${final_commit}" "${trace_candidate}" >&2
  exit 1
fi
jq -e \
  --arg selected "${selected_text}" \
  --arg model "${provider_model}" \
  --arg candidate "${final_commit}" '
    .event == "request" and
    .request_count == 1 and
    .method == "POST" and
    .path == "/v1/chat/completions" and
    .authorization_scheme == "Bearer" and
    .authorization_value_recorded == false and
    .model == $model and
    .stream == false and
    .response_format == {"type": "json_object"} and
    .selected_text == $selected and
    (.raw_asr_text | length) > 0 and
    .candidate == $candidate
  ' "${server_trace}" >/dev/null
if grep -Fq "${api_key}" "${server_trace}" "${server_ready}"; then
  echo "external text provider leaked its fixture API key into request evidence" >&2
  exit 1
fi
jq -s -e 'any(.[]; .event == "summary" and .ok == true and .selection_source == "surrounding" and .delete_count > 0 and .candidate_count >= 3 and (.commit | length) > 0)' \
  "${command_summary}" >/dev/null

VINPST_LIVE_NATIVE_WAV="${recognition_wav}" \
VINPST_LIVE_NATIVE_MODES=command \
VINPST_LIVE_SELECTED_TEXT="" \
VINPST_LIVE_CLEAR_PRIMARY_SELECTION=1 \
VINPST_LIVE_EXPECT_UNCHANGED_ON_ERROR=1 \
VINPST_LIVE_REQUIRE_PARTIAL=0 \
VINPST_LIVE_VIRTUAL_OUT_DIR="${out_dir_abs}/external-text-no-selection" \
  scripts/live/niri/run-ime-fcitx-virtual-source-live.sh

no_selection_summary="${out_dir_abs}/external-text-no-selection/fcitx/command.jsonl"
jq -s -e '
  any(.[];
    .event == "summary" and
    .ok == true and
    .selected_text == "" and
    .commit == "" and
    .delete_count == 0 and
    .final_buffer == ""
  ) and
  any(.[];
    (.event == "client-ui" or .event == "input-panel") and
    .text == "Please select text first."
  )
' "${no_selection_summary}" >/dev/null
"${cli_binary}" daemon status --json >"${out_dir_abs}/after-no-selection-status.json"
jq -e '.status == "idle" and .runtime_status.active_session == false' \
  "${out_dir_abs}/after-no-selection-status.json" >/dev/null

restore_profile
restored_daemon_pid="$(jq -r '.owner.unix_process_id' "${out_dir_abs}/restored-status.json")"
if [[ "${restored_daemon_pid}" == "${external_daemon_pid}" ]]; then
  echo "daemon PID did not change while restoring the local text adapter" >&2
  exit 1
fi
cmp "${out_dir_abs}/service-before.service" "${service_path}"
cmp "${out_dir_abs}/addon-config-before.conf" "${addon_config}"

restored_runtime_status="$(call_service GetRuntimeStatus)"
python3 - "${restored_runtime_status}" <<'PY'
import ast
import json
import sys

runtime = json.loads(ast.literal_eval(sys.argv[1])[0])
adapter_ids = runtime.get("text_adapters", {}).get("adapter_ids", [])
if "native-command-live-adapter" not in adapter_ids:
    raise SystemExit(f"local command adapter was not restored: {adapter_ids!r}")
PY

jq -n \
  --arg provider_id "${provider_id}" \
  --arg provider_model "${provider_model}" \
  --arg base_url "${base_url}" \
  --arg failure_base_url "${failure_base_url}" \
  --arg selected_text "${selected_text}" \
  --arg raw_asr_text "${raw_asr}" \
  --arg commit "${final_commit}" \
  --argjson failure_server_pid "${failure_server_pid}" \
  --argjson server_pid "${external_server_pid}" \
  --argjson original_daemon_pid "${original_daemon_pid}" \
  --argjson failure_daemon_pid "${failure_daemon_pid}" \
  --argjson post_failure_daemon_pid "${post_failure_daemon_pid}" \
  --argjson external_daemon_pid "${external_daemon_pid}" \
  --argjson restored_daemon_pid "${restored_daemon_pid}" '
  {
    event: "summary",
    external_http_provider: true,
    local_fixture_not_third_party_cloud: true,
    provider_id: $provider_id,
    provider_model: $provider_model,
    base_url: $base_url,
    failure_base_url: $failure_base_url,
    failure_http_status: 404,
    failure_selected_text_preserved: true,
    failure_commit_suppressed: true,
    failure_delete_suppressed: true,
    failure_daemon_idle_after_error: true,
    no_selection_rejected_before_recording: true,
    no_selection_commit_suppressed: true,
    no_selection_delete_suppressed: true,
    no_selection_primary_restored: true,
    request_path: "/v1/chat/completions",
    authorization_scheme: "Bearer",
    authorization_value_recorded: false,
    selected_text: $selected_text,
    raw_asr_text: $raw_asr_text,
    commit: $commit,
    failure_server_pid: $failure_server_pid,
    external_server_pid: $server_pid,
    surrounding_text_deleted: true,
    candidate_selected: true,
    original_daemon_pid: $original_daemon_pid,
    failure_daemon_pid: $failure_daemon_pid,
    post_failure_daemon_pid: $post_failure_daemon_pid,
    external_daemon_pid: $external_daemon_pid,
    restored_daemon_pid: $restored_daemon_pid,
    profile_restored: true,
    backup_restored: true,
    service_unchanged: true,
    addon_config_unchanged: true,
    local_adapter_restored: true,
    backend_restored: true,
    ok: true
  }
' | tee "${out_dir_abs}/summary.json"
