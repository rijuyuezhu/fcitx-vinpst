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

for command in dbus-run-session gdbus jq timeout; do
  command -v "${command}" >/dev/null
done

root="${repo_root}/target/tmp/daemon-removal-handoff-smoke"
rm -rf "${root}"
mkdir -p "${root}"

write_activation_fixture() {
  local case_root="$1"
  local service_dir="${case_root}/data-home/dbus-1/services"
  mkdir -p "${service_dir}" "${case_root}/data-dirs"
  printf '[D-BUS Service]\nName=org.fcitx.Vinpst\nExec=/removed/vinpst-daemon --dbus\n' \
    >"${service_dir}/org.fcitx.Vinpst.service"
  write_isolated_dbus_session_config "${case_root}/session.conf" "${service_dir}"
}

cargo build -q -p vinpst-cli --bin vinpst -p vinpst-daemon --bin vinpst-daemon

# No owner is an idempotent success and still attempts to disable stale enablement.
write_activation_fixture "${root}/no-owner"
XDG_DATA_HOME="${root}/no-owner/data-home" \
XDG_DATA_DIRS="${root}/no-owner/data-dirs" \
VINPST_REMOVE_ROOT="${root}/no-owner" \
  timeout 20s dbus-run-session --config-file="${root}/no-owner/session.conf" -- \
  bash -euo pipefail <<'INNER'
root="${VINPST_REMOVE_ROOT}"
activation_file="${XDG_DATA_HOME}/dbus-1/services/org.fcitx.Vinpst.service"
gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.ListActivatableNames |
  grep -Fq org.fcitx.Vinpst
rm -f "${activation_file}"
cat >"${root}/systemctl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
test "$*" = '--user disable --now vinpst-daemon.service'
if gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.ListActivatableNames |
  grep -Fq org.fcitx.Vinpst; then
  echo "disable ran before D-Bus activation metadata was reloaded" >&2
  exit 96
fi
printf '%s\n' "$*" >"${VINPST_REMOVE_SYSTEMCTL_LOG}"
SH
chmod +x "${root}/systemctl"
VINPST_DAEMON_SYSTEMCTL="${root}/systemctl" \
VINPST_REMOVE_SYSTEMCTL_LOG="${root}/systemctl.log" \
  target/debug/vinpst daemon prepare-remove --json >"${root}/remove.json"
INNER
jq -e '
  .ok == true and
  .removal_strategy == "no-owner" and
  .will_signal_owner == false and
  .verification.status == "owner-absent" and
  .service_disable.ok == true
' "${root}/no-owner/remove.json" >/dev/null
test "$(cat "${root}/no-owner/systemctl.log")" = '--user disable --now vinpst-daemon.service'

# A direct idle owner is identity-checked, signalled exactly, and not reactivated.
direct="${root}/direct"
mkdir -p "${direct}/config"
write_activation_fixture "${direct}"
cat >"${direct}/systemctl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
case "$*" in
  '--user show --property MainPID --value vinpst-daemon.service')
    if gdbus call --session \
      --dest org.freedesktop.DBus \
      --object-path /org/freedesktop/DBus \
      --method org.freedesktop.DBus.ListActivatableNames |
      grep -Fq org.fcitx.Vinpst; then
      echo "systemd probe ran before D-Bus activation metadata was reloaded" >&2
      exit 96
    fi
    printf '0\n'
    ;;
  '--user disable --now vinpst-daemon.service')
    printf '%s\n' "$*" >"${VINPST_REMOVE_SYSTEMCTL_LOG}"
    ;;
  *)
    printf 'unexpected systemctl arguments: %s\n' "$*" >&2
    exit 98
    ;;
esac
SH
cat >"${direct}/kill" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
test "$#" = 2
test "$1" = -TERM
test "$2" = "${VINPST_REMOVE_OWNER_PID}"
if gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.ListActivatableNames |
  grep -Fq org.fcitx.Vinpst; then
  echo "signal ran before D-Bus activation metadata was reloaded" >&2
  exit 96
fi
printf '%s\n' "$*" >"${VINPST_REMOVE_KILL_LOG}"
/usr/bin/kill -TERM "$2"
SH
chmod +x "${direct}/systemctl" "${direct}/kill"

