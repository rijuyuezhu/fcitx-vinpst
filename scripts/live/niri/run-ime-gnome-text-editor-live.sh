#!/usr/bin/env bash
set -euo pipefail

mode="${1:-normal}"
case "${mode}" in
normal | command) ;;
*)
  echo "usage: $0 [normal|command]" >&2
  exit 2
  ;;
esac

wav="${VINPUT_LIVE_TOOLKIT_WAV:-}"
playback_target="${VINPUT_LIVE_TOOLKIT_PLAYBACK_TARGET:-}"
out_dir="${VINPUT_LIVE_TOOLKIT_OUT_DIR:-target/tmp/ime-gnome-text-editor-live}"
uinput_sender="${VINPUT_LIVE_TOOLKIT_UINPUT_SENDER:-scripts/live/niri/probes/send-uinput-key.py}"
document="${out_dir}/${mode}.txt"
focus_log="${out_dir}/${mode}.focus.json"
uinput_log="${out_dir}/${mode}.uinput.jsonl"
monitor_log="${out_dir}/${mode}.dbus.log"
summary="${out_dir}/${mode}.summary.json"
editor_stdout="${out_dir}/${mode}.stdout.log"
editor_stderr="${out_dir}/${mode}.stderr.log"
seed_text="selected text"
expected_prefix="adapter-backed: selected text | command: "
editor_pid=""
monitor_pid=""

call_service() {
  gdbus call --session \
    --dest org.fcitx.Vinput \
    --object-path /org/fcitx/Vinput \
    --method "org.fcitx.Vinput.Service.$1" "${@:2}"
}

restore_idle() {
  local status
  status="$(call_service GetStatus 2>/dev/null || true)"
  if [[ "${status}" == *"'recording'"* ]]; then
    call_service StopRecording "" >/dev/null 2>&1 || true
  fi
}

cleanup() {
  local exit_code=$?
  trap - EXIT
  set +e
  restore_idle
  if [[ -n "${editor_pid}" ]]; then
    kill -TERM "${editor_pid}" 2>/dev/null || true
    wait "${editor_pid}" 2>/dev/null || true
  fi
  if [[ -n "${monitor_pid}" ]]; then
    kill -TERM "${monitor_pid}" 2>/dev/null || true
    wait "${monitor_pid}" 2>/dev/null || true
  fi
  exit "${exit_code}"
}
trap cleanup EXIT

if [[ -z "${wav}" || ! -f "${wav}" ]]; then
  echo "set VINPUT_LIVE_TOOLKIT_WAV to a validated speech WAV" >&2
  exit 2
fi
for command in gdbus gnome-text-editor jq niri pw-play python3 stdbuf timeout; do
  command -v "${command}" >/dev/null 2>&1 || {
    echo "required GNOME Text Editor live command is missing: ${command}" >&2
    exit 1
  }
done
if [[ ! -x "${uinput_sender}" || ! -w /dev/uinput ]]; then
  echo "the executable uinput sender and writable /dev/uinput are required" >&2
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
export WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-1}"

status="$(call_service GetStatus 2>/dev/null || true)"
if [[ "${status}" != *"'idle'"* ]]; then
  echo "org.fcitx.Vinput must be idle before the editor probe: ${status:-unavailable}" >&2
  exit 1
fi

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
: >"${uinput_log}"
if [[ "${mode}" == command ]]; then
  printf '%s' "${seed_text}" >"${document}"
else
  : >"${document}"
fi

stdbuf -oL -eL gdbus monitor --session \
  --dest org.fcitx.Vinput \
  --object-path /org/fcitx/Vinput >"${monitor_log}" 2>&1 &
monitor_pid=$!

GTK_IM_MODULE=fcitx gnome-text-editor --standalone --new-window "${document}" \
  >"${editor_stdout}" 2>"${editor_stderr}" &
editor_pid=$!

window_id=""
for _ in $(seq 1 200); do
  window_id="$(
    niri msg --json windows 2>/dev/null |
      jq -r --argjson pid "${editor_pid}" \
        '[.[] | select(.pid == $pid and .app_id == "org.gnome.TextEditor") | .id] | if length == 1 then .[0] else empty end'
  )"
  if [[ -n "${window_id}" ]]; then
    niri msg action focus-window --id "${window_id}" >/dev/null
    break
  fi
  if ! kill -0 "${editor_pid}" 2>/dev/null; then
    echo "GNOME Text Editor exited before creating its window" >&2
    exit 1
  fi
  sleep 0.1
