#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

for command in dbus-run-session gdbus install jq readlink timeout; do
  command -v "${command}" >/dev/null
done

root="${repo_root}/target/tmp/daemon-handoff-smoke"
rm -rf "${root}"
mkdir -p "${root}"

cargo build -q -p vinput-cli --bin vinput -p vinput-daemon --bin vinput-daemon

install_pair() {
  local destination="$1"
  mkdir -p "${destination}/bin" "${destination}/config"
  install -m755 target/debug/vinput "${destination}/bin/vinput"
  install -m755 target/debug/vinput-daemon "${destination}/bin/vinput-daemon"
}

wait_for_status() {
  local cli="$1"
  local config_home="$2"
  for _ in $(seq 1 100); do
    if XDG_CONFIG_HOME="${config_home}" "${cli}" daemon status --json >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.05
  done
  return 1
}

# A current direct owner must remain untouched and must not invoke systemctl or kill.
current="${root}/current"
install_pair "${current}"
cat >"${current}/must-not-run" <<'SH'
#!/usr/bin/env bash
exit 97
SH
chmod +x "${current}/must-not-run"

VINPUT_HANDOFF_ROOT="${current}" timeout 20s dbus-run-session -- bash -euo pipefail <<'INNER'
root="${VINPUT_HANDOFF_ROOT}"
"${root}/bin/vinput-daemon" --dbus >"${root}/daemon.log" 2>&1 &
daemon_pid=$!
trap 'kill "${daemon_pid}" 2>/dev/null || true; wait "${daemon_pid}" 2>/dev/null || true' EXIT

for _ in $(seq 1 100); do
  if XDG_CONFIG_HOME="${root}/config" "${root}/bin/vinput" daemon status --json >/dev/null 2>&1; then
    break
  fi
  sleep 0.05
done

XDG_CONFIG_HOME="${root}/config" \
VINPUT_DAEMON_SYSTEMCTL="${root}/must-not-run" \
VINPUT_DAEMON_KILL="${root}/must-not-run" \
  "${root}/bin/vinput" daemon handoff --json >"${root}/handoff.json"
kill -0 "${daemon_pid}"
INNER

jq -e '
  .ok == true
  and .handoff_strategy == "not-needed"
  and .restart_required == false
  and .restart_attempted == false
  and .restart_performed == false
  and .will_mutate_user_service == false
  and .will_signal_owner == false
  and .verification.status == "not-needed"
' "${current}/handoff.json" >/dev/null

# An old systemd owner must reload the new unit metadata before restarting.
systemd_case="${root}/systemd"
install_pair "${systemd_case}/expected"
mkdir -p "${systemd_case}/old"
install -m755 target/debug/vinput-daemon "${systemd_case}/old/vinput-daemon"
cat >"${systemd_case}/systemctl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
case "$*" in
  '--user show --property MainPID --value vinput-daemon.service')
    printf '%s\n' "${VINPUT_HANDOFF_OLD_PID}"
    ;;
  '--user daemon-reload')
    printf '%s\n' "$*" >>"${VINPUT_HANDOFF_SYSTEMCTL_LOG}"
    ;;
  '--user restart vinput-daemon.service')
    printf '%s\n' "$*" >>"${VINPUT_HANDOFF_SYSTEMCTL_LOG}"
    kill -TERM "${VINPUT_HANDOFF_OLD_PID}"
    XDG_CONFIG_HOME="${VINPUT_HANDOFF_CONFIG_HOME}" \
      "${VINPUT_HANDOFF_EXPECTED_DAEMON}" --dbus \
      >"${VINPUT_HANDOFF_NEW_DAEMON_LOG}" 2>&1 &
    printf '%s\n' "$!" >"${VINPUT_HANDOFF_NEW_PID_FILE}"
    ;;
  *)
    printf 'unexpected systemctl arguments: %s\n' "$*" >&2
    exit 98
    ;;
esac
SH
chmod +x "${systemd_case}/systemctl"

VINPUT_HANDOFF_ROOT="${systemd_case}" timeout 25s dbus-run-session -- bash -euo pipefail <<'INNER'
root="${VINPUT_HANDOFF_ROOT}"
"${root}/old/vinput-daemon" --dbus >"${root}/old-daemon.log" 2>&1 &
old_pid=$!
cleanup() {
  kill "${old_pid}" 2>/dev/null || true
  if test -s "${root}/new.pid"; then
    new_pid="$(cat "${root}/new.pid")"
    kill "${new_pid}" 2>/dev/null || true
    wait "${new_pid}" 2>/dev/null || true
  fi
  wait "${old_pid}" 2>/dev/null || true
}
trap cleanup EXIT

