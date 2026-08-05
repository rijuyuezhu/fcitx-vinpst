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

fcitx5_bin="${VINPST_LIVE_FCITX5_BIN:-fcitx5}"
fcitx5_remote_bin="${VINPST_LIVE_FCITX5_REMOTE_BIN:-fcitx5-remote}"
gdbus_bin="${VINPST_LIVE_GDBUS_BIN:-gdbus}"
python_bin="${VINPST_LIVE_PYTHON_BIN:-python3}"

home_dir="${HOME:?HOME must be set for live probe}"
data_home="${XDG_DATA_HOME:-${home_dir}/.local/share}"
default_data_home="${home_dir}/.local/share"
config_home="${XDG_CONFIG_HOME:-${home_dir}/.config}"
bin_dir="${VINPST_USER_BIN_DIR:-${home_dir}/.local/bin}"
lib_dir="${VINPST_USER_FCITX_LIB_DIR:-${home_dir}/.local/lib/fcitx5}"
addon_dir="${VINPST_USER_FCITX_ADDON_DIR:-${data_home}/fcitx5/addon}"
config_dir="${VINPST_USER_CONFIG_DIR:-${data_home}/fcitx-vinpst}"
autostart_dir="${VINPST_USER_AUTOSTART_DIR:-${config_home}/autostart}"
env_file="${config_dir}/fcitx-vinpst.env"
fcitx_env_wrapper="${config_dir}/fcitx5-with-vinpst-env.sh"
fcitx_autostart_file="${autostart_dir}/org.fcitx.Fcitx5.desktop"
daemon_path="${VINPST_USER_DAEMON:-${bin_dir}/vinpst-daemon}"
daemon_env_wrapper="${config_dir}/vinpst-daemon-with-vinpst-env.sh"
module_path="${lib_dir}/fcitx5-vinpst.so"
addon_conf_path="${addon_dir}/vinpst.conf"
persistent_service_file="${data_home}/dbus-1/services/org.fcitx.Vinpst.service"
runtime_service_file="${XDG_RUNTIME_DIR:-}/dbus-1/services/org.fcitx.Vinpst.service"
service_file="${persistent_service_file}"
if [[ -n "${XDG_RUNTIME_DIR:-}" && -f "${runtime_service_file}" ]]; then
  service_file="${runtime_service_file}"
fi
status_log="${TMPDIR:-/tmp}/vinpst-ime-live-status.$$.log"

failures=()
warnings=()

add_failure() {
  local code="$1"
  shift
  failures+=("${code}: $*")
  printf 'FAIL[%s] %s\n' "${code}" "$*" >&2
}

add_warning() {
  local code="$1"
  shift
  warnings+=("${code}: $*")
  printf 'WARN[%s] %s\n' "${code}" "$*" >&2
}

has_failures() {
  [[ "${#failures[@]}" -gt 0 ]]
}

print_summary_and_exit_if_failed() {
  if ! has_failures; then
    return 0
  fi

  printf '\nLive probe failed with classified issues:\n' >&2
  local item
  for item in "${failures[@]}"; do
    printf '  - %s\n' "${item}" >&2
  done
  if [[ "${#warnings[@]}" -gt 0 ]]; then
    printf '\nAdditional warnings:\n' >&2
    for item in "${warnings[@]}"; do
      printf '  - %s\n' "${item}" >&2
    done
  fi
  printf '\nSuggested next step: run VINPST_LIVE_INSTALL_COMMAND_DEMO=1 just ime-fcitx-live-probe, then restart Fcitx5 with %s -dr. If your desktop ignores the generated autostart override, source %s before launching Fcitx5.\n' "${fcitx_env_wrapper}" "${env_file}" >&2
  printf 'If stale-bus-owner is listed and the displayed process is safe to stop, rerun with VINPST_LIVE_STOP_STALE_OWNER=1 to stop it before probing activation.\n' >&2
  exit 1
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    add_failure "missing-command" "required command is not available: $1"
  fi
}

service_field() {
  local path="$1"
  local key="$2"
  "${python_bin}" - "$path" "$key" <<'PY'
import sys
path, key = sys.argv[1], sys.argv[2]
prefix = f"{key}="
try:
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if line.startswith(prefix):
                print(line[len(prefix):].strip())
                raise SystemExit(0)
except OSError:
    pass
raise SystemExit(0)
PY
}

