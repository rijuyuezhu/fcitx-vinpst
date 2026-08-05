#!/usr/bin/env bash
set -euo pipefail

gui_bin="${VINPUT_GUI_PORTAL_LIVE_BIN:-target/debug/vinput-gui}"
out_dir="${VINPUT_GUI_PORTAL_LIVE_OUT_DIR:-target/tmp/gui-portal-picker-live}"
portal_fixture="${VINPUT_GUI_PORTAL_LIVE_FIXTURE:-scripts/live/niri/probes/gui-filechooser-portal-fixture.py}"
key_sender="${VINPUT_GUI_PORTAL_LIVE_KEY_SENDER:-scripts/live/niri/probes/send-uinput-key.py}"
notification_url="${VINPUT_GUI_PORTAL_LIVE_NOTIFICATION_URL:-http://127.0.0.1:9/notification.json}"
clipboard_before=""
clipboard_had_text=0
clipboard_types_before=""
previous_niri_window_id=""
runtime_root=""
private_bus_pid=""
portal_pid=""
gui_pid=""
gui_window_id=""
tracked_gui_pids=()

fail() {
  echo "$*" >&2
  exit 1
}

restore_user_state() {
  set +e
  if [[ -n "${gui_pid}" ]]; then
    kill -TERM "${gui_pid}" 2>/dev/null || true
    wait "${gui_pid}" 2>/dev/null || true
    gui_pid=""
  fi
  if [[ -n "${portal_pid}" ]]; then
    kill -TERM "${portal_pid}" 2>/dev/null || true
    wait "${portal_pid}" 2>/dev/null || true
    portal_pid=""
  fi
  if [[ -n "${private_bus_pid}" ]]; then
    kill -TERM "${private_bus_pid}" 2>/dev/null || true
    wait "${private_bus_pid}" 2>/dev/null || true
    private_bus_pid=""
  fi
  if [[ -n "${previous_niri_window_id}" ]] &&
    niri msg --json windows 2>/dev/null |
      jq -e --argjson id "${previous_niri_window_id}" '.[] | select(.id == $id)' >/dev/null; then
    niri msg action focus-window --id "${previous_niri_window_id}" >/dev/null 2>&1 || true
    sleep 0.2
  fi
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

for command in dbus-daemon jq niri python3 sha256sum timeout wl-copy wl-paste; do
  command -v "${command}" >/dev/null 2>&1 ||
    fail "required portal-picker live command is missing: ${command}"
done
[[ -x "${gui_bin}" ]] || fail "GUI binary is missing or not executable: ${gui_bin}"
[[ -x "${portal_fixture}" ]] || fail "portal fixture is missing or not executable: ${portal_fixture}"
[[ -x "${key_sender}" ]] || fail "uinput key sender is missing or not executable: ${key_sender}"
[[ -w /dev/uinput ]] || fail "/dev/uinput is not writable"

session_environment="$(systemctl --user show-environment 2>/dev/null || true)"
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
previous_niri_window_id="$(
  niri msg --json windows |
    jq -r '[.[] | select(.is_focused) | .id] | if length == 1 then .[0] else empty end'
)"

if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
  WAYLAND_DISPLAY="$(sed -n 's/^WAYLAND_DISPLAY=//p' <<<"${session_environment}")"
  export WAYLAND_DISPLAY
fi
[[ -n "${WAYLAND_DISPLAY:-}" ]] || fail "a Wayland display is required"

clipboard_before="$(mktemp)"
clipboard_types_before="$(timeout 1s wl-paste --list-types 2>/dev/null || true)"
if [[ -n "${clipboard_types_before}" ]]; then
  if ! grep -Eq '^(text/plain|text/plain;charset=utf-8|UTF8_STRING)$' <<<"${clipboard_types_before}"; then
    fail "standard clipboard has no restorable text representation; refusing to replace it"
  fi
  timeout 2s wl-paste --no-newline >"${clipboard_before}"
  clipboard_had_text=1
fi

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
: >"${out_dir}/uinput.jsonl"
runtime_root="$(mktemp -d)"
config_home="${runtime_root}/config"
cache_home="${runtime_root}/cache"
data_home="${runtime_root}/data"
home_dir="${runtime_root}/home"
fixture_dir="${runtime_root}/portal files"
config_path="${config_home}/fcitx-vinput/config.json"
configured_path="${fixture_dir}/configured hotwords.txt"
selected_path="${fixture_dir}/selected hotwords-测试.txt"
request_log="${out_dir}/portal-requests.jsonl"
ready_file="${runtime_root}/portal.ready"
mkdir -p "$(dirname "${config_path}")" "${cache_home}" "${data_home}" "${home_dir}" "${fixture_dir}"
printf 'configured fixture\n' >"${configured_path}"
printf 'selected fixture\n' >"${selected_path}"
python3 - "${config_path}" "${configured_path}" <<'PY'
import json
import pathlib
import sys

config_path = pathlib.Path(sys.argv[1])
hotword_path = sys.argv[2]
with pathlib.Path("data/default-config.json").open(encoding="utf-8") as stream:
    config = json.load(stream)
