#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

model_dir="${VINPUT_SHERPA_MODEL:-}"
wav_path="${VINPUT_SHERPA_WAV:-}"
hotwords_file="${VINPUT_SHERPA_HOTWORDS_FILE:-}"
timeout_ms="${VINPUT_SHERPA_TIMEOUT_MS:-}"
out_dir="${VINPUT_SHERPA_SMOKE_DIR:-target/tmp/sherpa-sense-voice-local-smoke}"
config_path="${out_dir}/sherpa-sense-voice-local.json"

if [[ -z "${model_dir}" ]]; then
  echo "VINPUT_SHERPA_MODEL is required and must point at a local SenseVoice model directory" >&2
  exit 2
fi
if [[ -z "${wav_path}" ]]; then
  echo "VINPUT_SHERPA_WAV is required and must point at an uncompressed PCM16 WAV file" >&2
  exit 2
fi
if [[ ! -f "${wav_path}" ]]; then
  echo "VINPUT_SHERPA_WAV does not exist or is not a file: ${wav_path}" >&2
  exit 2
fi

mkdir -p "${out_dir}"

python3 - "${config_path}" "${model_dir}" "${hotwords_file}" "${timeout_ms}" <<'PY'
import json
import pathlib
import sys

config_path = pathlib.Path(sys.argv[1])
model_dir = pathlib.Path(sys.argv[2]).expanduser().resolve()
hotwords_arg = sys.argv[3].strip()
timeout_arg = sys.argv[4].strip()
if not model_dir.is_dir():
    raise SystemExit(f"VINPUT_SHERPA_MODEL must be an existing model directory: {model_dir}")
if not any((model_dir / name).is_file() for name in ("model.int8.onnx", "model.onnx")):
    raise SystemExit(
        "VINPUT_SHERPA_MODEL must contain model.int8.onnx or model.onnx "
        f"for the current SenseVoice backend: {model_dir}"
    )
if not (model_dir / "tokens.txt").is_file():
    raise SystemExit(f"VINPUT_SHERPA_MODEL must contain tokens.txt: {model_dir}")
provider = {"id": "sherpa-onnx", "type": "local", "model": str(model_dir)}
if hotwords_arg:
    hotwords = pathlib.Path(hotwords_arg).expanduser()
    if not hotwords.is_absolute():
        hotwords = model_dir / hotwords
    hotwords = hotwords.resolve()
    if not hotwords.is_file():
        raise SystemExit(f"VINPUT_SHERPA_HOTWORDS_FILE must be a regular file: {hotwords}")
    provider["hotwords_file"] = str(hotwords)
if timeout_arg:
    timeout = int(timeout_arg)
    if timeout <= 0:
        raise SystemExit("VINPUT_SHERPA_TIMEOUT_MS must be positive")
    provider["timeout_ms"] = timeout
config = {
    "version": 1,
    "asr": {
        "active_provider": "sherpa-onnx",
        "normalize_audio": False,
        "input_gain": 1.0,
        "providers": [provider],
    },
    "scenes": {
        "active_scene": "raw",
        "definitions": [{"id": "raw", "label": "Raw", "candidate_count": 0}],
    },
}
config_path.write_text(json.dumps(config, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
PY

cargo build -q -p vinput-daemon --features sherpa-onnx-backend

echo "== native sherpa runtime status =="
target/debug/vinput-daemon --configured-backends --config "${config_path}" runtime-status

echo "== native sherpa once result =="
target/debug/vinput-daemon --configured-backends --config "${config_path}" --once --wav "${wav_path}"
