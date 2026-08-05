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

mode="${1:-${VINPST_LIVE_TOOLKIT_MODE:-normal}}"
case "${mode}" in
normal | command) ;;
*)
  echo "mode must be normal or command" >&2
  exit 2
  ;;
esac

out_dir="${VINPST_LIVE_TOOLKIT_OUT_DIR:-target/tmp/ime-gtk4-native-live}"
binary="${out_dir}/gtk4-live-toolkit-probe"
log="${out_dir}/${mode}.jsonl"
wav="${VINPST_LIVE_TOOLKIT_WAV:-}"
playback_target="${VINPST_LIVE_TOOLKIT_PLAYBACK_TARGET:-}"
auto_trigger="${VINPST_LIVE_TOOLKIT_AUTO_TRIGGER:-0}"
expected_cycles="${VINPST_TOOLKIT_EXPECTED_CYCLES:-1}"
timeout_seconds="${VINPST_TOOLKIT_TIMEOUT_SECONDS:-}"
uinput_sender="${VINPST_LIVE_TOOLKIT_UINPUT_SENDER:-scripts/live/niri/probes/send-uinput-key.py}"
playback_done_prefix="${out_dir}/${mode}.playback-done"
trigger_armed_prefix="${out_dir}/${mode}.trigger-armed"
uinput_log="${out_dir}/${mode}.uinput.jsonl"
trigger_log="${out_dir}/${mode}.trigger.jsonl"
focus_log="${out_dir}/${mode}.focus.json"
window_title="fcitx-vinpst GTK4 live probe"

if [[ ! "${expected_cycles}" =~ ^[0-9]+$ ||
  "${expected_cycles}" -lt 1 || "${expected_cycles}" -gt 20 ]]; then
  echo "VINPST_TOOLKIT_EXPECTED_CYCLES must be an integer from 1 to 20" >&2
  exit 2
fi
if [[ -z "${timeout_seconds}" ]]; then
  timeout_seconds=$((expected_cycles * 15))
  if ((timeout_seconds < 60)); then
    timeout_seconds=60
  fi
fi
if [[ ! "${timeout_seconds}" =~ ^[0-9]+$ ||
  "${timeout_seconds}" -lt 1 || "${timeout_seconds}" -gt 3600 ]]; then
  echo "VINPST_TOOLKIT_TIMEOUT_SECONDS must be an integer from 1 to 3600" >&2
  exit 2
fi

command -v cc >/dev/null 2>&1 || {
  echo "cc is required to build the GTK4 live probe" >&2
  exit 1
}
pkg-config --exists gtk4 || {
  echo "GTK4 development files are required (pkg-config gtk4)" >&2
  exit 1
}
command -v fcitx5-remote >/dev/null 2>&1 || {
  echo "fcitx5-remote is required" >&2
  exit 1
}
if ! fcitx5-remote --check >/dev/null 2>&1; then
  echo "Fcitx5 is not running in this session" >&2
  exit 1
fi
if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  echo "DBUS_SESSION_BUS_ADDRESS is not set" >&2
  exit 1
fi
if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
  runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
  mapfile -t wayland_sockets < <(
    find "${runtime_dir}" -maxdepth 1 -type s -name 'wayland-*' -printf '%f\n' 2>/dev/null
  )
  if [[ "${#wayland_sockets[@]}" == 1 ]]; then
    export WAYLAND_DISPLAY="${wayland_sockets[0]}"
  fi
fi
if [[ -n "${wav}" ]]; then
  test -f "${wav}" || {
    echo "speech WAV does not exist: ${wav}" >&2
    exit 1
  }
  command -v pw-play >/dev/null 2>&1 || {
    echo "pw-play is required when VINPST_LIVE_TOOLKIT_WAV is set" >&2
    exit 1
  }
  command -v gdbus >/dev/null 2>&1 || {
    echo "gdbus is required when VINPST_LIVE_TOOLKIT_WAV is set" >&2
    exit 1
  }
