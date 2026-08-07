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

stage_root="${repo_root}/target/tmp/daemon-default-config-smoke"
config_home="${stage_root}/config"
config_path="${config_home}/fcitx-vinpst/config.json"
daemon_log="${stage_root}/daemon.log"
dbus_service_dir="${stage_root}/dbus-services"
dbus_config="${stage_root}/session.conf"

rm -rf "${stage_root}"
install -Dm644 data/default-config.json "${config_path}"
mkdir -p "${dbus_service_dir}"
write_isolated_dbus_session_config "${dbus_config}" "${dbus_service_dir}"
cargo build -q -p vinpst-daemon --bin vinpst-daemon

timeout 20s dbus-run-session --config-file="${dbus_config}" -- bash -euo pipefail -c '
  daemon_path="$1"
  config_home="$2"
  daemon_log="$3"

  XDG_CONFIG_HOME="${config_home}" "${daemon_path}" --dbus >"${daemon_log}" 2>&1 &
  daemon_pid=$!
  cleanup() {
    kill "${daemon_pid}" 2>/dev/null || true
    wait "${daemon_pid}" 2>/dev/null || true
  }
  trap cleanup EXIT

  for _ in $(seq 1 100); do
    if gdbus call --session \
      --dest org.fcitx.Vinpst \
      --object-path /org/fcitx/Vinpst \
      --method org.fcitx.Vinpst.Service.GetStatus >/dev/null 2>&1; then
      break
    fi
    sleep 0.05
  done

  status=$(gdbus call --session \
    --dest org.fcitx.Vinpst \
    --object-path /org/fcitx/Vinpst \
    --method org.fcitx.Vinpst.Service.GetStatus)
  test "${status}" = "('\''idle'\'',)"

  persisted=$(gdbus call --session \
    --dest org.fcitx.Vinpst \
    --object-path /org/fcitx/Vinpst \
    --method org.fcitx.Vinpst.Service.SetActiveScene \
    __command__)
  test "${persisted}" = "(true,)"
' bash "${repo_root}/target/debug/vinpst-daemon" "${config_home}" "${daemon_log}"

jq -e '.scenes.active_scene == "__command__"' "${config_path}" >/dev/null
summary="$({ XDG_CONFIG_HOME="${config_home}" target/debug/vinpst-daemon print-config; })"
test "$(jq -r '.active_scene' <<<"${summary}")" = "__command__"

echo "daemon default config discovery smoke passed"
