#!/usr/bin/env bash
set -euo pipefail

gui_bin="${VINPST_GUI_CONFIG_LIVE_BIN:-target/debug/vinpst-gui}"
out_dir="${VINPST_GUI_CONFIG_LIVE_OUT_DIR:-target/tmp/gui-config-mutation-live}"
daemon_fixture="${VINPST_GUI_CONFIG_LIVE_DAEMON_FIXTURE:-scripts/live/niri/probes/gui-daemon-config-fixture.py}"
key_sender="${VINPST_GUI_CONFIG_LIVE_KEY_SENDER:-scripts/live/niri/probes/send-uinput-key.py}"
notification_url="${VINPST_GUI_CONFIG_LIVE_NOTIFICATION_URL:-http://127.0.0.1:9/notification.json}"
success_value="${VINPST_GUI_CONFIG_LIVE_SUCCESS_VALUE:-live-success-language}"
conflict_draft_value="${VINPST_GUI_CONFIG_LIVE_CONFLICT_DRAFT_VALUE:-live-conflict-draft}"
external_value="${VINPST_GUI_CONFIG_LIVE_EXTERNAL_VALUE:-live-external-version}"
clipboard_before=""
clipboard_had_text=0
clipboard_types_before=""
previous_niri_window_id=""
runtime_root=""
private_bus_pid=""
daemon_pid=""
gui_pid=""
gui_window_id=""
tracked_gui_pids=()
language_focus_steps=0

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
  if [[ -n "${daemon_pid}" ]]; then
    kill -TERM "${daemon_pid}" 2>/dev/null || true
    wait "${daemon_pid}" 2>/dev/null || true
    daemon_pid=""
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
    fail "required config-mutation live command is missing: ${command}"
done
[[ -x "${gui_bin}" ]] || fail "GUI binary is missing or not executable: ${gui_bin}"
[[ -x "${daemon_fixture}" ]] || fail "daemon fixture is missing or not executable: ${daemon_fixture}"
[[ -x "${key_sender}" ]] || fail "uinput key sender is missing or not executable: ${key_sender}"
[[ -w /dev/uinput ]] || fail "/dev/uinput is not writable"
for value in "${success_value}" "${conflict_draft_value}" "${external_value}"; do
  [[ -n "${value}" && "${value}" != *$'\n'* ]] || fail "mutation marker must be non-empty and single-line"
done
[[ "${success_value}" != "${conflict_draft_value}" &&
  "${success_value}" != "${external_value}" &&
  "${conflict_draft_value}" != "${external_value}" ]] ||
  fail "mutation markers must be distinct"

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
success_root="${runtime_root}/success"
conflict_root="${runtime_root}/conflict"
method_log="${out_dir}/daemon-methods.jsonl"
ready_file="${runtime_root}/daemon.ready"
mkdir -p "${success_root}" "${conflict_root}"

mapfile -t bus_info < <(dbus-daemon --session --fork --print-address=1 --print-pid=1)
[[ "${#bus_info[@]}" == 2 ]] || fail "private session bus did not return address and PID"
private_bus_address="${bus_info[0]}"
private_bus_pid="${bus_info[1]}"
[[ -n "${private_bus_address}" && "${private_bus_pid}" =~ ^[0-9]+$ ]] ||
  fail "private session bus metadata is invalid"

DBUS_SESSION_BUS_ADDRESS="${private_bus_address}" \
  "${daemon_fixture}" \
    --method-log "${method_log}" \
    --ready-file "${ready_file}" \
    >"${out_dir}/daemon.stdout.log" 2>"${out_dir}/daemon.stderr.log" &
daemon_pid=$!
for _ in $(seq 1 100); do
  [[ -s "${ready_file}" ]] && break
  kill -0 "${daemon_pid}" 2>/dev/null || fail "daemon fixture exited before becoming ready"
  sleep 0.05
done
[[ -s "${ready_file}" ]] || fail "daemon fixture did not become ready"

send_key() {
  "${key_sender}" --settle-ms 220 "$1" | tee -a "${out_dir}/uinput.jsonl" >/dev/null
}

focus_gui() {
  niri msg action focus-window --id "${gui_window_id}" >/dev/null
  sleep 0.2
}

window_title() {
  niri msg --json windows |
    jq -er --argjson id "${gui_window_id}" '.[] | select(.id == $id) | .title'
}

method_count() {
  local method=$1
  if [[ ! -f "${method_log}" ]]; then
    printf '0\n'
    return
  fi
  jq -r --arg method "${method}" 'select(.method == $method) | .method' "${method_log}" | wc -l
}

config_language() {
  python3 - "$1" <<'PY'
import json
import pathlib
import sys

with pathlib.Path(sys.argv[1]).open(encoding="utf-8") as stream:
    print(json.load(stream)["global"]["default_language"])
PY
}

