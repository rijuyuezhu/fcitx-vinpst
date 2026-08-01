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

bridge="scripts/fixtures/legacy-command-asr-wav-bridge.py"
out_dir="target/tmp/legacy-command-asr-wav-bridge-smoke"
rm -rf "${out_dir}"
mkdir -p "${out_dir}"

python3 - <<'PY' | "${bridge}" --sample-rate 8000 --channels 1 --timeout-ms 5000 -- \
  python3 -c 'import os,wave; p=os.environ["VINPUT_ASR_WAV"]; w=wave.open(p,"rb"); print("wav %d %d %d %s" % (w.getframerate(), w.getnchannels(), w.getnframes(), os.environ["VINPUT_ASR_FRAMES"]))' \
  >"${out_dir}/success.txt"
import struct
import sys
sys.stdout.buffer.write(struct.pack('<5h', 0, 1000, -1000, 32767, -32768))
PY
grep -Fxq 'wav 8000 1 5 5' "${out_dir}/success.txt"

set +e
printf '' | "${bridge}" -- python3 -c 'print("unexpected")' \
  >"${out_dir}/empty.stdout" 2>"${out_dir}/empty.stderr"
empty_status=$?
printf '\001' | "${bridge}" -- python3 -c 'print("unexpected")' \
  >"${out_dir}/odd.stdout" 2>"${out_dir}/odd.stderr"
odd_status=$?
printf '\000\000' | "${bridge}" -- python3 -c 'pass' \
  >"${out_dir}/no-text.stdout" 2>"${out_dir}/no-text.stderr"
no_text_status=$?
printf '\000\000' | "${bridge}" --timeout-ms 20 -- \
  python3 -c 'import time; time.sleep(1); print("late")' \
  >"${out_dir}/timeout.stdout" 2>"${out_dir}/timeout.stderr"
timeout_status=$?
set -e

for status in "${empty_status}" "${odd_status}" "${no_text_status}" "${timeout_status}"; do
  if [[ "${status}" -eq 0 ]]; then
    echo "legacy command ASR bridge unexpectedly accepted an invalid case" >&2
    exit 1
  fi
done
grep -Fq 'PCM input is empty' "${out_dir}/empty.stderr"
grep -Fq 'PCM byte length must be even' "${out_dir}/odd.stderr"
grep -Fq 'external ASR command produced no text' "${out_dir}/no-text.stderr"
grep -Fq 'external ASR command timed out' "${out_dir}/timeout.stderr"
for output in empty odd no-text timeout; do
  test ! -s "${out_dir}/${output}.stdout"
done

printf 'legacy command ASR WAV bridge smoke passed\n'
