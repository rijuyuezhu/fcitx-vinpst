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
source scripts/tests/dbus-session-common.sh

for command in dbus-run-session gdbus install jq readlink timeout; do
  command -v "${command}" >/dev/null
done

root="${repo_root}/target/tmp/daemon-handoff-smoke"
rm -rf "${root}"
mkdir -p "${root}/dbus-services"
dbus_config="${root}/session.conf"
write_isolated_dbus_session_config "${dbus_config}" "${root}/dbus-services"

cargo build -q -p vinpst-cli --bin vinpst -p vinpst-daemon --bin vinpst-daemon

install_pair() {
  local destination="$1"
  mkdir -p "${destination}/bin" "${destination}/config"
  install -m755 target/debug/vinpst "${destination}/bin/vinpst"
  install -m755 target/debug/vinpst-daemon "${destination}/bin/vinpst-daemon"
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

VINPST_HANDOFF_ROOT="${current}" timeout 20s dbus-run-session --config-file="${dbus_config}" -- bash -euo pipefail <<'INNER'
root="${VINPST_HANDOFF_ROOT}"
"${root}/bin/vinpst-daemon" --dbus >"${root}/daemon.log" 2>&1 &
daemon_pid=$!
trap 'kill "${daemon_pid}" 2>/dev/null || true; wait "${daemon_pid}" 2>/dev/null || true' EXIT

for _ in $(seq 1 100); do
  if XDG_CONFIG_HOME="${root}/config" "${root}/bin/vinpst" daemon status --json >/dev/null 2>&1; then
    break
  fi
  sleep 0.05
done

XDG_CONFIG_HOME="${root}/config" \
VINPST_DAEMON_SYSTEMCTL="${root}/must-not-run" \
VINPST_DAEMON_KILL="${root}/must-not-run" \
  "${root}/bin/vinpst" daemon handoff --json >"${root}/handoff.json"
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
install -m755 target/debug/vinpst-daemon "${systemd_case}/old/vinpst-daemon"
cat >"${systemd_case}/systemctl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
case "$*" in
  '--user show --property MainPID --value vinpst-daemon.service')
    printf '%s\n' "${VINPST_HANDOFF_OLD_PID}"
    ;;
  '--user daemon-reload')
    printf '%s\n' "$*" >>"${VINPST_HANDOFF_SYSTEMCTL_LOG}"
    ;;
  '--user restart vinpst-daemon.service')
    printf '%s\n' "$*" >>"${VINPST_HANDOFF_SYSTEMCTL_LOG}"
    kill -TERM "${VINPST_HANDOFF_OLD_PID}"
    XDG_CONFIG_HOME="${VINPST_HANDOFF_CONFIG_HOME}" \
      "${VINPST_HANDOFF_EXPECTED_DAEMON}" --dbus \
      >"${VINPST_HANDOFF_NEW_DAEMON_LOG}" 2>&1 &
    printf '%s\n' "$!" >"${VINPST_HANDOFF_NEW_PID_FILE}"
    ;;
  *)
    printf 'unexpected systemctl arguments: %s\n' "$*" >&2
    exit 98
    ;;
esac
SH
chmod +x "${systemd_case}/systemctl"

VINPST_HANDOFF_ROOT="${systemd_case}" timeout 25s dbus-run-session --config-file="${dbus_config}" -- bash -euo pipefail <<'INNER'
root="${VINPST_HANDOFF_ROOT}"
"${root}/old/vinpst-daemon" --dbus >"${root}/old-daemon.log" 2>&1 &
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
    "${root}/expected/bin/vinpst" daemon status --json >/dev/null 2>&1; then
    break
  fi
  sleep 0.05
done

# A stale systemd owner must not be restarted while a recording is active.
XDG_CONFIG_HOME="${root}/expected/config" \
  "${root}/expected/bin/vinpst" recording start --json >"${root}/recording-start.json"
XDG_CONFIG_HOME="${root}/expected/config" \
  "${root}/expected/bin/vinpst" daemon status --json >"${root}/busy-status.json"
