#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

stage_root="${repo_root}/target/tmp/daemon-unavailable-asr-smoke"
config_home="${stage_root}/config"
config_path="${config_home}/fcitx-vinput/config.json"
daemon_log="${stage_root}/daemon.log"

rm -rf "${stage_root}"
install -Dm644 data/default-config.json "${config_path}"
cargo build -q -p vinput-daemon --bin vinput-daemon

timeout 25s dbus-run-session -- bash -euo pipefail -c '
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
      --dest org.fcitx.Vinput \
      --object-path /org/fcitx/Vinput \
      --method "org.fcitx.Vinput.Service.$1" "${@:2}"
  }

  for _ in $(seq 1 100); do
    if call GetStatus >/dev/null 2>&1; then
      break
    fi
    sleep 0.05
  done

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
' bash "${repo_root}/target/debug/vinput-daemon" "${config_home}" "${config_path}" "${daemon_log}"

echo "daemon unavailable ASR recovery smoke passed"