done
if [[ -z "${window_id}" ]]; then
  echo "GNOME Text Editor window did not appear" >&2
  exit 1
fi
sleep 0.5
focused_id="$(niri msg --json focused-window | jq -r '.id // empty')"
if [[ "${focused_id}" != "${window_id}" ]]; then
  echo "GNOME Text Editor window did not become focused" >&2
  exit 1
fi
jq -n \
  --arg backend niri \
  --arg socket "${NIRI_SOCKET}" \
  --arg app_id org.gnome.TextEditor \
  --argjson pid "${editor_pid}" \
  --argjson window_id "${window_id}" \
  '{
    event: "window-focus",
    backend: $backend,
    socket: $socket,
    app_id: $app_id,
    pid: $pid,
    window_id: $window_id,
    focused: true,
    ok: true
  }' >"${focus_log}"

if [[ "${mode}" == command ]]; then
  "${uinput_sender}" CTRL+A | tee -a "${uinput_log}"
  sleep 0.3
fi
trigger_key=F9
if [[ "${mode}" == command ]]; then
  trigger_key=F10
fi
"${uinput_sender}" "${trigger_key}" | tee -a "${uinput_log}"

recording=0
for _ in $(seq 1 200); do
  status="$(call_service GetStatus 2>/dev/null || true)"
  if [[ "${status}" == *"'recording'"* ]]; then
    recording=1
    break
  fi
  sleep 0.05
done
if [[ "${recording}" != 1 ]]; then
  echo "daemon did not enter recording after the editor shortcut" >&2
  exit 1
fi

if [[ -n "${playback_target}" ]]; then
  pw-play --target "${playback_target}" "${wav}"
else
  pw-play "${wav}"
fi
"${uinput_sender}" "${trigger_key}" | tee -a "${uinput_log}"

idle=0
for _ in $(seq 1 300); do
  status="$(call_service GetStatus 2>/dev/null || true)"
  if [[ "${status}" == *"'idle'"* ]]; then
    idle=1
    break
  fi
  sleep 0.05
done
if [[ "${idle}" != 1 ]]; then
  echo "daemon did not return to idle after the editor shortcut" >&2
  exit 1
fi
sleep 0.5
"${uinput_sender}" CTRL+S | tee -a "${uinput_log}"

saved=0
for _ in $(seq 1 200); do
  content="$(cat "${document}")"
  if [[ "${mode}" == normal && -n "${content}" ]]; then
    saved=1
    break
  fi
  if [[ "${mode}" == command && "${content}" == "${expected_prefix}"* ]]; then
    saved=1
    break
  fi
  sleep 0.05
done
if [[ "${saved}" != 1 ]]; then
  echo "GNOME Text Editor did not save the expected content" >&2
  printf 'content=%q\n' "$(cat "${document}")" >&2
  exit 1
fi

partial_count="$(grep -c 'RecognitionPartial' "${monitor_log}" || true)"
if [[ "${partial_count}" -lt 1 ]]; then
  echo "GNOME Text Editor probe observed no daemon partial signal" >&2
  exit 1
fi
content="$(cat "${document}")"
replacement=false
if [[ "${mode}" == command ]]; then
  replacement=true
fi
jq -n \
  --arg mode "${mode}" \
  --arg document "${document}" \
  --arg content "${content}" \
  --arg trigger_key "${trigger_key}" \
  --argjson editor_pid "${editor_pid}" \
  --argjson window_id "${window_id}" \
  --argjson partial_count "${partial_count}" \
  --argjson replacement "${replacement}" \
  '{
    event: "summary",
    application: "gnome-text-editor",
    mode: $mode,
    document: $document,
    content: $content,
    trigger_key: $trigger_key,
    editor_pid: $editor_pid,
    window_id: $window_id,
    partial_count: $partial_count,
    replacement: $replacement,
    saved: true,
    ok: true
  }' | tee "${summary}"

kill -TERM "${editor_pid}"
wait "${editor_pid}" 2>/dev/null || true
editor_pid=""
for _ in $(seq 1 100); do
  if ! niri msg --json windows | jq -e --argjson id "${window_id}" 'any(.[]; .id == $id)' >/dev/null; then
    break
  fi
  sleep 0.05
done
if niri msg --json windows | jq -e --argjson id "${window_id}" 'any(.[]; .id == $id)' >/dev/null; then
  echo "GNOME Text Editor window remained after process termination" >&2
  exit 1
fi

printf 'GNOME Text Editor %s live probe passed; evidence: %s\n' "${mode}" "${out_dir}"
