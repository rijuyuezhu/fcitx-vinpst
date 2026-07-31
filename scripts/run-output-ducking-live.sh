#!/usr/bin/env bash
set -euo pipefail

if [[ "${VINPUT_RUN_OUTPUT_DUCKING_LIVE:-}" != 1 ]]; then
  echo "set VINPUT_RUN_OUTPUT_DUCKING_LIVE=1 to run the output-ducking live gate" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

for command in awk cargo jq pw-cli pw-loopback python3 sed wpctl; do
  command -v "${command}" >/dev/null
done

root="${repo_root}/target/tmp/output-ducking-live"
config_path="${root}/config.json"
summary_path="${root}/summary.json"
rm -rf "${root}"
mkdir -p "${root}"

original_sink_info="$(wpctl inspect @DEFAULT_AUDIO_SINK@)"
original_sink_id="$(sed -n '1s/^id \([0-9][0-9]*\),.*/\1/p' <<<"${original_sink_info}")"
original_sink_name="$(sed -n 's/^[[:space:]]*\* node.name = "\([^"]*\)"/\1/p' <<<"${original_sink_info}")"
original_sink_volume="$(wpctl get-volume "${original_sink_id}" | awk '{print $2}')"
[[ "${original_sink_id}" =~ ^[0-9]+$ ]]
test -n "${original_sink_name}"

prefix="vinput_output_ducking_$$"
sink_name="${prefix}_sink"
source_name="${prefix}_source"
loopback_pid=""
daemon_pid=""
default_switched=false
cleanup() {
  local status=$?
  trap - EXIT
  set +e
  if [[ -n "${daemon_pid}" ]]; then
    kill "${daemon_pid}" 2>/dev/null || true
    wait "${daemon_pid}" 2>/dev/null || true
  fi
  if [[ "${default_switched}" == true ]]; then
    wpctl set-default "${original_sink_id}" >/dev/null 2>&1 || true
  fi
  if [[ -n "${loopback_pid}" ]]; then
    kill "${loopback_pid}" 2>/dev/null || true
    wait "${loopback_pid}" 2>/dev/null || true
  fi
  exit "${status}"
}
trap cleanup EXIT

pw-loopback \
  --name "${prefix}_loopback" \
  --channels 1 \
  --channel-map '[ MONO ]' \
  --capture-props "media.class=Audio/Sink node.name=${sink_name} node.description=\"Vinput Output Ducking Sink\" audio.position=[ MONO ]" \
  --playback-props "media.class=Audio/Source node.name=${source_name} node.description=\"Vinput Output Ducking Source\" audio.position=[ MONO ]" \
  >"${root}/pw-loopback.log" 2>&1 &
loopback_pid=$!

for _ in $(seq 1 100); do
  if pw-cli info "${sink_name}" >"${root}/sink-info.txt" 2>/dev/null &&
    pw-cli info "${source_name}" >"${root}/source-info.txt" 2>/dev/null; then
    break
  fi
  sleep 0.03
done

sink_id="$(sed -n 's/^[[:space:]]*id: \([0-9][0-9]*\)$/\1/p' "${root}/sink-info.txt")"
[[ "${sink_id}" =~ ^[0-9]+$ ]]
wpctl set-volume "${sink_id}" 0.80
wpctl set-default "${sink_id}"
default_switched=true
test "$(wpctl inspect @DEFAULT_AUDIO_SINK@ | sed -n '1s/^id \([0-9][0-9]*\),.*/\1/p')" = "${sink_id}"

jq '
  .global.duck_output_while_recording = true
  | .global.duck_output_volume = 0.25
' data/default-config.json >"${config_path}"

cargo build -q -p vinput-daemon --bin vinput-daemon
RUST_LOG=vinput_daemon=debug \
  target/debug/vinput-daemon \
    --once \
    --record-ms 1500 \
    --config "${config_path}" \
    >"${root}/daemon.out" \
    2>"${root}/daemon.log" &
daemon_pid=$!

ducked_volume=""
for _ in $(seq 1 100); do
  current_volume="$(wpctl get-volume "${sink_id}" | awk '{print $2}')"
  if awk -v volume="${current_volume}" 'BEGIN { exit !(volume >= 0.19 && volume <= 0.21) }'; then
    ducked_volume="${current_volume}"
    break
  fi
  sleep 0.03
done
test -n "${ducked_volume}"

wait "${daemon_pid}"
daemon_pid=""
restored_virtual_volume="$(wpctl get-volume "${sink_id}" | awk '{print $2}')"
awk -v volume="${restored_virtual_volume}" 'BEGIN { exit !(volume >= 0.79 && volume <= 0.81) }'

wpctl set-default "${original_sink_id}"
default_switched=false
restored_sink_info="$(wpctl inspect @DEFAULT_AUDIO_SINK@)"
restored_sink_id="$(sed -n '1s/^id \([0-9][0-9]*\),.*/\1/p' <<<"${restored_sink_info}")"
restored_sink_name="$(sed -n 's/^[[:space:]]*\* node.name = "\([^"]*\)"/\1/p' <<<"${restored_sink_info}")"
restored_sink_volume="$(wpctl get-volume "${original_sink_id}" | awk '{print $2}')"
test "${restored_sink_id}" = "${original_sink_id}"
test "${restored_sink_name}" = "${original_sink_name}"
awk -v before="${original_sink_volume}" -v after="${restored_sink_volume}" \
  'BEGIN { difference = before - after; if (difference < 0) difference = -difference; exit !(difference <= 0.001) }'

grep -q 'ducked default output sink' "${root}/daemon.out"
grep -q 'restored default output sink' "${root}/daemon.out"

python3 - \
  "${summary_path}" \
  "${original_sink_id}" \
  "${original_sink_name}" \
  "${original_sink_volume}" \
  "${sink_id}" \
  "${sink_name}" \
  "${ducked_volume}" \
  "${restored_virtual_volume}" \
  "${restored_sink_id}" \
  "${restored_sink_name}" \
  "${restored_sink_volume}" <<'PY'
import json
import pathlib
import sys

(
    output,
    original_sink_id,
    original_sink_name,
    original_sink_volume,
    virtual_sink_id,
    virtual_sink_name,
    ducked_volume,
    restored_virtual_volume,
    restored_sink_id,
    restored_sink_name,
    restored_sink_volume,
) = sys.argv[1:]
summary = {
    "ok": True,
    "recording_backend": "mock",
    "volume_control": "real-wpctl",
    "original_sink": {
        "id": int(original_sink_id),
        "name": original_sink_name,
        "volume": float(original_sink_volume),
    },
    "virtual_sink": {
        "id": int(virtual_sink_id),
        "name": virtual_sink_name,
        "initial_volume": 0.8,
        "duck_scale": 0.25,
        "ducked_volume": float(ducked_volume),
        "restored_volume": float(restored_virtual_volume),
    },
    "restored_sink": {
        "id": int(restored_sink_id),
        "name": restored_sink_name,
        "volume": float(restored_sink_volume),
    },
    "original_default_restored": True,
    "original_volume_unchanged": True,
}
pathlib.Path(output).write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
PY

kill "${loopback_pid}" 2>/dev/null || true
wait "${loopback_pid}" 2>/dev/null || true
loopback_pid=""
for _ in $(seq 1 100); do
  if ! wpctl status -n | grep -Fq "${sink_name}"; then
    break
  fi
  sleep 0.03
done
if wpctl status -n | grep -Fq "${sink_name}"; then
  echo "virtual output-ducking sink remained after cleanup" >&2
  exit 1
fi

trap - EXIT
echo "output ducking live gate passed"
