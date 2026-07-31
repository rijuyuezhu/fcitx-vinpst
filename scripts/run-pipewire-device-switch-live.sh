#!/usr/bin/env bash
set -euo pipefail

if [[ "${VINPUT_RUN_PIPEWIRE_DEVICE_SWITCH_LIVE:-}" != 1 ]]; then
  echo "set VINPUT_RUN_PIPEWIRE_DEVICE_SWITCH_LIVE=1 to run the PipeWire device-switch live gate" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

for command in cargo jq pw-cli pw-loopback pw-play python3 wpctl; do
  command -v "${command}" >/dev/null
done

root="${repo_root}/target/tmp/pipewire-device-switch-live"
summary_path="${root}/summary.json"
rm -rf "${root}"
mkdir -p "${root}"

prefix="vinput_device_switch_$$"
sink_a="${prefix}_sink_a"
source_a="${prefix}_source_a"
sink_b="${prefix}_sink_b"
source_b="${prefix}_source_b"
loopback_a_pid=""
loopback_b_pid=""
play_a_pid=""
play_b_pid=""
cleanup() {
  local status=$?
  trap - EXIT
  set +e
  for pid in "${play_a_pid}" "${play_b_pid}" "${loopback_a_pid}" "${loopback_b_pid}"; do
    if [[ -n "${pid}" ]]; then
      kill "${pid}" 2>/dev/null || true
      wait "${pid}" 2>/dev/null || true
    fi
  done
  exit "${status}"
}
trap cleanup EXIT

python3 - "${root}/tone-a.wav" 523.25 "${root}/tone-b.wav" 783.99 <<'PY'
import math
import struct
import sys
import wave

rate = 16_000
duration_s = 1.0
amplitude = 10_000
for path, frequency in ((sys.argv[1], float(sys.argv[2])), (sys.argv[3], float(sys.argv[4]))):
    samples = [
        int(amplitude * math.sin(2.0 * math.pi * frequency * index / rate))
        for index in range(int(rate * duration_s))
    ]
    with wave.open(path, "wb") as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(rate)
        output.writeframes(b"".join(struct.pack("<h", sample) for sample in samples))
PY

pw-loopback \
  --name "${prefix}_loopback_a" \
  --channels 1 \
  --channel-map '[ MONO ]' \
  --capture-props "media.class=Audio/Sink node.name=${sink_a} node.description=\"Vinput Device Switch Sink A\" audio.position=[ MONO ]" \
  --playback-props "media.class=Audio/Source node.name=${source_a} node.description=\"Vinput Device Switch Source A\" audio.position=[ MONO ]" \
  >"${root}/pw-loopback-a.log" 2>&1 &
loopback_a_pid=$!

pw-loopback \
  --name "${prefix}_loopback_b" \
  --channels 1 \
  --channel-map '[ MONO ]' \
  --capture-props "media.class=Audio/Sink node.name=${sink_b} node.description=\"Vinput Device Switch Sink B\" audio.position=[ MONO ]" \
  --playback-props "media.class=Audio/Source node.name=${source_b} node.description=\"Vinput Device Switch Source B\" audio.position=[ MONO ]" \
  >"${root}/pw-loopback-b.log" 2>&1 &
loopback_b_pid=$!

for _ in $(seq 1 100); do
  if pw-cli info "${sink_a}" >/dev/null 2>&1 &&
    pw-cli info "${source_a}" >/dev/null 2>&1 &&
    pw-cli info "${sink_b}" >/dev/null 2>&1 &&
    pw-cli info "${source_b}" >/dev/null 2>&1; then
    break
  fi
  sleep 0.03
done
for node in "${sink_a}" "${source_a}" "${sink_b}" "${source_b}"; do
  pw-cli info "${node}" >"${root}/${node}.txt"
done

