#!/usr/bin/env bash
set -euo pipefail

mode="${1:-provider}"
[[ "$#" -le 1 ]] || { echo "usage: $0 [provider|adapter|adapter-update]" >&2; exit 2; }
case "${mode}" in
  provider)
    default_out_dir="target/tmp/gui-script-recovery-live"
    resource_kind="provider"
    resource_id="${VINPST_GUI_SCRIPT_RECOVERY_LIVE_PROVIDER_ID:-provider.live.batch}"
    resource_short_id="${VINPST_GUI_SCRIPT_RECOVERY_LIVE_PROVIDER_SHORT_ID:-live-provider}"
    registry_filename="providers.json"
    managed_directory="providers"
    initial_page="resources"
    page_title="Vinpst Configuration — Resources"
    required_environment_name=""
    required_environment_value=""
    optional_environment_name=""
    optional_environment_value=""
    unrelated_environment_name=""
    unrelated_environment_value=""
    update_existing=false
    ;;
  adapter)
    default_out_dir="target/tmp/gui-script-recovery-live-adapter"
    resource_kind="adapter"
    resource_id="${VINPST_GUI_SCRIPT_RECOVERY_LIVE_ADAPTER_ID:-adapter.live.batch}"
    resource_short_id="${VINPST_GUI_SCRIPT_RECOVERY_LIVE_ADAPTER_SHORT_ID:-live-adapter}"
    registry_filename="adapters.json"
    managed_directory="adapters"
    initial_page="llm"
    page_title="Vinpst Configuration — LLM"
    required_environment_name="VINPST_LIVE_REQUIRED"
    required_environment_value="fixture-environment-value"
    optional_environment_name=""
    optional_environment_value=""
    unrelated_environment_name=""
    unrelated_environment_value=""
    update_existing=false
    ;;
  adapter-update)
    default_out_dir="target/tmp/gui-script-update-live-adapter"
    resource_kind="adapter"
    resource_id="${VINPST_GUI_SCRIPT_UPDATE_LIVE_ADAPTER_ID:-adapter.live.update}"
    resource_short_id="${VINPST_GUI_SCRIPT_UPDATE_LIVE_ADAPTER_SHORT_ID:-live-adapter-update}"
    registry_filename="adapters.json"
    managed_directory="adapters"
    initial_page="llm"
    page_title="Vinpst Configuration — LLM"
    required_environment_name="VINPST_LIVE_REQUIRED"
    required_environment_value="fixture-required-preserved"
    optional_environment_name="VINPST_LIVE_OPTIONAL"
    optional_environment_value="fixture-optional-preserved"
    unrelated_environment_name="VINPST_LIVE_UNRELATED"
    unrelated_environment_value="fixture-unrelated-preserved"
    update_existing=true
    ;;
  *)
    echo "usage: $0 [provider|adapter|adapter-update]" >&2
    exit 2
    ;;
esac

gui_bin="${VINPST_GUI_SCRIPT_RECOVERY_LIVE_BIN:-target/debug/vinpst-gui}"
out_dir="${VINPST_GUI_SCRIPT_RECOVERY_LIVE_OUT_DIR:-${default_out_dir}}"
fixture="${VINPST_GUI_SCRIPT_RECOVERY_LIVE_FIXTURE:-scripts/live/niri/probes/gui-resource-install-fixture.py}"
daemon_fixture="${VINPST_GUI_SCRIPT_RECOVERY_LIVE_DAEMON_FIXTURE:-scripts/live/niri/probes/gui-daemon-config-fixture.py}"
key_sender="${VINPST_GUI_SCRIPT_RECOVERY_LIVE_KEY_SENDER:-scripts/live/niri/probes/send-uinput-key.py}"
notification_url="${VINPST_GUI_SCRIPT_RECOVERY_LIVE_NOTIFICATION_URL:-http://127.0.0.1:9/notification.json}"

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
config_dir=""
tracked_gui_pids=()

fail() {
  echo "$*" >&2
  exit 1
}