for _ in $(seq 1 100); do
  if XDG_CONFIG_HOME="${root}/expected/config" \
    "${root}/expected/bin/vinput" daemon status --json >/dev/null 2>&1; then
    break
  fi
  sleep 0.05
done

XDG_CONFIG_HOME="${root}/expected/config" \
VINPUT_DAEMON_SYSTEMCTL="${root}/systemctl" \
VINPUT_HANDOFF_OLD_PID="${old_pid}" \
VINPUT_HANDOFF_SYSTEMCTL_LOG="${root}/systemctl.log" \
VINPUT_HANDOFF_CONFIG_HOME="${root}/expected/config" \
VINPUT_HANDOFF_EXPECTED_DAEMON="${root}/expected/bin/vinput-daemon" \
VINPUT_HANDOFF_NEW_DAEMON_LOG="${root}/new-daemon.log" \
VINPUT_HANDOFF_NEW_PID_FILE="${root}/new.pid" \
  "${root}/expected/bin/vinput" daemon handoff --json >"${root}/handoff.json"

test -s "${root}/new.pid"
new_pid="$(cat "${root}/new.pid")"
test "${new_pid}" != "${old_pid}"
kill -0 "${new_pid}"
INNER

mapfile -t systemctl_calls <"${systemd_case}/systemctl.log"
test "${systemctl_calls[0]}" = '--user daemon-reload'
test "${systemctl_calls[1]}" = '--user restart vinput-daemon.service'
expected_daemon="$(readlink -f "${systemd_case}/expected/bin/vinput-daemon")"
old_daemon="$(readlink -f "${systemd_case}/old/vinput-daemon")"
jq -e \
  --arg expected "${expected_daemon}" \
  --arg old "${old_daemon}" \
  --arg systemctl "${systemd_case}/systemctl" \
  '.ok == true
   and .handoff_strategy == "systemd-daemon-reload-and-restart"
   and .will_mutate_user_service == true
   and .will_signal_owner == false
   and .restart_attempted == true
   and .restart_performed == true
   and .before.handoff.owner_executable == $old
   and .systemd_probe.owner_matches_main_pid == true
   and .service_reload.command_argv == [$systemctl, "--user", "daemon-reload"]
   and .service_control.command_argv == [$systemctl, "--user", "restart", "vinput-daemon.service"]
   and .verification.status == "current-owner"
   and .after.handoff.owner_executable == $expected
   and .after.handoff.restart_recommended == false' \
  "${systemd_case}/handoff.json" >/dev/null

# An old direct owner must be signalled only after the identity/idle guard passes,
# then the updated D-Bus activation metadata must start the current daemon.
direct="${root}/direct"
install_pair "${direct}/expected"
mkdir -p "${direct}/old" "${direct}/share/dbus-1/services" "${direct}/home" "${direct}/runtime"
chmod 700 "${direct}/runtime"
install -m755 target/debug/vinput-daemon "${direct}/old/vinput-daemon"
cat >"${direct}/share/dbus-1/services/org.fcitx.Vinput.service" <<EOF
[D-BUS Service]
Name=org.fcitx.Vinput
Exec=${direct}/expected/bin/vinput-daemon --dbus --exit-when-executable-replaced
EOF
cat >"${direct}/systemctl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
test "$*" = '--user show --property MainPID --value vinput-daemon.service'
printf '0\n'
SH
cat >"${direct}/kill" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
test "$#" = 2
test "$1" = -TERM
test "$2" = "${VINPUT_HANDOFF_OLD_PID}"
printf '%s\n' "$*" >"${VINPUT_HANDOFF_KILL_LOG}"
/usr/bin/kill -TERM "$2"
SH
chmod +x "${direct}/systemctl" "${direct}/kill"

HOME="${direct}/home" \
XDG_DATA_HOME="${direct}/share" \
XDG_DATA_DIRS="${direct}/share" \
XDG_CONFIG_HOME="${direct}/expected/config" \
XDG_RUNTIME_DIR="${direct}/runtime" \
VINPUT_HANDOFF_ROOT="${direct}" \
  timeout 25s dbus-run-session -- bash -euo pipefail <<'INNER'
root="${VINPUT_HANDOFF_ROOT}"
"${root}/old/vinput-daemon" --dbus >"${root}/old-daemon.log" 2>&1 &
old_pid=$!
cleanup() {
  kill "${old_pid}" 2>/dev/null || true
  owner_reply="$(gdbus call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.GetConnectionUnixProcessID \
    org.fcitx.Vinput 2>/dev/null || true)"
  owner_pid="$(sed -n 's/.*uint32 \([0-9][0-9]*\).*/\1/p' <<<"${owner_reply}")"
  if [[ "${owner_pid}" =~ ^[0-9]+$ ]]; then
    kill "${owner_pid}" 2>/dev/null || true
  fi
  wait "${old_pid}" 2>/dev/null || true
}
trap cleanup EXIT