XDG_DATA_HOME="${direct}/data-home" \
XDG_DATA_DIRS="${direct}/data-dirs" \
VINPST_REMOVE_ROOT="${direct}" \
  timeout 25s dbus-run-session --config-file="${direct}/session.conf" -- \
  bash -euo pipefail <<'INNER'
root="${VINPST_REMOVE_ROOT}"
activation_file="${XDG_DATA_HOME}/dbus-1/services/org.fcitx.Vinpst.service"
gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.ListActivatableNames |
  grep -Fq org.fcitx.Vinpst
XDG_CONFIG_HOME="${root}/config" target/debug/vinpst-daemon --dbus >"${root}/daemon.log" 2>&1 &
daemon_pid=$!
cleanup() {
  kill "${daemon_pid}" 2>/dev/null || true
  wait "${daemon_pid}" 2>/dev/null || true
}
trap cleanup EXIT
for _ in $(seq 1 100); do
  if gdbus call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.NameHasOwner org.fcitx.Vinpst |
    grep -Fq true; then
    break
  fi
  sleep 0.05
done
gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.NameHasOwner org.fcitx.Vinpst |
  grep -Fq true
XDG_CONFIG_HOME="${root}/config" target/debug/vinpst daemon status --json >/dev/null
rm -f "${activation_file}"
VINPST_DAEMON_SYSTEMCTL="${root}/systemctl" \
VINPST_DAEMON_KILL="${root}/kill" \
VINPST_REMOVE_OWNER_PID="${daemon_pid}" \
VINPST_REMOVE_SYSTEMCTL_LOG="${root}/systemctl.log" \
VINPST_REMOVE_KILL_LOG="${root}/kill.log" \
XDG_CONFIG_HOME="${root}/config" \
  target/debug/vinpst daemon prepare-remove --json >"${root}/remove.json"
wait "${daemon_pid}" 2>/dev/null || true
if gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.NameHasOwner org.fcitx.Vinpst |
  grep -Fq true; then
  echo "direct daemon owner remained after removal handoff" >&2
  exit 1
fi
INNER
jq -e '
  .ok == true and
  .removal_strategy == "direct-owner-terminate" and
  .will_signal_owner == true and
  .direct_guard.approved == true and
  .direct_revalidation.ok == true and
  .direct_signal.ok == true and
  .verification.status == "owner-absent"
' "${direct}/remove.json" >/dev/null
test "$(cat "${direct}/kill.log")" = "-TERM $(jq -r '.before.owner.unix_process_id' "${direct}/remove.json")"
test "$(cat "${direct}/systemctl.log")" = '--user disable --now vinpst-daemon.service'

# A busy direct owner must reject removal before service mutation or signalling.
busy="${root}/busy"
mkdir -p "${busy}/config"
write_activation_fixture "${busy}"
cat >"${busy}/systemctl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
case "$*" in
  '--user show --property MainPID --value vinpst-daemon.service')
    if gdbus call --session \
      --dest org.freedesktop.DBus \
      --object-path /org/freedesktop/DBus \
      --method org.freedesktop.DBus.ListActivatableNames |
      grep -Fq org.fcitx.Vinpst; then
      echo "systemd probe ran before D-Bus activation metadata was reloaded" >&2
      exit 96
    fi
    printf '%s\n' "${VINPST_REMOVE_SYSTEMD_MAIN_PID:-0}"
    ;;
  *)
    exit 97
    ;;
esac
SH
cat >"${busy}/must-not-kill" <<'SH'
#!/usr/bin/env bash
exit 97
SH
chmod +x "${busy}/systemctl" "${busy}/must-not-kill"

XDG_DATA_HOME="${busy}/data-home" \
XDG_DATA_DIRS="${busy}/data-dirs" \
VINPST_REMOVE_ROOT="${busy}" \
  timeout 25s dbus-run-session --config-file="${busy}/session.conf" -- \
  bash -euo pipefail <<'INNER'
root="${VINPST_REMOVE_ROOT}"
activation_file="${XDG_DATA_HOME}/dbus-1/services/org.fcitx.Vinpst.service"
gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.ListActivatableNames |
  grep -Fq org.fcitx.Vinpst
