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

for command in dbus-run-session jq readlink timeout; do
  command -v "${command}" >/dev/null
done

root="${repo_root}/target/tmp/package-remove-handoff-smoke"
service_dir="${root}/data-home/dbus-1/services"
rm -rf "${root}"
mkdir -p "${service_dir}" "${root}/data-dirs"
printf '[D-BUS Service]\nName=org.fcitx.Vinput\nExec=/removed/vinput-daemon --dbus\n' \
  >"${service_dir}/org.fcitx.Vinput.service"
write_isolated_dbus_session_config "${root}/session.conf" "${service_dir}"

cargo build -q -p vinput-cli --bin vinput -p vinput-daemon --bin vinput-daemon

XDG_DATA_HOME="${root}/data-home" \
XDG_DATA_DIRS="${root}/data-dirs" \
VINPUT_REMOVE_ROOT="${root}" \
  timeout 30s dbus-run-session --config-file="${root}/session.conf" -- \
  bash -euo pipefail <<'INNER'
root="${VINPUT_REMOVE_ROOT}"
uid="$(id -u)"
runtime_root="$(mktemp -d "${TMPDIR:-/tmp}/vinput-remove-runtime.XXXXXX")"
runtime_dir="${runtime_root}/${uid}"
activation_file="${XDG_DATA_HOME}/dbus-1/services/org.fcitx.Vinput.service"
daemon_pid=""
cleanup() {
  if [[ -n "${daemon_pid}" ]]; then
    kill "${daemon_pid}" 2>/dev/null || true
    wait "${daemon_pid}" 2>/dev/null || true
  fi
  rm -rf "${runtime_root}"
}
trap cleanup EXIT
mkdir -p "${runtime_dir}" "${root}/config"
bus_path="${DBUS_SESSION_BUS_ADDRESS#unix:path=}"
bus_path="${bus_path%%,*}"
ln -s "${bus_path}" "${runtime_dir}/bus"
gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.ListActivatableNames |
  grep -Fq org.fcitx.Vinput

cat >"${root}/runuser" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
test "$1" = -u
shift 2
test "$1" = --
shift
exec "$@"
SH
cat >"${root}/systemctl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
assert_activation_absent() {
  if gdbus call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.ListActivatableNames |
    grep -Fq org.fcitx.Vinput; then
    echo "systemctl ran before D-Bus activation metadata was reloaded" >&2
    exit 96
  fi
}
case "$*" in
  '--user show --property MainPID --value vinput-daemon.service')
    assert_activation_absent
    printf '0\n'
    ;;
  '--user disable --now vinput-daemon.service')
    assert_activation_absent
    printf '%s\n' "$*" >"$(dirname "$0")/systemctl.log"
    ;;
  *)
    printf 'unexpected systemctl arguments: %s\n' "$*" >&2
    exit 98
    ;;
esac
SH
chmod +x "${root}/runuser" "${root}/systemctl"

XDG_CONFIG_HOME="${root}/config" target/debug/vinput-daemon --dbus >"${root}/daemon.log" 2>&1 &
daemon_pid=$!
for _ in $(seq 1 100); do
  if gdbus call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.NameHasOwner org.fcitx.Vinput |
    grep -Fq true; then
    break
  fi
  sleep 0.05
done
gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.NameHasOwner org.fcitx.Vinput |
  grep -Fq true
XDG_CONFIG_HOME="${root}/config" target/debug/vinput daemon status --json >/dev/null

VINPUT_REMOVE_ACTIVATION_FILE="${activation_file}" \
VINPUT_REMOVE_RUNTIME_ROOT="${runtime_root}" \
VINPUT_REMOVE_VINPUT="${PWD}/target/debug/vinput" \
VINPUT_REMOVE_RUNUSER="${root}/runuser" \
VINPUT_REMOVE_SYSTEMCTL="${root}/systemctl" \
  scripts/release/package-remove-handoff.sh >"${root}/handoff.log"

wait "${daemon_pid}" 2>/dev/null || true
test ! -e "${activation_file}"
test "$(cat "${root}/systemctl.log")" = '--user disable --now vinput-daemon.service'
if gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.ListActivatableNames |
  grep -Fq org.fcitx.Vinput; then
  echo "package removal helper left activation metadata visible" >&2
  exit 1
fi
if gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.NameHasOwner org.fcitx.Vinput |
  grep -Fq true; then
  echo "package removal helper left a daemon owner" >&2
  exit 1
fi
grep -Fq 'checked 1 live user session(s)' "${root}/handoff.log"
INNER

echo "package removal cross-user handoff smoke passed"

busy_root="${repo_root}/target/tmp/package-remove-handoff-busy-smoke"
busy_service_dir="${busy_root}/data-home/dbus-1/services"
rm -rf "${busy_root}"
mkdir -p "${busy_service_dir}" "${busy_root}/data-dirs"
printf '[D-BUS Service]\nName=org.fcitx.Vinput\nExec=/removed/vinput-daemon --dbus\n' \
  >"${busy_service_dir}/org.fcitx.Vinput.service"