service_exec_daemon() {
  local path="$1"
  "${python_bin}" - "$path" <<'PY'
import shlex
import sys
path = sys.argv[1]
try:
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("Exec="):
                parts = shlex.split(line.removeprefix("Exec=").strip())
                if parts:
                    print(parts[0])
                raise SystemExit(0)
except OSError:
    pass
raise SystemExit(0)
PY
}

wrapper_exec_daemon() {
  local path="$1"
  "${python_bin}" - "$path" <<'PY'
import shlex
import sys

path = sys.argv[1]
try:
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            stripped = line.strip()
            if not stripped.startswith("exec "):
                continue
            parts = shlex.split(stripped)
            if len(parts) >= 2:
                print(parts[1])
            raise SystemExit(0)
except OSError:
    pass
raise SystemExit(0)
PY
}

same_path() {
  local left="$1"
  local right="$2"
  "${python_bin}" - "$left" "$right" <<'PY'
import os
import sys
left, right = sys.argv[1], sys.argv[2]
raise SystemExit(0 if os.path.realpath(left) == os.path.realpath(right) else 1)
PY
}

bus_owner_from_reply() {
  local reply="$1"
  "${python_bin}" - "${reply}" <<'PY'
import ast
import sys
text = sys.argv[1]
try:
    value = ast.literal_eval(text)
    if isinstance(value, tuple) and value:
        print(value[0])
except Exception:
    pass
PY
}


bus_pid_from_reply() {
  local reply="$1"
  "${python_bin}" - "${reply}" <<'PY'
import re
import sys
match = re.search(r"\b(\d+)\b", sys.argv[1])
if match:
    print(match.group(1))
PY
}

query_bus_owner_pid() {
  local owner="$1"
  local pid_output pid_status pid
  set +e
  pid_output="$(${gdbus_bin} call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.GetConnectionUnixProcessID \
    "${owner}" 2>&1)"
  pid_status=$?
  set -e
  if [[ "${pid_status}" != "0" ]]; then
    add_warning "bus-owner-process-unavailable" "could not query process id for ${owner}: ${pid_output}"
    return 0
  fi
  pid="$(bus_pid_from_reply "${pid_output}")"
  if [[ -z "${pid}" ]]; then
    add_warning "bus-owner-process-unavailable" "could not parse process id for ${owner}: ${pid_output}"
    return 0
  fi
  printf '%s\n' "${pid}"
}

describe_bus_owner_process() {
  local owner="$1"
  local pid="$2"
  local exe cmdline
  if [[ -z "${pid}" ]]; then
    return 0
  fi
  exe="$(readlink "/proc/${pid}/exe" 2>/dev/null || true)"
  cmdline="$(tr '\0' ' ' <"/proc/${pid}/cmdline" 2>/dev/null || true)"
  printf 'Current org.fcitx.Vinpst owner process: pid=%s exe=%s cmdline=%s\n' \
    "${pid}" "${exe:-<unavailable>}" "${cmdline:-<unavailable>}"
}

