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
selection_probe="${repo_root}/scripts/live/niri/probes/fcitx-live-asr-selection-probe.py"
virtual_runner="${repo_root}/scripts/live/niri/run-ime-fcitx-virtual-source-live.sh"
out_dir="${VINPST_LIVE_CROSS_PROVIDER_FAILURE_OUT_DIR:-target/tmp/ime-fcitx-cross-provider-failure-live}"
out_dir_abs="$(realpath -m "${out_dir}")"
monitor_log="${out_dir_abs}/dbus-monitor.log"
service_path="${VINPST_LIVE_DBUS_SERVICE:-${HOME}/.local/share/dbus-1/services/org.fcitx.Vinpst.service}"
addon_config="${HOME}/.config/fcitx5/conf/vinpst.conf"
remote_provider="${VINPST_LIVE_FAILURE_PROVIDER_ID:-remote-unavailable}"
remote_model="${VINPST_LIVE_FAILURE_MODEL_ID:-remote-failure-fixture}"
remote_endpoint="${VINPST_LIVE_FAILURE_ENDPOINT:-ftp://127.0.0.1/unavailable}"
expected_notification_summary="${VINPST_LIVE_FAILURE_EXPECTED_NOTIFICATION_SUMMARY:-Voice Input}"
expected_switch_body_prefix="${VINPST_LIVE_FAILURE_EXPECTED_SWITCH_BODY_PREFIX:-}"
expected_switch_body_suffix="${VINPST_LIVE_FAILURE_EXPECTED_SWITCH_BODY_SUFFIX:-}"
if [[ -z "${expected_switch_body_prefix}" ]]; then
  expected_switch_body_prefix="ASR switch requested for '"
fi
if [[ -z "${expected_switch_body_suffix}" ]]; then
  expected_switch_body_suffix="'."
fi
fcitx_settle_seconds="${VINPST_LIVE_FCITX_SETTLE_SECONDS:-1}"
trigger_key="${VINPST_LIVE_ASR_MENU_KEY:-F8}"
recognition_wav="${VINPST_LIVE_FAILURE_RECOVERY_WAV:-${repo_root}/target/models/onnx-zf-ctc-zh-sm-int8-stream/test_wavs/0.wav}"
config_path=""
profile_mutated=0
backup_existed=0
monitor_pid=""
fcitx_restart_needed=0
before_pid=""
before_provider=""
before_model=""
fcitx_pid=""

stop_monitor() {
  if [[ -n "${monitor_pid}" ]]; then
    kill -TERM "${monitor_pid}" 2>/dev/null || true
    wait "${monitor_pid}" 2>/dev/null || true
    monitor_pid=""
  fi
}

wait_original_backend() {
  local output_path="$1"
  for _ in $(seq 1 300); do
    if "${cli_binary}" daemon status --json >"${output_path}" 2>/dev/null &&
      jq -e \
        --arg provider "${before_provider}" \
        --arg model "${before_model}" '
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
  echo "original ASR backend did not become ready" >&2
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
      { [[ -z "${previous_pid}" ]] || [[ ! -e "/proc/${previous_pid}" ]]; } &&
      fcitx5-remote --check >/dev/null 2>&1 &&
      grep -q "${HOME}/.local/lib/fcitx5/fcitx5-vinpst.so" "/proc/${pid}/maps"; then
      sleep "${fcitx_settle_seconds}"
      if [[ -e "/proc/${pid}" ]] && fcitx5-remote --check >/dev/null 2>&1 &&
        grep -q "${HOME}/.local/lib/fcitx5/fcitx5-vinpst.so" "/proc/${pid}/maps"; then
        printf '%s\n' "${pid}"
        return 0
      fi
    fi
    sleep 0.1
  done
  echo "Fcitx did not restart with the user-installed addon" >&2
  return 1
}