cp "${busy_service_dir}/org.fcitx.Vinput.service" \
  "${busy_root}/activation-before.service"
write_isolated_dbus_session_config "${busy_root}/session.conf" "${busy_service_dir}"

XDG_DATA_HOME="${busy_root}/data-home" \
XDG_DATA_DIRS="${busy_root}/data-dirs" \
VINPUT_REMOVE_ROOT="${busy_root}" \
  timeout 30s dbus-run-session --config-file="${busy_root}/session.conf" -- \
  bash -euo pipefail <<'INNER'
root="${VINPUT_REMOVE_ROOT}"
uid="$(id -u)"
runtime_root="$(mktemp -d "${TMPDIR:-/tmp}/vinput-remove-busy-runtime.XXXXXX")"
runtime_dir="${runtime_root}/${uid}"
activation_file="${XDG_DATA_HOME}/dbus-1/services/org.fcitx.Vinput.service"
daemon_pid=""
cleanup() {
  if [[ -n "${daemon_pid}" ]]; then
    gdbus call --session \
      --dest org.fcitx.Vinput \
      --object-path /org/fcitx/Vinput \
      --method org.fcitx.Vinput.Service.StopRecording "" >/dev/null 2>&1 || true
    kill "${daemon_pid}" 2>/dev/null || true
    wait "${daemon_pid}" 2>/dev/null || true
  fi
  rm -rf "${runtime_root}"
}
trap cleanup EXIT
mkdir -p "${runtime_dir}" "${root}/config"
bus_path="${DBUS_SESSION_BUS_ADDRESS#unix:path=}"
bus_path="${bus_path%%,*}"
ln -s "${bus_path}" "${runtime_dir}/bus"
gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.ListActivatableNames |
  grep -Fq org.fcitx.Vinput

cat >"${root}/runuser" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
test "$1" = -u
shift 2
test "$1" = --
shift
exec "$@"
SH
cat >"${root}/systemctl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
test "$*" = '--user show --property MainPID --value vinput-daemon.service'
if gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.ListActivatableNames |
  grep -Fq org.fcitx.Vinput; then
  echo "systemd probe ran before D-Bus activation metadata was reloaded" >&2
  exit 96
fi
printf '0\n'
SH
cat >"${root}/must-not-kill" <<'SH'
#!/usr/bin/env bash
exit 97
SH
chmod +x "${root}/runuser" "${root}/systemctl" "${root}/must-not-kill"

XDG_CONFIG_HOME="${root}/config" target/debug/vinput-daemon --dbus >"${root}/daemon.log" 2>&1 &
daemon_pid=$!
for _ in $(seq 1 100); do
  if gdbus call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.NameHasOwner org.fcitx.Vinput |
    grep -Fq true; then
    break
  fi
  sleep 0.05
done
gdbus call --session \
  --dest org.fcitx.Vinput \
  --object-path /org/fcitx/Vinput \
  --method org.fcitx.Vinput.Service.StartRecording >/dev/null
test "$(gdbus call --session \
  --dest org.fcitx.Vinput \
  --object-path /org/fcitx/Vinput \
  --method org.fcitx.Vinput.Service.GetStatus)" = "('recording',)"

if VINPUT_REMOVE_ACTIVATION_FILE="${activation_file}" \
  VINPUT_REMOVE_RUNTIME_ROOT="${runtime_root}" \
  VINPUT_REMOVE_VINPUT="${PWD}/target/debug/vinput" \
  VINPUT_REMOVE_RUNUSER="${root}/runuser" \
  VINPUT_REMOVE_SYSTEMCTL="${root}/systemctl" \
  VINPUT_REMOVE_KILL="${root}/must-not-kill" \
  scripts/release/package-remove-handoff.sh >"${root}/handoff.log" 2>"${root}/handoff.err"; then
  echo "busy package removal unexpectedly succeeded" >&2
  exit 1
fi

cmp "${root}/activation-before.service" "${activation_file}"
gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.ListActivatableNames |
  grep -Fq org.fcitx.Vinput
kill -0 "${daemon_pid}"
test "$(gdbus call --session \
  --dest org.fcitx.Vinput \
  --object-path /org/fcitx/Vinput \
  --method org.fcitx.Vinput.Service.GetStatus)" = "('recording',)"
grep -Fq 'failed to preflight vinput daemon removal' "${root}/handoff.err"
grep -Fq 'vinput removal preflight failed for 1 session(s)' "${root}/handoff.err"
jq -e '
  .ok == false and
  .preflight == true and
  .will_mutate_user_service == false and
  .will_signal_owner == false and
  .direct_guard.approved == false
' "${root}/handoff.log" >/dev/null
INNER

echo "package removal busy rollback smoke passed"
