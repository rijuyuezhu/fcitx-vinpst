#!/usr/bin/env bash
set -euo pipefail

gui_bin="${VINPST_GUI_RESOURCE_LIVE_BIN:-target/debug/vinpst-gui}"
out_dir="${VINPST_GUI_RESOURCE_LIVE_OUT_DIR:-target/tmp/gui-resource-install-live}"
fixture="${VINPST_GUI_RESOURCE_LIVE_FIXTURE:-scripts/live/niri/probes/gui-resource-install-fixture.py}"
daemon_fixture="${VINPST_GUI_RESOURCE_LIVE_DAEMON_FIXTURE:-scripts/live/niri/probes/gui-daemon-config-fixture.py}"
key_sender="${VINPST_GUI_RESOURCE_LIVE_KEY_SENDER:-scripts/live/niri/probes/send-uinput-key.py}"
notification_url="${VINPST_GUI_RESOURCE_LIVE_NOTIFICATION_URL:-http://127.0.0.1:9/notification.json}"
model_id="${VINPST_GUI_RESOURCE_LIVE_MODEL_ID:-model.live.fixture}"
model_short_id="${VINPST_GUI_RESOURCE_LIVE_MODEL_SHORT_ID:-live-model}"
model_title="${VINPST_GUI_RESOURCE_LIVE_MODEL_TITLE:-Vinpst live fixture model}"

runtime_root=""
server_pid=""
private_bus_pid=""
daemon_pid=""
gui_pid=""
gui_window_id=""
previous_niri_window_id=""
clipboard_before=""
clipboard_had_text=0
clipboard_mutated=0
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
  if [[ -n "${server_pid}" ]]; then
    kill -TERM "${server_pid}" 2>/dev/null || true
    wait "${server_pid}" 2>/dev/null || true
    server_pid=""
  fi
  if [[ -n "${previous_niri_window_id}" ]] &&
    niri msg --json windows 2>/dev/null |
      jq -e --argjson id "${previous_niri_window_id}" '.[] | select(.id == $id)' >/dev/null; then
    niri msg action focus-window --id "${previous_niri_window_id}" >/dev/null 2>&1 || true
    sleep 0.2
  fi
  if [[ "${clipboard_mutated}" == 1 ]]; then
    if [[ "${clipboard_had_text}" == 1 && -n "${clipboard_before}" ]]; then
      wl-copy --type 'text/plain;charset=utf-8' <"${clipboard_before}" >/dev/null 2>&1 || true
    else
      wl-copy --clear >/dev/null 2>&1 || true
    fi
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

for command in dbus-daemon jq niri python3 sha256sum stat tar timeout wl-copy wl-paste; do
  command -v "${command}" >/dev/null 2>&1 ||
    fail "required resource-install live command is missing: ${command}"
done
[[ -x "${gui_bin}" ]] || fail "GUI binary is missing or not executable: ${gui_bin}"
[[ -x "${fixture}" ]] || fail "resource fixture is missing or not executable: ${fixture}"
[[ -x "${daemon_fixture}" ]] || fail "daemon fixture is missing or not executable: ${daemon_fixture}"
[[ -x "${key_sender}" ]] || fail "uinput key sender is missing or not executable: ${key_sender}"
[[ -w /dev/uinput ]] || fail "/dev/uinput is not writable"
for value in "${model_id}" "${model_short_id}" "${model_title}"; do
  [[ -n "${value}" && "${value}" != *$'\n'* ]] ||
    fail "model fixture values must be non-empty and single-line"
done

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
fixture_root="${runtime_root}/fixture"
archive_source="${runtime_root}/archive-source/${model_short_id}"
archive_path="${fixture_root}/assets/${model_short_id}.tar"
registry_path="${fixture_root}/registry/models.json"
port_file="${runtime_root}/fixture.port"
request_log="${out_dir}/requests.jsonl"
config_home="${runtime_root}/config"
cache_home="${runtime_root}/cache"
data_home="${runtime_root}/data"
home_dir="${runtime_root}/home"
config_path="${config_home}/fcitx-vinpst/config.json"
model_root="${data_home}/fcitx-vinpst/models"
model_dir="${model_root}/${model_short_id}"
metadata_path="${model_dir}/vinpst-model.json"
method_log="${out_dir}/daemon-methods.jsonl"
daemon_ready="${runtime_root}/daemon.ready"

