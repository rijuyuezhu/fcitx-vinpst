#!/usr/bin/env bash
set -euo pipefail

gui_bin="${VINPUT_GUI_X11_LIVE_BIN:-target/debug/vinput-gui}"
out_dir="${VINPUT_GUI_X11_LIVE_OUT_DIR:-target/tmp/gui-x11-interaction-live}"
key_sender="${VINPUT_GUI_X11_LIVE_KEY_SENDER:-scripts/live/niri/probes/send-uinput-key.py}"
text_sender="${VINPUT_GUI_X11_LIVE_TEXT_SENDER:-scripts/live/niri/probes/send-uinput-text.py}"
rime_im="${VINPUT_GUI_X11_LIVE_RIME_IM:-rime}"
rime_input="${VINPUT_GUI_X11_LIVE_RIME_INPUT:-ceshi}"
notification_url="${VINPUT_GUI_X11_LIVE_NOTIFICATION_URL:-http://127.0.0.1:9/notification.json}"
first_marker="gui-x11-focus-clipboard-7x9"
second_marker="gui-x11-second-field-marker"
clipboard_before=""
clipboard_had_text=0
clipboard_types_before=""
previous_fcitx_state=""
previous_fcitx_im=""
gui_pid=""
gui_x11_window_id=""
gui_niri_window_id=""
runtime_root=""
restored=0
tracked_gui_pids=()

fail() {
  echo "$*" >&2
  exit 1
}

restore_user_state() {
  if [[ "${restored}" == 1 ]]; then
    return
  fi
  restored=1
  set +e
  if [[ -n "${gui_pid}" ]]; then
    kill -TERM "${gui_pid}" 2>/dev/null || true
    wait "${gui_pid}" 2>/dev/null || true
    gui_pid=""
  fi
  if [[ -n "${previous_fcitx_im}" ]]; then
    fcitx5-remote -s "${previous_fcitx_im}" >/dev/null 2>&1 || true
  fi
  case "${previous_fcitx_state}" in
  2) fcitx5-remote -o >/dev/null 2>&1 || true ;;
  0 | 1) fcitx5-remote -c >/dev/null 2>&1 || true ;;
  esac
  if [[ "${clipboard_had_text}" == 1 && -n "${clipboard_before}" ]]; then
    wl-copy --type 'text/plain;charset=utf-8' <"${clipboard_before}" >/dev/null 2>&1 || true
  else
    wl-copy --clear >/dev/null 2>&1 || true
  fi
  [[ -z "${runtime_root}" ]] || rm -rf "${runtime_root}"
}

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM
  restore_user_state
  [[ -z "${clipboard_before}" ]] || rm -f "${clipboard_before}"
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

for command in fcitx5-remote jq niri python3 timeout wl-copy wl-paste xclip xdpyinfo xprop xwininfo; do
  command -v "${command}" >/dev/null 2>&1 || fail "required X11 GUI live command is missing: ${command}"
done
[[ -x "${gui_bin}" ]] || fail "GUI binary is missing or not executable: ${gui_bin}"
[[ -x "${key_sender}" ]] || fail "uinput key sender is missing or not executable: ${key_sender}"
[[ -x "${text_sender}" ]] || fail "uinput text sender is missing or not executable: ${text_sender}"
[[ -w /dev/uinput ]] || fail "/dev/uinput is not writable"
fcitx5-remote --check >/dev/null 2>&1 || fail "Fcitx5 must already be running"

session_environment="$(systemctl --user show-environment 2>/dev/null || true)"
if [[ -z "${DISPLAY:-}" ]]; then
  DISPLAY="$(sed -n 's/^DISPLAY=//p' <<<"${session_environment}")"
  export DISPLAY
fi
[[ -n "${DISPLAY:-}" ]] || fail "an X11 display is required"
DISPLAY="${DISPLAY}" xdpyinfo >/dev/null 2>&1 || fail "cannot connect to X11 display ${DISPLAY}"

if [[ -z "${NIRI_SOCKET:-}" || ! -S "${NIRI_SOCKET}" ]]; then
  NIRI_SOCKET="$(sed -n 's/^NIRI_SOCKET=//p' <<<"${session_environment}")"
  export NIRI_SOCKET
fi
if [[ -z "${NIRI_SOCKET:-}" || ! -S "${NIRI_SOCKET}" ]]; then
  runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
  mapfile -t niri_sockets < <(
    find "${runtime_dir}" -maxdepth 1 -type s -name 'niri*.sock' -print 2>/dev/null
  )
  [[ "${#niri_sockets[@]}" == 1 ]] ||
    fail "expected exactly one live niri socket, found ${#niri_sockets[@]}"
  export NIRI_SOCKET="${niri_sockets[0]}"
fi
niri msg --json windows >/dev/null

