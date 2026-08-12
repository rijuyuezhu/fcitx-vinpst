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
expected_family="${VINPST_SHERPA_EXPECT_FAMILY:-}"
expected_text="${VINPST_SHERPA_EXPECT_TEXT:-}"
hotwords_file="${VINPST_SHERPA_HOTWORDS_FILE:-}"
timeout_ms="${VINPST_SHERPA_TIMEOUT_MS:-}"
out_dir="${VINPST_SHERPA_SMOKE_DIR:-target/tmp/sherpa-online-local-smoke}"
config_path="${out_dir}/sherpa-online-local.json"
family_path="${out_dir}/model-family.txt"
runtime_status_stderr="${out_dir}/runtime-status.stderr"
once_stderr="${out_dir}/once.stderr"
once_output_path="${out_dir}/once-output.json"

if [[ -z "${model_dir}" ]]; then
  echo "VINPST_SHERPA_MODEL is required and must point at a local online model directory" >&2
  exit 2
fi
if [[ -z "${wav_path}" ]]; then
  echo "VINPST_SHERPA_WAV is required and must point at an uncompressed PCM16 WAV file" >&2
  exit 2
fi
if [[ ! -f "${wav_path}" ]]; then
  echo "VINPST_SHERPA_WAV does not exist or is not a file: ${wav_path}" >&2
  exit 2
fi

mkdir -p "${out_dir}"

python3 - \
  "${config_path}" \
  "${family_path}" \
  "${model_dir}" \
  "${expected_family}" \
  "${hotwords_file}" \
  "${timeout_ms}" <<'PY'
import json
import pathlib
import sys

config_path = pathlib.Path(sys.argv[1])
family_path = pathlib.Path(sys.argv[2])
model_dir = pathlib.Path(sys.argv[3]).expanduser().resolve()
expected_family = sys.argv[4].strip()
hotwords_arg = sys.argv[5].strip()
timeout_arg = sys.argv[6].strip()

if not model_dir.is_dir():
    raise SystemExit(f"VINPST_SHERPA_MODEL must be an existing model directory: {model_dir}")
metadata_path = model_dir / "vinpst-model.json"
if not metadata_path.is_file():
    raise SystemExit(f"online sherpa model requires registry-generated metadata: {metadata_path}")
try:
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
except (OSError, json.JSONDecodeError) as error:
    raise SystemExit(f"failed to read {metadata_path}: {error}") from error

runtime = str(metadata.get("runtime") or "").strip()
backend = str(metadata.get("backend") or "").strip()
family = str(metadata.get("family") or metadata.get("model_type") or "").strip()
if runtime != "online" and backend != "sherpa-streaming":
    raise SystemExit(
        f"{metadata_path} must select runtime=online or backend=sherpa-streaming"
    )
if not family:
    raise SystemExit(f"{metadata_path} must declare a non-empty family or model_type")
if expected_family and family != expected_family:
    raise SystemExit(
        f"expected sherpa model family {expected_family!r}, found {family!r} in {model_dir}"
    )

provider = {"id": "sherpa-onnx", "type": "local", "model": str(model_dir)}
if hotwords_arg:
    hotwords = pathlib.Path(hotwords_arg).expanduser()
    if not hotwords.is_absolute():
        hotwords = model_dir / hotwords
    hotwords = hotwords.resolve()
    if not hotwords.is_file():
        raise SystemExit(f"VINPST_SHERPA_HOTWORDS_FILE must be a regular file: {hotwords}")
    provider["hotwords_file"] = str(hotwords)
if timeout_arg:
    timeout = int(timeout_arg)
    if timeout <= 0:
        raise SystemExit("VINPST_SHERPA_TIMEOUT_MS must be positive")
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
        "active_scene": "__raw__",
        "definitions": [
            {"id": "__raw__", "label": "Raw", "candidate_count": 0},
            {"id": "__command__", "label": "Command", "candidate_count": 1},
        ],
    },
}
config_path.write_text(json.dumps(config, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
family_path.write_text(family + "\n", encoding="utf-8")
PY

cargo build -q -p vinpst-daemon --features sherpa-onnx-backend

runtime_lib_dir="${VINPST_SHERPA_RUNTIME_LIB_DIR:-${repo_root}/target/debug}"
if [[ -d "${runtime_lib_dir}" ]]; then
  export LD_LIBRARY_PATH="${runtime_lib_dir}${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"
fi

family="$(<"${family_path}")"
echo "== native sherpa online model family: ${family} =="
echo "== native sherpa online runtime status =="
target/debug/vinpst-daemon --configured-backends --config "${config_path}" runtime-status \
  2>"${runtime_status_stderr}"
cat "${runtime_status_stderr}" >&2

echo "== native sherpa online once result =="
target/debug/vinpst-daemon --configured-backends --config "${config_path}" --once --wav "${wav_path}" \
  2>"${once_stderr}" | tee "${once_output_path}"
cat "${once_stderr}" >&2
if [[ -n "${expected_text}" ]]; then
  grep -Fq "${expected_text}" "${once_output_path}"
fi

grep -Fq \
  'vinpst: sherpa-onnx online recognizer warmup completed duration_ms=200' \
  "${runtime_status_stderr}"
grep -Fq \
  'vinpst: sherpa-onnx online recognizer warmup completed duration_ms=200' \
  "${once_stderr}"