XDG_CONFIG_HOME="${root}/config" target/debug/vinpst-daemon --dbus >"${root}/daemon.log" 2>&1 &
daemon_pid=$!
cleanup() {
  gdbus call --session \
    --dest org.fcitx.Vinpst \
    --object-path /org/fcitx/Vinpst \
    --method org.fcitx.Vinpst.Service.StopRecording "" >/dev/null 2>&1 || true
  kill "${daemon_pid}" 2>/dev/null || true
  wait "${daemon_pid}" 2>/dev/null || true
}
trap cleanup EXIT
for _ in $(seq 1 100); do
  if gdbus call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.NameHasOwner org.fcitx.Vinpst |
    grep -Fq true; then
    break
  fi
  sleep 0.05
done
gdbus call --session \
  --dest org.fcitx.Vinpst \
  --object-path /org/fcitx/Vinpst \
  --method org.fcitx.Vinpst.Service.StartRecording >/dev/null
test "$(gdbus call --session \
  --dest org.fcitx.Vinpst \
  --object-path /org/fcitx/Vinpst \
  --method org.fcitx.Vinpst.Service.GetStatus)" = "('recording',)"
rm -f "${activation_file}"
if VINPST_DAEMON_SYSTEMCTL="${root}/systemctl" \
  VINPST_DAEMON_KILL="${root}/must-not-kill" \
  XDG_CONFIG_HOME="${root}/config" \
  target/debug/vinpst daemon prepare-remove --json >"${root}/remove.json"; then
  echo "busy direct owner unexpectedly accepted removal" >&2
  exit 1
fi
kill -0 "${daemon_pid}"
test "$(gdbus call --session \
  --dest org.fcitx.Vinpst \
  --object-path /org/fcitx/Vinpst \
  --method org.fcitx.Vinpst.Service.GetStatus)" = "('recording',)"
if VINPST_REMOVE_SYSTEMD_MAIN_PID="${daemon_pid}" \
  VINPST_DAEMON_SYSTEMCTL="${root}/systemctl" \
  VINPST_DAEMON_KILL="${root}/must-not-kill" \
  XDG_CONFIG_HOME="${root}/config" \
  target/debug/vinpst daemon prepare-remove --json >"${root}/systemd-remove.json"; then
  echo "busy systemd owner unexpectedly accepted removal" >&2
  exit 1
fi
kill -0 "${daemon_pid}"
test "$(gdbus call --session \
  --dest org.fcitx.Vinpst \
  --object-path /org/fcitx/Vinpst \
  --method org.fcitx.Vinpst.Service.GetStatus)" = "('recording',)"
gdbus call --session \
  --dest org.fcitx.Vinpst \
  --object-path /org/fcitx/Vinpst \
  --method org.fcitx.Vinpst.Service.StopRecording "" >/dev/null
test "$(gdbus call --session \
  --dest org.fcitx.Vinpst \
  --object-path /org/fcitx/Vinpst \
  --method org.fcitx.Vinpst.Service.GetStatus)" = "('idle',)"
if VINPST_DAEMON_SYSTEMCTL="${root}/systemctl" \
  VINPST_DAEMON_KILL="${root}/must-not-kill" \
  XDG_CONFIG_HOME="${root}/config" \
  target/debug/vinpst daemon prepare-remove --json >"${root}/disable-failure.json"; then
  echo "direct owner unexpectedly terminated after disable failure" >&2
  exit 1
fi
kill -0 "${daemon_pid}"
test "$(gdbus call --session \
  --dest org.fcitx.Vinpst \
  --object-path /org/fcitx/Vinpst \
  --method org.fcitx.Vinpst.Service.GetStatus)" = "('idle',)"
INNER
jq -e '
  .ok == false and
  .removal_strategy == "direct-owner-terminate" and
  .will_mutate_user_service == false and
  .will_signal_owner == false and
  .direct_guard.approved == false and
  .direct_guard.status_idle == false and
  .direct_guard.active_session == true and
  .verification.status == "direct-owner-guard-rejected"
' "${busy}/remove.json" >/dev/null
jq -e '
  .ok == false and
  .removal_strategy == "systemd-disable-and-stop" and
  .will_mutate_user_service == false and
  .will_signal_owner == false and
  .systemd_probe.owner_matches_main_pid == true and
  .session_guard.approved == false and
  .session_guard.status_idle == false and
  .session_guard.active_session == true and
  .verification.status == "active-session-guard-rejected"