for _ in $(seq 1 100); do
  if "${root}/expected/bin/vinput" daemon status --json >/dev/null 2>&1; then
    break
  fi
  sleep 0.05
done

VINPUT_DAEMON_SYSTEMCTL="${root}/systemctl" \
VINPUT_DAEMON_KILL="${root}/kill" \
VINPUT_HANDOFF_OLD_PID="${old_pid}" \
VINPUT_HANDOFF_KILL_LOG="${root}/kill.log" \
  "${root}/expected/bin/vinput" daemon handoff --json >"${root}/handoff.json"

new_pid="$(jq -r '.after.owner.unix_process_id' "${root}/handoff.json")"
test "${new_pid}" != "${old_pid}"
kill -0 "${new_pid}"
INNER

test "$(cat "${direct}/kill.log")" = "-TERM $(jq -r '.before.owner.unix_process_id' "${direct}/handoff.json")"
direct_expected="$(readlink -f "${direct}/expected/bin/vinput-daemon")"
direct_old="$(readlink -f "${direct}/old/vinput-daemon")"
jq -e \
  --arg expected "${direct_expected}" \
  --arg old "${direct_old}" \
  --arg kill "${direct}/kill" \
  '.ok == true
   and .handoff_strategy == "direct-owner-terminate-and-reactivate"
   and .will_mutate_user_service == false
   and .will_signal_owner == true
   and .restart_attempted == true
   and .restart_performed == true
   and .before.handoff.owner_executable == $old
   and .systemd_probe.main_pid == null
   and .direct_guard.approved == true
   and .direct_guard.same_uid == true
   and .direct_guard.status_idle == true
   and .direct_guard.active_session == false
   and .direct_guard.systemd_unit_detected == false
   and .dbus_reload.ok == true
   and .direct_revalidation.ok == true
   and .direct_revalidation.uid_matches == true
   and .direct_revalidation.start_time_matches == true
   and .direct_revalidation.executable_matches == true
   and .direct_signal.command_argv == [$kill, "-TERM", (.before.owner.unix_process_id | tostring)]
   and .verification.status == "current-owner"
   and .after.handoff.owner_executable == $expected
   and .after.handoff.restart_recommended == false' \
  "${direct}/handoff.json" >/dev/null

# A systemd metadata reload failure must leave the old owner running.
failure="${root}/failure"
install_pair "${failure}/expected"
mkdir -p "${failure}/old"
install -m755 target/debug/vinput-daemon "${failure}/old/vinput-daemon"
cat >"${failure}/systemctl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
case "$*" in
  '--user show --property MainPID --value vinput-daemon.service')
    printf '%s\n' "${VINPUT_HANDOFF_OLD_PID}"
    ;;
  '--user daemon-reload')
    exit 19
    ;;
  *)
    exit 98
    ;;
esac
SH
chmod +x "${failure}/systemctl"

VINPUT_HANDOFF_ROOT="${failure}" timeout 20s dbus-run-session -- bash -euo pipefail <<'INNER'
root="${VINPUT_HANDOFF_ROOT}"
"${root}/old/vinput-daemon" --dbus >"${root}/old-daemon.log" 2>&1 &
old_pid=$!
trap 'kill "${old_pid}" 2>/dev/null || true; wait "${old_pid}" 2>/dev/null || true' EXIT

for _ in $(seq 1 100); do
  if XDG_CONFIG_HOME="${root}/expected/config" \
    "${root}/expected/bin/vinput" daemon status --json >/dev/null 2>&1; then
    break
  fi
  sleep 0.05
done

XDG_CONFIG_HOME="${root}/expected/config" \
VINPUT_DAEMON_SYSTEMCTL="${root}/systemctl" \
VINPUT_HANDOFF_OLD_PID="${old_pid}" \
  "${root}/expected/bin/vinput" daemon handoff --json >"${root}/handoff.json"
kill -0 "${old_pid}"
INNER

jq -e '
  .ok == false
  and .handoff_strategy == "systemd-daemon-reload-and-restart"
  and .restart_attempted == false
  and .restart_performed == false
  and .service_reload.ok == false
  and .service_reload.exit_status == 19
  and .service_control == null
  and .verification.status == "daemon-reload-failed"
' "${failure}/handoff.json" >/dev/null

echo "daemon guarded upgrade handoff smoke passed"
