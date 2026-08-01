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

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="${script_dir}"
while [[ ! -f "${repo_root}/Cargo.toml" || ! -d "${repo_root}/scripts" ]]; do
  parent="$(dirname "${repo_root}")"
  if [[ "${parent}" == "${repo_root}" ]]; then
    echo "cannot locate repository root from ${script_dir}" >&2
    exit 1
  fi
  repo_root="${parent}"
done
cd "${repo_root}"

wav="${VINPUT_LIVE_TOOLKIT_WAV:-}"
playback_target="${VINPUT_LIVE_TOOLKIT_PLAYBACK_TARGET:-}"
out_dir="$(realpath -m "${VINPUT_LIVE_TOOLKIT_OUT_DIR:-target/tmp/ime-vscode-live}")"
uinput_sender="${VINPUT_LIVE_TOOLKIT_UINPUT_SENDER:-scripts/live/niri/probes/send-uinput-key.py}"
sandbox_probe="${VINPUT_LIVE_ELECTRON_SANDBOX_PROBE:-scripts/live/niri/probes/electron-renderer-sandbox.py}"
document="${out_dir}/${mode}.txt"
user_data_dir="${out_dir}/${mode}.user-data"
extensions_dir="${out_dir}/${mode}.extensions"
focus_log="${out_dir}/${mode}.focus.json"
uinput_log="${out_dir}/${mode}.uinput.jsonl"
monitor_log="${out_dir}/${mode}.dbus.log"
sandbox_log="${out_dir}/${mode}.sandbox.json"
primary_log="${out_dir}/${mode}.primary-selection.json"
summary="${out_dir}/${mode}.summary.json"
code_stdout="${out_dir}/${mode}.stdout.log"
code_stderr="${out_dir}/${mode}.stderr.log"
surrounding_text="vscode surrounding selection"
primary_text="vscode primary selection"
surrounding_prefix="adapter-backed: ${surrounding_text} | command: "
primary_prefix="adapter-backed: ${primary_text} | command: "
primary_before="${out_dir}/${mode}.primary-before.txt"
primary_restored="${out_dir}/${mode}.primary-restored.txt"
launcher_pid=""
monitor_pid=""
window_pid=""
window_id=""
primary_owner_pid=""
primary_before_present=0
primary_snapshot_ready=0

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

vscode_pids() {
  local cmdline_path pid
  for cmdline_path in /proc/[0-9]*/cmdline; do
    [[ -r "${cmdline_path}" ]] || continue
    if ! grep -Fzq -- "${user_data_dir}" "${cmdline_path}" 2>/dev/null; then
      continue
    fi
    pid="${cmdline_path#/proc/}"
    pid="${pid%/cmdline}"
    printf '%s\n' "${pid}"
  done
}