mkdir -p \
  "${archive_source}" \
  "$(dirname "${archive_path}")" \
  "$(dirname "${registry_path}")" \
  "$(dirname "${config_path}")" \
  "${cache_home}" "${data_home}" "${home_dir}"
printf '%s\n' 'fixture payload' >"${archive_source}/tokens.txt"
printf '%s\n' 'fixture model bytes' >"${archive_source}/model.onnx"
tar --format=ustar --owner=0 --group=0 --numeric-owner \
  --mtime='UTC 2026-01-01' -cf "${archive_path}" \
  -C "$(dirname "${archive_source}")" "$(basename "${archive_source}")"
archive_sha256="$(sha256sum "${archive_path}" | awk '{print $1}')"
archive_size="$(stat -c '%s' "${archive_path}")"

"${fixture}" \
  --root "${fixture_root}" \
  --port-file "${port_file}" \
  --request-log "${request_log}" \
  >"${out_dir}/fixture.stdout.log" 2>"${out_dir}/fixture.stderr.log" &
server_pid=$!
for _ in $(seq 1 100); do
  [[ -s "${port_file}" ]] && break
  kill -0 "${server_pid}" 2>/dev/null || fail "resource fixture exited before publishing its port"
  sleep 0.05
done
[[ -s "${port_file}" ]] || fail "resource fixture did not publish its port"
fixture_port="$(tr -d '\n' <"${port_file}")"
[[ "${fixture_port}" =~ ^[0-9]+$ ]] || fail "resource fixture returned an invalid port"
base_url="http://127.0.0.1:${fixture_port}"
asset_url="${base_url}/assets/${model_short_id}.tar"

jq -n \
  --arg id "${model_id}" \
  --arg short_id "${model_short_id}" \
  --arg title "${model_title}" \
  --arg asset_url "${asset_url}" \
  --arg sha256 "${archive_sha256}" \
  --argjson size_bytes "${archive_size}" \
  '{
    version: 2,
    items: [
      {
        id: $id,
        short_id: $short_id,
        title: $title,
        urls: [$asset_url],
        sha256: $sha256,
        size_bytes: $size_bytes,
        language: "en",
        vinpst_model: {
          backend: "sherpa-offline",
          language: "en",
          size_bytes: $size_bytes,
          supports_hotwords: false,
          runtime: "offline",
          family: "sense_voice",
          model: {
            tokens: "tokens.txt",
            sense_voice: {
              model: "model.onnx",
              language: "en",
              use_itn: false
            }
          }
        }
      }
    ]
  }' >"${registry_path}"

jq --arg base_url "${base_url}" '.registry.base_urls = [$base_url]' \
  data/default-config.json >"${config_path}"
chmod 600 "${config_path}"
config_before_sha256="$(sha256sum "${config_path}" | awk '{print $1}')"

mapfile -t bus_info < <(dbus-daemon --session --fork --print-address=1 --print-pid=1)
[[ "${#bus_info[@]}" == 2 ]] || fail "private session bus did not return address and PID"
private_bus_address="${bus_info[0]}"
private_bus_pid="${bus_info[1]}"
[[ -n "${private_bus_address}" && "${private_bus_pid}" =~ ^[0-9]+$ ]] ||
  fail "private session bus metadata is invalid"

DBUS_SESSION_BUS_ADDRESS="${private_bus_address}" \
  "${daemon_fixture}" \
    --method-log "${method_log}" \
    --ready-file "${daemon_ready}" \
    >"${out_dir}/daemon.stdout.log" 2>"${out_dir}/daemon.stderr.log" &
daemon_pid=$!
for _ in $(seq 1 100); do
  [[ -s "${daemon_ready}" ]] && break
  kill -0 "${daemon_pid}" 2>/dev/null || fail "daemon fixture exited before becoming ready"
  sleep 0.05
done
[[ -s "${daemon_ready}" ]] || fail "daemon fixture did not become ready"

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

wait_for_title() {
  local expected=$1
  local actual=""
  for _ in $(seq 1 50); do
    actual="$(window_title 2>/dev/null || true)"
    [[ "${actual}" == "${expected}" ]] && return
    sleep 0.1
  done
  fail "GUI did not reach expected title '${expected}'; observed '${actual}'"
}