if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
  WAYLAND_DISPLAY="$(sed -n 's/^WAYLAND_DISPLAY=//p' <<<"${session_environment}")"
  export WAYLAND_DISPLAY
fi
if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
  runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
  mapfile -t wayland_sockets < <(
    find "${runtime_dir}" -maxdepth 1 -type s -name 'wayland-*' -printf '%f\n' 2>/dev/null
  )
  [[ "${#wayland_sockets[@]}" == 1 ]] ||
    fail "expected exactly one Wayland display socket for state restoration, found ${#wayland_sockets[@]}"
  export WAYLAND_DISPLAY="${wayland_sockets[0]}"
fi

clipboard_before="$(mktemp)"
clipboard_types_before="$(timeout 1s wl-paste --list-types 2>/dev/null || true)"
if [[ -n "${clipboard_types_before}" ]]; then
  if ! grep -Eq '^(text/plain|text/plain;charset=utf-8|UTF8_STRING)$' <<<"${clipboard_types_before}"; then
    fail "standard clipboard has no restorable text representation; refusing to replace it"
  fi
  timeout 2s wl-paste --no-newline >"${clipboard_before}"
  clipboard_had_text=1
fi
previous_fcitx_state="$(fcitx5-remote)"
previous_fcitx_im="$(fcitx5-remote -n)"
[[ "${previous_fcitx_state}" =~ ^[012]$ ]] || fail "unexpected Fcitx state: ${previous_fcitx_state}"
[[ -n "${previous_fcitx_im}" ]] || fail "current Fcitx input method is unavailable"

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
: >"${out_dir}/uinput.jsonl"
runtime_root="$(mktemp -d)"

send_key() {
  "${key_sender}" --settle-ms 300 "$1" | tee -a "${out_dir}/uinput.jsonl" >/dev/null
}

send_text() {
  "${text_sender}" --settle-ms 120 --key-delay-ms 35 "$1" |
    tee -a "${out_dir}/uinput.jsonl" >/dev/null
}

x11_window_ids() {
  DISPLAY="${DISPLAY}" xwininfo -root -tree |
    awk '{for (field = 1; field <= NF; field++) if ($field ~ /^0x[0-9a-fA-F]+$/) print $field}' |
    sort -u
}

x11_window_for_pid() {
  local target_pid=$1
  local window_id=""
  local property_pid=""
  while IFS= read -r candidate; do
    property_pid="$(
      DISPLAY="${DISPLAY}" xprop -id "${candidate}" _NET_WM_PID 2>/dev/null |
        awk '/_NET_WM_PID/ {print $3}'
    )"
    if [[ "${property_pid}" == "${target_pid}" ]]; then
      window_id="${candidate}"
      break
    fi
  done < <(x11_window_ids)
  printf '%s' "${window_id}"
}

x11_window_title() {
  DISPLAY="${DISPLAY}" xprop -id "${gui_x11_window_id}" _NET_WM_NAME 2>/dev/null |
    sed -n 's/^_NET_WM_NAME(UTF8_STRING) = "\(.*\)"$/\1/p'
}

expect_title() {
  local expected=$1
  local actual
  actual="$(x11_window_title)"
  [[ "${actual}" == "${expected}" ]] ||
    fail "unexpected X11 GUI title: expected '${expected}', got '${actual}'"
}

focus_gui() {
  niri msg action focus-window --id "${gui_niri_window_id}" >/dev/null
  sleep 0.2
}

send_page_shortcut() {
  local key=$1
  local expected=$2
  local actual=""
  for _ in $(seq 1 3); do
    focus_gui
    send_key "${key}"
    for _ in $(seq 1 10); do
      actual="$(x11_window_title)"
      [[ "${actual}" == "${expected}" ]] && return
      sleep 0.1
    done
  done
  fail "X11 page shortcut ${key} did not reach '${expected}'; last title was '${actual}'"
}

focus_text_field_with_marker() {
  local marker=$1
  local clipboard_probe=""
  for attempt in $(seq 1 40); do
    printf 'x11-not-a-text-field-%s' "${attempt}" | wl-copy
    send_key TAB
    send_key CTRL+A
    send_key BACKSPACE
    send_text "${marker}"
    send_key CTRL+A
    send_key CTRL+C
    clipboard_probe="$(timeout 2s xclip -selection clipboard -o 2>/dev/null || true)"
    [[ "${clipboard_probe}" == "${marker}" ]] && return
  done
  fail "X11 Tab traversal did not reach a writable text field for marker ${marker}"
}