jq -e '.status == "recording" and .runtime_status.active_session == true' \
  "${root}/busy-status.json" >/dev/null
if XDG_CONFIG_HOME="${root}/expected/config" \
  VINPST_DAEMON_SYSTEMCTL="${root}/systemctl" \
  VINPST_HANDOFF_OLD_PID="${old_pid}" \
  VINPST_HANDOFF_SYSTEMCTL_LOG="${root}/systemctl.log" \
  VINPST_HANDOFF_CONFIG_HOME="${root}/expected/config" \
  VINPST_HANDOFF_EXPECTED_DAEMON="${root}/expected/bin/vinpst-daemon" \
  VINPST_HANDOFF_NEW_DAEMON_LOG="${root}/new-daemon.log" \
  VINPST_HANDOFF_NEW_PID_FILE="${root}/new.pid" \
    "${root}/expected/bin/vinpst" daemon handoff --json \
    >"${root}/busy-handoff.json" 2>"${root}/busy-handoff.err"; then
  echo "busy systemd owner unexpectedly accepted upgrade handoff" >&2
  exit 1
fi
kill -0 "${old_pid}"
test ! -e "${root}/systemctl.log"
jq -e '
  .ok == false
  and .handoff_strategy == "systemd-daemon-reload-and-restart"
  and .will_mutate_user_service == false
  and .will_signal_owner == false
  and .restart_attempted == false
  and .restart_performed == false
  and .systemd_probe.owner_matches_main_pid == true
  and .systemd_guard.approved == false
  and .systemd_guard.status_idle == false
  and .systemd_guard.active_session == true
  and .service_reload == null
  and .service_control == null
  and .verification.status == "systemd-owner-session-guard-rejected"
' "${root}/busy-handoff.json" >/dev/null

XDG_CONFIG_HOME="${root}/expected/config" \
  "${root}/expected/bin/vinpst" recording stop --json >"${root}/recording-stop.json"

XDG_CONFIG_HOME="${root}/expected/config" \
VINPST_DAEMON_SYSTEMCTL="${root}/systemctl" \
VINPST_HANDOFF_OLD_PID="${old_pid}" \
VINPST_HANDOFF_SYSTEMCTL_LOG="${root}/systemctl.log" \
VINPST_HANDOFF_CONFIG_HOME="${root}/expected/config" \
VINPST_HANDOFF_EXPECTED_DAEMON="${root}/expected/bin/vinpst-daemon" \
VINPST_HANDOFF_NEW_DAEMON_LOG="${root}/new-daemon.log" \
VINPST_HANDOFF_NEW_PID_FILE="${root}/new.pid" \
  "${root}/expected/bin/vinpst" daemon handoff --json >"${root}/handoff.json"

test -s "${root}/new.pid"
new_pid="$(cat "${root}/new.pid")"
test "${new_pid}" != "${old_pid}"
kill -0 "${new_pid}"
INNER

mapfile -t systemctl_calls <"${systemd_case}/systemctl.log"
test "${systemctl_calls[0]}" = '--user daemon-reload'
test "${systemctl_calls[1]}" = '--user restart vinpst-daemon.service'
expected_daemon="$(readlink -f "${systemd_case}/expected/bin/vinpst-daemon")"
old_daemon="$(readlink -f "${systemd_case}/old/vinpst-daemon")"
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
   and .service_control.command_argv == [$systemctl, "--user", "restart", "vinpst-daemon.service"]
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
install -m755 target/debug/vinpst-daemon "${direct}/old/vinpst-daemon"
cat >"${direct}/share/dbus-1/services/org.fcitx.Vinpst.service" <<EOF
[D-BUS Service]
Name=org.fcitx.Vinpst
Exec=${direct}/expected/bin/vinpst-daemon --dbus --exit-when-executable-replaced
EOF
cat >"${direct}/systemctl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
test "$*" = '--user show --property MainPID --value vinpst-daemon.service'
printf '0\n'
SH
cat >"${direct}/kill" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
test "$#" = 2
test "$1" = -TERM
test "$2" = "${VINPST_HANDOFF_OLD_PID}"
printf '%s\n' "$*" >"${VINPST_HANDOFF_KILL_LOG}"
/usr/bin/kill -TERM "$2"
SH
chmod +x "${direct}/systemctl" "${direct}/kill"

