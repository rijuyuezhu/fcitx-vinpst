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

stage_root="${repo_root}/target/tmp/daemon-unavailable-asr-smoke"
config_home="${stage_root}/config"
config_path="${config_home}/fcitx-vinpst/config.json"
daemon_log="${stage_root}/daemon.log"
dbus_service_dir="${stage_root}/dbus-services"
dbus_config="${stage_root}/session.conf"

rm -rf "${stage_root}"
install -Dm644 data/default-config.json "${config_path}"
mkdir -p "${dbus_service_dir}"
write_isolated_dbus_session_config "${dbus_config}" "${dbus_service_dir}"
cargo build -q -p vinpst-daemon --bin vinpst-daemon --features pipewire-backend

# Match the native package service shape first. Constructing a PipeWire recorder
# must not turn an unconfigured ASR model into a daemon-startup failure. This
# phase never starts recording, so it does not require a live PipeWire server.
timeout 10s dbus-run-session --config-file="${dbus_config}" -- bash -euo pipefail -c '
  daemon_path="$1"
  config_home="$2"
  daemon_log="$3"

  XDG_CONFIG_HOME="${config_home}" "${daemon_path}" --dbus --configured-backends \
    --audio-backend pipewire >"${daemon_log}" 2>&1 &
  daemon_pid=$!
  cleanup() {
    kill "${daemon_pid}" 2>/dev/null || true
    wait "${daemon_pid}" 2>/dev/null || true
  }
  trap cleanup EXIT

  call() {
    gdbus call --session \
      --dest org.fcitx.Vinpst \
      --object-path /org/fcitx/Vinpst \
      --method "org.fcitx.Vinpst.Service.$1" "${@:2}"
  }

  for _ in $(seq 1 100); do
    kill -0 "${daemon_pid}" 2>/dev/null || {
      cat "${daemon_log}" >&2
      echo "spawned Vinpst daemon exited before owning D-Bus" >&2
      exit 1
    }
    owner=$(gdbus call --session \
      --dest org.freedesktop.DBus \
      --object-path /org/freedesktop/DBus \
      --method org.freedesktop.DBus.NameHasOwner org.fcitx.Vinpst)
    if [[ "${owner}" == "(true,)" ]]; then
      break
    fi
    sleep 0.05
  done
  test "${owner}" = "(true,)"

  test "$(call GetStatus)" = "('"'"'idle'"'"',)"
  state=$(call GetAsrBackendState)
  grep -q "sherpa-onnx" <<<"${state}"
  grep -q "false, false" <<<"${state}"
' bash "${repo_root}/target/debug/vinpst-daemon" "${config_home}" "${daemon_log}"

timeout 25s dbus-run-session --config-file="${dbus_config}" -- bash -euo pipefail -c '
  daemon_path="$1"
  config_home="$2"
  config_path="$3"
  daemon_log="$4"

  XDG_CONFIG_HOME="${config_home}" "${daemon_path}" --dbus --configured-backends \
    >"${daemon_log}" 2>&1 &
  daemon_pid=$!
  cleanup() {
    kill "${daemon_pid}" 2>/dev/null || true
    wait "${daemon_pid}" 2>/dev/null || true
  }
  trap cleanup EXIT

  call() {
    gdbus call --session \
      --dest org.fcitx.Vinpst \
      --object-path /org/fcitx/Vinpst \
      --method "org.fcitx.Vinpst.Service.$1" "${@:2}"
  }

  for _ in $(seq 1 100); do
    kill -0 "${daemon_pid}" 2>/dev/null || {
      cat "${daemon_log}" >&2
      echo "spawned Vinpst daemon exited before owning D-Bus" >&2
      exit 1
    }
    owner=$(gdbus call --session \
      --dest org.freedesktop.DBus \
      --object-path /org/freedesktop/DBus \
      --method org.freedesktop.DBus.NameHasOwner org.fcitx.Vinpst)
    if [[ "${owner}" == "(true,)" ]]; then
      break
    fi
    sleep 0.05
  done
  test "${owner}" = "(true,)"

  state=$(call GetAsrBackendState)
  grep -q "sherpa-onnx" <<<"${state}"
  grep -q "false, false" <<<"${state}"
  if call StartRecording >/dev/null 2>&1; then
    echo "unavailable configured ASR unexpectedly started recording" >&2
    exit 1
  fi
  test "$(call GetStatus)" = "('\''idle'\'',)"

  tmp_config="${config_path}.tmp"
  jq '\''.asr.active_provider = "mock" |
      .asr.providers += [{
        "id": "mock",
        "type": "local",
        "model": "mock-model"
      }]'\'' "${config_path}" >"${tmp_config}"
  mv "${tmp_config}" "${config_path}"
  call ReloadAsrBackend >/dev/null

  for _ in $(seq 1 200); do
    state=$(call GetAsrBackendState)
    if grep -q "mock-streaming" <<<"${state}" && grep -q "false, true" <<<"${state}"; then
      break
    fi
    sleep 0.05
  done
  grep -q "mock-streaming" <<<"${state}"
  grep -q "false, true" <<<"${state}"

  call StartRecording >/dev/null
  result=$(call StopRecording "")
  grep -q "mock recognition result" <<<"${result}"
  test "$(call GetStatus)" = "('\''idle'\'',)"
' bash "${repo_root}/target/debug/vinpst-daemon" "${config_home}" "${config_path}" "${daemon_log}"

echo "daemon unavailable ASR recovery smoke passed"