restore_user_state() {
  set +e
  if [[ -n "${config_dir}" && -d "${config_dir}" ]]; then
    chmod u+rwx "${config_dir}" 2>/dev/null || true
  fi
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

for command in cmp dbus-daemon jq niri python3 sha256sum stat timeout wl-copy wl-paste; do
  command -v "${command}" >/dev/null 2>&1 ||
    fail "required script-recovery live command is missing: ${command}"
done
[[ -x "${gui_bin}" ]] || fail "GUI binary is missing or not executable: ${gui_bin}"
[[ -x "${fixture}" ]] || fail "registry fixture is missing or not executable: ${fixture}"
[[ -x "${daemon_fixture}" ]] || fail "daemon fixture is missing or not executable: ${daemon_fixture}"
[[ -x "${key_sender}" ]] || fail "uinput key sender is missing or not executable: ${key_sender}"
[[ -w /dev/uinput ]] || fail "/dev/uinput is not writable"
for value in "${resource_id}" "${resource_short_id}"; do
  [[ -n "${value}" && "${value}" != *$'\n'* ]] ||
    fail "script fixture values must be non-empty and single-line"
done
[[ "${resource_id}" =~ ^${resource_kind}\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$ ]] ||
  fail "${mode} id must use the ${resource_kind}.<group>.<name> registry shape"

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
  niri msg --json workspaces |
    jq -r '[.[] | select(.is_focused) | .active_window_id] |
      if length == 1 and .[0] != null then .[0] else empty end'
)"

if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
  WAYLAND_DISPLAY="$(sed -n 's/^WAYLAND_DISPLAY=//p' <<<"${session_environment}")"
  export WAYLAND_DISPLAY
fi
[[ -n "${WAYLAND_DISPLAY:-}" ]] || fail "a Wayland display is required"

if [[ "${update_existing}" != true ]]; then
  clipboard_before="$(mktemp)"
  clipboard_types_before="$(timeout 1s wl-paste --list-types 2>/dev/null || true)"
  if [[ -n "${clipboard_types_before}" ]]; then
    if ! grep -Eq '^(text/plain|text/plain;charset=utf-8|UTF8_STRING)$' <<<"${clipboard_types_before}"; then
      fail "standard clipboard has no restorable text representation; refusing to replace it"
    fi
    timeout 2s wl-paste --no-newline >"${clipboard_before}"
    clipboard_had_text=1
  fi
fi

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
: >"${out_dir}/uinput.jsonl"
runtime_root="$(mktemp -d)"
fixture_root="${runtime_root}/fixture"
port_file="${runtime_root}/fixture.port"
request_log="${out_dir}/requests.jsonl"
config_home="${runtime_root}/config"
cache_home="${runtime_root}/cache"
data_home="${runtime_root}/data"
home_dir="${runtime_root}/home"
config_path="${config_home}/fcitx-vinpst/config.json"
config_dir="$(dirname "${config_path}")"
config_backup="${config_path}.bak"
script_group="${resource_id#*.}"
script_group="${script_group%%.*}"
script_name="${resource_id##*.}"
script_path="${data_home}/fcitx-vinpst/${managed_directory}/${script_group}/${script_name}"
rollback_path="${script_path}.rollback"
working_directory="${home_dir}/adapter-work"
asset_path="${fixture_root}/assets/${resource_short_id}.py"
registry_path="${fixture_root}/registry/${registry_filename}"
method_log="${out_dir}/daemon-methods.jsonl"
daemon_ready="${runtime_root}/daemon.ready"

mkdir -p \
  "$(dirname "${asset_path}")" \
  "$(dirname "${registry_path}")" \
  "${config_dir}" "${cache_home}" "${data_home}" "${home_dir}"
if [[ "${update_existing}" == true ]]; then
  mkdir -p "$(dirname "${script_path}")" "${working_directory}"
  cat >"${script_path}" <<'PY'
#!/usr/bin/env python3
print("old managed adapter")
PY
  chmod 700 "${script_path}"
  old_script_sha256="$(sha256sum "${script_path}" | awk '{print $1}')"
else
  old_script_sha256=""
fi
cat >"${asset_path}" <<'PY'
#!/usr/bin/env python3
import sys

sys.stdout.write('{"event":"final","text":"new fixture"}\n')
PY
asset_sha256="$(sha256sum "${asset_path}" | awk '{print $1}')"
asset_size="$(stat -c '%s' "${asset_path}")"
[[ "${update_existing}" != true || "${asset_sha256}" != "${old_script_sha256}" ]] ||
  fail "adapter update fixture did not produce a new script revision"

"${fixture}" \
  --root "${fixture_root}" \
  --port-file "${port_file}" \
  --request-log "${request_log}" \
  >"${out_dir}/fixture.stdout.log" 2>"${out_dir}/fixture.stderr.log" &
