#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

cli_binary="${VINPUT_LIVE_CLI_BINARY:-target/debug/vinput}"
out_dir="${VINPUT_LIVE_ERROR_NOTIFICATION_OUT_DIR:-target/tmp/ime-fcitx-error-notification-live}"
monitor_log="${out_dir}/dbus-monitor.log"
config_path=""
profile_mutated=0
backup_existed=0
monitor_pid=""
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

wait_ready_backend() {
  local output_path="$1"
  for _ in $(seq 1 300); do
    if "${cli_binary}" daemon status --json >"${output_path}" 2>/dev/null &&
      jq -e \
        --argjson owner_pid "${before_pid}" \
        --arg provider "${before_provider}" \
        --arg model "${before_model}" '
          .status == "idle" and
          .owner.ok == true and
          .owner.unix_process_id == $owner_pid and
          .asr_backend.has_effective_backend == true and
          .asr_backend.reload_in_progress == false and
          .asr_backend.last_error == "" and
          .asr_backend.effective_provider_id == $provider and
          .asr_backend.effective_model_id == $model
        ' "${output_path}" >/dev/null; then
      return 0
    fi
    sleep 0.1
  done
  echo "ASR backend did not return to the original ready state" >&2
  cat "${output_path}" >&2 2>/dev/null || true
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
  "${cli_binary}" daemon reload-asr --json \
    >"${out_dir}/restore-reload-call.json"
  wait_ready_backend "${out_dir}/restored-status.json"
  cmp "${out_dir}/config-before.json" "${config_path}"
  if [[ "${backup_existed}" == 1 ]]; then
    cmp "${out_dir}/config-backup-before.json" "${config_path}.bak"
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
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

for command in dbus-monitor fcitx5-remote gdbus jq python3 pgrep; do
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
fcitx_pid="$(pgrep -n -x fcitx5 || true)"
if [[ -z "${fcitx_pid}" ]]; then
  echo "could not resolve the current fcitx5 process" >&2
  exit 1
fi

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
"${cli_binary}" daemon status --json >"${out_dir}/before.json"
if ! jq -e '
  .status == "idle" and
  .owner.ok == true and
  .asr_backend.has_effective_backend == true and
  .asr_backend.reload_in_progress == false and
  .asr_backend.last_error == ""
' "${out_dir}/before.json" >/dev/null; then
  echo "ASR backend must be idle and ready before the failure probe" >&2
  cat "${out_dir}/before.json" >&2
  exit 1
fi
before_pid="$(jq -r '.owner.unix_process_id' "${out_dir}/before.json")"
before_provider="$(jq -r '.asr_backend.effective_provider_id' "${out_dir}/before.json")"
before_model="$(jq -r '.asr_backend.effective_model_id' "${out_dir}/before.json")"
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

missing_model="${out_dir}/missing-model"
rm -rf "${missing_model}"
python3 - "${out_dir}/config-before.json" "${out_dir}/config-invalid.json" \
  "${before_provider}" "${missing_model}" <<'PY'
import json
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
provider_id = sys.argv[3]
missing_model = sys.argv[4]
config = json.loads(source.read_text())
for provider in config["asr"]["providers"]:
    if provider["id"] == provider_id:
        provider["model"] = missing_model
        break
else:
    raise SystemExit(f"active provider not found: {provider_id}")
target.write_text(json.dumps(config, ensure_ascii=False, indent=2) + "\n")
PY
"${cli_binary}" config validate "${out_dir}/config-invalid.json" --json \
  | tee "${out_dir}/config-validate.json"

dbus-monitor --session \
  "type='signal',interface='org.fcitx.Vinput.Service',member='DaemonNotification'" \
  "type='method_call',interface='org.freedesktop.Notifications',member='Notify'" \
  >"${monitor_log}" 2>&1 &
monitor_pid=$!
for _ in $(seq 1 50); do
  if grep -q 'NameAcquired' "${monitor_log}" 2>/dev/null; then
    break
  fi
  if ! kill -0 "${monitor_pid}" 2>/dev/null; then
    echo "dbus-monitor exited before error notification capture" >&2
    cat "${monitor_log}" >&2 2>/dev/null || true
    exit 1
  fi
  sleep 0.05
done
if ! grep -q 'NameAcquired' "${monitor_log}"; then
  echo "dbus-monitor did not become ready" >&2
  exit 1
fi

install -m 0644 "${out_dir}/config-invalid.json" "${config_path}"
profile_mutated=1
"${cli_binary}" daemon reload-asr --json | tee "${out_dir}/failure-reload-call.json"

failure_seen=0
for _ in $(seq 1 300); do
  if "${cli_binary}" daemon status --json >"${out_dir}/failed-status.json" 2>/dev/null &&
    jq -e \
      --argjson owner_pid "${before_pid}" \
      --arg provider "${before_provider}" \
      --arg model "${before_model}" '
        .status == "idle" and
        .owner.ok == true and
        .owner.unix_process_id == $owner_pid and
        .asr_backend.has_effective_backend == true and
        .asr_backend.reload_in_progress == false and
        (.asr_backend.last_error | length) > 0 and
        .asr_backend.effective_provider_id == $provider and
        .asr_backend.effective_model_id == $model
      ' "${out_dir}/failed-status.json" >/dev/null; then
    failure_seen=1
    break
  fi
  sleep 0.1
done
if [[ "${failure_seen}" != "1" ]]; then
  echo "invalid ASR reload did not fail while preserving the previous backend" >&2
  cat "${out_dir}/failed-status.json" >&2 2>/dev/null || true
  exit 1
fi
sleep 0.5
stop_monitor
restore_profile

python3 - "${monitor_log}" "${out_dir}/failed-status.json" \
  "${out_dir}/notification.json" <<'PY'
import json
import re
import sys
from pathlib import Path

monitor_path = Path(sys.argv[1])
status_path = Path(sys.argv[2])
out_path = Path(sys.argv[3])
expected_error = json.loads(status_path.read_text())["asr_backend"]["last_error"]
blocks = re.split(r"(?=(?:signal|method call) time=)", monitor_path.read_text())
signals = []
notifications = []
for block in blocks:
    if "interface=org.fcitx.Vinput.Service; member=DaemonNotification" in block:
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
            if notification["app_name"] == "fcitx5-vinput":
                notifications.append(notification)

error_signals = [signal for signal in signals if signal["code"] == "asr_backend_reload_failed"]
error_notifications = [
    notification
    for notification in notifications
    if notification["icon"] == "dialog-error"
]
if len(error_signals) != 1:
    raise SystemExit(f"expected one ASR reload failure signal, found {len(error_signals)}")
if len(error_notifications) != 1:
    raise SystemExit(f"expected one Fcitx error notification, found {len(error_notifications)}")
signal = error_signals[0]
notification = error_notifications[0]
failures = []
if signal["raw_message"] != expected_error:
    failures.append("daemon notification did not match the runtime last_error")
if notification["body"] != expected_error:
    failures.append("desktop notification did not preserve the daemon error message")
if not notification["summary"]:
    failures.append("error notification summary was empty")
if notification["timeout_ms"] != 5000:
    failures.append("error notification timeout was not 5000 ms")
result = {
    "event": "notification",
    "daemon_sender": signal["sender"],
    "fcitx_sender": notification["sender"],
    "code": signal["code"],
    "error": expected_error,
    "app_name": notification["app_name"],
    "icon": notification["icon"],
    "summary": notification["summary"],
    "timeout_ms": notification["timeout_ms"],
    "ok": not failures,
    "failures": failures,
}
out_path.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
print(json.dumps(result, ensure_ascii=False))
if failures:
    raise SystemExit("; ".join(failures))
PY

daemon_sender="$(jq -r '.daemon_sender' "${out_dir}/notification.json")"
fcitx_sender="$(jq -r '.fcitx_sender' "${out_dir}/notification.json")"
daemon_sender_pid="$(gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.GetConnectionUnixProcessID "${daemon_sender}" \
  | python3 -c 'import re,sys; match=re.search(r"uint32 (\d+)", sys.stdin.read()); print(match.group(1) if match else "")')"
fcitx_sender_pid="$(gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.GetConnectionUnixProcessID "${fcitx_sender}" \
  | python3 -c 'import re,sys; match=re.search(r"uint32 (\d+)", sys.stdin.read()); print(match.group(1) if match else "")')"
if [[ "${daemon_sender_pid}" != "${before_pid}" ]]; then
  echo "DaemonNotification sender was not the current vinput-daemon" >&2
  exit 1
fi
if [[ "${fcitx_sender_pid}" != "${fcitx_pid}" ]]; then
  echo "desktop notification sender was not the current fcitx5 process" >&2
  exit 1
fi

jq \
  --argjson daemon_pid "${before_pid}" \
  --argjson daemon_sender_pid "${daemon_sender_pid}" \
  --argjson fcitx_pid "${fcitx_pid}" \
  --argjson fcitx_sender_pid "${fcitx_sender_pid}" \
  '. + {
    event: "summary",
    daemon_pid: $daemon_pid,
    daemon_sender_pid: $daemon_sender_pid,
    daemon_sender_verified: ($daemon_pid == $daemon_sender_pid),
    fcitx_pid: $fcitx_pid,
    fcitx_sender_pid: $fcitx_sender_pid,
    fcitx_sender_verified: ($fcitx_pid == $fcitx_sender_pid),
    profile_restored: true,
    backend_restored: true,
    ok: (.ok and ($daemon_pid == $daemon_sender_pid) and ($fcitx_pid == $fcitx_sender_pid))
  }' "${out_dir}/notification.json" | tee "${out_dir}/summary.json"