stop_stale_bus_owner_if_requested() {
  local owner="$1"
  local pid="$2"
  local exe remaining owner_check owner_status
  if [[ "${VINPST_LIVE_STOP_STALE_OWNER:-}" != "1" && "${VINPST_LIVE_STOP_STALE_OWNER:-}" != "true" ]]; then
    return 1
  fi
  if [[ ! -f "${service_file}" ]]; then
    add_warning "stale-owner-stop-skipped" "not stopping ${owner} because user activation service is missing"
    return 1
  fi
  if [[ -z "${pid}" ]]; then
    add_warning "stale-owner-stop-skipped" "not stopping ${owner} because its process id is unknown"
    return 1
  fi
  exe="$(readlink "/proc/${pid}/exe" 2>/dev/null || true)"
  if [[ -z "${exe}" ]]; then
    add_warning "stale-owner-stop-skipped" "not stopping ${owner} because /proc/${pid}/exe is unavailable"
    return 1
  fi
  if same_path "${exe}" "${daemon_path}"; then
    add_warning "stale-owner-stop-skipped" "not stopping ${owner}; it already points to the expected user daemon ${daemon_path}"
    return 1
  fi
  kill "${pid}" 2>/dev/null || {
    add_warning "stale-owner-stop-failed" "failed to stop stale org.fcitx.Vinpst owner pid ${pid} (${exe})"
    return 1
  }
  printf 'Stopped stale org.fcitx.Vinpst owner process pid=%s exe=%s.\n' "${pid}" "${exe}"
  remaining=20
  while [[ "${remaining}" -gt 0 ]]; do
    set +e
    owner_check="$(${gdbus_bin} call --session \
      --dest org.freedesktop.DBus \
      --object-path /org/freedesktop/DBus \
      --method org.freedesktop.DBus.GetNameOwner \
      org.fcitx.Vinpst 2>&1)"
    owner_status=$?
    set -e
    if [[ "${owner_status}" != "0" ]]; then
      return 0
    fi
    sleep 0.1
    remaining=$((remaining - 1))
  done
  add_warning "stale-owner-stop-timeout" "stale org.fcitx.Vinpst owner pid ${pid} was signalled but the bus name is still owned: ${owner_check}"
  return 1
}

check_install_shape() {
  if [[ -f "${module_path}" ]]; then
    printf 'User addon module: %s (present)\n' "${module_path}"
  else
    add_failure "addon-module-missing" "user addon module is missing: ${module_path}"
  fi

  if [[ -f "${addon_conf_path}" ]]; then
    printf 'User addon metadata: %s (present)\n' "${addon_conf_path}"
    if ! grep -qx 'Library=fcitx5-vinpst' "${addon_conf_path}"; then
      add_failure "addon-metadata-library-mismatch" "addon metadata does not declare Library=fcitx5-vinpst: ${addon_conf_path}"
    fi
    if ! grep -qx 'Type=SharedLibrary' "${addon_conf_path}"; then
      add_failure "addon-metadata-type-mismatch" "addon metadata does not declare Type=SharedLibrary: ${addon_conf_path}"
    fi
  else
    add_failure "addon-metadata-missing" "user addon metadata is missing: ${addon_conf_path}"
  fi

  if [[ -x "${daemon_path}" ]]; then
    printf 'User daemon: %s (executable)\n' "${daemon_path}"
  else
    add_failure "daemon-missing" "user daemon is missing or not executable: ${daemon_path}"
  fi

  if [[ -f "${service_file}" ]]; then
    printf 'User activation service: %s (present)\n' "${service_file}"
    local service_name
    service_name="$(service_field "${service_file}" Name)"
    if [[ "${service_name}" != "org.fcitx.Vinpst" ]]; then
      add_failure "activation-service-name-mismatch" "activation service Name is '${service_name:-<missing>}' instead of org.fcitx.Vinpst: ${service_file}"
    fi

    local service_daemon
    service_daemon="$(service_exec_daemon "${service_file}")"
    if [[ -z "${service_daemon}" ]]; then
      add_failure "activation-service-exec-missing" "activation service has no Exec daemon path: ${service_file}"
    elif same_path "${service_daemon}" "${daemon_path}"; then
      printf 'Activation service daemon: %s (matches user daemon)\n' "${service_daemon}"
    elif same_path "${service_daemon}" "${daemon_env_wrapper}"; then
      local wrapped_daemon
      wrapped_daemon="$(wrapper_exec_daemon "${daemon_env_wrapper}")"
      if [[ -n "${wrapped_daemon}" ]] && same_path "${wrapped_daemon}" "${daemon_path}"; then
        printf 'Activation service daemon: %s (native runtime wrapper for %s)\n' "${service_daemon}" "${daemon_path}"
      else
        add_failure "activation-service-wrapper-mismatch" "activation wrapper '${service_daemon}' does not exec '${daemon_path}'"
      fi
    else
      add_failure "activation-service-old-daemon" "activation service points to '${service_daemon}', expected '${daemon_path}'"
    fi
  else
    add_failure "activation-service-missing" "user D-Bus activation service is missing: ${service_file}"
  fi

  if [[ -f "${env_file}" ]]; then
    printf 'Fcitx environment file: %s (present)\n' "${env_file}"
  else
    add_warning "fcitx-env-file-missing" "generated Fcitx environment file is missing: ${env_file}"
  fi

  if [[ -x "${fcitx_env_wrapper}" ]]; then
    printf 'Fcitx env wrapper: %s (executable)\n' "${fcitx_env_wrapper}"
  else
    add_failure "fcitx-env-wrapper-missing" "generated Fcitx env wrapper is missing or not executable: ${fcitx_env_wrapper}"
  fi

  if [[ -f "${fcitx_autostart_file}" ]]; then
    printf 'Fcitx autostart override: %s (present)\n' "${fcitx_autostart_file}"
    if ! grep -qx "Exec=${fcitx_env_wrapper}" "${fcitx_autostart_file}"; then
      add_failure "fcitx-autostart-exec-mismatch" "Fcitx autostart override does not Exec the generated wrapper: ${fcitx_autostart_file}"
    fi
    if ! grep -qx 'X-fcitx-vinpst-managed=true' "${fcitx_autostart_file}"; then
      add_warning "fcitx-autostart-unmanaged" "Fcitx autostart override is not marked as managed by fcitx-vinpst: ${fcitx_autostart_file}"
    fi
  else
    add_failure "fcitx-autostart-missing" "generated Fcitx autostart override is missing: ${fcitx_autostart_file}"
  fi
}

