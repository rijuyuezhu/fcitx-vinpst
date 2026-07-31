#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

cli_binary="${VINPUT_LIVE_CLI_BINARY:-target/debug/vinput}"
out_dir="${VINPUT_LIVE_CROSS_PROVIDER_OUT_DIR:-target/tmp/ime-fcitx-cross-provider-live}"
if [[ "${out_dir}" == /* ]]; then
  out_dir_abs="${out_dir}"
else
  out_dir_abs="${repo_root}/${out_dir}"
fi
selection_probe="scripts/fcitx-live-asr-selection-probe.py"
bridge="${repo_root}/scripts/legacy-command-asr-wav-bridge.py"
service_path="${VINPUT_LIVE_DBUS_SERVICE:-${HOME}/.local/share/dbus-1/services/org.fcitx.Vinput.service}"
addon_config="${HOME}/.config/fcitx5/conf/vinput.conf"
daemon_wrapper="${HOME}/.local/share/fcitx-vinput/vinput-daemon-with-vinput-env.sh"

trigger_key="${VINPUT_LIVE_ASR_MENU_KEY:-F8}"
external_recognizer="${VINPUT_LIVE_EXTERNAL_RECOGNIZER:-sherpa-one-shot}"
external_provider="${VINPUT_LIVE_EXTERNAL_PROVIDER_ID:-external-command}"
external_model="${VINPUT_LIVE_EXTERNAL_MODEL_ID:-external-one-shot}"
recognition_wav="${VINPUT_LIVE_EXTERNAL_WAV:-${repo_root}/target/models/onnx-zf-ctc-zh-sm-int8-stream/test_wavs/0.wav}"
whisper_binary="${VINPUT_LIVE_WHISPER_BINARY:-${repo_root}/target/third-party/whisper.cpp-v1.9.1/build/bin/whisper-cli}"
whisper_source="${VINPUT_LIVE_WHISPER_SOURCE:-${repo_root}/target/third-party/whisper.cpp-v1.9.1/src}"
whisper_model="${VINPUT_LIVE_WHISPER_MODEL:-${HOME}/.local/share/voxtype/models/ggml-base.bin}"
whisper_language="${VINPUT_LIVE_WHISPER_LANGUAGE:-zh}"
whisper_commit="f049fff95a089aa9969deb009cdd4892b3e74916"
underlying_recognizer="sherpa-onnx one-shot daemon using the original model"
independent_recognizer=false
config_path=""
profile_mutated=0
fcitx_restart_needed=0
backup_existed=0
original_provider=""
original_model=""

case "${external_recognizer}" in
  sherpa-one-shot)
    ;;
  whisper-cpp)
    underlying_recognizer="whisper.cpp v1.9.1 with an independent multilingual model"
    independent_recognizer=true
    ;;
  *)
    echo "unsupported external recognizer mode: ${external_recognizer}" >&2
    exit 2
    ;;
esac

call_service() {
  gdbus call --session \
    --dest org.fcitx.Vinput \
    --object-path /org/fcitx/Vinput \
    --method "org.fcitx.Vinput.Service.$1" "${@:2}"
}

wait_backend() {
  local provider="$1" model="$2" output_path="$3"
  for _ in $(seq 1 600); do
    if "${cli_binary}" daemon status --json >"${output_path}" 2>/dev/null &&
      jq -e \
        --arg provider "${provider}" \
        --arg model "${model}" '
          .status == "idle" and
          .owner.ok == true and
          .asr_backend.has_effective_backend == true and
          .asr_backend.reload_in_progress == false and
          .asr_backend.last_error == "" and
          .asr_backend.target_provider_id == $provider and
          .asr_backend.target_model_id == $model and
          .asr_backend.effective_provider_id == $provider and
          .asr_backend.effective_model_id == $model
        ' "${output_path}" >/dev/null; then
      return 0
    fi
    sleep 0.1
  done
  echo "ASR backend did not become ready for ${provider}/${model}" >&2
  cat "${output_path}" >&2 2>/dev/null || true
  return 1
}

restart_fcitx() {
  local previous_pid pid
  previous_pid="$(pgrep -n -x fcitx5 || true)"
  fcitx5 -rd >/dev/null 2>&1
  for _ in $(seq 1 120); do
    pid="$(pgrep -n -x fcitx5 || true)"
    if [[ -n "${pid}" ]] && [[ "${pid}" != "${previous_pid}" ]] &&
      [[ ! -e "/proc/${previous_pid}" ]] &&
      fcitx5-remote --check >/dev/null 2>&1 &&
      grep -q "${HOME}/.local/lib/fcitx5/fcitx5-vinput.so" "/proc/${pid}/maps"; then
      printf '%s\n' "${pid}"
      return 0
    fi
    sleep 0.1
  done
  echo "restarted Fcitx did not load the user vinput addon" >&2
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
  "${cli_binary}" daemon reload-asr --json >"${out_dir_abs}/restore-profile-reload.json"
  wait_backend "${original_provider}" "${original_model}" \
    "${out_dir_abs}/restore-profile-status.json"
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
  cmp "${out_dir_abs}/addon-config-before.conf" "${addon_config}" || exit_code=1
  rm -rf scripts/__pycache__
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

for command in cmp fcitx5 fcitx5-remote gdbus git install jq pgrep python3 ruff sha256sum; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required cross-provider command is missing: ${command}" >&2
    exit 2
  fi
done
for path in \
  "${cli_binary}" \
  "${selection_probe}" \
  "${bridge}" \
  "${service_path}" \
  "${addon_config}" \
  "${daemon_wrapper}" \
  "${recognition_wav}"; do
  if [[ ! -e "${path}" ]]; then
    echo "cross-provider input is missing: ${path}" >&2
    exit 2
  fi
done
if [[ "${external_recognizer}" == "whisper-cpp" ]]; then
  for path in "${whisper_binary}" "${whisper_model}" "${whisper_source}/.git"; do
    if [[ ! -e "${path}" ]]; then
      echo "Whisper cross-provider input is missing: ${path}" >&2
      exit 2
    fi
  done
  actual_whisper_commit="$(git -C "${whisper_source}" rev-parse HEAD)"
  if [[ "${actual_whisper_commit}" != "${whisper_commit}" ]]; then
    echo "Whisper source commit mismatch: ${actual_whisper_commit}" >&2
    exit 2
  fi
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
"${cli_binary}" daemon status --json >"${out_dir_abs}/before.json"
if ! jq -e '
  .status == "idle" and
  .owner.ok == true and
  .asr_backend.has_effective_backend == true and
  .asr_backend.reload_in_progress == false and
  .asr_backend.last_error == ""
' "${out_dir_abs}/before.json" >/dev/null; then
  cat "${out_dir_abs}/before.json" >&2
  echo "ASR backend must be idle and ready before cross-provider switching" >&2
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

ruff check "${bridge}" "${selection_probe}"
ruff format --check "${bridge}" "${selection_probe}"
python3 -m py_compile "${bridge}" "${selection_probe}"

native_config="${out_dir_abs}/native-one-shot.json"
external_config="${out_dir_abs}/external-provider.json"
external_trace="${out_dir_abs}/external-process.log"
python3 - \
  "${out_dir_abs}/config-before.json" \
  "${native_config}" \
  "${external_config}" \
  "${bridge}" \
  "${daemon_wrapper}" \
  "${external_provider}" \
  "${external_model}" \
  "${external_trace}" \
  "${external_recognizer}" \
  "${whisper_binary}" \
  "${whisper_model}" \
  "${whisper_language}" <<'PY'
import json
import sys
from pathlib import Path

source = Path(sys.argv[1])
native_path = Path(sys.argv[2])
external_path = Path(sys.argv[3])
bridge = sys.argv[4]
daemon = sys.argv[5]
provider_id = sys.argv[6]
model_id = sys.argv[7]
trace_path = sys.argv[8]
recognizer = sys.argv[9]
whisper_binary = sys.argv[10]
whisper_model = sys.argv[11]
whisper_language = sys.argv[12]

native = json.loads(source.read_text(encoding="utf-8"))
if any(provider.get("id") == provider_id for provider in native["asr"]["providers"]):
    raise SystemExit(f"temporary provider id already exists: {provider_id}")
native_path.write_text(
    json.dumps(native, indent=2, ensure_ascii=False) + "\n",
    encoding="utf-8",
)

if recognizer == "sherpa-one-shot":
    command = (
        'set -euo pipefail; printf "recognizer=sherpa-one-shot pid=%s wav=%s\\n" '
        '"$$" "$VINPUT_ASR_WAV" >> "$VINPUT_EXTERNAL_TRACE"; '
        '"$VINPUT_EXTERNAL_DAEMON" --configured-backends '
        '--config "$VINPUT_EXTERNAL_CONFIG" --once --wav "$VINPUT_ASR_WAV" '
        "| jq -er '.commit_text | select(type == \"string\" and length > 0)'"
    )
    provider_env = {
        "VINPUT_EXTERNAL_DAEMON": daemon,
        "VINPUT_EXTERNAL_CONFIG": str(native_path),
        "VINPUT_EXTERNAL_TRACE": trace_path,
    }
elif recognizer == "whisper-cpp":
    command = (
        'set -euo pipefail; printf "recognizer=whisper-cpp pid=%s wav=%s\\n" '
        '"$$" "$VINPUT_ASR_WAV" >> "$VINPUT_EXTERNAL_TRACE"; '
        'export LD_LIBRARY_PATH="$VINPUT_WHISPER_LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"; '
        '"$VINPUT_WHISPER_BINARY" --no-gpu --threads 8 '
        '--language "$VINPUT_WHISPER_LANGUAGE" --no-timestamps --no-prints '
        '--model "$VINPUT_WHISPER_MODEL" --file "$VINPUT_ASR_WAV"'
    )
    provider_env = {
        "VINPUT_EXTERNAL_TRACE": trace_path,
        "VINPUT_WHISPER_BINARY": whisper_binary,
        "VINPUT_WHISPER_LIB_DIR": str(Path(whisper_binary).parent),
        "VINPUT_WHISPER_MODEL": whisper_model,
        "VINPUT_WHISPER_LANGUAGE": whisper_language,
    }
else:
    raise SystemExit(f"unsupported external recognizer: {recognizer}")

external = json.loads(source.read_text(encoding="utf-8"))
external["asr"]["providers"].append(
    {
        "id": provider_id,
        "type": "command",
        "command": bridge,
        "args": ["--timeout-ms", "90000", "--", "bash", "-c", command],
        "env": provider_env,
        "model": model_id,
        "timeout_ms": 95000,
    }
)
external_path.write_text(
    json.dumps(external, indent=2, ensure_ascii=False) + "\n",
    encoding="utf-8",
)
PY

# Prove the external command process before touching the user's active profile.
preflight_config="${out_dir_abs}/external-preflight.json"
python3 - "${external_config}" "${preflight_config}" "${external_provider}" <<'PY'
import json
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
provider_id = sys.argv[3]
config = json.loads(source.read_text(encoding="utf-8"))
config["asr"]["active_provider"] = provider_id
target.write_text(json.dumps(config, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
PY
"${daemon_wrapper}" \
  --configured-backends \
  --config "${preflight_config}" \
  --once \
  --wav "${recognition_wav}" \
  >"${out_dir_abs}/preflight-result.json" \
  2>"${out_dir_abs}/preflight-stderr.log"
jq -e '.commit_text | type == "string" and length > 0' \
  "${out_dir_abs}/preflight-result.json" >/dev/null
if [[ ! -s "${external_trace}" ]]; then
  echo "external command preflight did not record a child process" >&2
  exit 1
fi
preflight_wav="$(sed -n 's/^recognizer=[^ ]* pid=[0-9][0-9]* wav=//p' "${external_trace}" | head -1)"
if [[ -z "${preflight_wav}" || -e "${preflight_wav}" ]]; then
  echo "external bridge did not clean its preflight temporary WAV: ${preflight_wav}" >&2
  exit 1
fi
: >"${external_trace}"

# Expose the command provider while keeping the original backend effective.
install -m 0644 "${external_config}" "${config_path}"
profile_mutated=1
"${cli_binary}" daemon reload-asr --json >"${out_dir_abs}/provider-added-reload.json"
wait_backend "${original_provider}" "${original_model}" \
  "${out_dir_abs}/provider-added-status.json"

call_service GetAsrDisplayMenuState >"${out_dir_abs}/menu-state-before.txt"
python3 - "${out_dir_abs}/menu-state-before.txt" "${external_provider}" "${external_model}" <<'PY'
import ast
import re
import sys
from pathlib import Path

raw = Path(sys.argv[1]).read_text(encoding="utf-8")
raw = re.sub(r"\btrue\b", "True", raw)
raw = re.sub(r"\bfalse\b", "False", raw)
state = ast.literal_eval(raw)
provider = sys.argv[2]
model = sys.argv[3]
rows = state[6]
matches = [row for row in rows if row[0] == provider and row[4] == model]
if len(matches) != 1:
    raise SystemExit(f"expected one external command target, found {len(matches)}")
if matches[0][1] != "command":
    raise SystemExit(f"external target kind mismatch: {matches[0][1]!r}")
PY

restart_fcitx | tee "${out_dir_abs}/fcitx-before-selection.pid"
fcitx_restart_needed=1
python3 "${selection_probe}" \
  --trigger-key "${trigger_key}" \
  --expected-provider "${external_provider}" \
  --expected-model "${external_model}" \
  | tee "${out_dir_abs}/asr-selection.jsonl"
jq -s -e 'any(.[]; .event == "summary" and .ok == true and .selected == true and .filter_complete == true)' \
  "${out_dir_abs}/asr-selection.jsonl" >/dev/null
wait_backend "${external_provider}" "${external_model}" \
  "${out_dir_abs}/external-ready.json"

VINPUT_LIVE_NATIVE_WAV="${recognition_wav}" \
VINPUT_LIVE_NATIVE_MODES=normal \
VINPUT_LIVE_REQUIRE_PARTIAL=0 \
VINPUT_LIVE_VIRTUAL_OUT_DIR="${out_dir_abs}/external-recognition" \
  scripts/run-ime-fcitx-virtual-source-live.sh
jq -s -e 'any(.[]; .event == "summary" and .ok == true and .require_partial == false and (.commit | length) > 0)' \
  "${out_dir_abs}/external-recognition/fcitx/normal.jsonl" >/dev/null
wait_backend "${external_provider}" "${external_model}" \
  "${out_dir_abs}/external-after-recognition.json"
if [[ ! -s "${external_trace}" ]]; then
  echo "external provider recognition did not execute a child process" >&2
  exit 1
fi
live_wav="$(sed -n 's/^recognizer=[^ ]* pid=[0-9][0-9]* wav=//p' "${external_trace}" | tail -1)"
if [[ -z "${live_wav}" || -e "${live_wav}" ]]; then
  echo "external bridge did not clean its live temporary WAV: ${live_wav}" >&2
  exit 1
fi

external_commit="$(jq -r 'select(.event == "summary") | .commit' \
  "${out_dir_abs}/external-recognition/fcitx/normal.jsonl")"
external_child_count="$(wc -l <"${external_trace}")"
external_binary_sha256=""
external_model_sha256=""
if [[ "${external_recognizer}" == "whisper-cpp" ]]; then
  external_binary_sha256="$(sha256sum "${whisper_binary}" | awk '{print $1}')"
  external_model_sha256="$(sha256sum "${whisper_model}" | awk '{print $1}')"
fi

restore_profile
wait_backend "${original_provider}" "${original_model}" \
  "${out_dir_abs}/original-ready.json"

VINPUT_LIVE_NATIVE_WAV="${recognition_wav}" \
VINPUT_LIVE_NATIVE_MODES=normal \
VINPUT_LIVE_VIRTUAL_OUT_DIR="${out_dir_abs}/original-recognition" \
  scripts/run-ime-fcitx-virtual-source-live.sh
jq -s -e 'any(.[]; .event == "summary" and .ok == true and .require_partial == true and .partial_count > 0 and (.commit | length) > 0)' \
  "${out_dir_abs}/original-recognition/fcitx/normal.jsonl" >/dev/null
wait_backend "${original_provider}" "${original_model}" \
  "${out_dir_abs}/original-after-recognition.json"
original_commit="$(jq -r 'select(.event == "summary") | .commit' \
  "${out_dir_abs}/original-recognition/fcitx/normal.jsonl")"

cmp "${out_dir_abs}/service-before.service" "${service_path}"
cmp "${out_dir_abs}/addon-config-before.conf" "${addon_config}"
restart_fcitx | tee "${out_dir_abs}/fcitx-restored.pid"
fcitx_restart_needed=0

jq -n \
  --arg original_provider "${original_provider}" \
  --arg original_model "${original_model}" \
  --arg external_provider "${external_provider}" \
  --arg external_model "${external_model}" \
  --arg external_commit "${external_commit}" \
  --arg original_commit "${original_commit}" \
  --arg bridge "${bridge}" \
  --arg external_recognizer "${external_recognizer}" \
  --arg underlying_recognizer "${underlying_recognizer}" \
  --arg whisper_commit "${whisper_commit}" \
  --arg binary_sha256 "${external_binary_sha256}" \
  --arg model_sha256 "${external_model_sha256}" \
  --argjson independent_recognizer "${independent_recognizer}" \
  --argjson external_child_count "${external_child_count}" \
  '{
    event: "summary",
    menu_selection: true,
    cross_provider: true,
    external_process_boundary: true,
    external_provider: {
      provider: $external_provider,
      model: $external_model,
      kind: "command",
      bridge: $bridge,
      recognizer_mode: $external_recognizer,
      underlying_recognizer: $underlying_recognizer,
      independent_recognizer: $independent_recognizer,
      whisper_commit: (if $independent_recognizer then $whisper_commit else null end),
      binary_sha256: (if $independent_recognizer then $binary_sha256 else null end),
      model_sha256: (if $independent_recognizer then $model_sha256 else null end),
      final_only: true,
      child_process_count: $external_child_count,
      commit: $external_commit,
      recognition: true
    },
    original: {
      provider: $original_provider,
      model: $original_model,
      streaming: true,
      commit: $original_commit,
      recognition: true
    },
    temporary_wavs_cleaned: true,
    profile_restored: true,
    backup_restored: true,
    service_unchanged: true,
    addon_config_unchanged: true,
    fcitx_restored: true,
    backend_restored: true,
    ok: true
  }' | tee "${out_dir_abs}/summary.json"
