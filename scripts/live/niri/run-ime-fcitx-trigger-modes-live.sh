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
out_dir="${VINPUT_LIVE_TRIGGER_MODES_OUT_DIR:-target/tmp/ime-fcitx-trigger-modes-live}"
service_path="${VINPUT_LIVE_DBUS_SERVICE:-${HOME}/.local/share/dbus-1/services/org.fcitx.Vinput.service}"
addon_config="${VINPUT_LIVE_FCITX_ADDON_CONFIG:-${HOME}/.config/fcitx5/conf/vinput.conf}"
trigger_key="${VINPUT_LIVE_NORMAL_KEY:-F9}"
probe="scripts/live/niri/probes/fcitx-live-trigger-mode-probe.py"
config_path=""
original_provider=""
original_model=""
service_mutated=0
addon_config_mutated=0
fcitx_restart_needed=0
profile_backup_existed=0

call_service() {
  gdbus call --session \
    --dest org.fcitx.Vinput \
    --object-path /org/fcitx/Vinput \
    --method "org.fcitx.Vinput.Service.$1" "${@:2}"
}

wait_backend() {
  local output_path="$1"
  for _ in $(seq 1 600); do
    if "${cli_binary}" daemon status --json >"${output_path}" 2>/dev/null &&
      jq -e \
        --arg provider "${original_provider}" \
        --arg model "${original_model}" '
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
  echo "daemon did not return to the original idle backend" >&2
  cat "${output_path}" >&2 2>/dev/null || true
  return 1
}