fi
if [[ "${auto_trigger}" != 0 ]]; then
  rm -f "${uinput_log}" "${trigger_log}" "${focus_log}"
  if [[ -z "${wav}" ]]; then
    echo "automatic GTK4 triggering requires VINPST_LIVE_TOOLKIT_WAV" >&2
    exit 2
  fi
  command -v python3 >/dev/null 2>&1 || {
    echo "python3 is required for automatic GTK4 triggering" >&2
    exit 1
  }
  command -v jq >/dev/null 2>&1 || {
    echo "jq is required for automatic GTK4 focus validation" >&2
    exit 1
  }
  command -v niri >/dev/null 2>&1 || {
    echo "niri is required for automatic GTK4 focus validation" >&2
    exit 1
  }
  if [[ ! -x "${uinput_sender}" ]]; then
    echo "uinput key sender is missing or not executable: ${uinput_sender}" >&2
    exit 1
  fi
  if [[ ! -w /dev/uinput ]]; then
    echo "/dev/uinput is not writable; cannot send a real desktop key" >&2
    exit 1
  fi
  if [[ -z "${NIRI_SOCKET:-}" || ! -S "${NIRI_SOCKET}" ]]; then
    runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
    mapfile -t niri_sockets < <(
      find "${runtime_dir}" -maxdepth 1 -type s -name 'niri*.sock' -print 2>/dev/null
    )
    if [[ "${#niri_sockets[@]}" != 1 ]]; then
      echo "expected exactly one live niri socket, found ${#niri_sockets[@]}" >&2
      exit 1
    fi
    export NIRI_SOCKET="${niri_sockets[0]}"
  fi
  niri msg --json windows >/dev/null
fi

mkdir -p "${out_dir}"
read -r -a gtk_cflags <<<"$(pkg-config --cflags gtk4)"
read -r -a gtk_libs <<<"$(pkg-config --libs gtk4)"
cc -std=c11 -Wall -Wextra -Werror scripts/live/niri/probes/gtk4-live-toolkit-probe.c \
  -o "${binary}" "${gtk_cflags[@]}" "${gtk_libs[@]}"

