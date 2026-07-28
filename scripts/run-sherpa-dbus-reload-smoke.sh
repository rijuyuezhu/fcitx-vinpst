#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

model_dir="${VINPUT_SHERPA_MODEL:-}"
wav_path="${VINPUT_SHERPA_WAV:-}"
expected_family="${VINPUT_SHERPA_EXPECT_FAMILY:-moonshine}"
expected_text="${VINPUT_SHERPA_EXPECT_TEXT:-After early nightfall, the yellow lamps would light up here and there the squalid quarter of the brothels.}"
out_dir="${VINPUT_SHERPA_RELOAD_SMOKE_DIR:-target/tmp/sherpa-dbus-reload-smoke}"
initial_config="${out_dir}/initial-config.json"
reload_config="${out_dir}/reload-config.json"
active_config="${out_dir}/active-config.json"
reload_output="${out_dir}/reload-output.json"
status_output="${out_dir}/status-output.json"
stop_output="${out_dir}/stop-output.json"
daemon_log="${out_dir}/daemon.log"

if [[ -z "${model_dir}" ]]; then
  echo "VINPUT_SHERPA_MODEL is required and must point at a local model directory" >&2
  exit 2
fi
if [[ -z "${wav_path}" ]]; then
  echo "VINPUT_SHERPA_WAV is required and must point at an uncompressed PCM16 WAV file" >&2
  exit 2
fi

mkdir -p "${out_dir}"
python3 - \
  "${initial_config}" \
  "${reload_config}" \
  "${model_dir}" \
  "${wav_path}" \
  "${expected_family}" <<'PY'
import json
import pathlib
import sys

initial_path = pathlib.Path(sys.argv[1])
reload_path = pathlib.Path(sys.argv[2])
model_dir = pathlib.Path(sys.argv[3]).expanduser().resolve()
wav_path = pathlib.Path(sys.argv[4]).expanduser().resolve()
expected_family = sys.argv[5].strip()

if not model_dir.is_dir():
    raise SystemExit(f"VINPUT_SHERPA_MODEL must be an existing directory: {model_dir}")
if not wav_path.is_file():
    raise SystemExit(f"VINPUT_SHERPA_WAV must be an existing file: {wav_path}")
metadata_path = model_dir / "vinput-model.json"
if not metadata_path.is_file():
    raise SystemExit(f"native reload smoke requires typed metadata: {metadata_path}")
metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
family = str(metadata.get("family") or metadata.get("model_type") or "").strip()
if family != expected_family:
    raise SystemExit(
        f"expected sherpa model family {expected_family!r}, found {family!r} in {model_dir}"
    )

base = {
    "version": 1,
    "global": {"default_language": "en-US", "capture_device": "default"},
    "scenes": {
        "active_scene": "raw",
        "definitions": [{"id": "raw", "label": "Raw", "candidate_count": 0}],
    },
}
initial = dict(base)
initial["asr"] = {
    "active_provider": "mock",
    "normalize_audio": False,
    "input_gain": 1.0,
    "providers": [{"id": "mock", "type": "local", "model": "mock-startup"}],
}
reload = dict(base)
reload["asr"] = {
    "active_provider": "sherpa-onnx",
    "normalize_audio": False,
    "input_gain": 1.0,
    "providers": [
        {"id": "sherpa-onnx", "type": "local", "model": str(model_dir)}
    ],
}
initial_path.write_text(json.dumps(initial, indent=2) + "\n", encoding="utf-8")
reload_path.write_text(json.dumps(reload, indent=2) + "\n", encoding="utf-8")
PY
cp "${initial_config}" "${active_config}"

cargo build -q -p vinput-daemon --features sherpa-onnx-backend
cargo build -q -p vinput-cli

runtime_lib_dir="${VINPUT_SHERPA_RUNTIME_LIB_DIR:-${repo_root}/target/debug}"
if [[ -d "${runtime_lib_dir}" ]]; then
  export LD_LIBRARY_PATH="${runtime_lib_dir}${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"
fi