ensure_idle() {
  local status
  status="$(call_service GetStatus 2>/dev/null || true)"
  if [[ "${status}" == *"recording"* ]]; then
    call_service StopRecording "" >/dev/null 2>&1 || true
  fi
  for _ in $(seq 1 100); do
    status="$(call_service GetStatus 2>/dev/null || true)"
    if [[ "${status}" == *"idle"* ]]; then
      return 0
    fi
    sleep 0.1
  done
  echo "daemon did not become idle during trigger-mode cleanup: ${status}" >&2
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
  local output_path="$1"
  call_service GetStatus >/dev/null
  wait_backend "${output_path}"
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

verify_profile_unchanged() {
  cmp "${out_dir}/config-before.json" "${config_path}"
  if [[ "${profile_backup_existed}" == 1 ]]; then
    cmp "${out_dir}/config-backup-before.json" "${config_path}.bak"
  else
    test ! -e "${config_path}.bak"
  fi
}

restore_addon_config() {
  [[ "${addon_config_mutated}" == 0 ]] && return 0
  install -m 0644 "${out_dir}/addon-config-before.conf" "${addon_config}"
  cmp "${out_dir}/addon-config-before.conf" "${addon_config}"
  restart_fcitx >"${out_dir}/fcitx-restored.pid"
  fcitx_restart_needed=0
  addon_config_mutated=0
}

restore_service() {
  [[ "${service_mutated}" == 0 ]] && return 0
  ensure_idle
  stop_verified_owner
  install -m 0644 "${out_dir}/service-before.service" "${service_path}"
  activate_and_wait "${out_dir}/service-restored-status.json"
  cmp "${out_dir}/service-before.service" "${service_path}"
  verify_profile_unchanged
  service_mutated=0
}

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM
  set +e
  if ! ensure_idle; then
    exit_code=1
  fi
  if ! restore_addon_config; then
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

for command in fcitx5 fcitx5-remote gdbus jq pgrep python3 readlink; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required trigger-mode command is missing: ${command}" >&2
    exit 2
  fi
done
if [[ ! -x "${cli_binary}" ]]; then
  echo "vinput CLI is missing: ${cli_binary}" >&2
  exit 2
fi
for path in "${service_path}" "${addon_config}" "${probe}"; do
  if [[ ! -e "${path}" ]]; then
    echo "trigger-mode fixture is missing: ${path}" >&2
    exit 2
  fi
done
if ! fcitx5-remote --check >/dev/null 2>&1; then
  echo "Fcitx5 is not running in this session" >&2
  exit 1
fi
if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  echo "DBUS_SESSION_BUS_ADDRESS is not set" >&2
  exit 1
fi

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
"${cli_binary}" daemon status --json >"${out_dir}/status-before.json"
if ! jq -e '
  .status == "idle" and
  .owner.ok == true and
  .asr_backend.has_effective_backend == true and
  .asr_backend.reload_in_progress == false and
  .asr_backend.last_error == ""
' "${out_dir}/status-before.json" >/dev/null; then
  echo "ASR backend must be idle and ready before trigger-mode validation" >&2
  cat "${out_dir}/status-before.json" >&2
  exit 1
fi
original_provider="$(jq -r '.asr_backend.effective_provider_id' "${out_dir}/status-before.json")"
original_model="$(jq -r '.asr_backend.effective_model_id' "${out_dir}/status-before.json")"
config_path="$(jq -r '
  .owner.process.cmdline as $args |
  ($args | index("--config")) as $index |
  if $index == null then empty else $args[$index + 1] end
' "${out_dir}/status-before.json")"
if [[ -z "${config_path}" || ! -f "${config_path}" ]]; then
  echo "could not resolve the active daemon config path" >&2
  exit 1
fi
install -m 0644 "${config_path}" "${out_dir}/config-before.json"
if [[ -f "${config_path}.bak" ]]; then
  profile_backup_existed=1
  install -m 0644 "${config_path}.bak" "${out_dir}/config-backup-before.json"
fi
install -m 0644 "${service_path}" "${out_dir}/service-before.service"
install -m 0644 "${addon_config}" "${out_dir}/addon-config-before.conf"

python3 - "${out_dir}/service-before.service" "${out_dir}/service-mock-audio.service" <<'PY'
import shlex
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
lines = source.read_text().splitlines()
exec_rows = [index for index, line in enumerate(lines) if line.startswith("Exec=")]
if len(exec_rows) != 1:
    raise SystemExit(f"expected one D-Bus service Exec row, found {len(exec_rows)}")
index = exec_rows[0]
command = shlex.split(lines[index][len("Exec=") :])
if "--audio-backend" in command:
    audio_index = command.index("--audio-backend")
    if audio_index + 1 >= len(command):
        raise SystemExit("--audio-backend is missing its value")
    command[audio_index + 1] = "mock"
else:
    command.extend(["--audio-backend", "mock"])
lines[index] = "Exec=" + " ".join(shlex.quote(value) for value in command)
target.write_text("\n".join(lines) + "\n")
PY

stop_verified_owner
install -m 0644 "${out_dir}/service-mock-audio.service" "${service_path}"
service_mutated=1
activate_and_wait "${out_dir}/mock-audio-status.json"
if ! jq -e '.owner.process.cmdline | index("mock") != null' \
  "${out_dir}/mock-audio-status.json" >/dev/null; then
  echo "activated daemon did not use mock audio" >&2
  exit 1
fi

for mode in Tap Hold Both; do
  python3 - "${out_dir}/addon-config-before.conf" "${out_dir}/addon-config-${mode}.conf" \
    "${mode}" <<'PY'
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
mode = sys.argv[3]
lines = source.read_text().splitlines()
rows = [index for index, line in enumerate(lines) if line.startswith("TriggerMode=")]
if len(rows) != 1:
    raise SystemExit(f"expected one TriggerMode row, found {len(rows)}")
lines[rows[0]] = f"TriggerMode={mode}"
target.write_text("\n".join(lines) + "\n")
PY
  install -m 0644 "${out_dir}/addon-config-${mode}.conf" "${addon_config}"
  addon_config_mutated=1
  grep -qx "TriggerMode=${mode}" "${addon_config}"
  restart_fcitx | tee "${out_dir}/fcitx-${mode}.pid"
  fcitx_restart_needed=1
  python3 "${probe}" \
    --mode "${mode}" \
    --trigger-key "${trigger_key}" \
    | tee "${out_dir}/${mode}.jsonl"
  jq -s -e --arg mode "${mode}" 'any(.[];
    .event == "summary" and
    .mode == $mode and
    .ok == true and
    .final_status == "idle" and
    .commit_count == 0
  )' "${out_dir}/${mode}.jsonl" >/dev/null
  wait_backend "${out_dir}/${mode}-status-after.json"
done

verify_profile_unchanged
restore_addon_config
restore_service

jq -n \
  --arg provider "${original_provider}" \
  --arg model "${original_model}" \
  --arg trigger_key "${trigger_key}" \
  '{
    event: "summary",
    modes: ["Tap", "Hold", "Both"],
    trigger_key: $trigger_key,
    provider: $provider,
    model: $model,
    mock_audio: true,
    profile_unchanged: true,
    addon_config_restored: true,
    service_restored: true,
    fcitx_restored: true,
    backend_restored: true,
    ok: true
  }' | tee "${out_dir}/summary.json"