playback_pid=""
probe_pid=""
trigger_pid=""
# shellcheck disable=SC2329
cleanup() {
  if [[ -n "${playback_pid}" ]]; then
    kill "${playback_pid}" 2>/dev/null || true
    wait "${playback_pid}" 2>/dev/null || true
  fi
  if [[ -n "${probe_pid}" ]]; then
    kill "${probe_pid}" 2>/dev/null || true
    wait "${probe_pid}" 2>/dev/null || true
  fi
  if [[ -n "${trigger_pid}" ]]; then
    kill "${trigger_pid}" 2>/dev/null || true
    wait "${trigger_pid}" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

if [[ -n "${wav}" ]]; then
  rm -f "${playback_done_prefix}".* "${trigger_armed_prefix}".*
  (
    set -euo pipefail
    for cycle in $(seq 1 "${expected_cycles}"); do
      for _ in $(seq 1 300); do
        [[ -e "${trigger_armed_prefix}.${cycle}" ]] && break
        sleep 0.1
      done
      if [[ ! -e "${trigger_armed_prefix}.${cycle}" ]]; then
        echo "GTK4 cycle ${cycle} was not armed before playback" >&2
        exit 1
      fi
      if [[ "${cycle}" -gt 1 ]]; then
        idle_seen=0
        for _ in $(seq 1 300); do
          status="$(gdbus call --session --dest org.fcitx.Vinpst \
            --object-path /org/fcitx/Vinpst \
            --method org.fcitx.Vinpst.Service.GetStatus 2>/dev/null || true)"
          if [[ "${status}" == *"idle"* ]]; then
            idle_seen=1
            break
          fi
          sleep 0.1
        done
        if [[ "${idle_seen}" != 1 ]]; then
          echo "daemon did not return idle before GTK4 cycle ${cycle}" >&2
          exit 1
        fi
      fi

      recording_seen=0
      for _ in $(seq 1 300); do
        status="$(gdbus call --session --dest org.fcitx.Vinpst \
          --object-path /org/fcitx/Vinpst \
          --method org.fcitx.Vinpst.Service.GetStatus 2>/dev/null || true)"
        if [[ "${status}" == *"recording"* ]]; then
          recording_seen=1
          break
        fi
        sleep 0.1
      done
      if [[ "${recording_seen}" != 1 ]]; then
        echo "daemon did not enter recording before GTK4 cycle ${cycle} playback" >&2
        exit 1
      fi
      if [[ -n "${playback_target}" ]]; then
        pw-play --target "${playback_target}" "${wav}"
      else
        pw-play "${wav}"
      fi
      : >"${playback_done_prefix}.${cycle}"
    done
  ) &
  playback_pid=$!
fi

echo "GTK4 live probe (${mode})" >&2
echo "Use the real Fcitx shortcut in the focused field; no GDK key events are synthesized." >&2
if [[ -n "${wav}" ]]; then
  echo "The WAV starts automatically after the daemon enters recording; press the shortcut again after playback." >&2
fi

set +e
VINPST_TOOLKIT_EXTERNAL_WINDOW_FOCUS="${auto_trigger}" \
VINPST_TOOLKIT_EXPECTED_CYCLES="${expected_cycles}" \
VINPST_TOOLKIT_TIMEOUT_SECONDS="${timeout_seconds}" \
  GTK_IM_MODULE=fcitx "${binary}" "${mode}" > >(tee "${log}") &
probe_pid=$!
if [[ "${auto_trigger}" != 0 ]]; then
  (
    trigger_key=F9
    window_id=""
    if [[ "${mode}" == command ]]; then
      trigger_key=F10
    fi
    for _ in $(seq 1 300); do
      window_id="$(
        niri msg --json windows 2>/dev/null |
          jq -r --arg title "${window_title}" \
            '[.[] | select(.title == $title) | .id] | if length == 1 then .[0] else empty end'
      )"
      if [[ -n "${window_id}" ]]; then
        niri msg action focus-window --id "${window_id}" >/dev/null 2>&1 || true
      fi
      if grep -Fq '"event":"ready"' "${log}" 2>/dev/null; then
        break
      fi
      sleep 0.1
    done
    if ! grep -Fq '"event":"ready"' "${log}" 2>/dev/null; then
      echo "GTK4 probe did not become ready before the trigger deadline" >&2
      exit 1
    fi
    focused_id="$(niri msg --json focused-window | jq -r '.id // empty')"
    if [[ -z "${window_id}" || "${focused_id}" != "${window_id}" ]]; then
      echo "GTK4 probe window was not focused before the trigger" >&2
      exit 1
    fi
    jq -n \
      --arg backend niri \
      --arg socket "${NIRI_SOCKET}" \
      --arg title "${window_title}" \
      --argjson window_id "${window_id}" \
      '{
        event: "window-focus",
        backend: $backend,
        socket: $socket,
        title: $title,
        window_id: $window_id,
        focused: true,
        ok: true
      }' >"${focus_log}"
    for cycle in $(seq 1 "${expected_cycles}"); do
      if [[ "${cycle}" -gt 1 ]]; then
        ready_seen=0
        for _ in $(seq 1 300); do
          ready_count="$(grep -Fc '"event":"cycle-ready"' "${log}" 2>/dev/null || true)"
          if [[ "${ready_count}" -ge $((cycle - 1)) ]]; then
            ready_seen=1
            break
          fi
          sleep 0.1
        done
        if [[ "${ready_seen}" != 1 ]]; then
          echo "GTK4 probe did not become ready for cycle ${cycle}" >&2
          exit 1
        fi
        niri msg action focus-window --id "${window_id}" >/dev/null
        focused_id="$(niri msg --json focused-window | jq -r '.id // empty')"
        if [[ "${focused_id}" != "${window_id}" ]]; then
          echo "GTK4 probe window lost focus before cycle ${cycle}" >&2
          exit 1
        fi
      fi

      daemon_state="$(gdbus call --session --dest org.fcitx.Vinpst \
        --object-path /org/fcitx/Vinpst \
        --method org.fcitx.Vinpst.Service.GetStatus 2>/dev/null || true)"
      if [[ "${daemon_state}" != *"idle"* ]]; then
        echo "daemon was not idle immediately before GTK4 cycle ${cycle}" >&2
        exit 1
      fi
      if [[ "${cycle}" == 1 ]]; then
        if grep -Eq '"event":"(changed|daemon-partial)"' "${log}"; then
          echo "GTK4 probe changed or emitted partials before the first automatic trigger" >&2
          exit 1
        fi
      else
        previous_complete_line="$(
          grep -n '"event":"cycle-complete"' "${log}" |
            sed -n "$((cycle - 1))p" | cut -d: -f1
        )"
        if [[ -z "${previous_complete_line}" ]] ||
          tail -n "+$((previous_complete_line + 1))" "${log}" |
            grep -Eq '"event":"(changed|daemon-partial)"'; then
          echo "GTK4 probe changed before automatic cycle ${cycle}" >&2
          exit 1
        fi
      fi
      jq -nc --arg mode "${mode}" --argjson cycle "${cycle}" \
        '{event: "auto-trigger-start", toolkit: "gtk4", mode: $mode, cycle: $cycle}' \
        >>"${trigger_log}"
      : >"${trigger_armed_prefix}.${cycle}"

      "${uinput_sender}" "${trigger_key}" | tee -a "${uinput_log}"
      for _ in $(seq 1 300); do
        if [[ -e "${playback_done_prefix}.${cycle}" ]]; then
          break
        fi
        sleep 0.1
      done
      if [[ ! -e "${playback_done_prefix}.${cycle}" ]]; then
        echo "GTK4 cycle ${cycle} playback did not finish before the stop trigger" >&2
        exit 1
      fi
      sleep 0.3
      "${uinput_sender}" "${trigger_key}" | tee -a "${uinput_log}"
    done
  ) &
  trigger_pid=$!
