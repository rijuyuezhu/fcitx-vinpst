#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

for command in dbus-run-session gdbus jq nohup readlink timeout; do
  command -v "${command}" >/dev/null
done

stage_root="${repo_root}/target/tmp/daemon-handoff-smoke"
current_root="${stage_root}/current"
stale_root="${stage_root}/stale"
failure_root="${stage_root}/failure"
rm -rf "${stage_root}"
mkdir -p \
  "${current_root}/bin" "${current_root}/config" \
  "${stale_root}/expected/bin" "${stale_root}/old" "${stale_root}/config" \
  "${failure_root}/config"

cargo build -q -p vinput-cli --bin vinput -p vinput-daemon --bin vinput-daemon

install -Dm755 target/debug/vinput "${current_root}/bin/vinput"
install -Dm755 target/debug/vinput-daemon "${current_root}/bin/vinput-daemon"
install -Dm755 target/debug/vinput "${stale_root}/expected/bin/vinput"
install -Dm755 target/debug/vinput-daemon "${stale_root}/expected/bin/vinput-daemon"
install -Dm755 target/debug/vinput-daemon "${stale_root}/old/vinput-daemon"

cat >"${current_root}/systemctl-must-not-run" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >"${VINPUT_HANDOFF_SYSTEMCTL_MARKER}"
exit 97
SH
chmod +x "${current_root}/systemctl-must-not-run"

VINPUT_HANDOFF_CLI="${current_root}/bin/vinput" \
VINPUT_HANDOFF_DAEMON="${current_root}/bin/vinput-daemon" \
VINPUT_HANDOFF_SYSTEMCTL="${current_root}/systemctl-must-not-run" \
VINPUT_HANDOFF_SYSTEMCTL_MARKER="${current_root}/systemctl.marker" \
VINPUT_HANDOFF_CONFIG_HOME="${current_root}/config" \
VINPUT_HANDOFF_OUTPUT="${current_root}/handoff.json" \
  timeout 20s dbus-run-session -- bash -euo pipefail <<'INNER'
XDG_CONFIG_HOME="${VINPUT_HANDOFF_CONFIG_HOME}" \
  "${VINPUT_HANDOFF_DAEMON}" --dbus >"${VINPUT_HANDOFF_OUTPUT}.daemon.log" 2>&1 &
daemon_pid=$!
cleanup() {
  kill "${daemon_pid}" 2>/dev/null || true
  wait "${daemon_pid}" 2>/dev/null || true
}
trap cleanup EXIT

ready=0
for _ in $(seq 1 100); do
  if XDG_CONFIG_HOME="${VINPUT_HANDOFF_CONFIG_HOME}" \
    "${VINPUT_HANDOFF_CLI}" daemon status --json >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 0.05
done
test "${ready}" = 1

XDG_CONFIG_HOME="${VINPUT_HANDOFF_CONFIG_HOME}" \
VINPUT_DAEMON_SYSTEMCTL="${VINPUT_HANDOFF_SYSTEMCTL}" \
VINPUT_HANDOFF_SYSTEMCTL_MARKER="${VINPUT_HANDOFF_SYSTEMCTL_MARKER}" \
  "${VINPUT_HANDOFF_CLI}" daemon handoff --json >"${VINPUT_HANDOFF_OUTPUT}"
XDG_CONFIG_HOME="${VINPUT_HANDOFF_CONFIG_HOME}" \
VINPUT_DAEMON_SYSTEMCTL="${VINPUT_HANDOFF_SYSTEMCTL}" \
VINPUT_HANDOFF_SYSTEMCTL_MARKER="${VINPUT_HANDOFF_SYSTEMCTL_MARKER}" \
  "${VINPUT_HANDOFF_CLI}" daemon handoff >"${VINPUT_HANDOFF_OUTPUT}.txt"
! test -e "${VINPUT_HANDOFF_SYSTEMCTL_MARKER}"
kill -0 "${daemon_pid}"
INNER

current_expected="$(readlink -f "${current_root}/bin/vinput-daemon")"
jq -e \
  --arg expected "${current_expected}" \
  '.ok == true
   and .restart_required == false
   and .restart_attempted == false
   and .restart_performed == false
   and .will_mutate_user_service == false
   and .before.handoff.path_matches == true
   and .before.handoff.expected_executable == $expected
   and .verification.status == "not-needed"
   and .after.handoff.path_matches == true' \
  "${current_root}/handoff.json" >/dev/null