start_gui() {
  DBUS_SESSION_BUS_ADDRESS="${private_bus_address}" \
    LANG=en_US.UTF-8 \
    HOME="${home_dir}" \
    XDG_CONFIG_HOME="${config_home}" \
    XDG_CACHE_HOME="${cache_home}" \
    XDG_DATA_HOME="${data_home}" \
    VINPST_NOTIFICATION_URL="${notification_url}" \
    "${gui_bin}" >"${out_dir}/gui.stdout.log" 2>"${out_dir}/gui.stderr.log" &
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
    kill -0 "${gui_pid}" 2>/dev/null || fail "GUI exited before creating a window"
    sleep 0.1
  done
  [[ -n "${gui_window_id}" ]] || fail "GUI window did not appear"
  wait_for_title 'Vinpst Configuration — Control'
  if grep -Eq 'Cannot start a runtime from within a runtime|panicked at' \
    "${out_dir}/gui.stderr.log"; then
    fail "GUI emitted a runtime panic"
  fi
}

replace_focused_text() {
  local value=$1
  local sentinel="resource-copy-sentinel-${RANDOM}-${RANDOM}"
  clipboard_mutated=1
  printf '%s' "${value}" | wl-copy
  send_key CTRL+A
  send_key CTRL+V
  printf '%s' "${sentinel}" | wl-copy
  send_key CTRL+A
  send_key CTRL+C
  [[ "$(timeout 2s wl-paste --no-newline 2>/dev/null || true)" == "${value}" ]]
}

focus_first_resources_text_field() {
  local marker=""
  focus_gui
  send_key ESCAPE
  for step in $(seq 1 30); do
    send_key TAB
    marker="resource-filter-probe-${step}"
    if replace_focused_text "${marker}"; then
      send_key CTRL+A
      send_key BACKSPACE
      return
    fi
  done
  fail "Resources focus traversal did not reach the filter text field"
}

wait_for_path() {
  local path=$1
  local label=$2
  for _ in $(seq 1 300); do
    [[ -e "${path}" ]] && return
    kill -0 "${gui_pid}" 2>/dev/null || fail "GUI exited while waiting for ${label}"
    sleep 0.1
  done
  fail "${label} did not appear at ${path}"
}

wait_for_absence() {
  local path=$1
  local label=$2
  for _ in $(seq 1 200); do
    [[ ! -e "${path}" ]] && return
    kill -0 "${gui_pid}" 2>/dev/null || fail "GUI exited while waiting for ${label} removal"
    sleep 0.1
  done
  fail "${label} remained at ${path}"
}

start_gui
focus_gui
send_key CTRL+2
wait_for_title 'Vinpst Configuration — Resources'
focus_first_resources_text_field
send_key TAB
replace_focused_text "${model_short_id}" || fail "model selector did not accept the fixture short id"
send_key TAB
send_key ENTER

wait_for_path "${metadata_path}" "installed model metadata"
[[ -f "${model_dir}/tokens.txt" && -f "${model_dir}/model.onnx" ]] ||
  fail "installed model payload is incomplete"
[[ ! -L "${model_dir}" && ! -L "${metadata_path}" ]] ||
  fail "installed model path unexpectedly contains a symlink"
metadata_registry_id="$(jq -r '.display.registry_id // empty' "${metadata_path}")"
[[ "${metadata_registry_id}" == "${model_id}" ]] ||
  fail "installed metadata registry id mismatch: ${metadata_registry_id}"
[[ "$(jq -r '.family // empty' "${metadata_path}")" == sense_voice ]] ||
  fail "installed metadata family mismatch"
[[ "$(jq -r '.backend // empty' "${metadata_path}")" == sherpa-offline ]] ||
  fail "installed metadata backend mismatch"

for _ in $(seq 1 100); do
  registry_requests="$(jq -r 'select(.path == "/registry/models.json") | .path' "${request_log}" 2>/dev/null | wc -l)"
  asset_requests="$(jq -r --arg path "/assets/${model_short_id}.tar" 'select(.path == $path) | .path' "${request_log}" 2>/dev/null | wc -l)"
  ((registry_requests >= 1 && asset_requests >= 1)) && break
  sleep 0.05
done
((registry_requests >= 1)) || fail "GUI did not request the model registry"
((asset_requests >= 1)) || fail "GUI did not request the model archive"

# Reset focus and traverse the rendered Resources page. The successful install
# keeps the model selector non-empty, so enabled focus order is four page
# buttons, Open Config, filter, model selector, model install, provider
# selector, then the newly rendered model Details and Remove actions.
focus_gui
send_key ESCAPE
for _ in $(seq 1 11); do send_key TAB; done
send_key ENTER
wait_for_absence "${model_dir}" "installed model directory"

