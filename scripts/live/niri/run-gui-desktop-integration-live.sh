#!/usr/bin/env bash
set -euo pipefail

gui_bin="${VINPUT_GUI_DESKTOP_LIVE_BIN:-target/debug/vinput-gui}"
out_dir="${VINPUT_GUI_DESKTOP_LIVE_OUT_DIR:-target/tmp/gui-desktop-integration-live}"
fixture="${VINPUT_GUI_DESKTOP_LIVE_FIXTURE:-scripts/live/niri/probes/gui-desktop-integration-fixture.py}"
key_sender="${VINPUT_GUI_DESKTOP_LIVE_KEY_SENDER:-scripts/live/niri/probes/send-uinput-key.py}"
notification_id="${VINPUT_GUI_DESKTOP_LIVE_NOTIFICATION_ID:-424242}"
details_url="${VINPUT_GUI_DESKTOP_LIVE_DETAILS_URL:-https://example.invalid/vinput-details?source=live-fixture}"
runtime_root=""
server_pid=""
gui_pid=""
gui_window_id=""
tracked_gui_pids=()

fail() {
  echo "$*" >&2
  exit 1
}

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM
  set +e
  if [[ -n "${gui_pid}" ]]; then
    kill -TERM "${gui_pid}" 2>/dev/null || true
    wait "${gui_pid}" 2>/dev/null || true
    gui_pid=""
  fi
  if [[ -n "${server_pid}" ]]; then
    kill -TERM "${server_pid}" 2>/dev/null || true
    wait "${server_pid}" 2>/dev/null || true
    server_pid=""
  fi
  [[ -z "${runtime_root}" ]] || rm -rf "${runtime_root}"
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

for command in jq niri python3 timeout; do
  command -v "${command}" >/dev/null 2>&1 ||
    fail "required desktop-integration live command is missing: ${command}"