grep -qx 'action: handoff' "${current_root}/handoff.json.txt"
grep -qx 'ok: true' "${current_root}/handoff.json.txt"
grep -qx 'will_mutate_user_service: false' "${current_root}/handoff.json.txt"
grep -qx 'restart_required: false' "${current_root}/handoff.json.txt"
grep -qx 'restart_attempted: false' "${current_root}/handoff.json.txt"
grep -qx 'restart_performed: false' "${current_root}/handoff.json.txt"
grep -qx 'verification_status: not-needed' "${current_root}/handoff.json.txt"

cat >"${stale_root}/systemctl-restart" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

test "$#" = 3
test "$1" = --user
test "$2" = restart
test "$3" = vinput-daemon.service
printf '%s\n' "$*" >"${VINPUT_HANDOFF_SYSTEMCTL_MARKER}"
kill "${VINPUT_HANDOFF_OLD_PID}"

XDG_CONFIG_HOME="${VINPUT_HANDOFF_CONFIG_HOME}" \
  nohup "${VINPUT_HANDOFF_EXPECTED_DAEMON}" --dbus \
  >"${VINPUT_HANDOFF_NEW_DAEMON_LOG}" 2>&1 &
new_pid=$!
printf '%s\n' "${new_pid}" >"${VINPUT_HANDOFF_NEW_PID_FILE}"

for _ in $(seq 1 100); do
  pid_reply="$(gdbus call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.GetConnectionUnixProcessID \
    org.fcitx.Vinput 2>/dev/null || true)"
  if [[ "${pid_reply}" == *"uint32 ${new_pid}"* ]]; then
    exit 0
  fi
  sleep 0.05
done
exit 1
SH
chmod +x "${stale_root}/systemctl-restart"

VINPUT_HANDOFF_CLI="${stale_root}/expected/bin/vinput" \
VINPUT_HANDOFF_EXPECTED_DAEMON="${stale_root}/expected/bin/vinput-daemon" \
VINPUT_HANDOFF_OLD_DAEMON="${stale_root}/old/vinput-daemon" \
VINPUT_HANDOFF_SYSTEMCTL="${stale_root}/systemctl-restart" \
VINPUT_HANDOFF_SYSTEMCTL_MARKER="${stale_root}/systemctl.marker" \
VINPUT_HANDOFF_CONFIG_HOME="${stale_root}/config" \
VINPUT_HANDOFF_OUTPUT="${stale_root}/handoff.json" \
VINPUT_HANDOFF_NEW_PID_FILE="${stale_root}/new.pid" \
VINPUT_HANDOFF_NEW_DAEMON_LOG="${stale_root}/new-daemon.log" \
  timeout 30s dbus-run-session -- bash -euo pipefail <<'INNER'
XDG_CONFIG_HOME="${VINPUT_HANDOFF_CONFIG_HOME}" \
  "${VINPUT_HANDOFF_OLD_DAEMON}" --dbus >"${VINPUT_HANDOFF_OUTPUT}.old-daemon.log" 2>&1 &
old_pid=$!
cleanup() {
  kill "${old_pid}" 2>/dev/null || true
  if test -s "${VINPUT_HANDOFF_NEW_PID_FILE}"; then
    new_pid="$(cat "${VINPUT_HANDOFF_NEW_PID_FILE}")"
    kill "${new_pid}" 2>/dev/null || true
    wait "${new_pid}" 2>/dev/null || true
  fi
  wait "${old_pid}" 2>/dev/null || true
}
trap cleanup EXIT

ready=0
for _ in $(seq 1 100); do
  if XDG_CONFIG_HOME="${VINPUT_HANDOFF_CONFIG_HOME}" \
    "${VINPUT_HANDOFF_CLI}" daemon status --json >"${VINPUT_HANDOFF_OUTPUT}.before" 2>/dev/null; then
    ready=1
    break
  fi
  sleep 0.05
done
test "${ready}" = 1
jq -e \
  '.handoff.restart_recommended == true
   and .handoff.reason == "owner-executable-path-mismatch"' \
  "${VINPUT_HANDOFF_OUTPUT}.before" >/dev/null

XDG_CONFIG_HOME="${VINPUT_HANDOFF_CONFIG_HOME}" \
VINPUT_DAEMON_SYSTEMCTL="${VINPUT_HANDOFF_SYSTEMCTL}" \
VINPUT_HANDOFF_SYSTEMCTL_MARKER="${VINPUT_HANDOFF_SYSTEMCTL_MARKER}" \
VINPUT_HANDOFF_OLD_PID="${old_pid}" \
VINPUT_HANDOFF_EXPECTED_DAEMON="${VINPUT_HANDOFF_EXPECTED_DAEMON}" \
VINPUT_HANDOFF_CONFIG_HOME="${VINPUT_HANDOFF_CONFIG_HOME}" \
VINPUT_HANDOFF_NEW_PID_FILE="${VINPUT_HANDOFF_NEW_PID_FILE}" \
VINPUT_HANDOFF_NEW_DAEMON_LOG="${VINPUT_HANDOFF_NEW_DAEMON_LOG}" \
  "${VINPUT_HANDOFF_CLI}" daemon handoff --json >"${VINPUT_HANDOFF_OUTPUT}"