export VINPUT_RELOAD_ACTIVE_CONFIG="${active_config}"
export VINPUT_RELOAD_CONFIG="${reload_config}"
export VINPUT_RELOAD_WAV="$(realpath "${wav_path}")"
export VINPUT_RELOAD_EXPECTED_TEXT="${expected_text}"
export VINPUT_RELOAD_OUTPUT="${reload_output}"
export VINPUT_RELOAD_STATUS_OUTPUT="${status_output}"
export VINPUT_RELOAD_STOP_OUTPUT="${stop_output}"
export VINPUT_RELOAD_DAEMON_LOG="${daemon_log}"

timeout 120s dbus-run-session -- bash -euo pipefail <<'INNER'
target/debug/vinput-daemon \
  --dbus \
  --configured-backends \
  --config "${VINPUT_RELOAD_ACTIVE_CONFIG}" \
  --wav "${VINPUT_RELOAD_WAV}" \
  >"${VINPUT_RELOAD_DAEMON_LOG}" 2>&1 &
daemon_pid=$!
cleanup() {
  kill "${daemon_pid}" >/dev/null 2>&1 || true
  wait "${daemon_pid}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

ready=0
for _ in $(seq 1 100); do
  if target/debug/vinput daemon status --json >"${VINPUT_RELOAD_STATUS_OUTPUT}" 2>/dev/null; then
    ready=1
    break
  fi
  if ! kill -0 "${daemon_pid}" >/dev/null 2>&1; then
    cat "${VINPUT_RELOAD_DAEMON_LOG}" >&2
    exit 1
  fi
  sleep 0.05
done
if [[ "${ready}" != 1 ]]; then
  cat "${VINPUT_RELOAD_DAEMON_LOG}" >&2
  exit 1
fi
python3 - "${VINPUT_RELOAD_STATUS_OUTPUT}" <<'PY'
import json
import pathlib
import sys

status = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
asr = status["asr_backend"]
assert asr["target_provider_id"] == "mock", asr
assert asr["effective_provider_id"] == "mock", asr
assert asr["effective_model_id"] == "mock-streaming", asr
PY

cp "${VINPUT_RELOAD_CONFIG}" "${VINPUT_RELOAD_ACTIVE_CONFIG}.next"
mv "${VINPUT_RELOAD_ACTIVE_CONFIG}.next" "${VINPUT_RELOAD_ACTIVE_CONFIG}"
target/debug/vinput daemon reload-asr --json | tee "${VINPUT_RELOAD_OUTPUT}"

reloaded=0
for _ in $(seq 1 400); do
  target/debug/vinput daemon status --json >"${VINPUT_RELOAD_STATUS_OUTPUT}"
  if python3 - "${VINPUT_RELOAD_STATUS_OUTPUT}" <<'PY'
import json
import pathlib
import sys

status = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
asr = status["asr_backend"]
if asr["last_error"]:
    raise SystemExit(f"native ASR reload failed: {asr['last_error']}")
ready = (
    not asr["reload_in_progress"]
    and asr["target_provider_id"] == "sherpa-onnx"
    and asr["effective_provider_id"] == "sherpa-onnx"
    and asr["target_model_id"] == asr["effective_model_id"]
)
raise SystemExit(0 if ready else 1)
PY
  then
    reloaded=1
    break
  fi
  if ! kill -0 "${daemon_pid}" >/dev/null 2>&1; then
    cat "${VINPUT_RELOAD_DAEMON_LOG}" >&2
    exit 1
  fi
  sleep 0.05
done
if [[ "${reloaded}" != 1 ]]; then
  cat "${VINPUT_RELOAD_STATUS_OUTPUT}" >&2
  cat "${VINPUT_RELOAD_DAEMON_LOG}" >&2
  exit 1
fi

target/debug/vinput recording start --json >/dev/null
target/debug/vinput recording stop --json | tee "${VINPUT_RELOAD_STOP_OUTPUT}"
python3 - \
  "${VINPUT_RELOAD_STOP_OUTPUT}" \
  "${VINPUT_RELOAD_EXPECTED_TEXT}" <<'PY'
import json
import pathlib
import sys

outer = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
payload = json.loads(outer["payload_json"])
expected = sys.argv[2]
actual = payload["commit_text"]
if actual != expected:
    raise SystemExit(f"unexpected native reload recognition: {actual!r}")
print(f"native D-Bus reload recognition: {actual}")
PY
INNER

echo "sherpa native D-Bus reload smoke passed"
