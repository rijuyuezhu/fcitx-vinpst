#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

stage_root="${repo_root}/target/tmp/daemon-default-config-smoke"
config_home="${stage_root}/config"
config_path="${config_home}/fcitx-vinput/config.json"
daemon_log="${stage_root}/daemon.log"

rm -rf "${stage_root}"
install -Dm644 data/default-config.json "${config_path}"
cargo build -q -p vinput-daemon --bin vinput-daemon

timeout 20s dbus-run-session -- bash -euo pipefail -c '
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
      --dest org.fcitx.Vinput \
      --object-path /org/fcitx/Vinput \
      --method org.fcitx.Vinput.Service.GetStatus >/dev/null 2>&1; then
      break
    fi
    sleep 0.05
  done

  status=$(gdbus call --session \
    --dest org.fcitx.Vinput \
    --object-path /org/fcitx/Vinput \
    --method org.fcitx.Vinput.Service.GetStatus)
  test "${status}" = "('\''idle'\'',)"

  persisted=$(gdbus call --session \
    --dest org.fcitx.Vinput \
    --object-path /org/fcitx/Vinput \
    --method org.fcitx.Vinput.Service.SetActiveScene \
    __command__)
  test "${persisted}" = "(true,)"
' bash "${repo_root}/target/debug/vinput-daemon" "${config_home}" "${daemon_log}"

jq -e '.scenes.active_scene == "__command__"' "${config_path}" >/dev/null
summary="$({ XDG_CONFIG_HOME="${config_home}" target/debug/vinput-daemon print-config; })"
test "$(jq -r '.active_scene' <<<"${summary}")" = "__command__"

echo "daemon default config discovery smoke passed"