' "${busy}/systemd-remove.json" >/dev/null
jq -e '
  .ok == false and
  .removal_strategy == "direct-owner-terminate" and
  .will_mutate_user_service == false and
  .will_signal_owner == false and
  .direct_guard.approved == true and
  .service_disable.ok == false and
  .direct_revalidation == null and
  .direct_signal == null and
  .verification.status == "disable-failed"
' "${busy}/disable-failure.json" >/dev/null
test ! -e "${busy}/systemctl.log"
test ! -e "${busy}/kill.log"

# A systemd-owned process is stopped by disable --now and never signalled directly.
systemd_case="${root}/systemd"
mkdir -p "${systemd_case}/config"
write_activation_fixture "${systemd_case}"
cat >"${systemd_case}/systemctl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
case "$*" in
  '--user show --property MainPID --value vinpst-daemon.service')
    if gdbus call --session \
      --dest org.freedesktop.DBus \
      --object-path /org/freedesktop/DBus \
      --method org.freedesktop.DBus.ListActivatableNames |
      grep -Fq org.fcitx.Vinpst; then
      echo "systemd probe ran before D-Bus activation metadata was reloaded" >&2
      exit 96
    fi
    printf '%s\n' "${VINPST_REMOVE_OWNER_PID}"
    ;;
  '--user disable --now vinpst-daemon.service')
    printf '%s\n' "$*" >"${VINPST_REMOVE_SYSTEMCTL_LOG}"
    /usr/bin/kill -TERM "${VINPST_REMOVE_OWNER_PID}"
    ;;
  *)
    printf 'unexpected systemctl arguments: %s\n' "$*" >&2
    exit 98
    ;;
esac
SH
cat >"${systemd_case}/must-not-kill" <<'SH'
#!/usr/bin/env bash
exit 97
SH
chmod +x "${systemd_case}/systemctl" "${systemd_case}/must-not-kill"

XDG_DATA_HOME="${systemd_case}/data-home" \
XDG_DATA_DIRS="${systemd_case}/data-dirs" \
VINPST_REMOVE_ROOT="${systemd_case}" \
  timeout 25s dbus-run-session --config-file="${systemd_case}/session.conf" -- \
  bash -euo pipefail <<'INNER'
root="${VINPST_REMOVE_ROOT}"
activation_file="${XDG_DATA_HOME}/dbus-1/services/org.fcitx.Vinpst.service"
gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.ListActivatableNames |
  grep -Fq org.fcitx.Vinpst
XDG_CONFIG_HOME="${root}/config" target/debug/vinpst-daemon --dbus >"${root}/daemon.log" 2>&1 &
daemon_pid=$!
trap 'kill "${daemon_pid}" 2>/dev/null || true; wait "${daemon_pid}" 2>/dev/null || true' EXIT
for _ in $(seq 1 100); do
  if gdbus call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.NameHasOwner org.fcitx.Vinpst |
    grep -Fq true; then
    break
  fi
  sleep 0.05
done
gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.NameHasOwner org.fcitx.Vinpst |
  grep -Fq true
XDG_CONFIG_HOME="${root}/config" target/debug/vinpst daemon status --json >/dev/null
rm -f "${activation_file}"
VINPST_DAEMON_SYSTEMCTL="${root}/systemctl" \
VINPST_DAEMON_KILL="${root}/must-not-kill" \
VINPST_REMOVE_OWNER_PID="${daemon_pid}" \
VINPST_REMOVE_SYSTEMCTL_LOG="${root}/systemctl.log" \
XDG_CONFIG_HOME="${root}/config" \
  target/debug/vinpst daemon prepare-remove --json >"${root}/remove.json"
wait "${daemon_pid}" 2>/dev/null || true
INNER
jq -e '
  .ok == true and
  .removal_strategy == "systemd-disable-and-stop" and
  .will_mutate_user_service == true and
  .will_signal_owner == false and
  .systemd_probe.owner_matches_main_pid == true and
  .service_disable.ok == true and
  .verification.status == "owner-absent"
' "${systemd_case}/remove.json" >/dev/null
test "$(cat "${systemd_case}/systemctl.log")" = '--user disable --now vinpst-daemon.service'

echo "daemon guarded removal handoff smoke passed"