server_pid=$!
for _ in $(seq 1 100); do
  [[ -s "${port_file}" ]] && break
  kill -0 "${server_pid}" 2>/dev/null || fail "registry fixture exited before publishing its port"
  sleep 0.05
done
[[ -s "${port_file}" ]] || fail "registry fixture did not publish its port"
fixture_port="$(tr -d '\n' <"${port_file}")"
[[ "${fixture_port}" =~ ^[0-9]+$ ]] || fail "registry fixture returned an invalid port"
base_url="http://127.0.0.1:${fixture_port}"
asset_url="${base_url}/assets/${resource_short_id}.py"

if [[ "${resource_kind}" == adapter ]]; then
  if [[ "${update_existing}" == true ]]; then
    jq -n \
      --arg id "${resource_id}" \
      --arg short_id "${resource_short_id}" \
      --arg asset_url "${asset_url}" \
      --arg required_name "${required_environment_name}" \
      --arg optional_name "${optional_environment_name}" \
      '{
        version: 1,
        items: [
          {
            id: $id,
            short_id: $short_id,
            command: "python3",
            script_urls: [$asset_url],
            envs: [
              {name: $required_name, required: true},
              {name: $optional_name, required: false}
            ]
          }
        ]
      }' >"${registry_path}"
  else
    jq -n \
      --arg id "${resource_id}" \
      --arg short_id "${resource_short_id}" \
      --arg asset_url "${asset_url}" \
      --arg environment_name "${required_environment_name}" \
      '{
        version: 1,
        items: [
          {
            id: $id,
            short_id: $short_id,
            command: "python3",
            script_urls: [$asset_url],
            envs: [{name: $environment_name, required: true}]
          }
        ]
      }' >"${registry_path}"
  fi
else
  jq -n \
    --arg id "${resource_id}" \
    --arg short_id "${resource_short_id}" \
    --arg asset_url "${asset_url}" \
    '{
      version: 1,
      items: [
        {
          id: $id,
          short_id: $short_id,
          stream: false,
          command: "python3",
          script_urls: [$asset_url],
          envs: []
        }
      ]
    }' >"${registry_path}"
fi

if [[ "${update_existing}" == true ]]; then
  jq \
    --arg base_url "${base_url}" \
    --arg id "${resource_id}" \
    --arg script_path "${script_path}" \
    --arg required_name "${required_environment_name}" \
    --arg required_value "${required_environment_value}" \
    --arg optional_name "${optional_environment_name}" \
    --arg optional_value "${optional_environment_value}" \
    --arg unrelated_name "${unrelated_environment_name}" \
    --arg unrelated_value "${unrelated_environment_value}" \
    --arg working_directory "${working_directory}" \
    --arg old_revision "${old_script_sha256}" \
    '.registry.base_urls = [$base_url] |
     .llm.adapters += [{
       id: $id,
       command: "python-old",
       args: [$script_path],
       env: {
         ($required_name): $required_value,
         ($optional_name): $optional_value,
         ($unrelated_name): $unrelated_value
       },
       working_dir: $working_directory,
       "x-vinpst-managed-script-sha256": $old_revision,
       "x-vinpst-live-future-field": {preserved: true}
     }]' data/default-config.json >"${config_path}"
else
  jq --arg base_url "${base_url}" '.registry.base_urls = [$base_url]' \
    data/default-config.json >"${config_path}"
fi
chmod 600 "${config_path}"
config_before="${runtime_root}/config-before.json"
cp --preserve=mode,timestamps "${config_path}" "${config_before}"
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
  # A freshly-created uinput device needs enough time for udev and libinput to
  # classify and attach it to the active niri seat before the key is emitted.
  "${key_sender}" --settle-ms 600 "$1" | tee -a "${out_dir}/uinput.jsonl" >/dev/null
}

send_text() {
  "${key_sender}" --settle-ms 600 --text "$1" |
    tee -a "${out_dir}/uinput.jsonl" >/dev/null
}