[[ "$(sha256sum "${config_path}" | awk '{print $1}')" == "${config_before_sha256}" ]] ||
  fail "model install/remove unexpectedly changed application configuration"
[[ ! -e "${model_root}/${model_short_id}" ]] || fail "managed model directory still exists"
kill -0 "${gui_pid}" 2>/dev/null || fail "GUI exited after model removal"
if grep -Eq 'Cannot start a runtime from within a runtime|panicked at' \
  "${out_dir}/gui.stderr.log"; then
  fail "GUI emitted a runtime panic during resource mutation"
fi

python3 - "${method_log}" <<'PY'
import json
import pathlib
import sys

records = [
    json.loads(line)
    for line in pathlib.Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()
]
methods = [record["method"] for record in records]
for required in ["GetStatus", "GetRuntimeStatus", "GetTextAdapterState"]:
    assert required in methods
assert "ReloadAsrBackend" not in methods
assert [record["sequence"] for record in records] == list(range(1, len(records) + 1))
print("private daemon resource contract verified")
PY

kill -TERM "${gui_pid}" 2>/dev/null || true
wait "${gui_pid}" 2>/dev/null || true
gui_pid=""
for tracked_pid in "${tracked_gui_pids[@]}"; do
  kill -0 "${tracked_pid}" 2>/dev/null && fail "tracked GUI process still exists: ${tracked_pid}"
done

kill -TERM "${daemon_pid}" 2>/dev/null || true
wait "${daemon_pid}" 2>/dev/null || true
daemon_pid=""
kill -TERM "${private_bus_pid}" 2>/dev/null || true
wait "${private_bus_pid}" 2>/dev/null || true
private_bus_pid=""
kill -TERM "${server_pid}" 2>/dev/null || true
wait "${server_pid}" 2>/dev/null || true
server_pid=""

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
    fail "standard clipboard unexpectedly retained the resource probe value"
  fi
fi
clipboard_mutated=0

jq -n \
  --arg model_id "${model_id}" \
  --arg model_short_id "${model_short_id}" \
  --arg archive_sha256 "${archive_sha256}" \
  --argjson archive_size "${archive_size}" \
  --argjson registry_requests "${registry_requests}" \
  --argjson asset_requests "${asset_requests}" \
  '{
    ok: true,
    backend: "niri-wayland-private-registry-private-daemon",
    model: {
      id: $model_id,
      short_id: $model_short_id,
      archive_sha256: $archive_sha256,
      archive_size: $archive_size,
      checksum_verified: true,
      metadata_scanned: true,
      result_row_rendered: true,
      managed_remove_activated: true,
      removed: true,
      values_retained: false
    },
    network: {
      loopback_only: true,
      registry_requests: $registry_requests,
      asset_requests: $asset_requests
    },
    daemon: {
      private_session_bus: true,
      idle_guard: true,
      config_reload_calls: 0
    },
    isolation: {
      temporary_xdg_roots: true,
      real_daemon_untouched: true,
      user_config_untouched: true,
      user_models_untouched: true
    },
    restoration: {
      focused_window_restored: true,
      clipboard_restored: true,
      gui_processes: 0,
      daemon_processes: 0,
      fixture_processes: 0,
      private_bus_processes: 0
    }
  }' >"${out_dir}/summary.json"

jq -e '
  .ok == true and
  .model.checksum_verified == true and
  .model.metadata_scanned == true and
  .model.result_row_rendered == true and
  .model.managed_remove_activated == true and
  .model.removed == true and
  .model.values_retained == false and
  .network.loopback_only == true and
  .network.registry_requests >= 1 and
  .network.asset_requests >= 1 and
  .daemon.private_session_bus == true and
  .daemon.config_reload_calls == 0 and
  .isolation.real_daemon_untouched == true and
  .isolation.user_config_untouched == true and
  .isolation.user_models_untouched == true and
  .restoration.focused_window_restored == true and
  .restoration.clipboard_restored == true and
  .restoration.gui_processes == 0 and
  .restoration.daemon_processes == 0 and
  .restoration.fixture_processes == 0 and
  .restoration.private_bus_processes == 0
' "${out_dir}/summary.json" >/dev/null

rm -rf "${runtime_root}"
runtime_root=""
jq . "${out_dir}/summary.json"
