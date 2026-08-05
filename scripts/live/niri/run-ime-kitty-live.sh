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

wav="${VINPST_LIVE_TOOLKIT_WAV:-}"
playback_target="${VINPST_LIVE_TOOLKIT_PLAYBACK_TARGET:-}"
out_dir="${VINPST_LIVE_TOOLKIT_OUT_DIR:-target/tmp/ime-kitty-live}"
uinput_sender="${VINPST_LIVE_TOOLKIT_UINPUT_SENDER:-scripts/live/niri/probes/send-uinput-key.py}"
output_file="${out_dir}/${mode}.txt"
focus_log="${out_dir}/${mode}.focus.json"
uinput_log="${out_dir}/${mode}.uinput.jsonl"
monitor_log="${out_dir}/${mode}.dbus.log"
summary="${out_dir}/${mode}.summary.json"
kitty_stdout="${out_dir}/${mode}.stdout.log"
kitty_stderr="${out_dir}/${mode}.stderr.log"
primary_before="${out_dir}/primary-selection-before.txt"
primary_restored="${out_dir}/primary-selection-restored.txt"
selected_text="selected text"
expected_prefix="adapter-backed: selected text | command: "
kitty_pid=""
monitor_pid=""
primary_owner_pid=""
primary_before_present=0
primary_snapshot_ready=0
app_id="fcitx-vinpst-kitty-live-${mode}"
window_title="fcitx-vinpst kitty ${mode} live probe"

call_service() {
  gdbus call --session \
    --dest org.fcitx.Vinpst \
    --object-path /org/fcitx/Vinpst \
    --method "org.fcitx.Vinpst.Service.$1" "${@:2}"
}

restore_idle() {
  local status
  status="$(call_service GetStatus 2>/dev/null || true)"
  if [[ "${status}" == *"'recording'"* ]]; then
    call_service StopRecording "" >/dev/null 2>&1 || true
  fi
}

restore_primary_selection() {
  if [[ "${primary_snapshot_ready}" != 1 ]]; then
    return 0
  fi
  if [[ -n "${primary_owner_pid}" ]]; then
    kill -TERM "${primary_owner_pid}" 2>/dev/null || true
    wait "${primary_owner_pid}" 2>/dev/null || true
    primary_owner_pid=""
  fi
  if [[ "${primary_before_present}" == 1 ]]; then
    wl-copy --primary --type 'text/plain;charset=utf-8' <"${primary_before}" >/dev/null 2>&1
    for _ in $(seq 1 50); do
      if timeout 1s wl-paste --primary --no-newline >"${primary_restored}" 2>/dev/null &&
        cmp -s "${primary_before}" "${primary_restored}"; then
        return 0
      fi
      sleep 0.05
    done
    echo "failed to restore the previous Wayland primary selection" >&2
    return 1
  fi
  wl-copy --primary --clear
  for _ in $(seq 1 50); do
    if ! timeout 1s wl-paste --primary --no-newline >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.05
  done
  echo "failed to clear the temporary Wayland primary selection" >&2
  return 1
}

cleanup() {
  local exit_code=$?
  trap - EXIT
  set +e
  restore_idle
  if [[ -n "${kitty_pid}" ]]; then
    kill -TERM "${kitty_pid}" 2>/dev/null || true
    wait "${kitty_pid}" 2>/dev/null || true
  fi
  if [[ -n "${monitor_pid}" ]]; then
    kill -TERM "${monitor_pid}" 2>/dev/null || true
    wait "${monitor_pid}" 2>/dev/null || true
  fi
  if ! restore_primary_selection; then
    exit_code=1
  fi
  exit "${exit_code}"
}
trap cleanup EXIT

if [[ -z "${wav}" || ! -f "${wav}" ]]; then
  echo "set VINPST_LIVE_TOOLKIT_WAV to a validated speech WAV" >&2
  exit 2
fi
for command in cmp gdbus jq kitty niri pw-play python3 stdbuf timeout; do
  command -v "${command}" >/dev/null 2>&1 || {
    echo "required kitty live command is missing: ${command}" >&2
    exit 1
  }
done
if [[ "${mode}" == command ]]; then
  for command in wl-copy wl-paste; do
    command -v "${command}" >/dev/null 2>&1 || {
      echo "required primary-selection command is missing: ${command}" >&2
      exit 1
    }
  done
fi
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
  echo "org.fcitx.Vinpst must be idle before the kitty probe: ${status:-unavailable}" >&2
  exit 1
fi

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
: >"${output_file}"
: >"${uinput_log}"

