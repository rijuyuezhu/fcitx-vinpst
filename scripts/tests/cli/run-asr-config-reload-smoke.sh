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

stage_dir="target/tmp/vinpst-cli-asr-reload-smoke"
stage_abs="${repo_root}/${stage_dir}"
service_dir="${stage_abs}/services"
empty_service_dir="${stage_abs}/empty-services"
live_bus_config="${stage_abs}/live-session.conf"
blocked_bus_config="${stage_abs}/blocked-session.conf"
activation_marker="${stage_abs}/activation.marker"
activation_script="${stage_abs}/activation.sh"
cli_bin="${repo_root}/target/debug/vinpst"
daemon_bin="${repo_root}/target/debug/vinpst-daemon"

cargo build -p vinpst-cli -p vinpst-daemon >/dev/null
rm -rf "${stage_dir}"
mkdir -p "${service_dir}" "${empty_service_dir}" "${stage_abs}/config/fcitx-vinpst" \
  "${stage_abs}/data" "${stage_abs}/cache" "${stage_abs}/state"
write_isolated_dbus_session_config "${live_bus_config}" "${empty_service_dir}"
write_isolated_dbus_session_config "${blocked_bus_config}" "${service_dir}"

write_test_config() {
  python3 - "${repo_root}/data/default-config.json" \
    "${stage_abs}/config/fcitx-vinpst/config.json" <<'PY'
import json
import pathlib
import sys

source = pathlib.Path(sys.argv[1])
target = pathlib.Path(sys.argv[2])
config = json.loads(source.read_text())
config["asr"]["active_provider"] = "cmd-a"
config["asr"]["providers"] = [
    {"id": "cmd-a", "type": "command", "command": "/bin/true", "args": []},
    {"id": "cmd-b", "type": "command", "command": "/bin/true", "args": []},
]
target.write_text(json.dumps(config, indent=2) + "\n")
PY
}

write_test_config

# The quoted body is expanded by the child shell, not this script.
# shellcheck disable=SC2016
timeout 15s dbus-run-session --config-file="${live_bus_config}" -- bash -ceu '
  stage="$1"
  cli="$2"
  daemon="$3"
  export XDG_CONFIG_HOME="$stage/config"
  export XDG_DATA_HOME="$stage/data"
  export XDG_CACHE_HOME="$stage/cache"
  export XDG_STATE_HOME="$stage/state"

  "$daemon" --dbus --configured-backends \
    --config "$XDG_CONFIG_HOME/fcitx-vinpst/config.json" \
    >"$stage/daemon.log" 2>&1 &
  daemon_pid=$!
  trap "kill $daemon_pid 2>/dev/null || true; wait $daemon_pid 2>/dev/null || true" EXIT

  ready=false
  for _ in $(seq 1 200); do
    if [[ "$(busctl --user call org.freedesktop.DBus /org/freedesktop/DBus \
      org.freedesktop.DBus NameHasOwner s org.fcitx.Vinpst 2>/dev/null || true)" == "b true" ]]; then
      ready=true
      break
    fi
    sleep 0.02
  done
  [[ "$ready" == true ]] || { cat "$stage/daemon.log" >&2; exit 1; }

  "$cli" daemon status --json >"$stage/before.json"
  python3 - "$stage/before.json" <<"PY"
import json, sys
value = json.load(open(sys.argv[1]))
assert value["asr_backend"]["effective_provider_id"] == "cmd-a", value
PY

  "$cli" provider use cmd-b --in-place --json >"$stage/use.json"
  python3 - "$stage/use.json" <<"PY"
import json, sys
value = json.load(open(sys.argv[1]))
assert value["wrote_config"] is True, value
assert value["reloaded_daemon"] is True, value
PY

  applied=false
  for _ in $(seq 1 200); do
    "$cli" daemon status --json >"$stage/after.json"
    if python3 - "$stage/after.json" <<"PY"
import json, sys
value = json.load(open(sys.argv[1]))
raise SystemExit(0 if value["asr_backend"]["effective_provider_id"] == "cmd-b" else 1)
PY
    then
      applied=true
      break
    fi
    sleep 0.02
  done
  [[ "$applied" == true ]]
' bash "${stage_abs}" "${cli_bin}" "${daemon_bin}"

write_test_config
cat >"${activation_script}" <<ACTIVATION
#!/bin/sh
echo activated >"${activation_marker}"
exec sleep 10
ACTIVATION
chmod +x "${activation_script}"
cat >"${service_dir}/org.fcitx.Vinpst.service" <<SERVICE
[D-BUS Service]
Name=org.fcitx.Vinpst
Exec=${activation_script}
SERVICE
rm -f "${activation_marker}"

# The quoted body is expanded by the child shell, not this script.
# shellcheck disable=SC2016
timeout 10s dbus-run-session --config-file="${blocked_bus_config}" -- bash -ceu '
  stage="$1"
  cli="$2"
  marker="$3"
  export XDG_CONFIG_HOME="$stage/config"
  export XDG_DATA_HOME="$stage/data"
  export XDG_CACHE_HOME="$stage/cache"
  export XDG_STATE_HOME="$stage/state"

  "$cli" provider use cmd-b --in-place --json >"$stage/no-daemon-use.json"
  python3 - "$stage/no-daemon-use.json" "$XDG_CONFIG_HOME/fcitx-vinpst/config.json" <<"PY"
import json, sys
value = json.load(open(sys.argv[1]))
config = json.load(open(sys.argv[2]))
assert value["wrote_config"] is True, value
assert value["reloaded_daemon"] is False, value
assert config["asr"]["active_provider"] == "cmd-b", config
PY

  [[ ! -e "$marker" ]] || { echo "canonical config mutation activated the daemon" >&2; exit 1; }
  [[ "$(busctl --user call org.freedesktop.DBus /org/freedesktop/DBus \
    org.freedesktop.DBus NameHasOwner s org.fcitx.Vinpst)" == "b false" ]]
  [[ ! -e "$marker" ]] || { echo "owner probe activated the daemon" >&2; exit 1; }
' bash "${stage_abs}" "${cli_bin}" "${activation_marker}"
