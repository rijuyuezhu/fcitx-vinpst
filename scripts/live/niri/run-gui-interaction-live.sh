#!/usr/bin/env bash
set -euo pipefail

gui_bin="${VINPUT_GUI_LIVE_BIN:-target/debug/vinput-gui}"
out_dir="${VINPUT_GUI_LIVE_OUT_DIR:-target/tmp/gui-interaction-live}"
key_sender="${VINPUT_GUI_LIVE_KEY_SENDER:-scripts/live/niri/probes/send-uinput-key.py}"
text_sender="${VINPUT_GUI_LIVE_TEXT_SENDER:-scripts/live/niri/probes/send-uinput-text.py}"
rime_im="${VINPUT_GUI_LIVE_RIME_IM:-rime}"
rime_input="${VINPUT_GUI_LIVE_RIME_INPUT:-ceshi}"
notification_url="${VINPUT_GUI_LIVE_NOTIFICATION_URL:-http://127.0.0.1:9/notification.json}"
first_marker="gui-focus-clipboard-7x9"
second_marker="gui-second-field-marker"
clipboard_before=""
clipboard_had_text=0
clipboard_types_before=""
previous_fcitx_state=""
previous_fcitx_im=""
gui_pid=""
gui_window_id=""
runtime_root=""
restored=0

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

for command in fcitx5-remote jq niri python3 timeout wl-copy wl-paste; do
  command -v "${command}" >/dev/null 2>&1 || fail "required GUI live command is missing: ${command}"
done
[[ -x "${gui_bin}" ]] || fail "GUI binary is missing or not executable: ${gui_bin}"
[[ -x "${key_sender}" ]] || fail "uinput key sender is missing or not executable: ${key_sender}"
[[ -x "${text_sender}" ]] || fail "uinput text sender is missing or not executable: ${text_sender}"
[[ -w /dev/uinput ]] || fail "/dev/uinput is not writable"
fcitx5-remote --check >/dev/null 2>&1 || fail "Fcitx5 must already be running"

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
  runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
  mapfile -t wayland_sockets < <(
    find "${runtime_dir}" -maxdepth 1 -type s -name 'wayland-*' -printf '%f\n' 2>/dev/null
  )
  [[ "${#wayland_sockets[@]}" == 1 ]] ||
    fail "expected exactly one Wayland display socket, found ${#wayland_sockets[@]}"
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

window_title() {
  niri msg --json windows |
    jq -er --argjson id "${gui_window_id}" '.[] | select(.id == $id) | .title'
}

expect_title() {
  local expected=$1
  local actual
  actual="$(window_title)"
  [[ "${actual}" == "${expected}" ]] ||
    fail "unexpected GUI title: expected '${expected}', got '${actual}'"
}

send_page_shortcut() {
  local key=$1
  local expected=$2
  local actual=""
  for _ in $(seq 1 3); do
    niri msg action focus-window --id "${gui_window_id}" >/dev/null
    sleep 0.2
    send_key "${key}"
    for _ in $(seq 1 10); do
      actual="$(window_title)"
      [[ "${actual}" == "${expected}" ]] && return
      sleep 0.1
    done
  done
  fail "page shortcut ${key} did not reach '${expected}'; last title was '${actual}'"
}

stop_gui() {
  [[ -z "${gui_pid}" ]] && return
  kill -TERM "${gui_pid}" 2>/dev/null || true
  wait "${gui_pid}" 2>/dev/null || true
  gui_pid=""
  gui_window_id=""
}

start_gui() {
  local locale=$1
  local name=$2
  local expected_title=$3
  local root="${runtime_root}/${name}"
  mkdir -p "${root}/config" "${root}/cache" "${root}/data"
  LANG="${locale}" \
    XDG_CONFIG_HOME="${root}/config" \
    XDG_CACHE_HOME="${root}/cache" \
    XDG_DATA_HOME="${root}/data" \
    VINPUT_NOTIFICATION_URL="${notification_url}" \
    "${gui_bin}" >"${out_dir}/${name}.stdout.log" 2>"${out_dir}/${name}.stderr.log" &
  gui_pid=$!
  gui_window_id=""
  for _ in $(seq 1 200); do
    gui_window_id="$(
      niri msg --json windows 2>/dev/null |
        jq -r --argjson pid "${gui_pid}" \
          '[.[] | select(.pid == $pid) | .id] | if length == 1 then .[0] else empty end'
    )"
    if [[ -n "${gui_window_id}" ]]; then
      niri msg action focus-window --id "${gui_window_id}" >/dev/null
      break
    fi
    kill -0 "${gui_pid}" 2>/dev/null ||
      fail "${name} GUI exited before creating a window"
    sleep 0.1
  done
  [[ -n "${gui_window_id}" ]] || fail "${name} GUI window did not appear"
  sleep 0.6
  expect_title "${expected_title}"
  if grep -Eq 'Cannot start a runtime from within a runtime|panicked at' \
    "${out_dir}/${name}.stderr.log"; then
    fail "${name} GUI emitted a runtime panic"
  fi
}