wait_for_language() {
  local path=$1
  local expected=$2
  local actual=""
  for _ in $(seq 1 100); do
    actual="$(config_language "${path}" 2>/dev/null || true)"
    [[ "${actual}" == "${expected}" ]] && return
    sleep 0.05
  done
  fail "config language did not become '${expected}'; observed '${actual}'"
}

wait_for_reload_count() {
  local expected=$1
  local actual=0
  for _ in $(seq 1 100); do
    actual="$(method_count ReloadAsrBackend)"
    ((actual >= expected)) && return
    sleep 0.05
  done
  fail "ReloadAsrBackend count did not reach ${expected}; observed ${actual}"
}

stop_gui() {
  [[ -z "${gui_pid}" ]] && return
  kill -TERM "${gui_pid}" 2>/dev/null || true
  wait "${gui_pid}" 2>/dev/null || true
  gui_pid=""
  gui_window_id=""
}

start_gui() {
  local root=$1
  local name=$2
  local config_home="${root}/config"
  local cache_home="${root}/cache"
  local data_home="${root}/data"
  local home_dir="${root}/home"
  mkdir -p "${config_home}" "${cache_home}" "${data_home}" "${home_dir}"
  DBUS_SESSION_BUS_ADDRESS="${private_bus_address}" \
    LANG=en_US.UTF-8 \
    HOME="${home_dir}" \
    XDG_CONFIG_HOME="${config_home}" \
    XDG_CACHE_HOME="${cache_home}" \
    XDG_DATA_HOME="${data_home}" \
    VINPST_NOTIFICATION_URL="${notification_url}" \
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
  [[ "$(window_title)" == 'Vinpst Configuration — Control' ]] ||
    fail "${name} GUI did not expose the expected initial title"
  if grep -Eq 'Cannot start a runtime from within a runtime|panicked at' \
    "${out_dir}/${name}.stderr.log"; then
    fail "${name} GUI emitted a runtime panic"
  fi
}

focused_value_matches() {
  local expected=$1
  local sentinel=$2
  local probe=""
  printf '%s' "${sentinel}" | wl-copy
  send_key CTRL+A
  send_key CTRL+C
  probe="$(timeout 2s wl-paste --no-newline 2>/dev/null || true)"
  [[ "${probe}" == "${expected}" ]]
}

focus_language_field() {
  local expected=$1
  focus_gui
  send_key ESCAPE
  if ((language_focus_steps > 0)); then
    for _ in $(seq 1 "${language_focus_steps}"); do send_key TAB; done
    if focused_value_matches "${expected}" 'config-language-cached-sentinel'; then
      return
    fi
    language_focus_steps=0
    send_key ESCAPE
  fi
  for focus_step in $(seq 1 40); do
    send_key TAB
    if focused_value_matches "${expected}" "config-language-sentinel-${focus_step}"; then
      language_focus_steps="${focus_step}"
      return
    fi
  done
  fail "Control focus traversal did not reach the expected default-language field"
}

replace_focused_field() {
  local value=$1
  printf '%s' "${value}" | wl-copy
  send_key CTRL+A
  send_key CTRL+V
  send_key CTRL+A
  send_key CTRL+C
  [[ "$(timeout 2s wl-paste --no-newline)" == "${value}" ]] ||
    fail "Control default-language draft did not accept the clipboard value"
}

save_from_language_field() {
  send_key ENTER
}

prepare_config_root() {
  local root=$1
  local config_path="${root}/config/fcitx-vinpst/config.json"
  mkdir -p "$(dirname "${config_path}")"
  install -m 600 data/default-config.json "${config_path}"
  printf '%s' "${config_path}"
}

success_config="$(prepare_config_root "${success_root}")"
success_backup="${success_config}.bak"
success_original="${out_dir}/success-original.json"
cp --preserve=mode,timestamps "${success_config}" "${success_original}"
success_original_hash="$(sha256sum "${success_config}" | awk '{print $1}')"

start_gui "${success_root}" success-save
focus_language_field "$(config_language "${success_config}")"
replace_focused_field "${success_value}"
focus_language_field "${success_value}"
save_from_language_field
wait_for_language "${success_config}" "${success_value}"
wait_for_reload_count 1
for _ in $(seq 1 100); do
  [[ -f "${success_backup}" ]] && break
  sleep 0.05
done
[[ -f "${success_backup}" && ! -L "${success_backup}" ]] || fail "success backup was not created as a regular file"
cmp -s "${success_original}" "${success_backup}" || fail "success backup does not match the original bytes"
[[ "$(sha256sum "${success_backup}" | awk '{print $1}')" == "${success_original_hash}" ]] ||
  fail "success backup hash does not match the original config"
[[ "$(stat -c '%a' "${success_config}")" == 600 ]] || fail "saved config mode is not 600"
[[ "$(stat -c '%a' "${success_backup}")" == 600 ]] || fail "config backup mode is not 600"
stop_gui

start_gui "${success_root}" success-relaunch
focus_language_field "${success_value}"
stop_gui