config["asr"]["providers"][0]["hotwords_file"] = hotword_path
config_path.write_text(json.dumps(config, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
config_path.chmod(0o600)
PY
config_hash_before="$(sha256sum "${config_path}" | awk '{print $1}')"

mapfile -t bus_info < <(dbus-daemon --session --fork --print-address=1 --print-pid=1)
[[ "${#bus_info[@]}" == 2 ]] || fail "private session bus did not return address and PID"
private_bus_address="${bus_info[0]}"
private_bus_pid="${bus_info[1]}"
[[ -n "${private_bus_address}" && "${private_bus_pid}" =~ ^[0-9]+$ ]] ||
  fail "private session bus metadata is invalid"

DBUS_SESSION_BUS_ADDRESS="${private_bus_address}" \
  "${portal_fixture}" \
    --selected-path "${selected_path}" \
    --responses select,cancel \
    --request-log "${request_log}" \
    --ready-file "${ready_file}" \
    >"${out_dir}/portal.stdout.log" 2>"${out_dir}/portal.stderr.log" &
portal_pid=$!
for _ in $(seq 1 100); do
  [[ -s "${ready_file}" ]] && break
  kill -0 "${portal_pid}" 2>/dev/null || fail "portal fixture exited before becoming ready"
  sleep 0.05
done
[[ -s "${ready_file}" ]] || fail "portal fixture did not become ready"

send_key() {
  "${key_sender}" --settle-ms 300 "$1" | tee -a "${out_dir}/uinput.jsonl" >/dev/null
}

window_title() {
  niri msg --json windows |
    jq -er --argjson id "${gui_window_id}" '.[] | select(.id == $id) | .title'
}

focus_gui() {
  niri msg action focus-window --id "${gui_window_id}" >/dev/null
  sleep 0.2
}

line_count() {
  if [[ -f "$1" ]]; then wc -l <"$1"; else printf '0\n'; fi
}

wait_for_request_count() {
  local expected=$1
  local actual=0
  for _ in $(seq 1 100); do
    actual="$(line_count "${request_log}")"
    ((actual >= expected)) && return
    sleep 0.05
  done
  fail "portal request log did not reach ${expected}; observed ${actual}"
}

stop_gui() {
  [[ -z "${gui_pid}" ]] && return
  kill -TERM "${gui_pid}" 2>/dev/null || true
  wait "${gui_pid}" 2>/dev/null || true
  gui_pid=""
  gui_window_id=""
}

start_gui() {
  local name=$1
  DBUS_SESSION_BUS_ADDRESS="${private_bus_address}" \
    LANG=en_US.UTF-8 \
    HOME="${home_dir}" \
    XDG_CONFIG_HOME="${config_home}" \
    XDG_CACHE_HOME="${cache_home}" \
    XDG_DATA_HOME="${data_home}" \
    VINPUT_NOTIFICATION_URL="${notification_url}" \
    "${gui_bin}" >"${out_dir}/${name}.stdout.log" 2>"${out_dir}/${name}.stderr.log" &
  gui_pid=$!
  tracked_gui_pids+=("${gui_pid}")
  gui_window_id=""
  for _ in $(seq 1 200); do
    gui_window_id="$(
      niri msg --json windows 2>/dev/null |
        jq -r --argjson pid "${gui_pid}" \
          '[.[] | select(.pid == $pid) | .id] | if length == 1 then .[0] else empty end'
    )"
    if [[ -n "${gui_window_id}" ]]; then
      focus_gui
      break
    fi
    kill -0 "${gui_pid}" 2>/dev/null || fail "${name} GUI exited before creating a window"
    sleep 0.1
  done
  [[ -n "${gui_window_id}" ]] || fail "${name} GUI window did not appear"
  [[ "$(window_title)" == 'Vinput Configuration — Control' ]] ||
    fail "${name} GUI did not expose the expected initial title"
  if grep -Eq 'Cannot start a runtime from within a runtime|panicked at' \
    "${out_dir}/${name}.stderr.log"; then
    fail "${name} GUI emitted a runtime panic"
  fi
}

open_hotwords_page() {
  focus_gui
  send_key CTRL+4
  for _ in $(seq 1 20); do
    [[ "$(window_title)" == 'Vinput Configuration — Hotwords' ]] && return
    sleep 0.1
  done
  fail "Hotwords page shortcut did not update the title"
}

focus_path_draft() {
  local expected=$1
  local probe=""
  focus_gui
  send_key ESCAPE
  for focus_step in $(seq 1 40); do
    send_key TAB
    printf 'portal-draft-sentinel-%s' "${focus_step}" | wl-copy
    send_key CTRL+A
    send_key CTRL+C
    probe="$(timeout 2s wl-paste --no-newline 2>/dev/null || true)"
    if [[ "${probe}" == "${expected}" ]]; then
      return
    fi
  done
  fail "Hotwords focus traversal did not reach the expected path draft"
}

activate_browse_from_path() {
  send_key TAB
  send_key ENTER
}

start_gui selected
open_hotwords_page
focus_path_draft "${configured_path}"
activate_browse_from_path
wait_for_request_count 1
focus_path_draft "${selected_path}"
[[ "$(sha256sum "${config_path}" | awk '{print $1}')" == "${config_hash_before}" ]] ||
  fail "portal selection mutated the config before Set path"
stop_gui

start_gui cancelled
open_hotwords_page
focus_path_draft "${configured_path}"
activate_browse_from_path
wait_for_request_count 2
focus_path_draft "${configured_path}"
[[ "$(sha256sum "${config_path}" | awk '{print $1}')" == "${config_hash_before}" ]] ||
  fail "portal cancellation mutated the config"
stop_gui

[[ "$(line_count "${request_log}")" == 2 ]] || fail "portal fixture observed an unexpected request count"
python3 - "${request_log}" "${fixture_dir}" <<'PY'
import json
import pathlib
import sys

records = [json.loads(line) for line in pathlib.Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()]
expected_folder = sys.argv[2]
assert len(records) == 2
assert [record["response_mode"] for record in records] == ["select", "cancel"]
for index, record in enumerate(records, start=1):
    assert record["request_index"] == index
    assert record["parent_window"] == ""
    assert record["title"]
    assert record["handle_token_prefix"] == "rfd"
    assert record["multiple"] is False
    assert record["directory"] is False
    assert record["current_folder"] == expected_folder
    patterns = [pattern["pattern"] for file_filter in record["filters"] for pattern in file_filter["patterns"]]
    assert "*.txt" in patterns
    assert "*" in patterns
print("portal request contract verified")
PY

for tracked_pid in "${tracked_gui_pids[@]}"; do
  kill -0 "${tracked_pid}" 2>/dev/null && fail "tracked GUI process still exists: ${tracked_pid}"
done
kill -TERM "${portal_pid}" 2>/dev/null || true
wait "${portal_pid}" 2>/dev/null || true
portal_pid=""
kill -TERM "${private_bus_pid}" 2>/dev/null || true
wait "${private_bus_pid}" 2>/dev/null || true
private_bus_pid=""
if [[ -n "${previous_niri_window_id}" ]] &&
  niri msg --json windows |
    jq -e --argjson id "${previous_niri_window_id}" '.[] | select(.id == $id)' >/dev/null; then
  niri msg action focus-window --id "${previous_niri_window_id}" >/dev/null
  sleep 0.2
fi
if [[ "${clipboard_had_text}" == 1 ]]; then
  wl-copy --type 'text/plain;charset=utf-8' <"${clipboard_before}"
  timeout 2s wl-paste --no-newline >"${out_dir}/clipboard-restored.tmp"
  cmp -s "${clipboard_before}" "${out_dir}/clipboard-restored.tmp" ||
    fail "standard clipboard text was not restored"
  rm -f "${out_dir}/clipboard-restored.tmp"
else
  wl-copy --clear
  if timeout 1s wl-paste --no-newline >/dev/null 2>&1; then
    fail "standard clipboard unexpectedly retained the portal probe value"
  fi
fi

jq -n '{
  ok: true,
  backend: "niri-wayland-private-portal",
  portal: {
    private_session_bus: true,
    filechooser_open_file: true,
    request_count: 2,
    parent_window_empty: true,
    single_file: true,
    directory_mode: false,
    current_folder_preserved: true,
    text_and_all_filters: true,
    selected_utf8_file_uri: true,
    cancellation: true
  },
  hotwords: {
    selection_updates_draft: true,
    cancellation_preserves_configured_draft: true,
    config_unchanged_without_set_path: true,
    selected_path_retained: false
  },
  isolation: {
    private_bus: true,
    system_portal_untouched: true,
    temporary_xdg_roots: true,
    user_config_untouched: true
  },
  restoration: {
    focused_window_restored: true,
    clipboard_restored: true,
    gui_processes: 0,
    portal_processes: 0,
    private_bus_processes: 0
  }
}' >"${out_dir}/summary.json"

jq -e '
  .ok == true and
  .portal.private_session_bus == true and
  .portal.filechooser_open_file == true and
  .portal.request_count == 2 and
  .portal.current_folder_preserved == true and
  .portal.selected_utf8_file_uri == true and
  .portal.cancellation == true and
  .hotwords.selection_updates_draft == true and
  .hotwords.cancellation_preserves_configured_draft == true and
  .hotwords.config_unchanged_without_set_path == true and
  .hotwords.selected_path_retained == false and
  .isolation.system_portal_untouched == true and
  .isolation.user_config_untouched == true and
  .restoration.focused_window_restored == true and
  .restoration.clipboard_restored == true and
  .restoration.gui_processes == 0 and
  .restoration.portal_processes == 0 and
  .restoration.private_bus_processes == 0
' "${out_dir}/summary.json" >/dev/null

rm -rf "${runtime_root}"
runtime_root=""
jq . "${out_dir}/summary.json"