focus_gui() {
  local focused=""
  niri msg action focus-window --id "${gui_window_id}" >/dev/null
  for _ in $(seq 1 20); do
    focused="$(
      niri msg --json workspaces 2>/dev/null |
        jq -r '[.[] | select(.is_focused) | .active_window_id] |
          if length == 1 and .[0] != null then .[0] else empty end'
    )"
    [[ "${focused}" == "${gui_window_id}" ]] && return
    sleep 0.05
  done
  fail "niri did not focus GUI window ${gui_window_id}; active focused-workspace window is ${focused:-none}"
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
    "${gui_bin}" --page "${initial_page}" \
    >"${out_dir}/gui.stdout.log" 2>"${out_dir}/gui.stderr.log" &
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
  wait_for_title "${page_title}"
  if grep -Eq 'Cannot start a runtime from within a runtime|panicked at' \
    "${out_dir}/gui.stderr.log"; then
    fail "GUI emitted a runtime panic"
  fi
}

replace_focused_text() {
  local value=$1
  local sentinel="script-recovery-copy-sentinel-${RANDOM}-${RANDOM}"
  clipboard_mutated=1
  printf '%s' "${value}" | wl-copy
  send_key CTRL+A
  send_key CTRL+V
  printf '%s' "${sentinel}" | wl-copy
  send_key CTRL+A
  send_key CTRL+C
  [[ "$(timeout 2s wl-paste --no-newline 2>/dev/null || true)" == "${value}" ]]
}