HOME="${direct}/home" \
XDG_DATA_HOME="${direct}/share" \
XDG_DATA_DIRS="${direct}/share" \
XDG_CONFIG_HOME="${direct}/expected/config" \
XDG_RUNTIME_DIR="${direct}/runtime" \
VINPST_HANDOFF_ROOT="${direct}" \
  timeout 25s dbus-run-session -- bash -euo pipefail <<'INNER'
root="${VINPST_HANDOFF_ROOT}"
"${root}/old/vinpst-daemon" --dbus >"${root}/old-daemon.log" 2>&1 &
old_pid=$!
cleanup() {
  kill "${old_pid}" 2>/dev/null || true
  owner_reply="$(gdbus call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.GetConnectionUnixProcessID \
    org.fcitx.Vinpst 2>/dev/null || true)"
  owner_pid="$(sed -n 's/.*uint32 \([0-9][0-9]*\).*/\1/p' <<<"${owner_reply}")"
  if [[ "${owner_pid}" =~ ^[0-9]+$ ]]; then
    kill "${owner_pid}" 2>/dev/null || true
  fi
  wait "${old_pid}" 2>/dev/null || true
}
trap cleanup EXIT

for _ in $(seq 1 100); do
  if "${root}/expected/bin/vinpst" daemon status --json >/dev/null 2>&1; then
    break
  fi
  sleep 0.05
done

VINPST_DAEMON_SYSTEMCTL="${root}/systemctl" \
VINPST_DAEMON_KILL="${root}/kill" \
VINPST_HANDOFF_OLD_PID="${old_pid}" \
VINPST_HANDOFF_KILL_LOG="${root}/kill.log" \
  "${root}/expected/bin/vinpst" daemon handoff --json >"${root}/handoff.json"

new_pid="$(jq -r '.after.owner.unix_process_id' "${root}/handoff.json")"
test "${new_pid}" != "${old_pid}"
kill -0 "${new_pid}"
INNER

test "$(cat "${direct}/kill.log")" = "-TERM $(jq -r '.before.owner.unix_process_id' "${direct}/handoff.json")"
direct_expected="$(readlink -f "${direct}/expected/bin/vinpst-daemon")"
direct_old="$(readlink -f "${direct}/old/vinpst-daemon")"
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
install -m755 target/debug/vinpst-daemon "${failure}/old/vinpst-daemon"
cat >"${failure}/systemctl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
case "$*" in
  '--user show --property MainPID --value vinpst-daemon.service')
    printf '%s\n' "${VINPST_HANDOFF_OLD_PID}"
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

VINPST_HANDOFF_ROOT="${failure}" timeout 20s dbus-run-session --config-file="${dbus_config}" -- bash -euo pipefail <<'INNER'
root="${VINPST_HANDOFF_ROOT}"
"${root}/old/vinpst-daemon" --dbus >"${root}/old-daemon.log" 2>&1 &
old_pid=$!
trap 'kill "${old_pid}" 2>/dev/null || true; wait "${old_pid}" 2>/dev/null || true' EXIT

for _ in $(seq 1 100); do
  if XDG_CONFIG_HOME="${root}/expected/config" \
    "${root}/expected/bin/vinpst" daemon status --json >/dev/null 2>&1; then
    break
  fi
  sleep 0.05
done

if XDG_CONFIG_HOME="${root}/expected/config" \
  VINPST_DAEMON_SYSTEMCTL="${root}/systemctl" \
  VINPST_HANDOFF_OLD_PID="${old_pid}" \
    "${root}/expected/bin/vinpst" daemon handoff --json \
    >"${root}/handoff.json" 2>"${root}/handoff.err"; then
  echo "daemon-reload failure unexpectedly returned success" >&2
  exit 1
fi
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
