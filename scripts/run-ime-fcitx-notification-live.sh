#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

out_dir="${VINPUT_LIVE_NOTIFICATION_OUT_DIR:-target/tmp/ime-fcitx-notification-live}"
monitor_log="${out_dir}/dbus-monitor.log"
selection_log="${out_dir}/scene-selection.jsonl"
notification_json="${out_dir}/notification.json"
monitor_pid=""

stop_monitor() {
  if [[ -n "${monitor_pid}" ]]; then
    kill -TERM "${monitor_pid}" 2>/dev/null || true
    wait "${monitor_pid}" 2>/dev/null || true
    monitor_pid=""
  fi
}

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM
  stop_monitor
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

for command in dbus-monitor fcitx5-remote gdbus jq python3 pgrep; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required command is missing: ${command}" >&2
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
fcitx_pid="$(pgrep -n -x fcitx5 || true)"
if [[ -z "${fcitx_pid}" ]]; then
  echo "could not resolve the current fcitx5 process" >&2
  exit 1
fi

rm -rf "${out_dir}"
mkdir -p "${out_dir}"

dbus-monitor --session \
  "type='method_call',interface='org.freedesktop.Notifications',member='Notify'" \
  >"${monitor_log}" 2>&1 &
monitor_pid=$!
for _ in $(seq 1 50); do
  if grep -q 'NameAcquired' "${monitor_log}" 2>/dev/null; then
    break
  fi
  if ! kill -0 "${monitor_pid}" 2>/dev/null; then
    echo "dbus-monitor exited before notification capture" >&2
    cat "${monitor_log}" >&2 2>/dev/null || true
    exit 1
  fi
  sleep 0.05
done
if ! grep -q 'NameAcquired' "${monitor_log}"; then
  echo "dbus-monitor did not become ready" >&2
  exit 1
fi

VINPUT_LIVE_MENU_SELECTION_OUT_DIR="${out_dir}/selection-runner" \
  scripts/run-ime-fcitx-menu-selection-live.sh | tee "${selection_log}"
sleep 0.3
stop_monitor

python3 - "${monitor_log}" "${selection_log}" "${notification_json}" <<'PY'
import json
import re
import sys
from pathlib import Path

monitor_path = Path(sys.argv[1])
selection_path = Path(sys.argv[2])
out_path = Path(sys.argv[3])

target_label = None
for line in selection_path.read_text().splitlines():
    event = json.loads(line)
    if event.get("event") == "selection-target":
        target_label = event.get("target_label")
        break
if not isinstance(target_label, str) or not target_label:
    raise SystemExit("selection log did not expose a target label")

blocks = re.split(r"(?=method call time=)", monitor_path.read_text())
matches = []
for block in blocks:
    if "interface=org.freedesktop.Notifications; member=Notify" not in block:
        continue
    header = block.splitlines()[0]
    sender_match = re.search(r"sender=([^ ]+)", header)
    strings = [json.loads(value) for value in re.findall(r'^\s+string (".*")$', block, re.MULTILINE)]
    uint_match = re.search(r"^\s+uint32 (\d+)$", block, re.MULTILINE)
    timeout_match = re.search(r"^\s+int32 (-?\d+)$", block, re.MULTILINE)
    if sender_match is None or len(strings) < 4 or uint_match is None or timeout_match is None:
        continue
    notification = {
        "sender": sender_match.group(1),
        "app_name": strings[0],
        "replaces_id": int(uint_match.group(1)),
        "icon": strings[1],
        "summary": strings[2],
        "body": strings[3],
        "timeout_ms": int(timeout_match.group(1)),
        "target_label": target_label,
    }
    if notification["app_name"] == "fcitx5-vinput":
        matches.append(notification)

if len(matches) != 1:
    raise SystemExit(f"expected one fcitx5-vinput notification, found {len(matches)}")
notification = matches[0]
failures = []
if notification["replaces_id"] != 0:
    failures.append("notification unexpectedly replaced an existing id")
if notification["icon"] != "dialog-information":
    failures.append("scene switch notification did not use the information icon")
if not notification["summary"]:
    failures.append("notification summary was empty")
if notification["target_label"] not in notification["body"]:
    failures.append("notification body did not contain the selected scene label")
if notification["timeout_ms"] != 3000:
    failures.append("information notification timeout was not 3000 ms")
notification["ok"] = not failures
notification["failures"] = failures
out_path.write_text(json.dumps(notification, ensure_ascii=False, indent=2) + "\n")
print(json.dumps({"event": "notification", **notification}, ensure_ascii=False))
if failures:
    raise SystemExit("; ".join(failures))
PY

sender="$(jq -r '.sender' "${notification_json}")"
sender_pid="$(gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.GetConnectionUnixProcessID "${sender}" \
  | python3 -c 'import re,sys; match=re.search(r"uint32 (\d+)", sys.stdin.read()); print(match.group(1) if match else "")')"
if [[ "${sender_pid}" != "${fcitx_pid}" ]]; then
  echo "notification sender was not the current fcitx5 process: sender=${sender} sender_pid=${sender_pid} fcitx_pid=${fcitx_pid}" >&2
  exit 1
fi

jq --argjson fcitx_pid "${fcitx_pid}" \
  --argjson sender_pid "${sender_pid}" \
  '. + {
    event: "summary",
    fcitx_pid: $fcitx_pid,
    sender_pid: $sender_pid,
    sender_is_fcitx: ($fcitx_pid == $sender_pid),
    ok: (.ok and ($fcitx_pid == $sender_pid))
  }' "${notification_json}" | tee "${out_dir}/summary.json"
