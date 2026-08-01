#!/usr/bin/env bash
set -euo pipefail

activation_file="${VINPUT_REMOVE_ACTIVATION_FILE:-/usr/share/dbus-1/services/org.fcitx.Vinput.service}"
runtime_root="${VINPUT_REMOVE_RUNTIME_ROOT:-/run/user}"
vinput_binary="${VINPUT_REMOVE_VINPUT:-/usr/bin/vinput}"
runuser_binary="${VINPUT_REMOVE_RUNUSER:-/usr/bin/runuser}"
env_binary="${VINPUT_REMOVE_ENV:-/usr/bin/env}"
getent_binary="${VINPUT_REMOVE_GETENT:-/usr/bin/getent}"
stat_binary="${VINPUT_REMOVE_STAT:-/usr/bin/stat}"
rm_binary="${VINPUT_REMOVE_RM:-/usr/bin/rm}"
cp_binary="${VINPUT_REMOVE_CP:-/usr/bin/cp}"
mktemp_binary="${VINPUT_REMOVE_MKTEMP:-/usr/bin/mktemp}"
gdbus_binary="${VINPUT_REMOVE_GDBUS:-/usr/bin/gdbus}"
systemctl_binary="${VINPUT_REMOVE_SYSTEMCTL:-/usr/bin/systemctl}"
kill_binary="${VINPUT_REMOVE_KILL:-/usr/bin/kill}"

for command in \
  "${vinput_binary}" \
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

load_session_identity() {
  local bus_path="$1"
  local passwd_entry

  session_runtime_dir="$(dirname "${bus_path}")"
  session_uid="${session_runtime_dir##*/}"
  [[ "${session_uid}" =~ ^[0-9]+$ ]] || return 2
  [[ -S "${bus_path}" ]] || return 2
  if [[ "$("${stat_binary}" -c %u -- "${session_runtime_dir}")" != "${session_uid}" ||
    "$("${stat_binary}" -c %u -- "${bus_path}")" != "${session_uid}" ]]; then
    echo "skipping untrusted runtime bus ownership: ${bus_path}" >&2
    return 1
  fi

  passwd_entry="$("${getent_binary}" passwd "${session_uid}" || true)"
  [[ -n "${passwd_entry}" ]] || return 2
  IFS=: read -r session_user _ _ _ _ session_home _ <<<"${passwd_entry}"
  [[ -n "${session_user}" && -n "${session_home}" ]] || return 2
  session_bus_path="${bus_path}"
  return 0
}

run_in_session() {
  "${runuser_binary}" -u "${session_user}" -- \
    "${env_binary}" -i \
    HOME="${session_home}" \
    USER="${session_user}" \
    LOGNAME="${session_user}" \
    PATH=/usr/bin:/bin \
    XDG_RUNTIME_DIR="${session_runtime_dir}" \
    DBUS_SESSION_BUS_ADDRESS="unix:path=${session_bus_path}" \
    "$@"
}

reload_restored_activation() {
  local rollback_failures=0
  local identity_status

  ((activation_existed == 1)) || return 0
  "${cp_binary}" -a -- "${backup_dir}/activation.service" "${activation_file}"
  for bus_path in "${runtime_root}"/[0-9]*/bus; do
    if load_session_identity "${bus_path}"; then
      if ! run_in_session \
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
  if load_session_identity "${bus_path}"; then
    sessions=$((sessions + 1))
    if ! run_in_session \
      VINPUT_DAEMON_SYSTEMCTL="${systemctl_binary}" \
      VINPUT_DAEMON_KILL="${kill_binary}" \
      "${vinput_binary}" daemon prepare-remove --preflight --json; then
      echo "failed to preflight vinput daemon removal for uid ${session_uid}" >&2
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
  echo "vinput removal preflight failed for ${preflight_failures} session(s)" >&2
  if ! reload_restored_activation; then
    echo "vinput removal handoff also failed to restore activation metadata" >&2
  fi
  exit 1
fi

failures=0
for bus_path in "${runtime_root}"/[0-9]*/bus; do
  if load_session_identity "${bus_path}"; then
    if ! run_in_session \
      VINPUT_DAEMON_SYSTEMCTL="${systemctl_binary}" \
      VINPUT_DAEMON_KILL="${kill_binary}" \
      "${vinput_binary}" daemon prepare-remove --json; then
      echo "failed to prepare vinput daemon removal for uid ${session_uid}" >&2
      failures=$((failures + 1))
    fi
  else
    identity_status=$?
    if ((identity_status == 1)); then
      failures=$((failures + 1))
    fi
  fi
done

printf 'vinput removal handoff checked %d live user session(s)\n' "${sessions}"
if ((failures > 0)); then
  echo "vinput removal handoff failed for ${failures} session(s)" >&2
  if ! reload_restored_activation; then
    echo "vinput removal handoff also failed to restore activation metadata" >&2
  fi
  exit 1
fi
