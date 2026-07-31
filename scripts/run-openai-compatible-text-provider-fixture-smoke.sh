#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

fixture="scripts/openai-compatible-text-provider-fixture.py"
out_dir="target/tmp/openai-compatible-text-provider-fixture-smoke"
ready_file="${out_dir}/ready.json"
trace_file="${out_dir}/trace.json"
error_file="${out_dir}/error.txt"
response_file="${out_dir}/response.json"
server_log="${out_dir}/server.log"
api_key="fixture-secret"
model="fixture-model"
server_pid=""

cleanup() {
  local exit_code=$?
  set +e
  if [[ -n "${server_pid}" ]] && kill -0 "${server_pid}" 2>/dev/null; then
    kill "${server_pid}" 2>/dev/null || true
    wait "${server_pid}" 2>/dev/null || true
  fi
  trap - EXIT INT TERM
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
python3 "${fixture}" \
  --ready-file "${ready_file}" \
  --trace-file "${trace_file}" \
  --error-file "${error_file}" \
  --api-key "${api_key}" \
  --model "${model}" \
  >"${server_log}" 2>&1 &
server_pid=$!

for _ in $(seq 1 100); do
  if [[ -f "${ready_file}" ]]; then
    break
  fi
  if ! kill -0 "${server_pid}" 2>/dev/null; then
    cat "${server_log}" >&2
    echo "OpenAI-compatible fixture exited before readiness" >&2
    exit 1
  fi
  sleep 0.05
done
if [[ ! -f "${ready_file}" ]]; then
  echo "OpenAI-compatible fixture did not publish readiness" >&2
  exit 1
fi
base_url="$(jq -r '.base_url' "${ready_file}")"

python3 - "${base_url}" "${api_key}" "${model}" "${response_file}" <<'PY'
import json
import sys
import urllib.request
from pathlib import Path

base_url, api_key, model, output_path = sys.argv[1:]
body = {
    "model": model,
    "stream": False,
    "response_format": {"type": "json_object"},
    "messages": [
        {
            "role": "user",
            "content": (
                "Apply the command.\n\n"
                "<asr>\nmake it shorter\n</asr>\n"
                "<selected>\nThis is selected text.\n</selected>\n"
            ),
        }
    ],
}
request = urllib.request.Request(
    base_url + "/chat/completions",
    data=json.dumps(body).encode("utf-8"),
    headers={
        "Authorization": "Bearer " + api_key,
        "Content-Type": "application/json",
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
jq -e '
  .choices[0].message.content |
  fromjson |
  .candidates == ["external-http: This is selected text. | command: make it shorter"]
' "${response_file}" >/dev/null
jq -e '
  .event == "request" and
  .request_count == 1 and
  .method == "POST" and
  .path == "/v1/chat/completions" and
  .authorization_scheme == "Bearer" and
  .authorization_value_recorded == false and
  .model == "fixture-model" and
  .stream == false and
  .response_format == {"type": "json_object"} and
  .selected_text == "This is selected text." and
  .raw_asr_text == "make it shorter" and
  .candidate == "external-http: This is selected text. | command: make it shorter"
' "${trace_file}" >/dev/null
if grep -Fq "${api_key}" "${trace_file}" "${ready_file}"; then
  echo "OpenAI-compatible fixture leaked its API key into evidence" >&2
  exit 1
fi

printf 'OpenAI-compatible text provider fixture smoke passed\n'
