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

model_dir="${VINPST_SHERPA_MODEL:-}"
wav_path="${VINPST_SHERPA_WAV:-}"
expected_family="${VINPST_SHERPA_EXPECT_FAMILY:-moonshine}"
expected_text="${VINPST_SHERPA_EXPECT_TEXT:-After early nightfall, the yellow lamps would light up here and there the squalid quarter of the brothels.}"
out_dir="${VINPST_SHERPA_RELOAD_SMOKE_DIR:-target/tmp/sherpa-dbus-reload-smoke}"
initial_config="${out_dir}/initial-config.json"
reload_config="${out_dir}/reload-config.json"
active_config="${out_dir}/active-config.json"
reload_output="${out_dir}/reload-output.json"
status_output="${out_dir}/status-output.json"
stop_output="${out_dir}/stop-output.json"
daemon_log="${out_dir}/daemon.log"

if [[ -z "${model_dir}" ]]; then
  echo "VINPST_SHERPA_MODEL is required and must point at a local model directory" >&2
  exit 2
fi
if [[ -z "${wav_path}" ]]; then
  echo "VINPST_SHERPA_WAV is required and must point at an uncompressed PCM16 WAV file" >&2
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
    raise SystemExit(f"VINPST_SHERPA_MODEL must be an existing directory: {model_dir}")
if not wav_path.is_file():
    raise SystemExit(f"VINPST_SHERPA_WAV must be an existing file: {wav_path}")
metadata_path = model_dir / "vinpst-model.json"
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
        "active_scene": "__raw__",
        "definitions": [
            {"id": "__raw__", "label": "Raw", "candidate_count": 0},
            {"id": "__command__", "label": "Command", "candidate_count": 1},
        ],
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

cargo build -q -p vinpst-daemon --features sherpa-onnx-backend
cargo build -q -p vinpst-cli

runtime_lib_dir="${VINPST_SHERPA_RUNTIME_LIB_DIR:-${repo_root}/target/debug}"
if [[ -d "${runtime_lib_dir}" ]]; then
  export LD_LIBRARY_PATH="${runtime_lib_dir}${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"
fi

export VINPST_RELOAD_ACTIVE_CONFIG="${active_config}"
export VINPST_RELOAD_CONFIG="${reload_config}"
VINPST_RELOAD_WAV="$(realpath "${wav_path}")"
export VINPST_RELOAD_WAV
export VINPST_RELOAD_EXPECTED_TEXT="${expected_text}"
export VINPST_RELOAD_OUTPUT="${reload_output}"
export VINPST_RELOAD_STATUS_OUTPUT="${status_output}"
export VINPST_RELOAD_STOP_OUTPUT="${stop_output}"
export VINPST_RELOAD_DAEMON_LOG="${daemon_log}"

timeout 120s dbus-run-session -- bash -euo pipefail <<'INNER'
target/debug/vinpst-daemon \
  --dbus \
  --configured-backends \
  --config "${VINPST_RELOAD_ACTIVE_CONFIG}" \
  --wav "${VINPST_RELOAD_WAV}" \
  >"${VINPST_RELOAD_DAEMON_LOG}" 2>&1 &
daemon_pid=$!
cleanup() {
  kill "${daemon_pid}" >/dev/null 2>&1 || true
  wait "${daemon_pid}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

ready=0
for _ in $(seq 1 100); do
  if target/debug/vinpst daemon status --json >"${VINPST_RELOAD_STATUS_OUTPUT}" 2>/dev/null; then
    ready=1
    break
  fi
  if ! kill -0 "${daemon_pid}" >/dev/null 2>&1; then
    cat "${VINPST_RELOAD_DAEMON_LOG}" >&2
    exit 1
  fi
  sleep 0.05
done
if [[ "${ready}" != 1 ]]; then
  cat "${VINPST_RELOAD_DAEMON_LOG}" >&2
  exit 1
fi
python3 - "${VINPST_RELOAD_STATUS_OUTPUT}" <<'PY'
import json
import pathlib
import sys

status = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
asr = status["asr_backend"]
assert asr["target_provider_id"] == "mock", asr
assert asr["effective_provider_id"] == "mock", asr
assert asr["effective_model_id"] == "mock-streaming", asr
PY

cp "${VINPST_RELOAD_CONFIG}" "${VINPST_RELOAD_ACTIVE_CONFIG}.next"
mv "${VINPST_RELOAD_ACTIVE_CONFIG}.next" "${VINPST_RELOAD_ACTIVE_CONFIG}"
target/debug/vinpst daemon reload-asr --json | tee "${VINPST_RELOAD_OUTPUT}"

reloaded=0
for _ in $(seq 1 400); do
  target/debug/vinpst daemon status --json >"${VINPST_RELOAD_STATUS_OUTPUT}"
  if python3 - "${VINPST_RELOAD_STATUS_OUTPUT}" <<'PY'
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
    cat "${VINPST_RELOAD_DAEMON_LOG}" >&2
    exit 1
  fi
  sleep 0.05
done
if [[ "${reloaded}" != 1 ]]; then
  cat "${VINPST_RELOAD_STATUS_OUTPUT}" >&2
  cat "${VINPST_RELOAD_DAEMON_LOG}" >&2
  exit 1
fi

target/debug/vinpst recording start --json >/dev/null
target/debug/vinpst recording stop --json | tee "${VINPST_RELOAD_STOP_OUTPUT}"
python3 - \
  "${VINPST_RELOAD_STOP_OUTPUT}" \
  "${VINPST_RELOAD_EXPECTED_TEXT}" <<'PY'
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