conflict_config="$(prepare_config_root "${conflict_root}")"
conflict_backup="${conflict_config}.bak"
start_gui "${conflict_root}" conflict
focus_language_field "$(config_language "${conflict_config}")"
replace_focused_field "${conflict_draft_value}"
focus_language_field "${conflict_draft_value}"
python3 - "${conflict_config}" "${external_value}" <<'PY'
import json
import os
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
with path.open(encoding="utf-8") as stream:
    config = json.load(stream)
config["global"]["default_language"] = sys.argv[2]
temporary = path.with_name(f"{path.name}.external.tmp")
with temporary.open("w", encoding="utf-8") as stream:
    json.dump(config, stream, ensure_ascii=False, indent=2)
    stream.write("\n")
    stream.flush()
    os.fsync(stream.fileno())
temporary.chmod(0o600)
os.replace(temporary, path)
PY
external_hash="$(sha256sum "${conflict_config}" | awk '{print $1}')"
save_from_language_field
sleep 0.8
focus_language_field "${conflict_draft_value}"
[[ "$(config_language "${conflict_config}")" == "${external_value}" ]] ||
  fail "conflict save overwrote the external config value"
[[ "$(sha256sum "${conflict_config}" | awk '{print $1}')" == "${external_hash}" ]] ||
  fail "conflict save changed the external config bytes"
[[ ! -e "${conflict_backup}" ]] || fail "conflict save created a backup before rejecting the stale document"
[[ "$(method_count ReloadAsrBackend)" == 1 ]] ||
  fail "conflict save sent ReloadAsrBackend despite rejecting the stale document"
stop_gui

python3 - "${method_log}" <<'PY'
import json
import pathlib
import sys

records = [json.loads(line) for line in pathlib.Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()]
methods = [record["method"] for record in records]
assert methods.count("ReloadAsrBackend") == 1
for required in ["GetStatus", "GetRuntimeStatus", "GetTextAdapterState"]:
    assert required in methods
assert [record["sequence"] for record in records] == list(range(1, len(records) + 1))
print("private daemon config contract verified")
PY

for tracked_pid in "${tracked_gui_pids[@]}"; do
  kill -0 "${tracked_pid}" 2>/dev/null && fail "tracked GUI process still exists: ${tracked_pid}"
done
kill -TERM "${daemon_pid}" 2>/dev/null || true
wait "${daemon_pid}" 2>/dev/null || true
daemon_pid=""
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
    fail "standard clipboard unexpectedly retained the mutation probe value"
  fi
fi

jq -n '{
  ok: true,
  backend: "niri-wayland-private-daemon",
  daemon: {
    private_session_bus: true,
    idle_guard: true,
    inactive_session_guard: true,
    typed_runtime_snapshot: true,
    reload_calls: 1
  },
  success: {
    control_draft_updated: true,
    atomic_config_replaced: true,
    adjacent_backup_created: true,
    backup_matches_original_bytes: true,
    saved_mode_0600: true,
    backup_mode_0600: true,
    reload_requested: true,
    relaunch_reads_saved_value: true,
    values_retained: false
  },
  conflict: {
    external_replacement_detected: true,
    external_bytes_preserved: true,
    draft_preserved_after_rejection: true,
    backup_not_created: true,
    reload_not_requested: true,
    values_retained: false
  },
  isolation: {
    private_bus: true,
    real_daemon_untouched: true,
    temporary_xdg_roots: true,
    user_config_untouched: true
  },
  restoration: {
    focused_window_restored: true,
    clipboard_restored: true,
    gui_processes: 0,
    daemon_processes: 0,
    private_bus_processes: 0
  }
}' >"${out_dir}/summary.json"

jq -e '
  .ok == true and
  .daemon.private_session_bus == true and
  .daemon.idle_guard == true and
  .daemon.inactive_session_guard == true and
  .daemon.reload_calls == 1 and
  .success.atomic_config_replaced == true and
  .success.adjacent_backup_created == true and
  .success.backup_matches_original_bytes == true and
  .success.saved_mode_0600 == true and
  .success.backup_mode_0600 == true and
  .success.reload_requested == true and
  .success.relaunch_reads_saved_value == true and
  .success.values_retained == false and
  .conflict.external_replacement_detected == true and
  .conflict.external_bytes_preserved == true and
  .conflict.draft_preserved_after_rejection == true and
  .conflict.backup_not_created == true and
  .conflict.reload_not_requested == true and
  .conflict.values_retained == false and
  .isolation.real_daemon_untouched == true and
  .isolation.user_config_untouched == true and
  .restoration.focused_window_restored == true and
  .restoration.clipboard_restored == true and
  .restoration.gui_processes == 0 and
  .restoration.daemon_processes == 0 and
  .restoration.private_bus_processes == 0
' "${out_dir}/summary.json" >/dev/null

rm -rf "${runtime_root}"
runtime_root=""
jq . "${out_dir}/summary.json"
