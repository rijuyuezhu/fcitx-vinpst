#!/usr/bin/env bash
set -euo pipefail

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

mode="${1:-${VINPST_LIVE_TOOLKIT_MODE:-normal}}"
case "${mode}" in
normal | command) ;;
*)
  echo "mode must be normal or command" >&2
  exit 2
  ;;
esac

out_dir="$(realpath -m "${VINPST_LIVE_TOOLKIT_OUT_DIR:-target/tmp/ime-chromium-virtual-live}")"
wav="${VINPST_LIVE_TOOLKIT_WAV:-}"
playback_target="${VINPST_LIVE_TOOLKIT_PLAYBACK_TARGET:-}"
uinput_sender="${VINPST_LIVE_TOOLKIT_UINPUT_SENDER:-scripts/live/niri/probes/send-uinput-key.py}"
timeout_seconds="${VINPST_TOOLKIT_TIMEOUT_SECONDS:-120}"
log="${out_dir}/${mode}.jsonl"
stdout_log="${out_dir}/${mode}.stdout.log"
stderr_log="${out_dir}/${mode}.stderr.log"
uinput_log="${out_dir}/${mode}.uinput.jsonl"
focus_log="${out_dir}/${mode}.focus.json"
sandbox_log="${out_dir}/${mode}.sandbox.json"
primary_log="${out_dir}/${mode}.primary-selection.json"
playback_done="${out_dir}/${mode}.playback-done"
trigger_armed="${out_dir}/${mode}.trigger-armed"
browser_pid_file="${out_dir}/${mode}.browser-pid"
profile_path="${out_dir}/profile-${mode}"
window_title=""
browser_selected_text="chromium surrounding selection"
primary_selected_text="chromium primary selection"
expected_command_prefix="adapter-backed: ${primary_selected_text} | command: "
primary_before="${out_dir}/${mode}.primary-before.txt"
primary_restored_file="${out_dir}/${mode}.primary-restored.txt"
primary_owner_pid=""
primary_before_present=0
primary_snapshot_ready=0

if [[ -z "${wav}" || ! -f "${wav}" ]]; then
  echo "VINPST_LIVE_TOOLKIT_WAV must name an existing speech WAV" >&2
  exit 2
fi
for command_name in cmp gdbus jq niri pgrep ps pw-play python3 realpath timeout; do
  command -v "${command_name}" >/dev/null 2>&1 || {
    echo "${command_name} is required for the automatic Chromium live gate" >&2
    exit 1
  }
done
if [[ "${mode}" == command ]]; then
  for command_name in wl-copy wl-paste; do
    command -v "${command_name}" >/dev/null 2>&1 || {
      echo "${command_name} is required for Chromium primary-selection fallback" >&2
      exit 1
    }
  done
fi
if [[ ! -x "${uinput_sender}" ]]; then
  echo "uinput key sender is missing or not executable: ${uinput_sender}" >&2
  exit 1
fi
if [[ ! -w /dev/uinput ]]; then
  echo "/dev/uinput is not writable; cannot send a real desktop key" >&2
  exit 1
fi
if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  echo "DBUS_SESSION_BUS_ADDRESS is not set" >&2
  exit 1
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

mkdir -p "${out_dir}"
rm -f \
  "${log}" \
  "${stdout_log}" \
  "${stderr_log}" \
  "${uinput_log}" \
  "${focus_log}" \
  "${sandbox_log}" \
  "${primary_log}" \
  "${playback_done}" \
  "${trigger_armed}" \
  "${browser_pid_file}" \
  "${primary_before}" \
  "${primary_restored_file}"

playback_pid=""
probe_pid=""
trigger_pid=""

browser_pid() {
  local cmdline cmdline_path executable executable_name pid
  for cmdline_path in /proc/[0-9]*/cmdline; do
    [[ -r "${cmdline_path}" ]] || continue
    pid="${cmdline_path#/proc/}"
    pid="${pid%/cmdline}"
    executable="$(readlink -f "/proc/${pid}/exe" 2>/dev/null || true)"
    executable_name="$(basename "${executable}")"
    [[ "${executable_name}" == *chrome* || "${executable_name}" == *chromium* ]] || continue
    cmdline="$(tr '\0' ' ' <"${cmdline_path}" 2>/dev/null || true)"
    [[ "${cmdline}" == *"--user-data-dir=${profile_path}"* ]] || continue
    [[ "${cmdline}" == *"--app="* ]] || continue
    [[ "${cmdline}" != *"--type="* ]] || continue
    printf '%s\n' "${pid}"
  done
}

