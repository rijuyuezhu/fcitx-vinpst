#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

mode="${1:-${VINPUT_LIVE_TOOLKIT_MODE:-normal}}"
case "${mode}" in
normal | command) ;;
*)
  echo "mode must be normal or command" >&2
  exit 2
  ;;
esac

out_dir="${VINPUT_LIVE_TOOLKIT_OUT_DIR:-target/tmp/ime-gtk4-native-live}"
binary="${out_dir}/gtk4-live-toolkit-probe"
log="${out_dir}/${mode}.jsonl"
wav="${VINPUT_LIVE_TOOLKIT_WAV:-}"
playback_target="${VINPUT_LIVE_TOOLKIT_PLAYBACK_TARGET:-}"
auto_trigger="${VINPUT_LIVE_TOOLKIT_AUTO_TRIGGER:-0}"
uinput_sender="${VINPUT_LIVE_TOOLKIT_UINPUT_SENDER:-scripts/send-uinput-key.py}"
playback_done="${out_dir}/${mode}.playback-done"
uinput_log="${out_dir}/${mode}.uinput.jsonl"
focus_log="${out_dir}/${mode}.focus.json"
window_title="fcitx-vinput GTK4 live probe"

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
    echo "pw-play is required when VINPUT_LIVE_TOOLKIT_WAV is set" >&2
    exit 1
  }
  command -v gdbus >/dev/null 2>&1 || {
    echo "gdbus is required when VINPUT_LIVE_TOOLKIT_WAV is set" >&2
    exit 1
  }
fi
if [[ "${auto_trigger}" != 0 ]]; then
  rm -f "${uinput_log}" "${focus_log}"
  if [[ -z "${wav}" ]]; then
    echo "automatic GTK4 triggering requires VINPUT_LIVE_TOOLKIT_WAV" >&2
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
cc -std=c11 -Wall -Wextra -Werror scripts/gtk4-live-toolkit-probe.c \
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
  rm -f "${playback_done}"
  (
    for _ in $(seq 1 300); do
      status="$(gdbus call --session --dest org.fcitx.Vinput \
        --object-path /org/fcitx/Vinput \
        --method org.fcitx.Vinput.Service.GetStatus 2>/dev/null || true)"
      if [[ "${status}" == *"recording"* ]]; then
        if [[ -n "${playback_target}" ]]; then
          pw-play --target "${playback_target}" "${wav}"
        else
          pw-play "${wav}"
        fi
        : >"${playback_done}"
        exit 0
      fi
      sleep 0.1
    done
    echo "daemon did not enter recording before the WAV playback deadline" >&2
    exit 1
  ) &
  playback_pid=$!
fi

echo "GTK4 live probe (${mode})" >&2
echo "Use the real Fcitx shortcut in the focused field; no GDK key events are synthesized." >&2
if [[ -n "${wav}" ]]; then
  echo "The WAV starts automatically after the daemon enters recording; press the shortcut again after playback." >&2
fi

set +e
VINPUT_TOOLKIT_EXTERNAL_WINDOW_FOCUS="${auto_trigger}" \
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
    "${uinput_sender}" "${trigger_key}" | tee -a "${uinput_log}"
    for _ in $(seq 1 300); do
      if [[ -e "${playback_done}" ]]; then
        break
      fi
      sleep 0.1
    done
    if [[ ! -e "${playback_done}" ]]; then
      echo "GTK4 playback did not finish before the stop-trigger deadline" >&2
      exit 1
    fi
    sleep 0.3
    "${uinput_sender}" "${trigger_key}" | tee -a "${uinput_log}"
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
exit "${status}"
