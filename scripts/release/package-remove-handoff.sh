#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
activation_file="${VINPST_REMOVE_ACTIVATION_FILE:-/usr/share/dbus-1/services/org.fcitx.Vinpst.service}"
runtime_root="${VINPST_REMOVE_RUNTIME_ROOT:-/run/user}"
vinpst_binary="${VINPST_REMOVE_VINPST:-/usr/bin/vinpst}"
default_runuser_binary=/usr/bin/runuser
if [[ ! -x "${default_runuser_binary}" && -x /usr/sbin/runuser ]]; then
  default_runuser_binary=/usr/sbin/runuser
fi
runuser_binary="${VINPST_REMOVE_RUNUSER:-${default_runuser_binary}}"
env_binary="${VINPST_REMOVE_ENV:-/usr/bin/env}"
getent_binary="${VINPST_REMOVE_GETENT:-/usr/bin/getent}"
stat_binary="${VINPST_REMOVE_STAT:-/usr/bin/stat}"
rm_binary="${VINPST_REMOVE_RM:-/usr/bin/rm}"
cp_binary="${VINPST_REMOVE_CP:-/usr/bin/cp}"
mktemp_binary="${VINPST_REMOVE_MKTEMP:-/usr/bin/mktemp}"
gdbus_binary="${VINPST_REMOVE_GDBUS:-/usr/bin/gdbus}"
systemctl_binary="${VINPST_REMOVE_SYSTEMCTL:-/usr/bin/systemctl}"
kill_binary="${VINPST_REMOVE_KILL:-/usr/bin/kill}"

# shellcheck source=package-session-common.sh
source "${script_dir}/package-session-common.sh"

session_uid=""

for command in \
  "${vinpst_binary}" \
  "${runuser_binary}" \
  "${env_binary}" \
  "${getent_binary}" \
  "${stat_binary}" \
  "${rm_binary}" \
  "${cp_binary}" \
  "${mktemp_binary}" \
  "${gdbus_binary}" \
  "${systemctl_binary}" \
  "${kill_binary}"; do
  if [[ ! -x "${command}" ]]; then
    echo "required removal-handoff command is missing: ${command}" >&2
    exit 1
  fi
done

backup_dir=""
activation_existed=0
cleanup() {
  if [[ -n "${backup_dir}" ]]; then
    "${rm_binary}" -rf -- "${backup_dir}"
  fi
}
trap cleanup EXIT

if [[ -e "${activation_file}" ]]; then
  backup_dir="$("${mktemp_binary}" -d)"
  "${cp_binary}" -a -- "${activation_file}" "${backup_dir}/activation.service"
  activation_existed=1
fi
"${rm_binary}" -f -- "${activation_file}"

reload_restored_activation() {
  local rollback_failures=0
  local identity_status

  ((activation_existed == 1)) || return 0
  "${cp_binary}" -a -- "${backup_dir}/activation.service" "${activation_file}"
  for bus_path in "${runtime_root}"/[0-9]*/bus; do
    if vinpst_package_load_session_identity "${bus_path}"; then
      if ! vinpst_package_run_in_session \
        "${gdbus_binary}" call --session \
        --dest org.freedesktop.DBus \
        --object-path /org/freedesktop/DBus \
        --method org.freedesktop.DBus.ReloadConfig >/dev/null; then
        echo "failed to reload restored D-Bus activation for uid ${session_uid}" >&2
        rollback_failures=$((rollback_failures + 1))
      fi
    else
      identity_status=$?
      if ((identity_status == 1)); then
        rollback_failures=$((rollback_failures + 1))
      fi
    fi
  done
  ((rollback_failures == 0))
}

sessions=0
preflight_failures=0
shopt -s nullglob
for bus_path in "${runtime_root}"/[0-9]*/bus; do
  if vinpst_package_load_session_identity "${bus_path}"; then
    sessions=$((sessions + 1))
    if ! vinpst_package_run_in_session \
      VINPST_DAEMON_SYSTEMCTL="${systemctl_binary}" \
      VINPST_DAEMON_KILL="${kill_binary}" \
      "${vinpst_binary}" daemon prepare-remove --preflight --json; then
      echo "failed to preflight vinpst daemon removal for uid ${session_uid}" >&2
      preflight_failures=$((preflight_failures + 1))
    fi
  else
    identity_status=$?
    if ((identity_status == 1)); then
      preflight_failures=$((preflight_failures + 1))
    fi
  fi
done

if ((preflight_failures > 0)); then
  echo "vinpst removal preflight failed for ${preflight_failures} session(s)" >&2
  if ! reload_restored_activation; then
    echo "vinpst removal handoff also failed to restore activation metadata" >&2
  fi
  exit 1
fi

failures=0
for bus_path in "${runtime_root}"/[0-9]*/bus; do
  if vinpst_package_load_session_identity "${bus_path}"; then
    if ! vinpst_package_run_in_session \
      VINPST_DAEMON_SYSTEMCTL="${systemctl_binary}" \
      VINPST_DAEMON_KILL="${kill_binary}" \
      "${vinpst_binary}" daemon prepare-remove --json; then
      echo "failed to prepare vinpst daemon removal for uid ${session_uid}" >&2
      failures=$((failures + 1))
    fi
  else
    identity_status=$?
    if ((identity_status == 1)); then
      failures=$((failures + 1))
    fi
  fi
done

printf 'vinpst removal handoff checked %d live user session(s)\n' "${sessions}"
if ((failures > 0)); then
  echo "vinpst removal handoff failed for ${failures} session(s)" >&2
  if ! reload_restored_activation; then
    echo "vinpst removal handoff also failed to restore activation metadata" >&2
  fi
  exit 1
fi