renderer_pid() {
  local browser_sid candidate candidate_sid cmdline
  browser_sid="$(ps -o sid= -p "$1" 2>/dev/null | tr -d ' ')"
  [[ -n "${browser_sid}" ]] || return 1
  while IFS= read -r candidate; do
    [[ -n "${candidate}" && -r "/proc/${candidate}/cmdline" ]] || continue
    candidate_sid="$(ps -o sid= -p "${candidate}" 2>/dev/null | tr -d ' ')"
    [[ "${candidate_sid}" == "${browser_sid}" ]] || continue
    cmdline="$(tr '\0' ' ' <"/proc/${candidate}/cmdline" 2>/dev/null || true)"
    [[ "${cmdline}" == *"--type=renderer"* ]] || continue
    [[ "${cmdline}" != *"--extension-process"* ]] || continue
    printf '%s\n' "${candidate}"
    return 0
  done < <(pgrep -f -- '--type=renderer' || true)
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
      if timeout 1s wl-paste --primary --no-newline >"${primary_restored_file}" 2>/dev/null &&
        cmp -s "${primary_before}" "${primary_restored_file}"; then
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

# shellcheck disable=SC2329
cleanup() {
  local exit_code=$?
  local pid
  trap - EXIT
  set +e
  if [[ -n "${playback_pid}" ]]; then
    kill "${playback_pid}" 2>/dev/null || true
    wait "${playback_pid}" 2>/dev/null || true
  fi
  if [[ -n "${trigger_pid}" ]]; then
    kill "${trigger_pid}" 2>/dev/null || true
    wait "${trigger_pid}" 2>/dev/null || true
  fi
  if [[ -s "${browser_pid_file}" ]]; then
    pid="$(cat "${browser_pid_file}")"
    if [[ "${pid}" =~ ^[0-9]+$ ]]; then
      kill -TERM -- "-${pid}" 2>/dev/null || true
    fi
  fi
  if [[ -n "${probe_pid}" ]]; then
    kill "${probe_pid}" 2>/dev/null || true
    wait "${probe_pid}" 2>/dev/null || true
  fi
  if ! restore_primary_selection; then
    exit_code=1
  fi
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

if [[ "${mode}" == command ]]; then
  if timeout 2s wl-paste --primary --no-newline >"${primary_before}" 2>/dev/null; then
    primary_before_present=1
  else
    : >"${primary_before}"
  fi
  primary_snapshot_ready=1
  wl-copy --primary --foreground --type 'text/plain;charset=utf-8' \
    < <(printf '%s' "${primary_selected_text}") &
  primary_owner_pid=$!
  primary_ready=0
  for _ in $(seq 1 100); do
    if ! kill -0 "${primary_owner_pid}" 2>/dev/null; then
      break
    fi
    if current_primary="$(timeout 1s wl-paste --primary --no-newline 2>/dev/null)" &&
      [[ "${current_primary}" == "${primary_selected_text}" ]]; then
      primary_ready=1
      break
    fi
    sleep 0.05
  done
  if [[ "${primary_ready}" != 1 ]]; then
    echo "temporary Chromium primary selection did not become readable" >&2
    exit 1
  fi
fi

(
  for _ in $(seq 1 300); do
    if [[ ! -e "${trigger_armed}" ]]; then
      sleep 0.1
      continue
    fi
    status="$(gdbus call --session --dest org.fcitx.Vinpst \
      --object-path /org/fcitx/Vinpst \
      --method org.fcitx.Vinpst.Service.GetStatus 2>/dev/null || true)"
    if [[ "${status}" == *"recording"* ]]; then
      if [[ -n "${playback_target}" ]]; then
        pw-play --target "${playback_target}" "${wav}"
      else
        pw-play "${wav}"
      fi
      : >"${playback_done}"
      exit 0
    fi
    sleep 0.1
  done
  echo "daemon did not enter recording before the Chromium WAV playback deadline" >&2
  exit 1
) &
playback_pid=$!

set +e
VINPST_LIVE_TOOLKIT_OUT_DIR="${out_dir}" \
VINPST_TOOLKIT_TIMEOUT_SECONDS="${timeout_seconds}" \
VINPST_TOOLKIT_INITIAL_TEXT="$( [[ "${mode}" == command ]] && printf '%s' "${browser_selected_text}" || true )" \
VINPST_TOOLKIT_EXPECTED_COMMIT_SUBSTRING="$( [[ "${mode}" == command ]] && printf '%s' "${expected_command_prefix}" || true )" \
  scripts/live/niri/run-ime-chromium-native-live.sh "${mode}" \
  > >(tee "${stdout_log}") \
  2> >(tee "${stderr_log}" >&2) &
probe_pid=$!

(
  set -euo pipefail
  trigger_key=F9
  if [[ "${mode}" == command ]]; then
    trigger_key=F10
  fi

  for _ in $(seq 1 300); do
    if grep -Fq '"event":"ready"' "${log}" 2>/dev/null; then
      break
    fi
    sleep 0.1
  done
  if ! grep -Fq '"event":"ready"' "${log}" 2>/dev/null; then
    echo "Chromium probe did not become ready before the trigger deadline" >&2
    exit 1
  fi

  browser_candidates=()
  while IFS= read -r candidate; do
    [[ -n "${candidate}" ]] && browser_candidates+=("${candidate}")
  done < <(browser_pid)
  if [[ "${#browser_candidates[@]}" != 1 ]]; then
    echo "expected exactly one Chromium browser process, found ${#browser_candidates[@]}" >&2
    exit 1
  fi
  browser="${browser_candidates[0]}"
  printf '%s\n' "${browser}" >"${browser_pid_file}"

  window_id=""
  for _ in $(seq 1 300); do
    window_id="$(
      niri msg --json windows 2>/dev/null |
        jq -r --argjson pid "${browser}" \
          '[.[] | select(.pid == $pid) | .id] | if length == 1 then .[0] else empty end'
    )"
    if [[ -n "${window_id}" ]]; then
      niri msg action focus-window --id "${window_id}" >/dev/null 2>&1 || true
      focused_id="$(niri msg --json focused-window 2>/dev/null | jq -r '.id // empty')"
      if [[ "${focused_id}" == "${window_id}" ]]; then
        break
      fi
    fi
    sleep 0.1
  done
  focused_id="$(niri msg --json focused-window | jq -r '.id // empty')"
  if [[ -z "${window_id}" || "${focused_id}" != "${window_id}" ]]; then
    echo "Chromium probe window was not focused before the trigger" >&2
    exit 1
  fi
  window_title="$(
    niri msg --json focused-window |
      jq -r '.title // empty'
  )"
  jq -n \
    --arg backend niri \
    --arg socket "${NIRI_SOCKET}" \
    --arg title "${window_title}" \
    --argjson browser_pid "${browser}" \
    --argjson window_id "${window_id}" \
    '{
      event: "window-focus",
      backend: $backend,
      socket: $socket,
      title: $title,
      browser_pid: $browser_pid,
      window_id: $window_id,
      focused: true,
      ok: true
    }' >"${focus_log}"

  browser_cmdline="$(tr '\0' ' ' <"/proc/${browser}/cmdline")"
  if [[ "${browser_cmdline}" == *"--no-sandbox"* ||
    "${browser_cmdline}" == *"--disable-setuid-sandbox"* ]]; then
    echo "Chromium browser process disabled its sandbox" >&2
    exit 1
  fi

  renderer=""
  for _ in $(seq 1 300); do
    renderer="$(renderer_pid "${browser}" || true)"
    [[ -n "${renderer}" ]] && break
    sleep 0.1
  done
  if [[ -z "${renderer}" || ! -r "/proc/${renderer}/status" ]]; then
    echo "could not locate a non-extension Chromium renderer" >&2
    exit 1
  fi
  no_new_privs="$(awk '/^NoNewPrivs:/ {print $2}' "/proc/${renderer}/status")"
  seccomp="$(awk '/^Seccomp:/ {print $2}' "/proc/${renderer}/status")"
  cap_eff="$(awk '/^CapEff:/ {print $2}' "/proc/${renderer}/status")"
  nspid_values="$(awk '/^NSpid:/ {$1=""; sub(/^ /, ""); print}' "/proc/${renderer}/status")"
  nspid_depth="$(awk '/^NSpid:/ {print NF - 1}' "/proc/${renderer}/status")"
  if [[ "${no_new_privs}" != 1 || "${seccomp}" != 2 ||
    "${cap_eff}" != 0000000000000000 || "${nspid_depth}" -lt 2 ]]; then
    echo "Chromium renderer sandbox status was incomplete" >&2
    exit 1
  fi
  browser_exe="$(readlink -f "/proc/${browser}/exe")"
  browser_version="$("${browser_exe}" --version 2>/dev/null | head -1 || true)"
  jq -n \
    --arg browser_exe "${browser_exe}" \
    --arg browser_version "${browser_version}" \
    --arg nspid "${nspid_values}" \
    --arg cap_eff "${cap_eff}" \
    --argjson browser_pid "${browser}" \
    --argjson renderer_pid "${renderer}" \
    --argjson no_new_privs "${no_new_privs}" \
    --argjson seccomp "${seccomp}" \
    --argjson nspid_depth "${nspid_depth}" \
    '{
      event: "renderer-sandbox",
      browser_executable: $browser_exe,
      browser_version: $browser_version,
      browser_pid: $browser_pid,
      renderer_pid: $renderer_pid,
      browser_no_sandbox_flag: false,
      no_new_privs: $no_new_privs,
      seccomp: $seccomp,
      cap_eff: $cap_eff,
      nspid: $nspid,
      nspid_depth: $nspid_depth,
      ok: true
    }' >"${sandbox_log}"

  if grep -Fq '"event":"daemon-partial"' "${log}"; then
    echo "Chromium emitted recognition partials before the automatic trigger" >&2
    exit 1
  fi
  daemon_state="$(gdbus call --session --dest org.fcitx.Vinpst \
    --object-path /org/fcitx/Vinpst \
    --method org.fcitx.Vinpst.Service.GetStatus 2>/dev/null || true)"
  if [[ "${daemon_state}" != *"idle"* ]]; then
    echo "daemon was not idle immediately before the Chromium automatic trigger" >&2
    exit 1
  fi
  jq -nc --arg mode "${mode}" \
    '{event: "auto-trigger-start", toolkit: "chromium", mode: $mode}' >>"${log}"
  : >"${trigger_armed}"
  "${uinput_sender}" "${trigger_key}" | tee -a "${uinput_log}"
  for _ in $(seq 1 300); do
    [[ -e "${playback_done}" ]] && break
    sleep 0.1
  done
  if [[ ! -e "${playback_done}" ]]; then
    echo "Chromium playback did not finish before the stop-trigger deadline" >&2
    exit 1
  fi
  sleep 0.3
  "${uinput_sender}" "${trigger_key}" | tee -a "${uinput_log}"
) &
trigger_pid=$!

