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

just addon-build
cargo build -q -p vinpst-daemon

dbus_root="target/tmp/vinpst-cpp-dbus-smoke-bus"
dbus_service_dir="${repo_root}/${dbus_root}/services"
dbus_config="${repo_root}/${dbus_root}/session.conf"
rm -rf "${dbus_root}"
mkdir -p "${dbus_service_dir}"
write_isolated_dbus_session_config "${dbus_config}" "${dbus_service_dir}"

dbus-run-session --config-file="${dbus_config}" -- bash -euo pipefail <<'INNER'
log_file="target/tmp/vinpst-cpp-dbus-smoke-daemon.log"
config_home="target/tmp/vinpst-cpp-dbus-smoke-config"
cache_home="target/tmp/vinpst-cpp-dbus-smoke-cache"
bridge_smoke_bin="target/cpp/fcitx5-addon/vinpst_fcitx_bridge_dbus_smoke"
addon_smoke_bin="target/cpp/fcitx5-addon/vinpst_fcitx_addon_dbus_smoke"
native_addon_smoke_bin="target/cpp/fcitx5-addon/vinpst_fcitx_native_addon_dbus_smoke"
mkdir -p "$(dirname "${log_file}")"
rm -rf "${config_home}" "${cache_home}"
mkdir -p "${config_home}" "${cache_home}"
export XDG_CONFIG_HOME="$(pwd)/${config_home}"
export XDG_CACHE_HOME="$(pwd)/${cache_home}"

daemon_pid=""

start_daemon() {
  target/debug/vinpst-daemon --dbus >"${log_file}" 2>&1 &
  daemon_pid=$!
}

stop_daemon() {
  if [[ -n "${daemon_pid}" ]]; then
    kill "${daemon_pid}" >/dev/null 2>&1 || true
    wait "${daemon_pid}" >/dev/null 2>&1 || true
    daemon_pid=""
  fi
}

cleanup() {
  stop_daemon
}
trap cleanup EXIT

wait_for_spawned_owner() {
  local owner_pid
  for _ in $(seq 1 100); do
    if ! kill -0 "${daemon_pid}" >/dev/null 2>&1; then
      cat "${log_file}" >&2
      echo "spawned Vinpst daemon exited before owning D-Bus" >&2
      return 1
    fi
    if [[ "$(busctl --user call org.freedesktop.DBus /org/freedesktop/DBus \
      org.freedesktop.DBus NameHasOwner s org.fcitx.Vinpst)" == "b true" ]]; then
      owner_pid="$(busctl --user call org.freedesktop.DBus /org/freedesktop/DBus \
        org.freedesktop.DBus GetConnectionUnixProcessID s org.fcitx.Vinpst | awk '$1 == "u" { print $2 }')"
      if [[ "${owner_pid}" == "${daemon_pid}" ]]; then
        return 0
      fi
      echo "unexpected Vinpst D-Bus owner pid: ${owner_pid}; expected ${daemon_pid}" >&2
      return 1
    fi
    sleep 0.05
  done
  cat "${log_file}" >&2
  echo "spawned Vinpst daemon did not acquire D-Bus ownership" >&2
  return 1
}

start_daemon
wait_for_spawned_owner
"${bridge_smoke_bin}"
stop_daemon

start_daemon
wait_for_spawned_owner
if [[ -x "${addon_smoke_bin}" ]]; then
  "${addon_smoke_bin}"
fi
if [[ -x "${native_addon_smoke_bin}" ]]; then
  VINPST_NATIVE_ADDON_MENU_PROBE=scene-ready "${native_addon_smoke_bin}"
  VINPST_NATIVE_ADDON_MENU_PROBE=asr-ready "${native_addon_smoke_bin}"
fi
stop_daemon
trap - EXIT
INNER