test "$(cat "${VINPUT_HANDOFF_SYSTEMCTL_MARKER}")" = \
  '--user restart vinput-daemon.service'
test -s "${VINPUT_HANDOFF_NEW_PID_FILE}"
new_pid="$(cat "${VINPUT_HANDOFF_NEW_PID_FILE}")"
test "${new_pid}" != "${old_pid}"
kill -0 "${new_pid}"
INNER

stale_expected="$(readlink -f "${stale_root}/expected/bin/vinput-daemon")"
stale_old="$(readlink -f "${stale_root}/old/vinput-daemon")"
jq -e \
  --arg expected "${stale_expected}" \
  --arg old "${stale_old}" \
  --arg systemctl "${stale_root}/systemctl-restart" \
  '.ok == true
   and .restart_required == true
   and .restart_attempted == true
   and .restart_performed == true
   and .will_mutate_user_service == true
   and .before.handoff.reason == "owner-executable-path-mismatch"
   and .before.handoff.owner_executable == $old
   and .service_control.ok == true
   and .service_control.command_argv == [$systemctl, "--user", "restart", "vinput-daemon.service"]
   and .verification.ok == true
   and .verification.status == "current-owner"
   and .verification.attempts >= 1
   and .after.handoff.expected_executable == $expected
   and .after.handoff.owner_executable == $expected
   and .after.handoff.path_matches == true
   and .after.handoff.owner_executable_deleted == false
   and .after.handoff.restart_recommended == false' \
  "${stale_root}/handoff.json" >/dev/null

cat >"${failure_root}/systemctl-fail" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >"${VINPUT_HANDOFF_SYSTEMCTL_MARKER}"
exit 19
SH
chmod +x "${failure_root}/systemctl-fail"

VINPUT_HANDOFF_CLI="${stale_root}/expected/bin/vinput" \
VINPUT_HANDOFF_OLD_DAEMON="${stale_root}/old/vinput-daemon" \
VINPUT_HANDOFF_SYSTEMCTL="${failure_root}/systemctl-fail" \
VINPUT_HANDOFF_SYSTEMCTL_MARKER="${failure_root}/systemctl.marker" \
VINPUT_HANDOFF_CONFIG_HOME="${failure_root}/config" \
VINPUT_HANDOFF_OUTPUT="${failure_root}/handoff.json" \
  timeout 20s dbus-run-session -- bash -euo pipefail <<'INNER'
XDG_CONFIG_HOME="${VINPUT_HANDOFF_CONFIG_HOME}" \
  "${VINPUT_HANDOFF_OLD_DAEMON}" --dbus >"${VINPUT_HANDOFF_OUTPUT}.daemon.log" 2>&1 &
old_pid=$!
cleanup() {
  kill "${old_pid}" 2>/dev/null || true
  wait "${old_pid}" 2>/dev/null || true
}
trap cleanup EXIT

ready=0
for _ in $(seq 1 100); do
  if XDG_CONFIG_HOME="${VINPUT_HANDOFF_CONFIG_HOME}" \
    "${VINPUT_HANDOFF_CLI}" daemon status --json >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 0.05
done
test "${ready}" = 1

XDG_CONFIG_HOME="${VINPUT_HANDOFF_CONFIG_HOME}" \
VINPUT_DAEMON_SYSTEMCTL="${VINPUT_HANDOFF_SYSTEMCTL}" \
VINPUT_HANDOFF_SYSTEMCTL_MARKER="${VINPUT_HANDOFF_SYSTEMCTL_MARKER}" \
  "${VINPUT_HANDOFF_CLI}" daemon handoff --json >"${VINPUT_HANDOFF_OUTPUT}"
test "$(cat "${VINPUT_HANDOFF_SYSTEMCTL_MARKER}")" = \
  '--user restart vinput-daemon.service'
kill -0 "${old_pid}"
INNER

jq -e \
  '.ok == false
   and .restart_required == true
   and .restart_attempted == true
   and .restart_performed == false
   and .service_control.ok == false
   and .service_control.exit_status == 19
   and .verification.status == "restart-failed"
   and .verification.attempts == 0
   and .after == null' \
  "${failure_root}/handoff.json" >/dev/null

echo "daemon conditional handoff smoke passed"
