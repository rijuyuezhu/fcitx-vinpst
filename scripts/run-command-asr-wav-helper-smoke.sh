#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

request='{"provider_id":"helper-smoke","timeout_ms":5000,"pcm":{"sample_rate_hz":8000,"channels":1},"context":{"mode":"normal","selected_text":null},"samples":[0,1000,-1000,32767,-32768]}'
output="$(printf '%s\n' "${request}" | scripts/command-asr-wav-helper.py -- python3 -c 'import os,wave; p=os.environ["VINPUT_ASR_WAV"]; w=wave.open(p,"rb"); print("wav %d %d %d %s" % (w.getframerate(), w.getnchannels(), w.getnframes(), os.environ["VINPUT_ASR_PROVIDER_ID"]))')"
python3 - "${output}" <<'PY'
import json
import sys
payload = json.loads(sys.argv[1])
assert payload == {"text": "wav 8000 1 5 helper-smoke"}, payload
PY

empty_output="$(printf '%s\n' "${request}" | scripts/command-asr-wav-helper.py -- python3 -c 'pass')"
python3 - "${empty_output}" <<'PY'
import json
import sys
payload = json.loads(sys.argv[1])
assert "error" in payload and "no text" in payload["error"], payload
PY

printf 'command-asr-wav-helper smoke passed\n'
