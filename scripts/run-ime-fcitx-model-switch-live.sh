#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

cli_binary="${VINPUT_LIVE_CLI_BINARY:-target/debug/vinput}"
out_dir="${VINPUT_LIVE_MODEL_SWITCH_OUT_DIR:-target/tmp/ime-fcitx-model-switch-live}"
if [[ "${out_dir}" == /* ]]; then
  out_dir_abs="${out_dir}"
else
  out_dir_abs="${repo_root}/${out_dir}"
fi
probe="scripts/fcitx-live-asr-selection-probe.py"
service_path="${VINPUT_LIVE_DBUS_SERVICE:-${HOME}/.local/share/dbus-1/services/org.fcitx.Vinput.service}"
model_root="${VINPUT_LIVE_MODEL_ROOT:-${out_dir_abs}/model-root}"
trigger_key="${VINPUT_LIVE_ASR_MENU_KEY:-F8}"
alt_model_source="${VINPUT_LIVE_ALT_MODEL:-${repo_root}/target/models/onnx-pf-zh-sm-off}"
alt_model="${model_root}/onnx-pf-zh-sm-off"
alt_wav="${VINPUT_LIVE_ALT_WAV:-${alt_model_source}/test_wavs/0.wav}"
original_wav="${VINPUT_LIVE_ORIGINAL_WAV:-${repo_root}/target/models/onnx-zf-ctc-zh-sm-int8-stream/test_wavs/0.wav}"
config_path=""
profile_mutated=0
service_mutated=0
fcitx_restart_needed=0
backup_existed=0
original_provider=""
original_model=""

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

stop_verified_owner() {
  local status pid exe cmdline proc_exe proc_cmdline
  status="$("${cli_binary}" daemon status --json 2>/dev/null || true)"
  pid="$(jq -r '.owner.unix_process_id // empty' <<<"${status}")"
  [[ -z "${pid}" ]] && return 0
  exe="$(jq -r '.owner.process.exe // empty' <<<"${status}")"
  cmdline="$(jq -r '.owner.process.cmdline | join(" ")' <<<"${status}")"
  if [[ "${exe}" != *vinput-daemon* || "${cmdline}" != *"${config_path}"* ]]; then
    echo "refusing to stop unexpected org.fcitx.Vinput owner: pid=${pid} exe=${exe}" >&2
    return 1
  fi
  proc_exe="$(readlink "/proc/${pid}/exe")"
  proc_cmdline="$(tr '\0' ' ' <"/proc/${pid}/cmdline")"
  if [[ "${proc_exe}" != *vinput-daemon* || "${proc_cmdline}" != *"${config_path}"* ]]; then
    echo "live owner changed during verification: pid=${pid} exe=${proc_exe}" >&2
    return 1
  fi
  kill -TERM "${pid}"
  for _ in $(seq 1 100); do
    if ! kill -0 "${pid}" 2>/dev/null; then
      return 0
    fi
    sleep 0.05
  done
  echo "verified daemon did not terminate after SIGTERM: ${pid}" >&2
  return 1
}

activate_and_wait() {
  local provider="$1" model="$2" output_path="$3"
  call_service GetStatus >/dev/null
  wait_backend "${provider}" "${model}" "${output_path}"
}

restart_fcitx() {
  local pid
  fcitx5 -rd >/dev/null 2>&1
  for _ in $(seq 1 100); do
    pid="$(pgrep -n -x fcitx5 || true)"
    if [[ -n "${pid}" ]] && fcitx5-remote --check >/dev/null 2>&1 &&
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
  install -m 0644 "${out_dir}/config-before.json" "${config_path}"
  if [[ "${backup_existed}" == 1 ]]; then
    install -m 0644 "${out_dir}/config-backup-before.json" "${config_path}.bak"
  else
    rm -f "${config_path}.bak"
  fi
  "${cli_binary}" daemon reload-asr --json >"${out_dir}/restore-profile-reload.json"
  wait_backend "${original_provider}" "${original_model}" \
    "${out_dir}/restore-profile-status.json"
  cmp "${out_dir}/config-before.json" "${config_path}"
  if [[ "${backup_existed}" == 1 ]]; then
    cmp "${out_dir}/config-backup-before.json" "${config_path}.bak"
  else
    test ! -e "${config_path}.bak"
  fi
  profile_mutated=0
}

restore_service() {
  [[ "${service_mutated}" == 0 ]] && return 0
  stop_verified_owner
  install -m 0644 "${out_dir}/service-before.service" "${service_path}"
  activate_and_wait "${original_provider}" "${original_model}" \
    "${out_dir}/restored-status.json"
  cmp "${out_dir}/service-before.service" "${service_path}"
  if jq -e --arg model_root "${model_root}" \
    '.owner.process.cmdline | index($model_root) != null' \
    "${out_dir}/restored-status.json" >/dev/null; then
    echo "temporary model root remained in the restored daemon command line" >&2
    return 1
  fi
  service_mutated=0
}

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM
  set +e
  if ! restore_profile; then
    exit_code=1
  fi
  if ! restore_service; then
    exit_code=1
  fi
  if [[ "${fcitx_restart_needed}" == 1 ]]; then
    if ! restart_fcitx >"${out_dir}/fcitx-restored.pid"; then
      exit_code=1
    fi
    fcitx_restart_needed=0
  fi
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

for command in cp fcitx5 fcitx5-remote gdbus jq pgrep python3 readlink; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required command is missing: ${command}" >&2
    exit 2
  fi
done
if [[ ! -x "${cli_binary}" ]]; then
  echo "vinput CLI is missing: ${cli_binary}" >&2
  exit 2
fi
if ! fcitx5-remote --check >/dev/null 2>&1; then
  echo "Fcitx5 is not running in this session" >&2
  exit 1
fi
if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  echo "DBUS_SESSION_BUS_ADDRESS is not set" >&2
  exit 1
fi
for path in "${service_path}" "${alt_model_source}" "${alt_wav}" "${original_wav}"; do
  if [[ ! -e "${path}" ]]; then
    echo "model-switch input is missing: ${path}" >&2
    exit 2
  fi
done

rm -rf "${out_dir}"
mkdir -p "${out_dir}" "${alt_model}"
cp -al "${alt_model_source}/." "${alt_model}/"
if [[ ! -f "${alt_model}/vinput-model.json" ]]; then
  echo "temporary model root did not materialize installed-model metadata" >&2
  exit 1
fi
"${cli_binary}" daemon status --json >"${out_dir}/before.json"
if ! jq -e '
  .status == "idle" and
  .owner.ok == true and
  .asr_backend.has_effective_backend == true and
  .asr_backend.reload_in_progress == false and
  .asr_backend.last_error == ""
' "${out_dir}/before.json" >/dev/null; then
  echo "ASR backend must be idle and ready before model switching" >&2
  cat "${out_dir}/before.json" >&2
  exit 1
fi
original_provider="$(jq -r '.asr_backend.effective_provider_id' "${out_dir}/before.json")"
original_model="$(jq -r '.asr_backend.effective_model_id' "${out_dir}/before.json")"
config_path="$(jq -r '
  .owner.process.cmdline as $args |
  ($args | index("--config")) as $index |
  if $index == null then empty else $args[$index + 1] end
' "${out_dir}/before.json")"
if [[ -z "${config_path}" || ! -f "${config_path}" ]]; then
  echo "could not resolve the active daemon config path" >&2
  exit 1
fi
install -m 0644 "${config_path}" "${out_dir}/config-before.json"
if [[ -f "${config_path}.bak" ]]; then
  backup_existed=1
  install -m 0644 "${config_path}.bak" "${out_dir}/config-backup-before.json"
fi
install -m 0644 "${service_path}" "${out_dir}/service-before.service"

python3 - "${out_dir}/service-before.service" "${out_dir}/service-model-root.service" \
  "${model_root}" <<'PY'
import shlex
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
model_root = sys.argv[3]
lines = source.read_text().splitlines()
exec_rows = [index for index, line in enumerate(lines) if line.startswith("Exec=")]
if len(exec_rows) != 1:
    raise SystemExit(f"expected one D-Bus service Exec row, found {len(exec_rows)}")
index = exec_rows[0]
command = shlex.split(lines[index][len("Exec=") :])
if "--model-root" in command:
    raise SystemExit("D-Bus activation service already contains --model-root")
command.extend(["--model-root", model_root])
lines[index] = "Exec=" + " ".join(shlex.quote(value) for value in command)
target.write_text("\n".join(lines) + "\n")
PY

stop_verified_owner
install -m 0644 "${out_dir}/service-model-root.service" "${service_path}"
service_mutated=1
activate_and_wait "${original_provider}" "${original_model}" \
  "${out_dir}/model-root-status.json"
if ! jq -e --arg model_root "${model_root}" \
  '.owner.process.cmdline | index($model_root) != null' \
  "${out_dir}/model-root-status.json" >/dev/null; then
  echo "activated daemon did not use the temporary model root" >&2
  exit 1
fi

call_service GetAsrDisplayMenuState >"${out_dir}/menu-state-before.txt"
python3 - "${out_dir}/menu-state-before.txt" "${original_provider}" "${alt_model}" <<'PY'
import ast
import re
import sys
from pathlib import Path

raw = Path(sys.argv[1]).read_text()
raw = re.sub(r"\btrue\b", "True", raw)
raw = re.sub(r"\bfalse\b", "False", raw)
state = ast.literal_eval(raw)
provider = sys.argv[2]
model = sys.argv[3]
rows = state[6]
matches = [row for row in rows if row[0] == provider and row[4] == model]
if len(matches) != 1:
    raise SystemExit(f"expected one installed-model ASR target, found {len(matches)}")
PY

profile_mutated=1
restart_fcitx | tee "${out_dir}/fcitx-before-selection.pid"
fcitx_restart_needed=1
python3 "${probe}" \
  --trigger-key "${trigger_key}" \
  --expected-provider "${original_provider}" \
  --expected-model "${alt_model}" \
  | tee "${out_dir}/asr-selection.jsonl"
jq -s -e 'any(.[]; .event == "summary" and .ok == true and .selected == true and .filter_complete == true)' \
  "${out_dir}/asr-selection.jsonl" >/dev/null
wait_backend "${original_provider}" "${alt_model}" "${out_dir}/alt-ready.json"

VINPUT_LIVE_NATIVE_WAV="${alt_wav}" \
VINPUT_LIVE_NATIVE_MODES=normal \
VINPUT_LIVE_REQUIRE_PARTIAL=0 \
VINPUT_LIVE_VIRTUAL_OUT_DIR="${out_dir}/alt-recognition" \
  scripts/run-ime-fcitx-virtual-source-live.sh
jq -s -e 'any(.[]; .event == "summary" and .ok == true and .require_partial == false and (.commit | length) > 0)' \
  "${out_dir}/alt-recognition/fcitx/normal.jsonl" >/dev/null
wait_backend "${original_provider}" "${alt_model}" \
  "${out_dir}/alt-after-recognition.json"

restore_profile
wait_backend "${original_provider}" "${original_model}" \
  "${out_dir}/original-ready.json"

VINPUT_LIVE_NATIVE_WAV="${original_wav}" \
VINPUT_LIVE_NATIVE_MODES=normal \
VINPUT_LIVE_VIRTUAL_OUT_DIR="${out_dir}/original-recognition" \
  scripts/run-ime-fcitx-virtual-source-live.sh
jq -s -e 'any(.[]; .event == "summary" and .ok == true and .require_partial == true and .partial_count > 0 and (.commit | length) > 0)' \
  "${out_dir}/original-recognition/fcitx/normal.jsonl" >/dev/null
wait_backend "${original_provider}" "${original_model}" \
  "${out_dir}/original-after-recognition.json"

alt_commit="$(jq -r 'select(.event == "summary") | .commit' \
  "${out_dir}/alt-recognition/fcitx/normal.jsonl")"
original_commit="$(jq -r 'select(.event == "summary") | .commit' \
  "${out_dir}/original-recognition/fcitx/normal.jsonl")"
restore_profile
restore_service
restart_fcitx | tee "${out_dir}/fcitx-restored.pid"
fcitx_restart_needed=0

jq -n \
  --arg provider "${original_provider}" \
  --arg original_model "${original_model}" \
  --arg alt_model "${alt_model}" \
  --arg alt_commit "${alt_commit}" \
  --arg original_commit "${original_commit}" \
  '{
    event: "summary",
    menu_selection: true,
    provider: $provider,
    alt: {model: $alt_model, commit: $alt_commit, recognition: true},
    original: {model: $original_model, commit: $original_commit, recognition: true},
    profile_restored: true,
    service_restored: true,
    fcitx_restored: true,
    backend_restored: true,
    ok: true
  }' | tee "${out_dir}/summary.json"