focus_first_editable_text_field() {
  local marker=""
  focus_gui
  send_key ESCAPE
  for step in $(seq 1 30); do
    send_key TAB
    marker="script-recovery-field-probe-${step}"
    if replace_focused_text "${marker}"; then
      send_key CTRL+A
      send_key BACKSPACE
      return
    fi
  done
  fail "${page_title} focus traversal did not reach its first editable text field"
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

resource_config_exists() {
  if [[ "${resource_kind}" == adapter ]]; then
    jq -e --arg id "${resource_id}" '.llm.adapters[] | select(.id == $id)' \
      "${config_path}" >/dev/null 2>&1
  else
    jq -e --arg id "${resource_id}" '.asr.providers[] | select(.id == $id)' \
      "${config_path}" >/dev/null 2>&1
  fi
}

wait_for_resource_config() {
  for _ in $(seq 1 300); do
    resource_config_exists && return
    kill -0 "${gui_pid}" 2>/dev/null ||
      fail "GUI exited while waiting for recovered ${mode} config"
    sleep 0.1
  done
  fail "recovered ${mode} config did not appear"
}

request_count() {
  local path=$1
  jq -r --arg path "${path}" 'select(.path == $path) | .path' \
    "${request_log}" 2>/dev/null | wc -l
}

method_count() {
  local method=$1
  jq -r --arg method "${method}" 'select(.method == $method) | .method' \
    "${method_log}" 2>/dev/null | wc -l
}

catalog_request_path="/registry/${registry_filename}"
asset_request_path="/assets/${resource_short_id}.py"
required_environment_confirmed=false
required_environment_prefilled=false
first_update_restored_previous=false
retry_reused_catalog=false
retry_redownloaded_asset=false
revision_changed=false
rollback_revision_verified=false

start_gui
# F6 focuses the active registry workflow: the selector while idle, then the
# enabled Install/Update or Retry primary action during a script operation.
focus_gui
send_key F6
send_key CTRL+A
send_text "${resource_short_id}"

# Provider and new-adapter modes prove publication followed by config-only recovery.
# Adapter-update mode instead starts from an existing managed adapter, forces the
# first replacement commit to fail, requires restoration of the previous script,
# and then retries the stored plan without resolving the catalog again.
if [[ "${resource_kind}" == adapter ]]; then
  send_key TAB
  send_key ENTER
  for _ in $(seq 1 100); do
    registry_requests="$(request_count "${catalog_request_path}")"
    asset_requests="$(request_count "${asset_request_path}")"
    ((registry_requests == 1 && asset_requests == 0)) && break
    sleep 0.05
  done
  ((registry_requests == 1)) || fail "adapter preparation did not request its registry exactly once"
  ((asset_requests == 0)) || fail "adapter script download began before environment confirmation"
  sleep 0.8

  focus_gui
  if [[ "${update_existing}" == true ]]; then
    # Existing values are prefilled. F6 focuses the enabled Install/Update
    # primary action without depending on the global traversal origin.
    send_key F6
    required_environment_prefilled=true
  else
    # New adapter: the required secure input is the seventh global focus target.
    for _ in $(seq 1 7); do send_key TAB; done
    clipboard_mutated=1
    printf '%s' "${required_environment_value}" | wl-copy
    send_key CTRL+A
    send_key CTRL+V
    send_key TAB
  fi
  chmod 500 "${config_dir}"
  [[ "$(stat -c '%a' "${config_dir}")" == 500 ]] ||
    fail "config directory did not become read-only"
  send_key ENTER
  required_environment_confirmed=true
else
  chmod 500 "${config_dir}"
  [[ "$(stat -c '%a' "${config_dir}")" == 500 ]] ||
    fail "config directory did not become read-only"
  send_key TAB
  send_key ENTER
fi

for _ in $(seq 1 100); do
  registry_requests="$(request_count "${catalog_request_path}")"
  asset_requests="$(request_count "${asset_request_path}")"
  ((registry_requests >= 1 && asset_requests >= 1)) && break
  sleep 0.05
done
((registry_requests == 1)) || fail "expected exactly one ${mode} registry request before recovery"
((asset_requests == 1)) || fail "expected exactly one ${mode} script request before recovery"

if [[ "${update_existing}" == true ]]; then
  # The replacement transaction must retain the old rollback artifact and restore
  # the canonical script after the forced config failure.
  wait_for_path "${rollback_path}" "managed adapter rollback script"
  for _ in $(seq 1 100); do
    [[ "$(sha256sum "${script_path}" 2>/dev/null | awk '{print $1}')" == "${old_script_sha256}" ]] && break
    sleep 0.05
  done
  [[ -f "${script_path}" && ! -L "${script_path}" && -x "${script_path}" ]] ||
    fail "restored managed adapter script is not a regular executable file"
  [[ "$(sha256sum "${script_path}" | awk '{print $1}')" == "${old_script_sha256}" ]] ||
    fail "failed adapter update did not restore the previous canonical script"
  [[ -f "${rollback_path}" && ! -L "${rollback_path}" ]] ||
    fail "adapter update rollback artifact is not a regular file"
  [[ "$(sha256sum "${rollback_path}" | awk '{print $1}')" == "${old_script_sha256}" ]] ||
    fail "adapter update rollback artifact does not contain the previous revision"
  first_update_restored_previous=true
else
  wait_for_path "${script_path}" "published ${mode} script"
  [[ -f "${script_path}" && ! -L "${script_path}" ]] ||
    fail "published ${mode} script is not a regular file"
  [[ -x "${script_path}" ]] || fail "published ${mode} script is not executable"
  script_published_sha256="$(sha256sum "${script_path}" | awk '{print $1}')"
  [[ "${script_published_sha256}" == "${asset_sha256}" ]] ||
    fail "published script bytes differ from fixture asset"
fi

sleep 0.8
[[ "$(sha256sum "${config_path}" | awk '{print $1}')" == "${config_before_sha256}" ]] ||
  fail "failed install changed config bytes before retry"
[[ ! -e "${config_backup}" ]] || fail "failed config write unexpectedly created a backup"
[[ "$(method_count ReloadAsrBackend)" == 0 ]] || fail "failed config write requested daemon reload"

chmod 700 "${config_dir}"
if [[ "${update_existing}" == true ]]; then
  # In the failed update state F6 focuses the primary Retry action directly.
  focus_gui
  send_key F6
  send_key ENTER
  for _ in $(seq 1 100); do
    registry_requests="$(request_count "${catalog_request_path}")"
    asset_requests="$(request_count "${asset_request_path}")"
    ((registry_requests == 1 && asset_requests == 2)) && break
    sleep 0.05
  done
  ((registry_requests == 1)) || fail "adapter update retry resolved the catalog again"
  ((asset_requests == 2)) || fail "adapter update retry did not redownload exactly one replacement asset"
  retry_reused_catalog=true
  retry_redownloaded_asset=true
else
  if [[ "${resource_kind}" == provider ]]; then
    focus_first_editable_text_field
    for _ in $(seq 1 4); do send_key TAB; done
  else
    focus_gui
    send_key ESCAPE
    for _ in $(seq 1 9); do send_key TAB; done
  fi
  send_key ENTER
fi

wait_for_resource_config
for _ in $(seq 1 100); do
  [[ -f "${config_backup}" ]] && [[ "$(method_count ReloadAsrBackend)" == 1 ]] && break
  sleep 0.05
done
[[ -f "${config_backup}" && ! -L "${config_backup}" ]] ||
  fail "recovery did not create a regular config backup"
cmp -s "${config_before}" "${config_backup}" ||
  fail "recovery backup does not match original config bytes"
[[ "$(stat -c '%a' "${config_path}")" == 600 ]] || fail "recovered config mode is not 600"
[[ "$(stat -c '%a' "${config_backup}")" == 600 ]] || fail "recovery backup mode is not 600"
if [[ "${update_existing}" != true ]]; then
  [[ "$(sha256sum "${script_path}" | awk '{print $1}')" == "${script_published_sha256}" ]] ||
    fail "configuration recovery changed the already-published script"
fi

if [[ "${resource_kind}" == adapter ]]; then
  if [[ "${update_existing}" == true ]]; then
    jq -e \
      --arg id "${resource_id}" \
      --arg script_path "${script_path}" \
      --arg required_name "${required_environment_name}" \
      --arg required_value "${required_environment_value}" \
      --arg optional_name "${optional_environment_name}" \
      --arg optional_value "${optional_environment_value}" \
      --arg unrelated_name "${unrelated_environment_name}" \
      --arg unrelated_value "${unrelated_environment_value}" \
      --arg working_directory "${working_directory}" \
      --arg new_revision "${asset_sha256}" \
      --arg old_revision "${old_script_sha256}" \
      '.llm.adapters[] |
        select(
          .id == $id and
          .command == "python3" and
          .args == [$script_path] and
          .working_dir == $working_directory and
          .env[$required_name] == $required_value and
          .env[$optional_name] == $optional_value and
          .env[$unrelated_name] == $unrelated_value and
          .["x-vinpst-managed-script-sha256"] == $new_revision and
          .["x-vinpst-managed-script-rollback-sha256"] == $old_revision and
          .["x-vinpst-live-future-field"].preserved == true
        )' "${config_path}" >/dev/null ||
      fail "updated adapter config did not preserve values and revision metadata"
    [[ "$(sha256sum "${script_path}" | awk '{print $1}')" == "${asset_sha256}" ]] ||
      fail "successful adapter update did not publish the new canonical revision"
    [[ "$(sha256sum "${rollback_path}" | awk '{print $1}')" == "${old_script_sha256}" ]] ||
      fail "successful adapter update did not retain the previous rollback revision"
    revision_changed=true
    rollback_revision_verified=true
  else
    jq -e \
      --arg id "${resource_id}" \
      --arg script_path "${script_path}" \
      --arg environment_name "${required_environment_name}" \
      --arg environment_value "${required_environment_value}" \
      '.llm.adapters[] |
        select(
          .id == $id and
          .command == "python3" and
          .args == [$script_path] and
          .working_dir == null and
          .env[$environment_name] == $environment_value
        )' "${config_path}" >/dev/null ||
      fail "recovered adapter config does not match the managed script and required environment contract"
  fi
else
  jq -e \
    --arg id "${resource_id}" \
    --arg script_path "${script_path}" \
    '.asr.providers[] |
      select(
        .id == $id and
        .type == "command" and
        .command == "python3" and
        .args == [$script_path] and
        .timeout_ms == 60000
      )' "${config_path}" >/dev/null ||
    fail "recovered provider config does not match the managed script contract"
fi
registry_requests="$(request_count "${catalog_request_path}")"
asset_requests="$(request_count "${asset_request_path}")"
((registry_requests == 1)) || fail "config recovery fetched the ${mode} registry again"
if [[ "${update_existing}" == true ]]; then
  ((asset_requests == 2)) || fail "adapter update retry did not use exactly two total asset requests"
else
  ((asset_requests == 1)) || fail "config recovery downloaded the published script again"
fi
[[ "$(method_count ReloadAsrBackend)" == 1 ]] ||
  fail "config recovery did not request exactly one daemon reload"
kill -0 "${gui_pid}" 2>/dev/null || fail "GUI exited after successful script recovery"
if grep -Eq 'Cannot start a runtime from within a runtime|panicked at' \
  "${out_dir}/gui.stderr.log"; then
  fail "GUI emitted a runtime panic during script recovery"
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
assert methods.count("ReloadAsrBackend") == 1
assert [record["sequence"] for record in records] == list(range(1, len(records) + 1))
print("private daemon script recovery contract verified")
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
if [[ "${clipboard_mutated}" == 1 ]]; then
  if [[ "${clipboard_had_text}" == 1 ]]; then
    wl-copy --type 'text/plain;charset=utf-8' <"${clipboard_before}"
    timeout 2s wl-paste --no-newline >"${out_dir}/clipboard-restored.tmp"
    cmp -s "${clipboard_before}" "${out_dir}/clipboard-restored.tmp" ||
      fail "standard clipboard text was not restored"
    rm -f "${out_dir}/clipboard-restored.tmp"
  else
    wl-copy --clear
    if timeout 1s wl-paste --no-newline >/dev/null 2>&1; then
      fail "standard clipboard unexpectedly retained the recovery probe value"
    fi
  fi
fi
clipboard_mutated=0

jq -n \
  --arg mode "${mode}" \
  --arg resource_id "${resource_id}" \
  --arg resource_short_id "${resource_short_id}" \
  --arg asset_sha256 "${asset_sha256}" \
  --argjson asset_size "${asset_size}" \
  --argjson required_environment_confirmed "${required_environment_confirmed}" \
  --argjson required_environment_prefilled "${required_environment_prefilled}" \
  --argjson update_existing "${update_existing}" \
  --argjson first_update_restored_previous "${first_update_restored_previous}" \
  --argjson retry_reused_catalog "${retry_reused_catalog}" \
  --argjson retry_redownloaded_asset "${retry_redownloaded_asset}" \
  --argjson revision_changed "${revision_changed}" \
  --argjson rollback_revision_verified "${rollback_revision_verified}" \
  --argjson registry_requests "${registry_requests}" \
  --argjson asset_requests "${asset_requests}" \
  '{
    ok: true,
    backend: "niri-wayland-private-registry-private-daemon",
    script: {
      kind: $mode,
      id: $resource_id,
      short_id: $resource_short_id,
      asset_sha256: $asset_sha256,
      asset_size: $asset_size,
      required_environment_confirmed: $required_environment_confirmed,
      required_environment_prefilled: $required_environment_prefilled,
      update_existing: $update_existing,
      first_update_restored_previous: $first_update_restored_previous,
      retry_reused_catalog: $retry_reused_catalog,
      retry_redownloaded_asset: $retry_redownloaded_asset,
      revision_changed: $revision_changed,
      rollback_revision_verified: $rollback_revision_verified,
      published_before_config: true,
      recovery_panel_keyboard_reached: ($update_existing | not),
      failed_retry_keyboard_reached: $update_existing,
      config_failure_preserved_original: true,
      backup_absent_before_recovery: true,
      config_retry_only: ($update_existing | not),
      script_reused_without_download: ($update_existing | not),
      managed_config_committed: true,
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
      config_reload_calls: 1
    },
    isolation: {
      temporary_xdg_roots: true,
      real_daemon_untouched: true,
      user_config_untouched: true,
      user_scripts_untouched: true
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

jq -e \
  --arg mode "${mode}" \
  --argjson required_environment_confirmed "${required_environment_confirmed}" \
  --argjson required_environment_prefilled "${required_environment_prefilled}" \
  --argjson update_existing "${update_existing}" \
  --argjson expected_asset_requests "$([[ "${update_existing}" == true ]] && echo 2 || echo 1)" '
  .ok == true and
  .script.kind == $mode and
  .script.required_environment_confirmed == $required_environment_confirmed and
  .script.required_environment_prefilled == $required_environment_prefilled and
  .script.update_existing == $update_existing and
  .script.published_before_config == true and
  .script.first_update_restored_previous == $update_existing and
  .script.retry_reused_catalog == $update_existing and
  .script.retry_redownloaded_asset == $update_existing and
  .script.revision_changed == $update_existing and
  .script.rollback_revision_verified == $update_existing and
  .script.recovery_panel_keyboard_reached == ($update_existing | not) and
  .script.failed_retry_keyboard_reached == $update_existing and
  .script.config_failure_preserved_original == true and
  .script.backup_absent_before_recovery == true and
  .script.config_retry_only == ($update_existing | not) and
  .script.script_reused_without_download == ($update_existing | not) and
  .script.managed_config_committed == true and
  .script.values_retained == false and
  .network.loopback_only == true and
  .network.registry_requests == 1 and
  .network.asset_requests == $expected_asset_requests and
  .daemon.private_session_bus == true and
  .daemon.config_reload_calls == 1 and
  .isolation.real_daemon_untouched == true and
  .isolation.user_config_untouched == true and
  .isolation.user_scripts_untouched == true and
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
