#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
runtime_root="${VINPUT_UPGRADE_RUNTIME_ROOT:-/run/user}"
vinput_binary="${VINPUT_UPGRADE_VINPUT:-/usr/bin/vinput}"
runuser_binary="${VINPUT_UPGRADE_RUNUSER:-/usr/bin/runuser}"
env_binary="${VINPUT_UPGRADE_ENV:-/usr/bin/env}"
getent_binary="${VINPUT_UPGRADE_GETENT:-/usr/bin/getent}"
stat_binary="${VINPUT_UPGRADE_STAT:-/usr/bin/stat}"
gdbus_binary="${VINPUT_UPGRADE_GDBUS:-/usr/bin/gdbus}"
systemctl_binary="${VINPUT_UPGRADE_SYSTEMCTL:-/usr/bin/systemctl}"
kill_binary="${VINPUT_UPGRADE_KILL:-/usr/bin/kill}"

# shellcheck source=package-session-common.sh
source "${script_dir}/package-session-common.sh"

session_uid=""

for command in \
  "${vinput_binary}" \
  "${runuser_binary}" \
  "${env_binary}" \
  "${getent_binary}" \
  "${stat_binary}" \
  "${gdbus_binary}" \
  "${systemctl_binary}" \
  "${kill_binary}"; do
  if [[ ! -x "${command}" ]]; then
    echo "required upgrade-handoff command is missing: ${command}" >&2
    exit 1
  fi
done

sessions=0
owners=0
failures=0
shopt -s nullglob
for bus_path in "${runtime_root}"/[0-9]*/bus; do
  if vinput_package_load_session_identity "${bus_path}"; then
    sessions=$((sessions + 1))
    if vinput_package_session_name_has_owner org.fcitx.Vinput; then
      owners=$((owners + 1))
      if ! vinput_package_run_in_session \
        VINPUT_DAEMON_SYSTEMCTL="${systemctl_binary}" \
        VINPUT_DAEMON_KILL="${kill_binary}" \
        "${vinput_binary}" daemon handoff --json; then
        echo "failed to hand off vinput daemon upgrade for uid ${session_uid}" >&2
        failures=$((failures + 1))
      fi
    else
      owner_status=$?
      if ((owner_status == 2)); then
        echo "failed to query vinput daemon owner for uid ${session_uid}" >&2
        failures=$((failures + 1))
      fi
    fi
  else
    identity_status=$?
    if ((identity_status == 1)); then
      failures=$((failures + 1))
    fi
  fi
done

printf 'vinput upgrade handoff checked %d live user session(s), %d active owner(s)\n' \
  "${sessions}" "${owners}"
if ((failures > 0)); then
  echo "vinput upgrade handoff failed for ${failures} session(s)" >&2
  exit 1
fi