play_forever() {
  local sink="$1"
  local wav="$2"
  local log="$3"
  while true; do
    pw-play --target "${sink}" "${wav}" >>"${log}" 2>&1
  done
}
play_forever "${sink_a}" "${root}/tone-a.wav" "${root}/pw-play-a.log" &
play_a_pid=$!
play_forever "${sink_b}" "${root}/tone-b.wav" "${root}/pw-play-b.log" &
play_b_pid=$!

cargo test -q -p vinput-audio --features pipewire-backend \
  pipewire_recorder_live_rebuilds_for_target_switch_when_enabled --no-run
VINPUT_TEST_PIPEWIRE_SWITCH_SOURCE_A="${source_a}" \
VINPUT_TEST_PIPEWIRE_SWITCH_SOURCE_B="${source_b}" \
VINPUT_TEST_PIPEWIRE_SWITCH_SUMMARY="${summary_path}" \
VINPUT_TEST_PIPEWIRE_RECORD_MS=500 \
VINPUT_TEST_PIPEWIRE_MIN_PEAK=512 \
  cargo test -q -p vinput-audio --features pipewire-backend \
    pipewire_recorder_live_rebuilds_for_target_switch_when_enabled -- --nocapture \
    >"${root}/cargo-test.log" 2>&1

jq -e \
  --arg source_a "${source_a}" \
  --arg source_b "${source_b}" \
  '.ok == true
   and .same_recorder == true
   and .target_switch_rebuilt_stream == true
   and (.recordings | length) == 2
   and .recordings[0].source == $source_a
   and .recordings[1].source == $source_b
   and .recordings[0].reported_source == ("pipewire:" + $source_a)
   and .recordings[1].reported_source == ("pipewire:" + $source_b)
   and .recordings[0].peak_abs >= 512
   and .recordings[1].peak_abs >= 512
   and .recordings[0].created_new_stream == true
   and .recordings[1].created_new_stream == true
   and .recordings[0].stream_reused == false
   and .recordings[1].stream_reused == false' \
  "${summary_path}" >/dev/null

jq --arg capture_device "${source_a}" \
  '.global.capture_device = $capture_device' \
  data/default-config.json >"${root}/daemon-config.json"
cargo build -q -p vinput-daemon --bin vinput-daemon --features pipewire-backend
mkdir -p "${root}/home" "${root}/share" "${root}/config-home"

HOME="${root}/home" \
XDG_DATA_HOME="${root}/share" \
XDG_DATA_DIRS="${root}/share" \
XDG_CONFIG_HOME="${root}/config-home" \
VINPUT_DEVICE_SWITCH_ROOT="${root}" \
VINPUT_DEVICE_SWITCH_SOURCE_A="${source_a}" \
VINPUT_DEVICE_SWITCH_SOURCE_B="${source_b}" \
  timeout 25s dbus-run-session -- bash -euo pipefail <<'INNER'
root="${VINPUT_DEVICE_SWITCH_ROOT}"
source_a="${VINPUT_DEVICE_SWITCH_SOURCE_A}"
source_b="${VINPUT_DEVICE_SWITCH_SOURCE_B}"
daemon_bin="${PWD}/target/debug/vinput-daemon"
config_path="${root}/daemon-config.json"

call_service() {
  local method="$1"
  shift
  gdbus call --session \
    --dest org.fcitx.Vinput \
    --object-path /org/fcitx/Vinput \
    --method "org.fcitx.Vinput.Service.${method}" \
    "$@"
}

owner_pid() {
  gdbus call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.GetConnectionUnixProcessID \
    org.fcitx.Vinput | sed -n 's/.*uint32 \([0-9][0-9]*\).*/\1/p'
}

name_has_owner() {
  gdbus call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.NameHasOwner \
    org.fcitx.Vinput 2>/dev/null | grep -q true
}

finish_recording() {
  local output="$1"
  local status
  status="$(call_service GetStatus)"
  if [[ "${status}" == "('recording',)" ]]; then
    call_service StopRecording "" >"${output}"
  else
    test "${status}" = "('idle',)"
    printf '%s\n' "auto-completed" >"${output}"
  fi
  for _ in $(seq 1 100); do
    test "$(call_service GetStatus)" = "('idle',)" && return 0
    sleep 0.02
  done
  return 1
}