return_to_text_marker() {
  local marker=$1
  local clipboard_probe=""
  for attempt in $(seq 1 40); do
    printf 'x11-not-the-target-field-%s' "${attempt}" | wl-copy
    send_key SHIFT+TAB
    send_key CTRL+A
    send_key CTRL+C
    clipboard_probe="$(timeout 2s xclip -selection clipboard -o 2>/dev/null || true)"
    [[ "${clipboard_probe}" == "${marker}" ]] && return
  done
  fail "X11 Shift+Tab traversal did not return to the target text field"
}

stop_gui() {
  [[ -z "${gui_pid}" ]] && return
  kill -TERM "${gui_pid}" 2>/dev/null || true
  wait "${gui_pid}" 2>/dev/null || true
  gui_pid=""
  gui_x11_window_id=""
  gui_niri_window_id=""
}

start_gui() {
  local locale=$1
  local name=$2
  local expected_title=$3
  local root="${runtime_root}/${name}"
  mkdir -p "${root}/config" "${root}/cache" "${root}/data"
  env -u WAYLAND_DISPLAY \
    DISPLAY="${DISPLAY}" \
    WINIT_UNIX_BACKEND=x11 \
    XMODIFIERS=@im=fcitx \
    LANG="${locale}" \
    XDG_CONFIG_HOME="${root}/config" \
    XDG_CACHE_HOME="${root}/cache" \
    XDG_DATA_HOME="${root}/data" \
    VINPUT_NOTIFICATION_URL="${notification_url}" \
    "${gui_bin}" >"${out_dir}/${name}.stdout.log" 2>"${out_dir}/${name}.stderr.log" &
  gui_pid=$!
  tracked_gui_pids+=("${gui_pid}")
  gui_x11_window_id=""
  for _ in $(seq 1 200); do
    gui_x11_window_id="$(x11_window_for_pid "${gui_pid}")"
    [[ -n "${gui_x11_window_id}" ]] && break
    kill -0 "${gui_pid}" 2>/dev/null || fail "${name} X11 GUI exited before creating a window"
    sleep 0.1
  done
  [[ -n "${gui_x11_window_id}" ]] || fail "${name} X11 GUI window did not appear"

  gui_niri_window_id=""
  for _ in $(seq 1 100); do
    gui_niri_window_id="$(
      niri msg --json windows 2>/dev/null |
        jq -r --arg title "${expected_title}" \
          '[.[] | select(.title == $title) | .id] | if length == 1 then .[0] else empty end'
    )"
    [[ -n "${gui_niri_window_id}" ]] && break
    sleep 0.1
  done
  [[ -n "${gui_niri_window_id}" ]] || fail "${name} X11 GUI compositor window did not appear"

  focus_gui
  expect_title "${expected_title}"
  DISPLAY="${DISPLAY}" xprop -id "${gui_x11_window_id}" \
    _NET_WM_PID _NET_WM_NAME WM_CLASS WM_CLIENT_MACHINE >"${out_dir}/${name}.xprop.txt"
  grep -Eq "_NET_WM_PID\(CARDINAL\) = ${gui_pid}$" "${out_dir}/${name}.xprop.txt" ||
    fail "${name} X11 window PID does not match the GUI process"
  if grep -Eq 'Cannot start a runtime from within a runtime|panicked at' \
    "${out_dir}/${name}.stderr.log"; then
    fail "${name} X11 GUI emitted a runtime panic"
  fi
}

start_gui en_US.UTF-8 en 'Vinput Configuration — Control'
en_titles=("$(x11_window_title)")

send_key ESCAPE
send_key TAB
send_key TAB
send_key ENTER
sleep 0.3
expect_title 'Vinput Configuration — Resources'
send_key SHIFT+TAB
send_key SPACE
sleep 0.3
expect_title 'Vinput Configuration — Control'

send_page_shortcut CTRL+2 'Vinput Configuration — Resources'
en_titles+=("$(x11_window_title)")
send_page_shortcut CTRL+4 'Vinput Configuration — Hotwords'
en_titles+=("$(x11_window_title)")
send_page_shortcut CTRL+1 'Vinput Configuration — Control'
en_titles+=("$(x11_window_title)")

send_key ESCAPE
focus_text_field_with_marker "${first_marker}"
focus_text_field_with_marker "${second_marker}"
return_to_text_marker "${first_marker}"

send_key CTRL+A
send_key BACKSPACE
fcitx5-remote -s "${rime_im}" >/dev/null
fcitx5-remote -o >/dev/null
sleep 0.5
[[ "$(fcitx5-remote)" == 2 && "$(fcitx5-remote -n)" == "${rime_im}" ]] ||
  fail "requested Fcitx input method did not become active for X11"
send_text "${rime_input}"
sleep 0.5
send_key SPACE
sleep 0.7
fcitx5-remote -c >/dev/null
send_key CTRL+A
send_key CTRL+C
ime_commit="$(timeout 2s xclip -selection clipboard -o)"
ime_commit_bytes="$(python3 - "${ime_commit}" "${rime_input}" <<'PY'
import sys