wait "${probe_pid}"
status=$?
probe_pid=""
set -e

if [[ -n "${trigger_pid}" ]]; then
  wait "${trigger_pid}" || status=1
  trigger_pid=""
fi
if [[ -n "${playback_pid}" ]]; then
  wait "${playback_pid}" || status=1
  playback_pid=""
fi

if [[ "${mode}" == command ]]; then
  if ! restore_primary_selection; then
    status=1
  else
    jq -n \
      --arg browser_selection "${browser_selected_text}" \
      --arg primary_selection "${primary_selected_text}" \
      --arg expected_prefix "${expected_command_prefix}" \
      --argjson previous_selection_present "${primary_before_present}" \
      '{
        event: "primary-selection-fallback",
        browser_selection: $browser_selection,
        primary_selection: $primary_selection,
        selections_are_distinct: ($browser_selection != $primary_selection),
        expected_prefix: $expected_prefix,
        previous_selection_present: ($previous_selection_present == 1),
        restored: true,
        ok: true
      }' >"${primary_log}"
  fi
fi

if [[ "${status}" == 0 ]]; then
  jq -s -e --arg mode "${mode}" --arg expected_prefix "${expected_command_prefix}" '
    any(.[];
      .event == "summary" and
      .toolkit == "chromium" and
      .mode == $mode and
      .ready == true and
      .partial == true and
      .commit == true and
      .replacement == ($mode == "command") and
      .selection_ready == ($mode == "command") and
      .timed_out == false and
      .ok == true and
      (.text | length) > 5 and
      ($mode == "normal" or (.text | startswith($expected_prefix)))
    )
  ' "${log}" >/dev/null
  marker_line="$(grep -n -m1 '"event":"auto-trigger-start"' "${log}" | cut -d: -f1)"
  first_partial_line="$(grep -n -m1 '"event":"daemon-partial"' "${log}" | cut -d: -f1)"
  partial_count="$(grep -Fc '"event":"daemon-partial"' "${log}")"
  if [[ -z "${marker_line}" || -z "${first_partial_line}" ||
    "${marker_line}" -ge "${first_partial_line}" || "${partial_count}" -lt 3 ]]; then
    echo "Chromium automatic trigger did not precede a complete partial sequence" >&2
    status=1
  fi
  jq -s -e --arg key "$( [[ "${mode}" == command ]] && echo F10 || echo F9 )" '
    length == 2 and all(.[]; .event == "uinput-key" and .key == $key and .ok == true)
  ' "${uinput_log}" >/dev/null
  jq -e '.event == "window-focus" and .backend == "niri" and .focused == true and .ok == true' "${focus_log}" >/dev/null
  jq -e '
    .event == "renderer-sandbox" and
    .browser_no_sandbox_flag == false and
    .no_new_privs == 1 and
    .seccomp == 2 and
    .cap_eff == "0000000000000000" and
    .nspid_depth >= 2 and
    .ok == true
  ' "${sandbox_log}" >/dev/null
  if [[ "${mode}" == command ]]; then
    jq -e '
      .event == "primary-selection-fallback" and
      .selections_are_distinct == true and
      .restored == true and
      .ok == true
    ' "${primary_log}" >/dev/null
  fi
  if browser_pid | grep -q .; then
    echo "Chromium process residue remains for the temporary profile" >&2
    status=1
  fi
fi

exit "${status}"