stop_fcitx() {
  local pid
  if ! pgrep -x fcitx5 >/dev/null 2>&1; then
    return 0
  fi
  fcitx5-remote -e >/dev/null 2>&1 || true
  for _ in $(seq 1 180); do
    if ! pgrep -x fcitx5 >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.1
  done
  pid="$(pgrep -n -x fcitx5 || true)"
  echo "Fcitx did not exit before addon configuration restoration: ${pid}" >&2
  return 1
}

restore_addon_config() {
  if [[ ! -f "${out_dir_abs}/addon-before.conf" ]]; then
    return 0
  fi
  stop_fcitx
  install -m 0644 "${out_dir_abs}/addon-before.conf" "${addon_config}"
  cmp "${out_dir_abs}/addon-before.conf" "${addon_config}"
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
  wait_original_backend "${out_dir_abs}/restored-status.json"
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
  stop_monitor
  if ! restore_profile; then
    exit_code=1
  fi
  if [[ "${fcitx_restart_needed}" == 1 ]]; then
    if ! restore_addon_config; then
      exit_code=1
    fi
    if ! restart_fcitx >"${out_dir_abs}/fcitx-cleanup.pid"; then
      exit_code=1
    fi
    fcitx_restart_needed=0
  fi
  cmp "${out_dir_abs}/service-before.conf" "${service_path}" || exit_code=1
  cmp "${out_dir_abs}/addon-before.conf" "${addon_config}" || exit_code=1
  find scripts -type d -name __pycache__ -prune -exec rm -rf {} +
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

for command in cmp dbus-monitor fcitx5 fcitx5-remote gdbus install jq pgrep python3 ruff; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required cross-provider failure command is missing: ${command}" >&2
    exit 2
  fi
done
for path in "${cli_binary}" "${selection_probe}" "${virtual_runner}" "${recognition_wav}" \
  "${service_path}" "${addon_config}"; do
  if [[ ! -e "${path}" ]]; then
    echo "cross-provider failure input is missing: ${path}" >&2
    exit 2
  fi
done
if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  echo "DBUS_SESSION_BUS_ADDRESS is not set" >&2
  exit 2
fi
if ! fcitx5-remote --check >/dev/null 2>&1; then
  echo "Fcitx5 is not running in this session" >&2
  exit 1
fi

ruff check "${selection_probe}"
ruff format --check "${selection_probe}"
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
  echo "ASR backend must be idle and ready before cross-provider failure testing" >&2
  cat "${out_dir_abs}/before.json" >&2
  exit 1
fi
before_pid="$(jq -r '.owner.unix_process_id' "${out_dir_abs}/before.json")"
before_provider="$(jq -r '.asr_backend.effective_provider_id' "${out_dir_abs}/before.json")"
before_model="$(jq -r '.asr_backend.effective_model_id' "${out_dir_abs}/before.json")"
config_path="$(jq -r '
  .owner.process.cmdline as $args |
  ($args | index("--config")) as $index |
  if $index == null then empty else $args[$index + 1] end
' "${out_dir_abs}/before.json")"
if [[ -z "${config_path}" || ! -f "${config_path}" ]]; then
  echo "could not resolve the active daemon config path" >&2
  exit 1
fi
fcitx_pid="$(pgrep -n -x fcitx5 || true)"
if [[ -z "${fcitx_pid}" ]]; then
  echo "could not resolve the current fcitx5 process" >&2
  exit 1
fi

install -m 0644 "${config_path}" "${out_dir_abs}/config-before.json"
install -m 0644 "${service_path}" "${out_dir_abs}/service-before.conf"
install -m 0644 "${addon_config}" "${out_dir_abs}/addon-before.conf"
if [[ -f "${config_path}.bak" ]]; then
  backup_existed=1
  install -m 0644 "${config_path}.bak" "${out_dir_abs}/config-backup-before.json"
fi

python3 - \
  "${out_dir_abs}/config-before.json" \
  "${out_dir_abs}/config-remote.json" \
  "${remote_provider}" \
  "${remote_model}" \
  "${remote_endpoint}" <<'PY'
import json
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
provider_id = sys.argv[3]
model_id = sys.argv[4]
endpoint = sys.argv[5]
config = json.loads(source.read_text(encoding="utf-8"))
if any(provider.get("id") == provider_id for provider in config["asr"]["providers"]):
    raise SystemExit(f"temporary provider id already exists: {provider_id}")
config["asr"]["providers"].append(
    {
        "id": provider_id,
        "type": "remote",
        "endpoint": endpoint,
        "model": model_id,
        "timeout_ms": 1000,
    }
)
target.write_text(
    json.dumps(config, indent=2, ensure_ascii=False) + "\n",
    encoding="utf-8",
)
PY
"${cli_binary}" config validate "${out_dir_abs}/config-remote.json" --json \
  | tee "${out_dir_abs}/config-validate.json"

install -m 0644 "${out_dir_abs}/config-remote.json" "${config_path}"
profile_mutated=1
"${cli_binary}" daemon reload-asr --json \
  | tee "${out_dir_abs}/provider-list-reload-call.json"
wait_original_backend "${out_dir_abs}/provider-list-ready.json"

# Reset addon-owned menu/filter state so repeated live gates cannot inherit a
# delayed InputPanel clear from an earlier input context.
fcitx_pid="$(restart_fcitx)"
printf '%s\n' "${fcitx_pid}" | tee "${out_dir_abs}/fcitx-before-selection.pid"
fcitx_restart_needed=1

# Capture both the daemon failure signal and the desktop notification generated by Fcitx.
dbus-monitor --session \
  "type='signal',interface='org.fcitx.Vinpst.Service',member='DaemonNotification'" \
  "type='method_call',interface='org.freedesktop.Notifications',member='Notify'" \
  >"${monitor_log}" 2>&1 &
monitor_pid=$!
for _ in $(seq 1 50); do
  if grep -q 'NameAcquired' "${monitor_log}" 2>/dev/null; then
    break
  fi
  if ! kill -0 "${monitor_pid}" 2>/dev/null; then
    echo "dbus-monitor exited before cross-provider failure capture" >&2
    cat "${monitor_log}" >&2 2>/dev/null || true
    exit 1
  fi
  sleep 0.05
done
if ! grep -q 'NameAcquired' "${monitor_log}"; then
  echo "dbus-monitor did not become ready" >&2
  exit 1
fi

python3 "${selection_probe}" \
  --trigger-key "${trigger_key}" \
  --expected-provider "${remote_provider}" \
  --expected-model "${remote_model}" \
  --expect-reload-failure \
  --timeout-ms 30000 \
  | tee "${out_dir_abs}/selection.jsonl"

failure_seen=0
for _ in $(seq 1 300); do
  if "${cli_binary}" daemon status --json >"${out_dir_abs}/failed-status.json" 2>/dev/null &&
    jq -e \
      --argjson owner_pid "${before_pid}" \
      --arg target_provider "${remote_provider}" \
      --arg target_model "${remote_model}" \
      --arg effective_provider "${before_provider}" \
      --arg effective_model "${before_model}" '
        .status == "idle" and
        .owner.ok == true and
        .owner.unix_process_id == $owner_pid and
        .asr_backend.has_effective_backend == true and
        .asr_backend.reload_in_progress == false and
        (.asr_backend.last_error | length) > 0 and
        .asr_backend.target_provider_id == $target_provider and
        .asr_backend.target_model_id == $target_model and
        .asr_backend.effective_provider_id == $effective_provider and
        .asr_backend.effective_model_id == $effective_model
      ' "${out_dir_abs}/failed-status.json" >/dev/null; then
    failure_seen=1
    break
  fi
  sleep 0.1
done
if [[ "${failure_seen}" != "1" ]]; then
  echo "remote provider reload did not fail while preserving the original backend" >&2
  cat "${out_dir_abs}/failed-status.json" >&2 2>/dev/null || true
  exit 1
fi
failure_error="$(jq -r '.asr_backend.last_error' "${out_dir_abs}/failed-status.json")"
if [[ "${failure_error}" != *"unsupported remote ASR endpoint scheme"* ]]; then
  echo "remote provider failure did not identify the unsupported endpoint scheme" >&2
  cat "${out_dir_abs}/failed-status.json" >&2
  exit 1
fi
sleep 0.5
stop_monitor

python3 - \
  "${monitor_log}" \
  "${out_dir_abs}/failed-status.json" \
  "${out_dir_abs}/notification.json" \
  "${expected_notification_summary}" \
  "${remote_model}" \
  "${expected_switch_body_prefix}" \
  "${expected_switch_body_suffix}" <<'PY'
import json
import re
import sys
from pathlib import Path

monitor_path = Path(sys.argv[1])
status_path = Path(sys.argv[2])
out_path = Path(sys.argv[3])
expected_summary = sys.argv[4]
target_model = sys.argv[5]
expected_switch_body = f"{sys.argv[6]}{target_model}{sys.argv[7]}"
expected_error = json.loads(status_path.read_text(encoding="utf-8"))["asr_backend"][
    "last_error"
]
blocks = re.split(r"(?=(?:signal|method call) time=)", monitor_path.read_text())
signals = []
notifications = []
for block in blocks:
    if "interface=org.fcitx.Vinpst.Service; member=DaemonNotification" in block:
        header = block.splitlines()[0]
        sender_match = re.search(r"sender=([^ ]+)", header)
        strings = [
            json.loads(value)
            for value in re.findall(r'^\s+string (".*")$', block, re.MULTILINE)
        ]
        if sender_match is not None and len(strings) >= 4:
            signals.append(
                {
                    "sender": sender_match.group(1),
                    "code": strings[0],
                    "subject": strings[1],
                    "detail": strings[2],
                    "raw_message": strings[3],
                }
            )
    if "interface=org.freedesktop.Notifications; member=Notify" in block:
        header = block.splitlines()[0]
        sender_match = re.search(r"sender=([^ ]+)", header)
        strings = [
            json.loads(value)
            for value in re.findall(r'^\s+string (".*")$', block, re.MULTILINE)
        ]
        timeout_match = re.search(r"^\s+int32 (-?\d+)$", block, re.MULTILINE)
        if sender_match is not None and len(strings) >= 4 and timeout_match is not None:
            notification = {
                "sender": sender_match.group(1),
                "app_name": strings[0],
                "icon": strings[1],
                "summary": strings[2],
                "body": strings[3],
                "timeout_ms": int(timeout_match.group(1)),
            }
            if notification["app_name"] == "fcitx5-vinpst":
                notifications.append(notification)

error_signals = [signal for signal in signals if signal["code"] == "asr_backend_reload_failed"]
info_notifications = [
    notification
    for notification in notifications
    if notification["icon"] == "dialog-information"
]
error_notifications = [
    notification
    for notification in notifications
    if notification["icon"] == "dialog-error"
]
if len(error_signals) != 1:
    raise SystemExit(f"expected one ASR reload failure signal, found {len(error_signals)}")
if len(info_notifications) != 1:
    raise SystemExit(f"expected one Fcitx switch notification, found {len(info_notifications)}")
if len(error_notifications) != 1:
    raise SystemExit(f"expected one Fcitx error notification, found {len(error_notifications)}")
signal = error_signals[0]
info_notification = info_notifications[0]
error_notification = error_notifications[0]
failures = []
if signal["raw_message"] != expected_error:
    failures.append("daemon notification did not match runtime last_error")
if info_notification["summary"] != expected_summary:
    failures.append("ASR switch notification summary did not match the expected locale")
if info_notification["body"] != expected_switch_body:
    failures.append("ASR switch notification body did not match the expected locale")
if info_notification["timeout_ms"] != 3000:
    failures.append("ASR switch notification timeout was not 3000 ms")
if error_notification["summary"] != expected_summary:
    failures.append("error notification summary did not match the expected locale")
if error_notification["body"] != expected_error:
    failures.append("desktop notification did not preserve runtime last_error")
if error_notification["timeout_ms"] != 5000:
    failures.append("error notification timeout was not 5000 ms")
result = {
    "event": "notification",
    "daemon_sender": signal["sender"],
    "fcitx_sender": error_notification["sender"],
    "info_fcitx_sender": info_notification["sender"],
    "error_fcitx_sender": error_notification["sender"],
    "code": signal["code"],
    "error": expected_error,
    "expected_summary": expected_summary,
    "switch": {
        "app_name": info_notification["app_name"],
        "icon": info_notification["icon"],
        "summary": info_notification["summary"],
        "body": info_notification["body"],
        "expected_body": expected_switch_body,
        "timeout_ms": info_notification["timeout_ms"],
    },
    "failure": {
        "app_name": error_notification["app_name"],
        "icon": error_notification["icon"],
        "summary": error_notification["summary"],
        "body_preserved": error_notification["body"] == expected_error,
        "timeout_ms": error_notification["timeout_ms"],
    },
    "ok": not failures,
    "failures": failures,
}
out_path.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
print(json.dumps(result, ensure_ascii=False))
if failures:
    raise SystemExit("; ".join(failures))
PY

daemon_sender="$(jq -r '.daemon_sender' "${out_dir_abs}/notification.json")"
info_fcitx_sender="$(jq -r '.info_fcitx_sender' "${out_dir_abs}/notification.json")"
error_fcitx_sender="$(jq -r '.error_fcitx_sender' "${out_dir_abs}/notification.json")"
daemon_sender_pid="$(gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.GetConnectionUnixProcessID "${daemon_sender}" \
  | python3 -c 'import re,sys; match=re.search(r"uint32 (\d+)", sys.stdin.read()); print(match.group(1) if match else "")')"
info_fcitx_sender_pid="$(gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.GetConnectionUnixProcessID "${info_fcitx_sender}" \
  | python3 -c 'import re,sys; match=re.search(r"uint32 (\d+)", sys.stdin.read()); print(match.group(1) if match else "")')"
error_fcitx_sender_pid="$(gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.GetConnectionUnixProcessID "${error_fcitx_sender}" \
  | python3 -c 'import re,sys; match=re.search(r"uint32 (\d+)", sys.stdin.read()); print(match.group(1) if match else "")')"
if [[ "${daemon_sender_pid}" != "${before_pid}" ]]; then
  echo "DaemonNotification sender was not the current vinpst-daemon" >&2
  exit 1
fi
if [[ "${info_fcitx_sender_pid}" != "${fcitx_pid}" ]]; then
  echo "ASR switch notification sender was not the current fcitx5 process" >&2
  exit 1
fi
if [[ "${error_fcitx_sender_pid}" != "${fcitx_pid}" ]]; then
  echo "error notification sender was not the current fcitx5 process" >&2
  exit 1
fi

restore_profile

VINPST_LIVE_NATIVE_WAV="${recognition_wav}" \
VINPST_LIVE_NATIVE_MODES=normal \
VINPST_LIVE_REQUIRE_PARTIAL=1 \
VINPST_LIVE_VIRTUAL_OUT_DIR="${out_dir_abs}/recovered-recognition" \
  "${virtual_runner}"

recovery_jsonl="${out_dir_abs}/recovered-recognition/fcitx/normal.jsonl"
recovery_partial_count="$(jq -s '[.[] | select(.event == "summary")][0].partial_count' "${recovery_jsonl}")"
recovery_commit="$(jq -r 'select(.event == "summary") | .commit' "${recovery_jsonl}")"
if [[ "${recovery_partial_count}" -le 0 || -z "${recovery_commit}" ]]; then
  echo "original backend did not recover with streaming recognition" >&2
  exit 1
fi

cmp "${out_dir_abs}/config-before.json" "${config_path}"
if [[ "${backup_existed}" == 1 ]]; then
  cmp "${out_dir_abs}/config-backup-before.json" "${config_path}.bak"
else
  test ! -e "${config_path}.bak"
fi
cmp "${out_dir_abs}/service-before.conf" "${service_path}"
restore_addon_config
restart_fcitx | tee "${out_dir_abs}/fcitx-restored.pid"
fcitx_restart_needed=0
"${cli_binary}" daemon status --json >"${out_dir_abs}/final-status.json"
if ! jq -e \
  --arg provider "${before_provider}" \
  --arg model "${before_model}" '
    .status == "idle" and
    .owner.ok == true and
    .asr_backend.has_effective_backend == true and
    .asr_backend.reload_in_progress == false and
    .asr_backend.last_error == "" and
    .asr_backend.target_provider_id == $provider and
    .asr_backend.target_model_id == $model and
    .asr_backend.effective_provider_id == $provider and
    .asr_backend.effective_model_id == $model
  ' "${out_dir_abs}/final-status.json" >/dev/null; then
  echo "final backend state did not match the original provider/model" >&2
  cat "${out_dir_abs}/final-status.json" >&2
  exit 1
fi

failure_error="$(jq -r '.asr_backend.last_error' "${out_dir_abs}/failed-status.json")"
selection_ok="$(jq -s '[.[] | select(.event == "summary")][0].ok' "${out_dir_abs}/selection.jsonl")"
selection_preserved="$(jq -s '[.[] | select(.event == "summary")][0].failure_preserved' "${out_dir_abs}/selection.jsonl")"
notification_ok="$(jq -r '.ok' "${out_dir_abs}/notification.json")"

jq -n \
  --arg remote_provider "${remote_provider}" \
  --arg remote_model "${remote_model}" \
  --arg endpoint "${remote_endpoint}" \
  --arg original_provider "${before_provider}" \
  --arg original_model "${before_model}" \
  --arg error "${failure_error}" \
  --arg recovery_commit "${recovery_commit}" \
  --argjson before_pid "${before_pid}" \
  --argjson daemon_sender_pid "${daemon_sender_pid}" \
  --argjson fcitx_pid "${fcitx_pid}" \
  --argjson info_fcitx_sender_pid "${info_fcitx_sender_pid}" \
  --argjson error_fcitx_sender_pid "${error_fcitx_sender_pid}" \
  --argjson recovery_partial_count "${recovery_partial_count}" \
  --argjson selection_ok "${selection_ok}" \
  --argjson selection_preserved "${selection_preserved}" \
  --argjson notification_ok "${notification_ok}" '
  {
    event: "summary",
    menu_selection: true,
    expected_failure: true,
    target: {
      provider: $remote_provider,
      model: $remote_model,
      kind: "remote",
      endpoint: $endpoint
    },
    failure: {
      error: $error,
      previous_backend_preserved: $selection_preserved,
      daemon_pid: $before_pid,
      daemon_sender_pid: $daemon_sender_pid,
      daemon_sender_verified: ($before_pid == $daemon_sender_pid),
      fcitx_pid: $fcitx_pid,
      info_fcitx_sender_pid: $info_fcitx_sender_pid,
      error_fcitx_sender_pid: $error_fcitx_sender_pid,
      info_fcitx_sender_verified: ($fcitx_pid == $info_fcitx_sender_pid),
      error_fcitx_sender_verified: ($fcitx_pid == $error_fcitx_sender_pid),
      notification: $notification_ok
    },
    recovery: {
      provider: $original_provider,
      model: $original_model,
      partial_count: $recovery_partial_count,
      commit: $recovery_commit,
      recognition: ($recovery_partial_count > 0 and ($recovery_commit | length) > 0)
    },
    profile_restored: true,
    backup_restored: true,
    service_unchanged: true,
    addon_config_unchanged: true,
    fcitx_restored: true,
    backend_restored: true,
    ok: (
      $selection_ok and
      $selection_preserved and
      $notification_ok and
      ($before_pid == $daemon_sender_pid) and
      ($fcitx_pid == $info_fcitx_sender_pid) and
      ($fcitx_pid == $error_fcitx_sender_pid) and
      ($recovery_partial_count > 0) and
      (($recovery_commit | length) > 0)
    )
  }' | tee "${out_dir_abs}/summary.json"