stop_vscode() {
  local pid
  local -a pids=()
  while IFS= read -r pid; do
    [[ -n "${pid}" ]] && pids+=("${pid}")
  done < <(vscode_pids)
  if ((${#pids[@]})); then
    kill -TERM "${pids[@]}" 2>/dev/null || true
  fi
  for _ in $(seq 1 100); do
    mapfile -t pids < <(vscode_pids)
    ((${#pids[@]} == 0)) && return 0
    sleep 0.05
  done
  kill -KILL "${pids[@]}" 2>/dev/null || true
  for _ in $(seq 1 100); do
    mapfile -t pids < <(vscode_pids)
    ((${#pids[@]} == 0)) && return 0
    sleep 0.05
  done
  echo "isolated VS Code processes remained after termination" >&2
  return 1
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
        primary_snapshot_ready=0
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
      primary_snapshot_ready=0
      return 0
    fi
    sleep 0.05
  done
  echo "failed to clear the temporary Wayland primary selection" >&2
  return 1
}

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM
  set +e
  restore_idle
  stop_vscode || exit_code=1
  if [[ -n "${launcher_pid}" ]]; then
    wait "${launcher_pid}" 2>/dev/null || true
  fi
  if [[ -n "${monitor_pid}" ]]; then
    kill -TERM "${monitor_pid}" 2>/dev/null || true
    wait "${monitor_pid}" 2>/dev/null || true
  fi
  restore_primary_selection || exit_code=1
  rm -rf "${user_data_dir}" "${extensions_dir}"
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

if [[ -z "${wav}" || ! -f "${wav}" ]]; then
  echo "set VINPUT_LIVE_TOOLKIT_WAV to a validated speech WAV" >&2
  exit 2
fi
for command in cmp code gdbus jq niri pw-play python3 realpath stdbuf timeout; do
  command -v "${command}" >/dev/null 2>&1 || {
    echo "required VS Code live command is missing: ${command}" >&2
    exit 1
  }
done
if [[ "${mode}" == command ]]; then
  for command in wl-copy wl-paste; do
    command -v "${command}" >/dev/null 2>&1 || {
      echo "required VS Code selection command is missing: ${command}" >&2
      exit 1
    }
  done
fi
if [[ ! -x "${uinput_sender}" || ! -x "${sandbox_probe}" || ! -w /dev/uinput ]]; then
  echo "the executable uinput/sandbox probes and writable /dev/uinput are required" >&2
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
if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
  runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
  mapfile -t wayland_sockets < <(
    find "${runtime_dir}" -maxdepth 1 -type s -name 'wayland-*' -printf '%f\n' 2>/dev/null
  )
  if [[ "${#wayland_sockets[@]}" == 1 ]]; then
    export WAYLAND_DISPLAY="${wayland_sockets[0]}"
  fi
fi
niri msg --json windows >/dev/null

status="$(call_service GetStatus 2>/dev/null || true)"
if [[ "${status}" != *"'idle'"* ]]; then
  echo "org.fcitx.Vinput must be idle before the VS Code probe: ${status:-unavailable}" >&2
  exit 1
fi

rm -rf "${out_dir}"
mkdir -p "${user_data_dir}/User" "${extensions_dir}"
: >"${uinput_log}"
if [[ "${mode}" == command ]]; then
  printf '%s' "${surrounding_text}" >"${document}"
else
  : >"${document}"
fi
cat >"${user_data_dir}/User/settings.json" <<'JSON'
{
  "extensions.autoCheckUpdates": false,
  "extensions.autoUpdate": false,
  "security.workspace.trust.enabled": false,
  "telemetry.telemetryLevel": "off",
  "update.mode": "none",
  "workbench.startupEditor": "none"
}
JSON

stdbuf -oL -eL gdbus monitor --session \
  --dest org.fcitx.Vinput \
  --object-path /org/fcitx/Vinput >"${monitor_log}" 2>&1 &
monitor_pid=$!

GTK_IM_MODULE=fcitx \
VSCODE_DISABLE_CRASH_REPORTER=1 \
VSCODE_DISABLE_UPDATE=1 \
  code \
  --new-window \
  --disable-extensions \
  --disable-workspace-trust \
  --skip-welcome \
  --sync off \
  --log off \
  --user-data-dir "${user_data_dir}" \
  --extensions-dir "${extensions_dir}" \
  --ozone-platform=wayland \
  "${document}" \
  >"${code_stdout}" 2>"${code_stderr}" &
launcher_pid=$!

for _ in $(seq 1 300); do
  window_record="$(
    niri msg --json windows 2>/dev/null |
      jq -c --arg title "$(basename "${document}")" \
        '[.[] | select(.app_id == "code" and ((.title // "") | contains($title)))] | if length == 1 then .[0] else empty end'
  )"
  if [[ -n "${window_record}" ]]; then
    window_id="$(jq -r '.id' <<<"${window_record}")"
    window_pid="$(jq -r '.pid' <<<"${window_record}")"
    niri msg action focus-window --id "${window_id}" >/dev/null
    focused_id="$(niri msg --json focused-window | jq -r '.id // empty')"
    [[ "${focused_id}" == "${window_id}" ]] && break
  fi
  sleep 0.1
done
focused_id="$(niri msg --json focused-window | jq -r '.id // empty')"
if [[ -z "${window_id}" || -z "${window_pid}" || "${focused_id}" != "${window_id}" ]]; then
  echo "VS Code file window did not become focused" >&2
  exit 1
fi
sleep 0.5

jq -n \
  --arg backend niri \
  --arg socket "${NIRI_SOCKET}" \
  --arg app_id code \
  --arg title "$(niri msg --json focused-window | jq -r '.title // empty')" \
  --argjson pid "${window_pid}" \
  --argjson window_id "${window_id}" \
  '{
    event: "window-focus",
    backend: $backend,
    socket: $socket,
    app_id: $app_id,
    title: $title,
    pid: $pid,
    window_id: $window_id,
    focused: true,
    ok: true
  }' >"${focus_log}"

python3 "${sandbox_probe}" \
  --application vscode \
  --user-data-dir "${user_data_dir}" \
  --window-pid "${window_pid}" \
  --output "${sandbox_log}" >/dev/null

selection_transport=none
if [[ "${mode}" == command ]]; then
  "${uinput_sender}" CTRL+A | tee -a "${uinput_log}"
  sleep 0.3
  if timeout 2s wl-paste --primary --no-newline >"${primary_before}" 2>/dev/null; then
    primary_before_present=1
  else
    : >"${primary_before}"
  fi
  primary_snapshot_ready=1
  wl-copy --primary --foreground --type 'text/plain;charset=utf-8' \
    < <(printf '%s' "${primary_text}") &
  primary_owner_pid=$!
  primary_ready=0
  for _ in $(seq 1 100); do
    if ! kill -0 "${primary_owner_pid}" 2>/dev/null; then
      break
    fi
    if current_primary="$(timeout 1s wl-paste --primary --no-newline 2>/dev/null)" &&
      [[ "${current_primary}" == "${primary_text}" ]]; then
      primary_ready=1
      break
    fi
    sleep 0.05
  done
  if [[ "${primary_ready}" != 1 ]]; then
    echo "temporary VS Code primary selection did not become readable" >&2
    exit 1
  fi
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
  echo "daemon did not enter recording after the VS Code shortcut" >&2
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
  echo "daemon did not return to idle after the VS Code shortcut" >&2
  exit 1
fi
sleep 0.5
"${uinput_sender}" CTRL+S | tee -a "${uinput_log}"

saved=0
content=""
for _ in $(seq 1 200); do
  content="$(cat "${document}")"
  if [[ "${mode}" == normal && -n "${content}" ]]; then
    saved=1
    break
  fi
  if [[ "${mode}" == command && "${content}" == "${surrounding_prefix}"* ]]; then
    selection_transport=surrounding-text
    saved=1
    break
  fi
  if [[ "${mode}" == command && "${content}" == "${primary_prefix}"* ]]; then
    selection_transport=primary-selection-fallback
    saved=1
    break
  fi
  sleep 0.05
done
if [[ "${saved}" != 1 ]]; then
  echo "VS Code did not save the expected content" >&2
  printf 'content=%q\n' "$(cat "${document}")" >&2
  exit 1
fi

partial_count="$(grep -c 'RecognitionPartial' "${monitor_log}" || true)"
if [[ "${partial_count}" -lt 1 ]]; then
  echo "VS Code probe observed no daemon partial signal" >&2
  exit 1
fi

previous_selection_present=false
primary_restored_ok=true
if [[ "${mode}" == command ]]; then
  if [[ "${primary_before_present}" == 1 ]]; then
    previous_selection_present=true
  fi
  restore_primary_selection
fi
jq -n \
  --argjson previous_selection_present "${previous_selection_present}" \
  --arg selection_transport "${selection_transport}" \
  '{
    event: "primary-selection",
    previous_selection_present: $previous_selection_present,
    selection_transport: $selection_transport,
    restored: true,
    ok: true
  }' >"${primary_log}"

stop_vscode
if [[ -n "${launcher_pid}" ]]; then
  wait "${launcher_pid}" 2>/dev/null || true
  launcher_pid=""
fi
for _ in $(seq 1 100); do
  if ! niri msg --json windows | jq -e --argjson id "${window_id}" 'any(.[]; .id == $id)' >/dev/null; then
    break
  fi
  sleep 0.05
done
if niri msg --json windows | jq -e --argjson id "${window_id}" 'any(.[]; .id == $id)' >/dev/null; then
  echo "VS Code window remained after isolated process termination" >&2
  exit 1
fi
rm -rf "${user_data_dir}" "${extensions_dir}"
test ! -e "${user_data_dir}"
test ! -e "${extensions_dir}"

content="$(cat "${document}")"
replacement=false
if [[ "${mode}" == command ]]; then
  replacement=true
fi
code_version="$(code --version | head -1)"
jq -n \
  --arg mode "${mode}" \
  --arg document "${document}" \
  --arg content "${content}" \
  --arg trigger_key "${trigger_key}" \
  --arg selection_transport "${selection_transport}" \
  --arg code_version "${code_version}" \
  --argjson window_pid "${window_pid}" \
  --argjson window_id "${window_id}" \
  --argjson partial_count "${partial_count}" \
  --argjson replacement "${replacement}" \
  --argjson primary_restored "${primary_restored_ok}" \
  '{
    event: "summary",
    application: "vscode",
    version: $code_version,
    mode: $mode,
    document: $document,
    content: $content,
    trigger_key: $trigger_key,
    window_pid: $window_pid,
    window_id: $window_id,
    partial_count: $partial_count,
    replacement: $replacement,
    selection_transport: $selection_transport,
    primary_selection_restored: $primary_restored,
    saved: true,
    profile_removed: true,
    process_residue: false,
    window_residue: false,
    ok: true
  }' | tee "${summary}"

kill -TERM "${monitor_pid}" 2>/dev/null || true
wait "${monitor_pid}" 2>/dev/null || true
monitor_pid=""
trap - EXIT INT TERM
printf 'VS Code %s live probe passed; evidence: %s\n' "${mode}" "${out_dir}"