check_fcitx_process_env() {
  if [[ "${VINPST_LIVE_SKIP_FCITX_ENV_CHECK:-}" == "1" ]]; then
    return 0
  fi
  if [[ ! -f "${module_path}" || ! -f "${addon_conf_path}" ]]; then
    return 0
  fi
  if ! command -v pgrep >/dev/null 2>&1; then
    add_warning "fcitx-env-unchecked" "pgrep is not available; cannot inspect the running Fcitx5 process environment"
    return 0
  fi

  local pids
  pids="$(pgrep -u "$(id -u)" -x fcitx5 2>/dev/null || true)"
  if [[ -z "${pids}" ]]; then
    add_warning "fcitx-env-unchecked" "fcitx5-remote reports Fcitx5 running, but no fcitx5 process was found for this user"
    return 0
  fi

  local pid env_text addon_dirs xdg_data_home has_matching_process=0 inspected=0
  for pid in ${pids}; do
    if [[ ! -r "/proc/${pid}/environ" ]]; then
      continue
    fi
    inspected=1
    env_text="$(tr '\0' '\n' <"/proc/${pid}/environ" || true)"
    addon_dirs="$(printf '%s\n' "${env_text}" | sed -n 's/^FCITX_ADDON_DIRS=//p' | tail -n 1)"
    xdg_data_home="$(printf '%s\n' "${env_text}" | sed -n 's/^XDG_DATA_HOME=//p' | tail -n 1)"
    if [[ ":${addon_dirs}:" != *":${lib_dir}:"* ]]; then
      continue
    fi
    if [[ "${data_home}" != "${default_data_home}" && "${xdg_data_home}" != "${data_home}" ]]; then
      continue
    fi
    has_matching_process=1
    break
  done

  if [[ "${inspected}" == "0" ]]; then
    add_warning "fcitx-env-unchecked" "no readable fcitx5 process environment was found"
  elif [[ "${has_matching_process}" == "0" ]]; then
    add_failure "fcitx-env-not-restarted" "Fcitx5 is running without the generated user addon environment; restart it with ${fcitx_env_wrapper} -dr or source ${env_file} before launching Fcitx5"
  else
    printf 'Fcitx5 process environment includes the user addon path.\n'
  fi
}