fi
wait "${probe_pid}"
status=$?
probe_pid=""
set -e

if [[ -n "${trigger_pid}" ]]; then
  wait "${trigger_pid}" || status=1
  trigger_pid=""
fi

if [[ -n "${playback_pid}" ]]; then
  wait "${playback_pid}" || status=1
  playback_pid=""
fi

if [[ "${status}" == 0 && "${auto_trigger}" != 0 ]]; then
  jq -s -e --arg mode "${mode}" --argjson cycles "${expected_cycles}" '
    any(.[];
      .event == "summary" and
      .toolkit == "gtk4" and
      .mode == $mode and
      .completed_cycles == $cycles and
      .expected_cycles == $cycles and
      .timed_out == false and
      .ok == true)
  ' "${log}" >/dev/null
  jq -s -e --arg key "$( [[ "${mode}" == command ]] && echo F10 || echo F9 )" \
    --argjson expected "$((expected_cycles * 2))" '
      length == $expected and
      all(.[]; .event == "uinput-key" and .key == $key and .ok == true)
    ' "${uinput_log}" >/dev/null
  cycle_complete_count="$(grep -Fc '"event":"cycle-complete"' "${log}" || true)"
  cycle_ready_count="$(grep -Fc '"event":"cycle-ready"' "${log}" || true)"
  if [[ "${cycle_complete_count}" -ne "${expected_cycles}" ||
    "${cycle_ready_count}" -ne $((expected_cycles - 1)) ]]; then
    echo "GTK4 cycle event counts did not match the expected repeat contract" >&2
    status=1
  fi
  jq -s -e --arg mode "${mode}" --argjson cycles "${expected_cycles}" '
    length == $cycles and
    all(.[]; .event == "auto-trigger-start" and .toolkit == "gtk4" and
      .mode == $mode and .cycle >= 1 and .cycle <= $cycles) and
    ([.[].cycle] == [range(1; $cycles + 1)])
  ' "${trigger_log}" >/dev/null
  for cycle in $(seq 1 "${expected_cycles}"); do
    complete_line="$(
      grep -n '"event":"cycle-complete"' "${log}" |
        sed -n "${cycle}p" | cut -d: -f1
    )"
    previous_complete_line=0
    if [[ "${cycle}" -gt 1 ]]; then
      previous_complete_line="$(
        grep -n '"event":"cycle-complete"' "${log}" |
          sed -n "$((cycle - 1))p" | cut -d: -f1
      )"
    fi
    if [[ -z "${complete_line}" || -z "${previous_complete_line}" ||
      "${previous_complete_line}" -ge "${complete_line}" ]]; then
      echo "GTK4 cycle ${cycle} completion ordering was invalid" >&2
      status=1
      continue
    fi
    partial_count="$(
      sed -n "$((previous_complete_line + 1)),$((complete_line - 1))p" "${log}" |
        grep -Fc '"event":"daemon-partial"' || true
    )"
    if [[ "${partial_count}" -lt 3 ]]; then
      echo "GTK4 cycle ${cycle} did not contain a complete D-Bus partial sequence" >&2
      status=1
    fi
  done
fi
exit "${status}"