done
[[ -x "${gui_bin}" ]] || fail "GUI binary is missing or not executable: ${gui_bin}"
[[ -x "${fixture}" ]] || fail "desktop integration fixture is missing or not executable: ${fixture}"
[[ -x "${key_sender}" ]] || fail "uinput key sender is missing or not executable: ${key_sender}"
[[ -w /dev/uinput ]] || fail "/dev/uinput is not writable"
[[ "${notification_id}" =~ ^[1-9][0-9]*$ ]] || fail "notification id must be a positive integer"
[[ "${details_url}" == https://* ]] || fail "details URL must use HTTPS"

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

if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
  WAYLAND_DISPLAY="$(sed -n 's/^WAYLAND_DISPLAY=//p' <<<"${session_environment}")"
  export WAYLAND_DISPLAY
fi
[[ -n "${WAYLAND_DISPLAY:-}" ]] || fail "a Wayland display is required"

if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  DBUS_SESSION_BUS_ADDRESS="$(sed -n 's/^DBUS_SESSION_BUS_ADDRESS=//p' <<<"${session_environment}")"
  export DBUS_SESSION_BUS_ADDRESS
fi
[[ -n "${DBUS_SESSION_BUS_ADDRESS:-}" ]] || fail "a session bus address is required"

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
runtime_root="$(mktemp -d)"
config_home="${runtime_root}/config"
cache_home="${runtime_root}/cache"
data_home="${runtime_root}/data"
home_dir="${runtime_root}/home"
config_path="${config_home}/fcitx-vinput/config.json"
read_state_path="${cache_home}/fcitx-vinput/read_notifications"
open_log="${out_dir}/opener.jsonl"
request_log="${out_dir}/requests.jsonl"
port_file="${runtime_root}/fixture.port"
mkdir -p "$(dirname "${config_path}")" "${cache_home}" "${data_home}" "${home_dir}"
install -m 600 data/default-config.json "${config_path}"

"${fixture}" serve \
  --port-file "${port_file}" \
  --request-log "${request_log}" \
  --notification-id "${notification_id}" \
  --details-url "${details_url}" \
  >"${out_dir}/fixture.stdout.log" 2>"${out_dir}/fixture.stderr.log" &
server_pid=$!
for _ in $(seq 1 100); do
  [[ -s "${port_file}" ]] && break
  kill -0 "${server_pid}" 2>/dev/null || fail "notification fixture exited before publishing its port"
  sleep 0.05
done
[[ -s "${port_file}" ]] || fail "notification fixture did not publish its port"
fixture_port="$(tr -d '\n' <"${port_file}")"
[[ "${fixture_port}" =~ ^[0-9]+$ ]] || fail "notification fixture returned an invalid port"
feed_url="http://127.0.0.1:${fixture_port}/notification.json"

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

json_line_count() {
  local path=$1
  if [[ -f "${path}" ]]; then
    wc -l <"${path}"
  else
    printf '0\n'
  fi
}

wait_for_count() {
  local path=$1
  local expected=$2
  local label=$3
  local actual=0
  for _ in $(seq 1 100); do
    actual="$(json_line_count "${path}")"
    ((actual >= expected)) && return
    sleep 0.05
  done
  fail "${label} did not reach ${expected} records; observed ${actual}"
}

opener_record_target() {
  local index=$1
  jq -er --argjson index "${index}" '.[ $index - 1 ].target' < <(jq -s . "${open_log}")
}

opener_record_argument_count() {
  local index=$1
  jq -er --argjson index "${index}" '.[ $index - 1 ].argument_count' < <(jq -s . "${open_log}")
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
  local expected_request_count=$2
  LANG=en_US.UTF-8 \
    HOME="${home_dir}" \
    XDG_CONFIG_HOME="${config_home}" \
    XDG_CACHE_HOME="${cache_home}" \
    XDG_DATA_HOME="${data_home}" \
    VINPUT_NOTIFICATION_URL="${feed_url}" \
    VINPUT_DESKTOP_OPENER="${fixture}" \
    VINPUT_GUI_DESKTOP_LIVE_OPEN_LOG="${open_log}" \
    VINPUT_FLATPAK_INFO_PATH="${runtime_root}/missing-flatpak-info" \
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
  wait_for_count "${request_log}" "${expected_request_count}" "notification request log"
  sleep 1
  if grep -Eq 'Cannot start a runtime from within a runtime|panicked at' \
    "${out_dir}/${name}.stderr.log"; then
    fail "${name} GUI emitted a runtime panic"
  fi
}

activate_open_config() {
  local expected_record_count=$1
  focus_gui
  send_key ESCAPE
  for _ in $(seq 1 5); do
    send_key TAB
  done
  send_key ENTER
  wait_for_count "${open_log}" "${expected_record_count}" "desktop opener log"
  sleep 0.5
}

: >"${out_dir}/uinput.jsonl"
start_gui first 1
activate_open_config 1
[[ "$(opener_record_argument_count 1)" == 1 ]] || fail "Open Config did not use one direct argv target"
[[ "$(opener_record_target 1)" == "${config_path}" ]] || fail "Open Config target did not match the loaded config"

focus_gui
send_key ESCAPE
for _ in $(seq 1 6); do
  send_key TAB
done
send_key ENTER
wait_for_count "${open_log}" 2 "notification Details opener log"
[[ "$(opener_record_argument_count 2)" == 1 ]] || fail "notification Details did not use one direct argv target"
[[ "$(opener_record_target 2)" == "${details_url}" ]] || fail "notification Details target did not match the validated URL"
for _ in $(seq 1 100); do
  [[ -f "${read_state_path}" ]] && [[ "$(tr -d '\n' <"${read_state_path}")" == "${notification_id}" ]] && break
  sleep 0.05
done
[[ -f "${read_state_path}" ]] || fail "notification read-state file was not created"
[[ ! -L "${read_state_path}" ]] || fail "notification read-state path is a symbolic link"
[[ "$(tr -d '\n' <"${read_state_path}")" == "${notification_id}" ]] ||
  fail "notification read-state id does not match the opened notification"
read_state_mode="$(stat -c '%a' "${read_state_path}")"
[[ "${read_state_mode}" == 600 ]] || fail "notification read-state mode is ${read_state_mode}, expected 600"
stop_gui

start_gui second 2
activate_open_config 3
[[ "$(opener_record_argument_count 3)" == 1 ]] || fail "relaunch Open Config did not use one direct argv target"
[[ "$(opener_record_target 3)" == "${config_path}" ]] || fail "relaunch Open Config target changed"
focus_gui
send_key ESCAPE
for _ in $(seq 1 6); do
  send_key TAB
done
send_key ENTER
sleep 1
[[ "$(json_line_count "${open_log}")" == 3 ]] ||
  fail "the acknowledged startup notification reappeared after relaunch"
[[ "$(tr -d '\n' <"${read_state_path}")" == "${notification_id}" ]] ||
  fail "notification read-state changed after relaunch"
stop_gui

request_count="$(json_line_count "${request_log}")"
[[ "${request_count}" -ge 2 ]] || fail "notification fixture observed fewer than two requests"
if jq -e 'select(.path != "/notification.json")' "${request_log}" >/dev/null; then
  fail "notification fixture observed an unexpected request path"
fi
for tracked_pid in "${tracked_gui_pids[@]}"; do
  kill -0 "${tracked_pid}" 2>/dev/null && fail "tracked GUI process still exists: ${tracked_pid}"
done
kill -TERM "${server_pid}" 2>/dev/null || true
wait "${server_pid}" 2>/dev/null || true
server_pid=""

jq -n \
  --arg backend niri-wayland \
  --argjson notification_id "${notification_id}" \
  --argjson request_count "${request_count}" \
  '{
    ok: true,
    backend: $backend,
    open_config: {
      direct_argv: true,
      exact_loaded_path: true,
      relaunch_exact_path: true
    },
    startup_notification: {
      notification_id: $notification_id,
      feed_requests: $request_count,
      details_direct_argv: true,
      exact_validated_details_target: true,
      read_state_persisted: true,
      read_state_mode_0600: true,
      hidden_after_relaunch: true,
      remote_text_retained: false
    },
    isolation: {
      temporary_xdg_roots: true,
      user_config_untouched: true,
      real_browser_launched: false
    },
    restoration: {
      gui_processes: 0,
      fixture_processes: 0
    }
  }' >"${out_dir}/summary.json"

jq -e '
  .ok == true and
  .open_config.direct_argv == true and
  .open_config.exact_loaded_path == true and
  .open_config.relaunch_exact_path == true and
  .startup_notification.details_direct_argv == true and
  .startup_notification.exact_validated_details_target == true and
  .startup_notification.read_state_persisted == true and
  .startup_notification.read_state_mode_0600 == true and
  .startup_notification.hidden_after_relaunch == true and
  .startup_notification.remote_text_retained == false and
  .isolation.temporary_xdg_roots == true and
  .isolation.user_config_untouched == true and
  .isolation.real_browser_launched == false and
  .restoration.gui_processes == 0 and
  .restoration.fixture_processes == 0
' "${out_dir}/summary.json" >/dev/null

rm -rf "${runtime_root}"
runtime_root=""
jq . "${out_dir}/summary.json"