if [[ "${mode}" == command ]]; then
  if timeout 2s wl-paste --primary --no-newline >"${primary_before}" 2>/dev/null; then
    primary_before_present=1
  else
    : >"${primary_before}"
  fi
  primary_snapshot_ready=1
  wl-copy --primary --foreground --type 'text/plain;charset=utf-8' \
    < <(printf '%s' "${selected_text}") &
  primary_owner_pid=$!
  primary_ready=0
  for _ in $(seq 1 100); do
    if ! kill -0 "${primary_owner_pid}" 2>/dev/null; then
      break
    fi
    if current_primary="$(timeout 1s wl-paste --primary --no-newline 2>/dev/null)" &&
      [[ "${current_primary}" == "${selected_text}" ]]; then
      primary_ready=1
      break
    fi
    sleep 0.05
  done
  if [[ "${primary_ready}" != 1 ]]; then
    echo "temporary Wayland primary selection did not become readable" >&2
    exit 1
  fi
fi

stdbuf -oL -eL gdbus monitor --session \
  --dest org.fcitx.Vinpst \
  --object-path /org/fcitx/Vinpst >"${monitor_log}" 2>&1 &
monitor_pid=$!

GTK_IM_MODULE=fcitx kitty \
  --config NONE \
  --class "${app_id}" \
  --title "${window_title}" \
  bash --noprofile --norc -c 'cat > "$1"' _ "${output_file}" \
  >"${kitty_stdout}" 2>"${kitty_stderr}" &
kitty_pid=$!

window_id=""
for _ in $(seq 1 200); do
  window_id="$(
    niri msg --json windows 2>/dev/null |
      jq -r --argjson pid "${kitty_pid}" --arg app_id "${app_id}" \
        '[.[] | select(.pid == $pid and .app_id == $app_id) | .id] | if length == 1 then .[0] else empty end'
  )"
  if [[ -n "${window_id}" ]]; then
    niri msg action focus-window --id "${window_id}" >/dev/null
    break
  fi
  if ! kill -0 "${kitty_pid}" 2>/dev/null; then
    echo "kitty exited before creating its window" >&2
    exit 1
  fi
  sleep 0.1
done
if [[ -z "${window_id}" ]]; then
  echo "kitty window did not appear" >&2
  exit 1
fi
sleep 0.5
focused_id="$(niri msg --json focused-window | jq -r '.id // empty')"
if [[ "${focused_id}" != "${window_id}" ]]; then
  echo "kitty window did not become focused" >&2
  exit 1
fi
jq -n \
  --arg backend niri \
  --arg socket "${NIRI_SOCKET}" \
  --arg app_id "${app_id}" \
  --argjson pid "${kitty_pid}" \
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
  echo "daemon did not enter recording after the kitty shortcut" >&2
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
  echo "daemon did not return to idle after the kitty shortcut" >&2
  exit 1
fi
sleep 0.5
"${uinput_sender}" ENTER | tee -a "${uinput_log}"

written=0
for _ in $(seq 1 200); do
  content="$(cat "${output_file}")"
  if [[ "${mode}" == normal && -n "${content}" ]]; then
    written=1
    break
  fi
  if [[ "${mode}" == command && "${content}" == "${expected_prefix}"* ]]; then
    written=1
    break
  fi
  sleep 0.05
done
if [[ "${written}" != 1 ]]; then
  echo "kitty did not write the expected terminal input" >&2
  printf 'content=%q\n' "$(cat "${output_file}")" >&2
  exit 1
fi

partial_count="$(grep -c 'RecognitionPartial' "${monitor_log}" || true)"
if [[ "${partial_count}" -lt 1 ]]; then
  echo "kitty probe observed no daemon partial signal" >&2
  exit 1
fi
content="$(cat "${output_file}")"
primary_fallback=false
if [[ "${mode}" == command ]]; then
  primary_fallback=true
fi
jq -n \
  --arg mode "${mode}" \
  --arg output_file "${output_file}" \
  --arg content "${content}" \
  --arg trigger_key "${trigger_key}" \
  --arg selected_text "$( [[ "${mode}" == command ]] && printf '%s' "${selected_text}" || true )" \
  --argjson kitty_pid "${kitty_pid}" \
  --argjson window_id "${window_id}" \
  --argjson partial_count "${partial_count}" \
  --argjson primary_selection_fallback "${primary_fallback}" \
  '{
    event: "summary",
    application: "kitty",
    mode: $mode,
    output_file: $output_file,
    content: $content,
    trigger_key: $trigger_key,
    selected_text: $selected_text,
    kitty_pid: $kitty_pid,
    window_id: $window_id,
    partial_count: $partial_count,
    primary_selection_fallback: $primary_selection_fallback,
    written: true,
    ok: true
  }' | tee "${summary}"

kill -TERM "${kitty_pid}"
wait "${kitty_pid}" 2>/dev/null || true
kitty_pid=""
for _ in $(seq 1 100); do
  if ! niri msg --json windows | jq -e --argjson id "${window_id}" 'any(.[]; .id == $id)' >/dev/null; then
    break
  fi
  sleep 0.05
done
if niri msg --json windows | jq -e --argjson id "${window_id}" 'any(.[]; .id == $id)' >/dev/null; then
  echo "kitty window remained after process termination" >&2
  exit 1
fi
if ! restore_primary_selection; then
  exit 1
fi

printf 'kitty %s live probe passed; evidence: %s\n' "${mode}" "${out_dir}"
