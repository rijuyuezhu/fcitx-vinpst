#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

output="$(scripts/bench-capture-cold-start.sh --input fixtures/logs/capture-cold-start.log)"

grep -Fq 'first_buffer_ms: n=2 min=40' <<<"${output}"
grep -Fq 'create_stream_ms: n=2 min=0' <<<"${output}"
grep -Fq 'set_active_ms: n=2 min=1' <<<"${output}"
grep -Fq 'capture_open_ms: n=2 min=2' <<<"${output}"
grep -Fq 'session_create_ms: n=2 min=0' <<<"${output}"
grep -Fq 'idle_gap_ms: n=1 min=1200' <<<"${output}"
grep -Fq 'vad_removed_ms: n=1 min=125' <<<"${output}"
grep -Fq 'stream_reuse: starts=2 reused=1 created=1' <<<"${output}"
grep -Fq 'startup_failures: capture=1 session=1' <<<"${output}"

scripts/bench-capture-cold-start.sh --help | grep -Fq -- '--input saved-journal.log'

printf 'capture cold-start analyzer smoke passed\n'