start_gui en_US.UTF-8 en 'Vinput Configuration — Control'
en_titles=("$(window_title)")
send_page_shortcut CTRL+2 'Vinput Configuration — Resources'
en_titles+=("$(window_title)")
send_page_shortcut CTRL+4 'Vinput Configuration — Hotwords'
en_titles+=("$(window_title)")
send_page_shortcut CTRL+1 'Vinput Configuration — Control'
en_titles+=("$(window_title)")

send_key ESCAPE
send_key TAB
send_key CTRL+A
send_key BACKSPACE
send_text "${first_marker}"
send_key CTRL+A
send_key CTRL+C
clipboard_probe="$(timeout 2s wl-paste --no-newline)"
[[ "${clipboard_probe}" == "${first_marker}" ]] || fail "Tab did not focus the first text field"

send_key TAB
send_key CTRL+A
send_key BACKSPACE
send_text "${second_marker}"
send_key SHIFT+TAB
send_key CTRL+A
send_key CTRL+C
reverse_probe="$(timeout 2s wl-paste --no-newline)"
[[ "${reverse_probe}" == "${first_marker}" ]] || fail "Shift+Tab did not return to the first text field"

send_key CTRL+A
send_key BACKSPACE
fcitx5-remote -s "${rime_im}" >/dev/null
fcitx5-remote -o >/dev/null
sleep 0.5
[[ "$(fcitx5-remote)" == 2 && "$(fcitx5-remote -n)" == "${rime_im}" ]] ||
  fail "requested Fcitx input method did not become active"
send_text "${rime_input}"
sleep 0.5
send_key SPACE
sleep 0.7
fcitx5-remote -c >/dev/null
send_key CTRL+A
send_key CTRL+C
ime_commit="$(timeout 2s wl-paste --no-newline)"
ime_commit_bytes="$(python3 - "${ime_commit}" "${rime_input}" <<'PY'
import sys

text, raw = sys.argv[1:]
if not text or text.strip() == raw or not any(ord(character) > 127 for character in text):
    raise SystemExit("Rime did not commit non-ASCII text into the Iced field")
print(len(text.encode("utf-8")))
PY
)"
stop_gui

start_gui zh_CN.UTF-8 zh 'Vinput 配置 — 控制'
zh_titles=("$(window_title)")
send_page_shortcut CTRL+2 'Vinput 配置 — 资源'
zh_titles+=("$(window_title)")
send_page_shortcut CTRL+4 'Vinput 配置 — 热词'
zh_titles+=("$(window_title)")
send_page_shortcut CTRL+1 'Vinput 配置 — 控制'
zh_titles+=("$(window_title)")
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
    fail "standard clipboard unexpectedly retained the probe value"
  fi
fi

jq -n \
  --arg backend niri \
  --arg socket "${NIRI_SOCKET}" \
  --arg wayland_display "${WAYLAND_DISPLAY}" \
  --arg previous_fcitx_im "${previous_fcitx_im}" \
  --arg rime_im "${rime_im}" \
  --argjson previous_fcitx_state "${previous_fcitx_state}" \
  --argjson ime_commit_bytes "${ime_commit_bytes}" \
  --argjson en_titles "$(printf '%s\n' "${en_titles[@]}" | jq -R . | jq -s .)" \
  --argjson zh_titles "$(printf '%s\n' "${zh_titles[@]}" | jq -R . | jq -s .)" \
  '{
    ok: true,
    backend: $backend,
    niri_socket: $socket,
    wayland_display: $wayland_display,
    english_titles: $en_titles,
    zh_cn_titles: $zh_titles,
    keyboard: {
      page_shortcuts: true,
      tab_text_focus: true,
      shift_tab_text_focus: true
    },
    clipboard: {
      standard_copy: true,
      restored: true
    },
    input_method: {
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
  .keyboard.page_shortcuts == true and
  .keyboard.tab_text_focus == true and
  .keyboard.shift_tab_text_focus == true and
  .clipboard.standard_copy == true and
  .clipboard.restored == true and
  .input_method.preedit_commit == true and
  .input_method.committed_utf8_bytes > 0 and
  .input_method.content_retained == false and
  .restoration.gui_processes == 0
' "${out_dir}/summary.json" >/dev/null
cat "${out_dir}/summary.json"