text, raw = sys.argv[1:]
if not text or text.strip() == raw or not any(ord(character) > 127 for character in text):
    raise SystemExit("Rime did not commit non-ASCII text into the X11 Iced field")
print(len(text.encode("utf-8")))
PY
)"
stop_gui

start_gui zh_CN.UTF-8 zh 'Vinput 配置 — 控制'
zh_titles=("$(x11_window_title)")
send_page_shortcut CTRL+2 'Vinput 配置 — 资源'
zh_titles+=("$(x11_window_title)")
send_page_shortcut CTRL+4 'Vinput 配置 — 热词'
zh_titles+=("$(x11_window_title)")
send_page_shortcut CTRL+1 'Vinput 配置 — 控制'
zh_titles+=("$(x11_window_title)")
stop_gui

restore_user_state
sleep 0.2
[[ "$(fcitx5-remote)" == "${previous_fcitx_state}" ]] || fail "Fcitx state was not restored"
[[ "$(fcitx5-remote -n)" == "${previous_fcitx_im}" ]] || fail "Fcitx input method was not restored"
if [[ "${clipboard_had_text}" == 1 ]]; then
  timeout 2s wl-paste --no-newline >"${out_dir}/clipboard-restored.tmp"
  cmp -s "${clipboard_before}" "${out_dir}/clipboard-restored.tmp" ||
    fail "standard clipboard text was not restored"
  rm -f "${out_dir}/clipboard-restored.tmp"
else
  if timeout 1s wl-paste --no-newline >/dev/null 2>&1; then
    fail "standard clipboard unexpectedly retained the X11 probe value"
  fi
fi
for tracked_pid in "${tracked_gui_pids[@]}"; do
  kill -0 "${tracked_pid}" 2>/dev/null && fail "tracked X11 GUI process still exists: ${tracked_pid}"
done

jq -n \
  --arg backend x11-xwayland \
  --arg display "${DISPLAY}" \
  --arg socket "${NIRI_SOCKET}" \
  --arg previous_fcitx_im "${previous_fcitx_im}" \
  --arg rime_im "${rime_im}" \
  --argjson previous_fcitx_state "${previous_fcitx_state}" \
  --argjson ime_commit_bytes "${ime_commit_bytes}" \
  --argjson en_titles "$(printf '%s\n' "${en_titles[@]}" | jq -R . | jq -s .)" \
  --argjson zh_titles "$(printf '%s\n' "${zh_titles[@]}" | jq -R . | jq -s .)" \
  '{
    ok: true,
    backend: $backend,
    display: $display,
    niri_socket: $socket,
    x11: {
      forced_winit_backend: true,
      wayland_display_removed_from_gui: true,
      client_pid_match: true,
      utf8_window_title: true
    },
    english_titles: $en_titles,
    zh_cn_titles: $zh_titles,
    keyboard: {
      page_shortcuts: true,
      non_text_tab_focus: true,
      enter_activation: true,
      space_activation: true,
      mixed_control_tab_focus: true,
      tab_text_focus: true,
      shift_tab_text_focus: true
    },
    clipboard: {
      x11_standard_copy: true,
      restored: true
    },
    input_method: {
      transport: "XIM",
      engine: $rime_im,
      preedit_commit: true,
      committed_utf8_bytes: $ime_commit_bytes,
      content_retained: false
    },
    restoration: {
      fcitx_state: $previous_fcitx_state,
      fcitx_input_method: $previous_fcitx_im,
      gui_processes: 0
    }
  }' >"${out_dir}/summary.json"

jq -e '
  .ok == true and
  .backend == "x11-xwayland" and
  .x11.forced_winit_backend == true and
  .x11.wayland_display_removed_from_gui == true and
  .x11.client_pid_match == true and
  .x11.utf8_window_title == true and
  .keyboard.page_shortcuts == true and
  .keyboard.non_text_tab_focus == true and
  .keyboard.enter_activation == true and
  .keyboard.space_activation == true and
  .keyboard.mixed_control_tab_focus == true and
  .keyboard.tab_text_focus == true and
  .keyboard.shift_tab_text_focus == true and
  .clipboard.x11_standard_copy == true and
  .clipboard.restored == true and
  .input_method.transport == "XIM" and
  .input_method.preedit_commit == true and
  .input_method.committed_utf8_bytes > 0 and
  .input_method.content_retained == false and
  .restoration.fcitx_state >= 0 and
  .restoration.fcitx_state <= 2 and
  (.restoration.fcitx_input_method | length) > 0 and
  .restoration.gui_processes == 0
' "${out_dir}/summary.json" >/dev/null

jq . "${out_dir}/summary.json"