RUST_LOG=vinput_audio=debug \
  "${daemon_bin}" --dbus --audio-backend pipewire --config "${config_path}" \
  >"${root}/daemon-live.log" 2>&1 &
daemon_pid=$!
cleanup_daemon() {
  kill "${daemon_pid}" 2>/dev/null || true
  wait "${daemon_pid}" 2>/dev/null || true
}
trap cleanup_daemon EXIT

for _ in $(seq 1 100); do
  if name_has_owner; then
    break
  fi
  sleep 0.05
done
name_has_owner
test "$(call_service GetStatus)" = "('idle',)"

test "$(call_service GetCaptureDevice)" = "('${source_a}',)"
first_owner="$(owner_pid)"
[[ "${first_owner}" =~ ^[0-9]+$ ]]

call_service StartRecording >/dev/null
for _ in $(seq 1 100); do
  if grep -Fq "target=pipewire:${source_a}" "${root}/daemon-live.log" &&
    grep -Fq 'PipeWire capture received first buffer' "${root}/daemon-live.log"; then
    break
  fi
  sleep 0.02
done
finish_recording "${root}/daemon-stop-a.txt"

test "$(call_service SetCaptureDevice "${source_b}")" = "(true,)"
test "$(call_service GetCaptureDevice)" = "('${source_b}',)"
second_owner="$(owner_pid)"
test "${second_owner}" = "${first_owner}"

call_service StartRecording >/dev/null
for _ in $(seq 1 100); do
  grep -Fq "target=pipewire:${source_b}" "${root}/daemon-live.log" && break
  sleep 0.02
done
finish_recording "${root}/daemon-stop-b.txt"
third_owner="$(owner_pid)"
test "${third_owner}" = "${first_owner}"

printf '%s\n' "${first_owner}" >"${root}/daemon-owner.txt"
INNER

test "$(jq -r '.global.capture_device' "${root}/daemon-config.json")" = "${source_b}"
python3 - "${root}/daemon-live.log" "${root}/daemon-live.clean.log" <<'PY'
import pathlib
import re
import sys

source, output = sys.argv[1:]
text = pathlib.Path(source).read_text(encoding="utf-8")
text = re.sub(r"\x1b\[[0-9;]*[A-Za-z]", "", text)
pathlib.Path(output).write_text(text, encoding="utf-8")
PY
grep -F "target=pipewire:${source_a}" "${root}/daemon-live.clean.log" | grep -Fq 'created_new_stream=true'
grep -F "target=pipewire:${source_b}" "${root}/daemon-live.clean.log" | grep -Fq 'created_new_stream=true'
daemon_owner="$(cat "${root}/daemon-owner.txt")"
jq \
  --arg source_a "${source_a}" \
  --arg source_b "${source_b}" \
  --argjson owner_pid "${daemon_owner}" \
  '.daemon = {
      same_owner: true,
      owner_pid: $owner_pid,
      initial_capture_device: $source_a,
      switched_capture_device: $source_b,
      persisted_capture_device: $source_b,
      source_a_created_new_stream: true,
      source_b_created_new_stream: true
    }' \
  "${summary_path}" >"${root}/summary.next.json"
mv "${root}/summary.next.json" "${summary_path}"

for pid in "${play_a_pid}" "${play_b_pid}" "${loopback_a_pid}" "${loopback_b_pid}"; do
  kill "${pid}" 2>/dev/null || true
  wait "${pid}" 2>/dev/null || true
done
play_a_pid=""
play_b_pid=""
loopback_a_pid=""
loopback_b_pid=""

for _ in $(seq 1 100); do
  if ! wpctl status -n | grep -Fq "${prefix}"; then
    break
  fi
  sleep 0.03
done
if wpctl status -n | grep -Fq "${prefix}"; then
  echo "PipeWire device-switch nodes remained after cleanup" >&2
  exit 1
fi

trap - EXIT
echo "PipeWire device-switch live gate passed"