probe_runtime_status() {
  local owner_output owner_status owner owner_pid runtime_output runtime_status
  set +e
  owner_output="$(${gdbus_bin} call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.GetNameOwner \
    org.fcitx.Vinpst 2>&1)"
  owner_status=$?
  set -e

  owner=""
  if [[ "${owner_status}" == "0" ]]; then
    owner="$(bus_owner_from_reply "${owner_output}")"
    if [[ -n "${owner}" ]]; then
      add_warning "bus-name-owned" "org.fcitx.Vinpst is already owned on the current session bus by ${owner}; probing it for Rust runtime diagnostics"
      owner_pid="$(query_bus_owner_pid "${owner}")"
      describe_bus_owner_process "${owner}" "${owner_pid}"
      if stop_stale_bus_owner_if_requested "${owner}" "${owner_pid}"; then
        owner=""
      fi
    fi
  fi

  if [[ ! -f "${service_file}" && -z "${owner}" ]]; then
    add_failure "runtime-status-skipped" "cannot activate org.fcitx.Vinpst because the activation service is missing and no current bus owner exists"
    return 0
  fi

  printf 'Probing org.fcitx.Vinpst GetRuntimeStatus...\n'
  set +e
  runtime_output="$(${gdbus_bin} call --session \
    --dest org.fcitx.Vinpst \
    --object-path /org/fcitx/Vinpst \
    --method org.fcitx.Vinpst.Service.GetRuntimeStatus 2>&1)"
  runtime_status=$?
  set -e

  if [[ "${runtime_status}" == "0" ]]; then
    printf '%s\n' "${runtime_output}"
    return 0
  fi

  if grep -qiE 'UnknownMethod|No such method|GetRuntimeStatus' <<<"${runtime_output}"; then
    add_failure "runtime-status-unavailable" "GetRuntimeStatus is unavailable on org.fcitx.Vinpst; this usually means the current bus owner is the legacy/stale daemon, not the Rust daemon"
  else
    add_failure "runtime-status-call-failed" "GetRuntimeStatus call failed: ${runtime_output}"
  fi

  if [[ -n "${owner}" ]]; then
    add_failure "stale-bus-owner" "org.fcitx.Vinpst was already owned by ${owner} before activation and did not expose Rust runtime diagnostics"
  fi
}

if [[ "${VINPST_LIVE_INSTALL_COMMAND_DEMO:-}" == "1" || "${VINPST_LIVE_INSTALL_COMMAND_DEMO:-}" == "true" ]]; then
  printf 'Installing command-demo user IME profile because VINPST_LIVE_INSTALL_COMMAND_DEMO is set.\n'
  VINPST_USER_PROFILE=command-demo scripts/install/install-user-ime.sh
else
  printf 'Non-mutating live probe. Set VINPST_LIVE_INSTALL_COMMAND_DEMO=1 to install the command-demo user IME profile.\n'
fi

require_cmd "${fcitx5_bin}"
require_cmd "${fcitx5_remote_bin}"
require_cmd "${gdbus_bin}"
require_cmd "${python_bin}"
if has_failures; then
  print_summary_and_exit_if_failed
fi

if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  printf 'FAIL[user-dbus-session-missing] DBUS_SESSION_BUS_ADDRESS is not set; run this inside a desktop user session.\n' >&2
  exit 2
fi

if ! "${fcitx5_remote_bin}" --check >/dev/null 2>&1; then
  printf 'FAIL[fcitx5-not-running] Fcitx5 is not running on the current session bus.\n' >&2
  printf 'Start or restart Fcitx5 after installing the addon, then retry.\n' >&2
  exit 2
fi

printf 'Fcitx5 is running.\n'
printf 'Fcitx DBus address: %s\n' "$("${fcitx5_remote_bin}" -a 2>/dev/null || true)"
printf 'Current input method group: %s\n' "$("${fcitx5_remote_bin}" -q 2>/dev/null || true)"
printf 'Current input method: %s\n' "$("${fcitx5_remote_bin}" -n 2>/dev/null || true)"

check_install_shape
check_fcitx_process_env

if [[ "${VINPST_LIVE_SKIP_USER_STATUS:-}" != "1" ]]; then
  VINPST_USER_STATUS=1 scripts/install/install-user-ime.sh >"${status_log}" 2>&1 || {
    cat "${status_log}" >&2
    add_failure "user-status-failed" "User IME install/status check failed"
  }
  cat "${status_log}"
fi

probe_runtime_status
print_summary_and_exit_if_failed

printf 'Live probe complete. Trigger keys are controlled by VINPST_FCITX_NORMAL_TRIGGER and VINPST_FCITX_COMMAND_TRIGGER before launching Fcitx5.\n'
