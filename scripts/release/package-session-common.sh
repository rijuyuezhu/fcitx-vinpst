#!/usr/bin/env bash
# shellcheck shell=bash

# Shared trusted-session helpers for package lifecycle dispatchers.
# Callers must define: runuser_binary, env_binary, getent_binary, stat_binary,
# and gdbus_binary before using these functions.

: "${runuser_binary:?package session caller must set runuser_binary}"
: "${env_binary:?package session caller must set env_binary}"
: "${getent_binary:?package session caller must set getent_binary}"
: "${stat_binary:?package session caller must set stat_binary}"
: "${gdbus_binary:?package session caller must set gdbus_binary}"

session_runtime_dir=""
session_uid=""
session_user=""
session_home=""
session_bus_path=""

vinpst_package_load_session_identity() {
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

vinpst_package_run_in_session() {
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

vinpst_package_session_name_has_owner() {
  local service_name="$1"
  local output

  if ! output="$(
    vinpst_package_run_in_session \
      "${gdbus_binary}" call --session \
      --dest org.freedesktop.DBus \
      --object-path /org/freedesktop/DBus \
      --method org.freedesktop.DBus.NameHasOwner "${service_name}"
  )"; then
    return 2
  fi
  case "${output}" in
    *true*) return 0 ;;
    *false*) return 1 ;;
    *) return 2 ;;
  esac
}
