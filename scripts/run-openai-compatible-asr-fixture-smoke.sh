#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

fixture="scripts/openai-compatible-asr-fixture.py"
out_dir="target/tmp/openai-compatible-asr-fixture-smoke"
ready_file="${out_dir}/ready.json"
trace_file="${out_dir}/trace.json"
error_file="${out_dir}/error.txt"
response_file="${out_dir}/response.json"
wav_file="${out_dir}/fixture.wav"
server_log="${out_dir}/server.log"
api_key="fixture-remote-secret"
model="fixture-asr-model"
language="zh"
prompt="fixture names"
response_text="remote fixture final"
server_pid=""

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM
  set +e
  if [[ -n "${server_pid}" ]] && kill -0 "${server_pid}" 2>/dev/null; then
    kill "${server_pid}" 2>/dev/null || true
    wait "${server_pid}" 2>/dev/null || true
  fi
  rm -rf scripts/__pycache__
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
python3 - "${wav_file}" <<'PY'
import math
import struct
import sys
import wave
from pathlib import Path

path = Path(sys.argv[1])
sample_rate = 16_000
samples = [
    int(8_000 * math.sin(2 * math.pi * 440 * index / sample_rate))
    for index in range(sample_rate // 4)
]
with wave.open(str(path), "wb") as wav:
    wav.setnchannels(1)
    wav.setsampwidth(2)
    wav.setframerate(sample_rate)
    wav.writeframes(b"".join(struct.pack("<h", sample) for sample in samples))
PY

ruff check "${fixture}"
ruff format --check "${fixture}"
python3 "${fixture}" \
  --ready-file "${ready_file}" \
  --trace-file "${trace_file}" \
  --error-file "${error_file}" \
  --api-key "${api_key}" \
  --model "${model}" \
  --language "${language}" \
  --prompt "${prompt}" \
  --response-text "${response_text}" \
  >"${server_log}" 2>&1 &
server_pid=$!

for _ in $(seq 1 100); do
  [[ -f "${ready_file}" ]] && break
  if ! kill -0 "${server_pid}" 2>/dev/null; then
    cat "${server_log}" >&2
    echo "OpenAI-compatible ASR fixture exited before readiness" >&2
    exit 1
  fi
  sleep 0.05
done
if [[ ! -f "${ready_file}" ]]; then
  echo "OpenAI-compatible ASR fixture did not publish readiness" >&2
  exit 1
fi
base_url="$(jq -r '.base_url' "${ready_file}")"

python3 - \
  "${base_url}" \
  "${api_key}" \
  "${model}" \
  "${language}" \
  "${prompt}" \
  "${wav_file}" \
  "${response_file}" <<'PY'
import json
import sys
import urllib.request
import uuid
from pathlib import Path

base_url, api_key, model, language, prompt, wav_path, output_path = sys.argv[1:]
boundary = "vinput-fixture-" + uuid.uuid4().hex
parts: list[bytes] = []


def field(name: str, value: str) -> None:
    parts.extend(
        [
            f"--{boundary}\r\n".encode(),
            f'Content-Disposition: form-data; name="{name}"\r\n\r\n'.encode(),
            value.encode(),
            b"\r\n",
        ]
    )


field("model", model)
field("language", language)
field("prompt", prompt)
parts.extend(
    [
        f"--{boundary}\r\n".encode(),
        b'Content-Disposition: form-data; name="file"; filename="audio.wav"\r\n',
        b"Content-Type: audio/wav\r\n\r\n",
        Path(wav_path).read_bytes(),
        b"\r\n",
        f"--{boundary}--\r\n".encode(),
    ]
)
body = b"".join(parts)
request = urllib.request.Request(
    base_url + "/audio/transcriptions",
    data=body,
    headers={
        "Authorization": "Bearer " + api_key,
        "Content-Type": "multipart/form-data; boundary=" + boundary,
        "Content-Length": str(len(body)),
    },
    method="POST",
)
with urllib.request.urlopen(request, timeout=5) as response:
    payload = response.read().decode("utf-8")
Path(output_path).write_text(payload + "\n", encoding="utf-8")
PY

wait "${server_pid}"
server_pid=""
test ! -e "${error_file}"
jq -e --arg text "${response_text}" '.text == $text' "${response_file}" >/dev/null
jq -e \
  --arg model "${model}" \
  --arg language "${language}" \
  --arg text "${response_text}" '
    .event == "request" and
    .request_count == 1 and
    .method == "POST" and
    .path == "/v1/audio/transcriptions" and
    .authorization_scheme == "Bearer" and
    .authorization_value_recorded == false and
    .content_type == "multipart/form-data" and
    .file_field == "file" and
    .file_name == "audio.wav" and
    .file_content_type == "audio/wav" and
    .model == $model and
    .language == $language and
    .prompt_matched == true and
    .prompt_value_recorded == false and
    .wav.sample_rate == 16000 and
    .wav.channels == 1 and
    .wav.sample_width_bits == 16 and
    .wav.frames == 4000 and
    .wav.peak > 0 and
    (.wav.sha256 | length) == 64 and
    .response_text == $text
  ' "${trace_file}" >/dev/null
if grep -Fq "${api_key}" "${trace_file}" "${ready_file}"; then
  echo "OpenAI-compatible ASR fixture leaked its API key into evidence" >&2
  exit 1
fi
if grep -Fq "${prompt}" "${trace_file}" "${ready_file}"; then
  echo "OpenAI-compatible ASR fixture leaked its prompt into evidence" >&2
  exit 1
fi

printf 'OpenAI-compatible ASR fixture smoke passed\n'
