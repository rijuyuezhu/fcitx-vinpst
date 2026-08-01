#!/usr/bin/env bash
# Analyze Rust daemon capture startup timing without driving the microphone.
#
# Enable structured debug logs for the user service, then restart it:
#   mkdir -p ~/.config/systemd/user/vinput-daemon.service.d
#   printf '%s\n' '[Service]' \
#     'Environment=RUST_LOG=vinput_audio=debug,vinput_daemon=debug' \
#     > ~/.config/systemd/user/vinput-daemon.service.d/debug.conf
#   systemctl --user daemon-reload
#   systemctl --user restart vinput-daemon.service
#
# Manual protocols:
#   Cold: wait at least 10 seconds, start recording, speak immediately, stop.
#   Warm: restart recording within 2 seconds of the previous stop.
#
# Usage:
#   scripts/tools/bench-capture-cold-start.sh
#   scripts/tools/bench-capture-cold-start.sh --since '2 hours ago'
#   scripts/tools/bench-capture-cold-start.sh --follow
#   scripts/tools/bench-capture-cold-start.sh --input saved-journal.log

set -euo pipefail

since='24 hours ago'
follow=0
unit='vinput-daemon.service'
input=''

while [[ $# -gt 0 ]]; do
    case "$1" in
        --since)
            since="${2:?missing value for --since}"
            shift 2
            ;;
        --follow|-f)
            follow=1
            shift
            ;;
        --unit)
            unit="${2:?missing value for --unit}"
            shift 2
            ;;
        --input)
            input="${2:?missing value for --input}"
            shift 2
            ;;
        -h|--help)
            sed -n '2,25p' "$0"
            exit 0
            ;;
        *)
            printf 'unknown argument: %s\n' "$1" >&2
            exit 2
            ;;
    esac
done

if [[ ${follow} -eq 1 && -n ${input} ]]; then
    printf '%s\n' '--follow and --input cannot be used together' >&2
    exit 2
fi

timing_pattern='PipeWire capture started|PipeWire capture received first buffer|recording startup completed|recording capture startup failed|recording ASR session startup failed|VAD trimmed'

if [[ ${follow} -eq 1 ]]; then
    exec journalctl --user -u "${unit}" -f --no-pager | grep --line-buffered -E "${timing_pattern}"
fi

temporary=''
if [[ -z ${input} ]]; then
    temporary="$(mktemp)"
    trap 'rm -f "${temporary}"' EXIT
    journalctl --user -u "${unit}" --since "${since}" --no-pager >"${temporary}" 2>/dev/null || true
    input="${temporary}"
fi

python3 - "${input}" <<'PY'
from __future__ import annotations

import re
import statistics
import sys
from pathlib import Path


def integer_field(line: str, name: str) -> int | None:
    match = re.search(rf"\b{re.escape(name)}=(?:Some\()?(-?\d+)", line)
    return int(match.group(1)) if match else None


def boolean_field(line: str, name: str) -> bool | None:
    match = re.search(rf"\b{re.escape(name)}=(true|false)\b", line)
    return match.group(1) == "true" if match else None


def append_nonnegative(values: list[int], value: int | None) -> None:
    if value is not None and value >= 0:
        values.append(value)


def percentile(sorted_values: list[int], percent: int) -> int:
    index = round((percent / 100) * (len(sorted_values) - 1))
    return sorted_values[max(0, min(len(sorted_values) - 1, index))]


def print_stats(name: str, values: list[int]) -> None:
    if not values:
        print(f"{name}: (no samples)")
        return
    ordered = sorted(values)
    fast = sum(value < 150 for value in values)
    slow = sum(value >= 350 for value in values)
    print(
        f"{name}: n={len(values)} min={ordered[0]} p25={percentile(ordered, 25)} "
        f"median={statistics.median(values):.1f} p75={percentile(ordered, 75)} "
        f"p90={percentile(ordered, 90)} max={ordered[-1]} "
        f"mean={statistics.mean(values):.1f} "
        f"fast<150={fast}({fast / len(values) * 100:.1f}%) "
        f"slow>=350={slow}({slow / len(values) * 100:.1f}%)"
    )


path = Path(sys.argv[1])
lines = path.read_text(errors="replace").splitlines()

metrics: dict[str, list[int]] = {
    "first_buffer_ms": [],
    "create_stream_ms": [],
    "set_active_ms": [],
    "pipewire_start_total_ms": [],
    "capture_open_ms": [],
    "session_create_ms": [],
    "runtime_start_total_ms": [],
    "idle_gap_ms": [],
    "vad_removed_ms": [],
}
starts = 0
reused = 0
created = 0
capture_failures = 0
session_failures = 0

for line in lines:
    if "PipeWire capture started" in line:
        starts += 1
        append_nonnegative(metrics["idle_gap_ms"], integer_field(line, "idle_gap_ms"))
        append_nonnegative(metrics["create_stream_ms"], integer_field(line, "create_stream_ms"))
        append_nonnegative(metrics["set_active_ms"], integer_field(line, "set_active_ms"))
        append_nonnegative(metrics["pipewire_start_total_ms"], integer_field(line, "start_total_ms"))
        reused += boolean_field(line, "stream_reused") is True
        created += boolean_field(line, "created_new_stream") is True
    elif "PipeWire capture received first buffer" in line:
        append_nonnegative(metrics["first_buffer_ms"], integer_field(line, "first_buffer_ms"))
    elif "recording startup completed" in line:
        append_nonnegative(metrics["capture_open_ms"], integer_field(line, "capture_open_ms"))
        append_nonnegative(metrics["session_create_ms"], integer_field(line, "session_create_ms"))
        append_nonnegative(metrics["runtime_start_total_ms"], integer_field(line, "start_total_ms"))
    elif "recording capture startup failed" in line:
        capture_failures += 1
    elif "recording ASR session startup failed" in line:
        session_failures += 1

    if "VAD trimmed" in line:
        leading = integer_field(line, "leading_removed_ms")
        trailing = integer_field(line, "trailing_removed_ms")
        if leading is not None and trailing is not None:
            append_nonnegative(metrics["vad_removed_ms"], leading + trailing)

print("=== vinput Rust capture cold-start scrape ===")
for name in metrics:
    print_stats(name, metrics[name])
print(f"stream_reuse: starts={starts} reused={reused} created={created}")
print(f"startup_failures: capture={capture_failures} session={session_failures}")
print()
print("Manual cohort guidance:")
print("  cold: idle_gap_ms >= 10000")
print("  warm: idle_gap_ms < 2000")
print("  compare first_buffer_ms, capture_open_ms, and session_create_ms separately")
PY
