#!/usr/bin/env bash
set -euo pipefail

mode="${1:-normal}"
cycles="${2:-10}"
case "${mode}" in
normal | command) ;;
*)
  echo "usage: $0 [normal|command] [cycles: 10-20]" >&2
  exit 2
  ;;
esac
if [[ ! "${cycles}" =~ ^[0-9]+$ ]] ||
  ((cycles < 10 || cycles > 20)); then
  echo "bounded GTK4 soak cycles must be an integer from 10 to 20" >&2
  exit 2
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"
cd "${repo_root}"

out_dir="target/tmp/ime-gtk4-soak-virtual-source-live/${mode}"
start_epoch="$(date +%s)"
VINPUT_TOOLKIT_EXPECTED_CYCLES="${cycles}" \
VINPUT_LIVE_VIRTUAL_PROBE_KIND=gtk4 \
VINPUT_LIVE_TOOLKIT_MODE="${mode}" \
VINPUT_LIVE_VIRTUAL_OUT_DIR="${out_dir}" \
  scripts/live/niri/run-ime-fcitx-virtual-source-live.sh
end_epoch="$(date +%s)"
duration_seconds=$((end_epoch - start_epoch))

probe_log="${out_dir}/gtk4/${mode}.jsonl"
uinput_log="${out_dir}/gtk4/${mode}.uinput.jsonl"
focus_log="${out_dir}/gtk4/${mode}.focus.json"
outer_summary="${out_dir}/summary.json"
soak_summary="${out_dir}/bounded-soak-summary.json"
trigger_key=F9
if [[ "${mode}" == command ]]; then
  trigger_key=F10
fi

probe_summary="$(jq -sc 'map(select(.event == "summary")) | last' "${probe_log}")"
cycle_complete_count="$(grep -Fc '"event":"cycle-complete"' "${probe_log}" || true)"
cycle_ready_count="$(grep -Fc '"event":"cycle-ready"' "${probe_log}" || true)"
uinput_count="$(jq -s 'length' "${uinput_log}")"
minimum_partial_count="$(python3 - "${probe_log}" "${cycles}" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
cycles = int(sys.argv[2])
events = [json.loads(line) for line in path.read_text().splitlines() if line]
complete_indices = [
    index for index, event in enumerate(events) if event.get("event") == "cycle-complete"
]
if len(complete_indices) != cycles:
    raise SystemExit("unexpected GTK4 cycle completion count")
minimum = None
previous = -1
for complete in complete_indices:
    count = sum(
        event.get("event") == "daemon-partial"
        for event in events[previous + 1 : complete]
    )
    minimum = count if minimum is None else min(minimum, count)
    previous = complete
print(minimum if minimum is not None else 0)
PY
)"

jq -e \
  --arg mode "${mode}" \
  --arg key "${trigger_key}" \
  --argjson cycles "${cycles}" \
  --argjson complete_count "${cycle_complete_count}" \
  --argjson ready_count "${cycle_ready_count}" \
  --argjson uinput_count "${uinput_count}" \
  --argjson minimum_partials "${minimum_partial_count}" '
  .event == "summary" and
  .toolkit == "gtk4" and
  .mode == $mode and
  .completed_cycles == $cycles and
  .expected_cycles == $cycles and
  .timed_out == false and
  .ok == true and
  $complete_count == $cycles and
  $ready_count == ($cycles - 1) and
  $uinput_count == ($cycles * 2) and
  $minimum_partials >= 3
' <<<"${probe_summary}" >/dev/null
jq -s -e --arg key "${trigger_key}" --argjson expected "$((cycles * 2))" '
  length == $expected and
  all(.[]; .event == "uinput-key" and .key == $key and .ok == true)
' "${uinput_log}" >/dev/null
jq -e '
  .event == "window-focus" and
  .backend == "niri" and
  .title == "fcitx-vinput GTK4 live probe" and
  (.window_id | type) == "number" and
  .focused == true and
  .ok == true
' "${focus_log}" >/dev/null
jq -e '
  .event == "summary" and
  .probe_kind == "gtk4" and
  .profile_restored == true and
  .physical_speaker_or_microphone_used == false and
  .same_daemon_owner == true and
  .ok == true
' "${outer_summary}" >/dev/null

jq -n \
  --arg event bounded-soak-summary \
  --arg toolkit gtk4 \
  --arg mode "${mode}" \
  --arg trigger_key "${trigger_key}" \
  --argjson cycles "${cycles}" \
  --argjson duration_seconds "${duration_seconds}" \
  --argjson cycle_complete_count "${cycle_complete_count}" \
  --argjson cycle_ready_count "${cycle_ready_count}" \
  --argjson uinput_count "${uinput_count}" \
  --argjson minimum_partial_count "${minimum_partial_count}" \
  --argjson window_id "$(jq '.window_id' "${focus_log}")" \
  --argjson outer "$(cat "${outer_summary}")" \
  '{
    event: $event,
    toolkit: $toolkit,
    mode: $mode,
    cycles: $cycles,
    trigger_key: $trigger_key,
    duration_seconds: $duration_seconds,
    cycle_complete_count: $cycle_complete_count,
    cycle_ready_count: $cycle_ready_count,
    uinput_count: $uinput_count,
    minimum_partial_count: $minimum_partial_count,
    window_id: $window_id,
    same_window: true,
    same_daemon_owner: $outer.same_daemon_owner,
    profile_restored: $outer.profile_restored,
    physical_speaker_or_microphone_used: $outer.physical_speaker_or_microphone_used,
    bounded_soak_proof: true,
    extended_duration_soak_proof: false,
    ok: true
  }' | tee "${soak_summary}"
