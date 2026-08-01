#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

wav_path="${VINPUT_LIVE_NATIVE_WAV:-}"
modes="${VINPUT_LIVE_NATIVE_MODES:-normal}"
probe_kind="${VINPUT_LIVE_VIRTUAL_PROBE_KIND:-fcitx}"
toolkit_mode="${VINPUT_LIVE_TOOLKIT_MODE:-normal}"
toolkit_cycles="${VINPUT_TOOLKIT_EXPECTED_CYCLES:-1}"
reload_before_probe="${VINPUT_LIVE_RELOAD_BEFORE_PROBE:-0}"
require_partial="${VINPUT_LIVE_REQUIRE_PARTIAL:-1}"
env_file="${VINPUT_LIVE_ENV_FILE:-${HOME}/.local/share/fcitx-vinput/fcitx-vinput.env}"
cli_binary="${VINPUT_LIVE_CLI_BINARY:-target/debug/vinput}"
out_dir="${VINPUT_LIVE_VIRTUAL_OUT_DIR:-target/tmp/ime-fcitx-virtual-source-live}"
node_prefix="vinput_e2e_${$}"
sink_name="${node_prefix}_sink"
source_name="${node_prefix}_source"
loopback_pid=""
record_pid=""
config_path=""
profile_mutated=0
backup_existed=0
primary_restore_needed=0
primary_restore_present=0
primary_restore_path=""
primary_restore_proven=false

call_service() {
  gdbus call --session \
    --dest org.fcitx.Vinput \
    --object-path /org/fcitx/Vinput \
    --method "org.fcitx.Vinput.Service.$1" "${@:2}"
}

stop_verified_owner() {
  local status pid exe cmdline proc_exe proc_cmdline
  status="$("${cli_binary}" daemon status --json 2>/dev/null || true)"
  pid="$(jq -r '.owner.unix_process_id // empty' <<<"${status}")"
  [[ -z "${pid}" ]] && return 0
  exe="$(jq -r '.owner.process.exe // empty' <<<"${status}")"
  cmdline="$(jq -r '.owner.process.cmdline | join(" ")' <<<"${status}")"
  if [[ "${exe}" != *vinput-daemon* || "${cmdline}" != *"${config_path}"* ]]; then
    echo "refusing to stop unexpected org.fcitx.Vinput owner: pid=${pid} exe=${exe}" >&2
    return 1
  fi
  proc_exe="$(readlink "/proc/${pid}/exe")"
  proc_cmdline="$(tr '\0' ' ' <"/proc/${pid}/cmdline")"
  if [[ "${proc_exe}" != *vinput-daemon* || "${proc_cmdline}" != *"${config_path}"* ]]; then
    echo "live owner changed during verification: pid=${pid} exe=${proc_exe}" >&2
    return 1
  fi
  if [[ "$(jq -r '.status // empty' <<<"${status}")" != "idle" ]]; then
    call_service StopRecording "" >/dev/null 2>&1 || true
    sleep 0.5
  fi
  kill -TERM "${pid}"
  for _ in $(seq 1 100); do
    if ! kill -0 "${pid}" 2>/dev/null; then
      return 0
    fi
    sleep 0.05
  done
  echo "verified daemon did not terminate after SIGTERM: ${pid}" >&2
  return 1
}

activate_and_wait() {
  call_service GetStatus >/dev/null
  for _ in $(seq 1 200); do
    if "${cli_binary}" daemon status --json >"${out_dir}/status-current.json" 2>/dev/null &&
      jq -e \
        --arg config_path "${config_path}" '
          .status == "idle" and
          .owner.ok == true and
          (.owner.process.exe | endswith("vinput-daemon")) and
          (.owner.process.cmdline | index($config_path)) != null
        ' "${out_dir}/status-current.json" >/dev/null; then
      return 0
    fi
    sleep 0.05
  done
  echo "D-Bus activation did not restore an idle verified daemon" >&2
  cat "${out_dir}/status-current.json" >&2 2>/dev/null || true
  return 1
}

restore_profile() {
  [[ "${profile_mutated}" == 0 ]] && return 0
  stop_verified_owner
  install -m 0644 "${out_dir}/config-before.json" "${config_path}"
  if [[ "${backup_existed}" == 1 ]]; then
    install -m 0644 "${out_dir}/config-backup-before.json" "${config_path}.bak"
  else
    rm -f "${config_path}.bak"
  fi
  activate_and_wait
  profile_mutated=0
}

restore_primary_after_gate() {
  local current runtime_dir
  [[ "${primary_restore_needed}" == 1 ]] || return 0
  if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
    runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
    mapfile -t wayland_sockets < <(
      find "${runtime_dir}" -maxdepth 1 -type s -name 'wayland-*' -printf '%f\n' 2>/dev/null
    )
    if [[ "${#wayland_sockets[@]}" != 1 ]]; then
      echo "expected exactly one Wayland socket for primary-selection restoration" >&2
      return 1
    fi
    export WAYLAND_DISPLAY="${wayland_sockets[0]}"
  fi
  if [[ "${primary_restore_present}" == 1 ]]; then
    wl-copy --primary --type 'text/plain;charset=utf-8' <"${primary_restore_path}" >/dev/null 2>&1
    for _ in $(seq 1 50); do
      if timeout 1s wl-paste --primary --no-newline >"${out_dir}/primary-final.txt" 2>/dev/null &&
        cmp -s "${primary_restore_path}" "${out_dir}/primary-final.txt"; then
        primary_restore_needed=0
        primary_restore_proven=true
        return 0
      fi
      sleep 0.05
    done
    echo "failed to restore Chromium primary-selection bytes after outer cleanup" >&2
    return 1
  fi
  wl-copy --primary --clear
  for _ in $(seq 1 50); do
    if ! timeout 1s wl-paste --primary --no-newline >"${out_dir}/primary-final.txt" 2>/dev/null; then
      primary_restore_needed=0
      primary_restore_proven=true
      return 0
    fi
    current="$(cat "${out_dir}/primary-final.txt")"
    if [[ -z "${current}" ]]; then
      primary_restore_needed=0
      primary_restore_proven=true
      return 0
    fi
    sleep 0.05
  done
  echo "failed to clear Chromium primary selection after outer cleanup" >&2
  return 1
}

cleanup() {
  local exit_code=$?
  trap - EXIT
  set +e
  if [[ -n "${record_pid}" ]] && kill -0 "${record_pid}" 2>/dev/null; then
    kill -INT "${record_pid}" 2>/dev/null || true
    wait "${record_pid}" 2>/dev/null || true
  fi
  restore_profile || true
  if [[ -n "${loopback_pid}" ]] && kill -0 "${loopback_pid}" 2>/dev/null; then
    kill -TERM "${loopback_pid}" 2>/dev/null || true
    wait "${loopback_pid}" 2>/dev/null || true
  fi
  if ! restore_primary_after_gate; then
    exit_code=1
  fi
  exit "${exit_code}"
}
trap cleanup EXIT

if [[ -z "${wav_path}" || ! -f "${wav_path}" ]]; then
  echo "set VINPUT_LIVE_NATIVE_WAV to a validated speech WAV" >&2
  exit 2
fi
if [[ -f "${env_file}" ]]; then
  # shellcheck disable=SC1090
  . "${env_file}"
fi
for command in fcitx5-remote gdbus jq pw-cli pw-loopback pw-play pw-record python3; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required virtual-source command is missing: ${command}" >&2
    exit 2
  fi
done
if [[ ! -x "${cli_binary}" ]]; then
  echo "virtual-source CLI is missing or not executable: ${cli_binary}" >&2
  exit 2
fi
if ! fcitx5-remote --check; then
  echo "Fcitx5 is not running in the current desktop session" >&2
  exit 2
fi

if [[ "${require_partial}" != "0" && "${require_partial}" != "1" ]]; then
  echo "VINPUT_LIVE_REQUIRE_PARTIAL must be 0 or 1" >&2
  exit 2
fi
if [[ "${VINPUT_LIVE_NATIVE_OWNER_LOSS:-0}" != 0 && "${require_partial}" == "0" ]]; then
  echo "owner-loss validation requires streaming partial evidence" >&2
  exit 2
fi
case "${probe_kind}" in
fcitx) ;;
gtk4 | gnome-text-editor | kitty | chromium)
  case "${toolkit_mode}" in
  normal | command) ;;
  *)
    echo "VINPUT_LIVE_TOOLKIT_MODE must be normal or command" >&2
    exit 2
    ;;
  esac
  ;;
*)
  echo "VINPUT_LIVE_VIRTUAL_PROBE_KIND must be fcitx, gtk4, gnome-text-editor, kitty, or chromium" >&2
  exit 2
  ;;
esac
if [[ "${probe_kind}" == gtk4 ]] &&
  [[ ! "${toolkit_cycles}" =~ ^[0-9]+$ || "${toolkit_cycles}" -lt 1 ||
    "${toolkit_cycles}" -gt 20 ]]; then
  echo "VINPUT_TOOLKIT_EXPECTED_CYCLES must be an integer from 1 to 20" >&2
  exit 2
fi
if [[ "${probe_kind}" == chromium && "${toolkit_mode}" == command ]]; then
  for command in timeout wl-copy wl-paste; do
    if ! command -v "${command}" >/dev/null 2>&1; then
      echo "required Chromium primary-restoration command is missing: ${command}" >&2
      exit 2
    fi
  done
fi

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
"${cli_binary}" daemon status --json >"${out_dir}/status-before.json"
if ! jq -e '.status == "idle" and .owner.ok == true' "${out_dir}/status-before.json" >/dev/null; then
  echo "daemon must be idle before virtual-source validation" >&2
  cat "${out_dir}/status-before.json" >&2
  exit 1
fi
config_path="$(jq -r '
  .owner.process.cmdline as $cmd |
  ($cmd | index("--config")) as $index |
  if $index == null then "" else $cmd[$index + 1] end
' "${out_dir}/status-before.json")"
if [[ -z "${config_path}" || ! -f "${config_path}" ]]; then
  echo "failed to resolve the live daemon config path" >&2
  exit 1
fi
install -m 0644 "${config_path}" "${out_dir}/config-before.json"
if [[ -f "${config_path}.bak" ]]; then
  backup_existed=1
  install -m 0644 "${config_path}.bak" "${out_dir}/config-backup-before.json"
fi
before_pid="$(jq -r '.owner.unix_process_id' "${out_dir}/status-before.json")"
before_capture="$(jq -r '.global.capture_device' "${out_dir}/config-before.json")"

pw-loopback \
  --name "${node_prefix}_loopback" \
  --channels 1 \
  --channel-map '[ MONO ]' \
  --capture-props "media.class=Audio/Sink node.name=${sink_name} node.description=\"Vinput E2E Sink\" audio.position=[ MONO ]" \
  --playback-props "media.class=Audio/Source node.name=${source_name} node.description=\"Vinput E2E Source\" audio.position=[ MONO ]" \
  >"${out_dir}/pw-loopback.log" 2>&1 &
loopback_pid=$!
for _ in $(seq 1 100); do
  if pw-cli info "${sink_name}" >/dev/null 2>&1 &&
    pw-cli info "${source_name}" >/dev/null 2>&1; then
    break
  fi
  sleep 0.05
done
if ! pw-cli info "${sink_name}" >"${out_dir}/sink-info.txt" 2>&1 ||
  ! pw-cli info "${source_name}" >"${out_dir}/source-info.txt" 2>&1; then
  echo "virtual PipeWire sink/source did not appear" >&2
  exit 1
fi

preflight_wav="${out_dir}/virtual-source-preflight.wav"
pw-record \
  --target "${source_name}" \
  --rate 16000 \
  --channels 1 \
  --channel-map MONO \
  --format s16 \
  "${preflight_wav}" >"${out_dir}/pw-record.log" 2>&1 &
record_pid=$!
sleep 0.5
pw-play --target "${sink_name}" "${wav_path}" >"${out_dir}/pw-play-preflight.log" 2>&1
sleep 0.3
kill -INT "${record_pid}" 2>/dev/null || true
wait "${record_pid}" 2>/dev/null || true
record_pid=""
python3 - "${preflight_wav}" >"${out_dir}/preflight.json" <<'PY'
import array
import json
import sys
import wave
from pathlib import Path

path = Path(sys.argv[1])
with wave.open(str(path), "rb") as wav:
    channels = wav.getnchannels()
    sample_rate = wav.getframerate()
    width = wav.getsampwidth()
    frames = wav.getnframes()
    payload = wav.readframes(frames)
if width != 2:
    raise SystemExit(f"expected 16-bit PCM, found sample width {width}")
samples = array.array("h")
samples.frombytes(payload)
if sys.byteorder != "little":
    samples.byteswap()
peak = max((abs(value) for value in samples), default=0)
nonzero = sum(value != 0 for value in samples)
summary = {
    "path": str(path),
    "channels": channels,
    "sample_rate": sample_rate,
    "frames": frames,
    "peak": peak,
    "nonzero_samples": nonzero,
    "ok": channels == 1 and sample_rate == 16000 and peak >= 512 and nonzero >= 1000,
}
print(json.dumps(summary, ensure_ascii=False))
if not summary["ok"]:
    raise SystemExit("virtual source preflight captured silence or an invalid format")
PY

profile_mutated=1
"${cli_binary}" device use "${source_name}" \
  --config "${config_path}" \
  --in-place \
  --json | tee "${out_dir}/device-use.json"
jq -e --arg source_name "${source_name}" \
  '.global.capture_device == $source_name' "${config_path}" >/dev/null
stop_verified_owner
activate_and_wait
virtual_pid="$(jq -r '.owner.unix_process_id' "${out_dir}/status-current.json")"
if [[ "${virtual_pid}" == "${before_pid}" ]]; then
  echo "daemon PID did not change after selecting the virtual source" >&2
  exit 1
fi

reload_proven=false
if [[ "${reload_before_probe}" != 0 ]]; then
  cp "${out_dir}/status-current.json" "${out_dir}/reload-before.json"
  reload_provider="$(jq -r '.asr_backend.effective_provider_id' "${out_dir}/reload-before.json")"
  reload_model="$(jq -r '.asr_backend.effective_model_id' "${out_dir}/reload-before.json")"
  "${cli_binary}" daemon reload-asr --json | tee "${out_dir}/reload-call.json"
  reload_ready=0
  for _ in $(seq 1 300); do
    if "${cli_binary}" daemon status --json >"${out_dir}/reload-after.json" 2>/dev/null &&
      jq -e \
        --argjson virtual_pid "${virtual_pid}" \
        --arg provider "${reload_provider}" \
        --arg model "${reload_model}" '
          .status == "idle" and
          .owner.ok == true and
          .owner.unix_process_id == $virtual_pid and
          .asr_backend.has_effective_backend == true and
          .asr_backend.reload_in_progress == false and
          .asr_backend.last_error == "" and
          .asr_backend.effective_provider_id == $provider and
          .asr_backend.effective_model_id == $model
        ' "${out_dir}/reload-after.json" >/dev/null; then
      reload_ready=1
      break
    fi
    sleep 0.1
  done
  if [[ "${reload_ready}" != 1 ]]; then
    echo "ASR reload did not return to the same ready provider/model" >&2
    cat "${out_dir}/reload-after.json" >&2 2>/dev/null || true
    exit 1
  fi
  reload_proven=true
fi

if [[ "${probe_kind}" == fcitx ]]; then
  VINPUT_LIVE_NATIVE_WAV="${wav_path}" \
  VINPUT_LIVE_NATIVE_MODES="${modes}" \
  VINPUT_LIVE_REQUIRE_PARTIAL="${require_partial}" \
  VINPUT_LIVE_PLAYBACK_TARGET="${sink_name}" \
  VINPUT_LIVE_NATIVE_OUT_DIR="${out_dir}/fcitx" \
    scripts/run-ime-fcitx-native-live.sh

  for mode in $(tr ',' ' ' <<<"${modes}"); do
    if [[ "${VINPUT_LIVE_EXPECT_UNCHANGED_ON_ERROR:-0}" != 0 ]]; then
      jq -s -e --arg selected "${VINPUT_LIVE_SELECTED_TEXT:-}" '
        any(.[];
          .event == "summary" and
          .ok == true and
          .expect_unchanged_on_error == true and
          .selection_source == "surrounding" and
          .selected_text == $selected and
          .commit == "" and
          .delete_count == 0 and
          .final_buffer == $selected
        )
      ' "${out_dir}/fcitx/${mode}.jsonl" >/dev/null
    elif [[ "${VINPUT_LIVE_NATIVE_OWNER_LOSS:-0}" != 0 ]]; then
      jq -s -e '
        any(.[];
          .event == "summary" and
          .ok == true and
          .require_partial == true and
          .partial_count > 0 and
          .owner_loss == true and
          .owner_loss_preedit_count > 0 and
          (.owner_loss_preedit | ascii_downcase | contains("unavailable")) and
          .commit == ""
        )
      ' "${out_dir}/fcitx/${mode}.jsonl" >/dev/null
    else
      jq -s -e --argjson require_partial "$( [[ "${require_partial}" == "1" ]] && echo true || echo false )" '
        any(.[];
          .event == "summary" and
          .ok == true and
          .require_partial == $require_partial and
          ($require_partial == false or .partial_count > 0) and
          (.commit | length) > 0
        )
      ' "${out_dir}/fcitx/${mode}.jsonl" >/dev/null
    fi
  done
elif [[ "${probe_kind}" == gtk4 ]]; then
  toolkit_expected="${VINPUT_TOOLKIT_EXPECTED_COMMIT_SUBSTRING:-}"
  if [[ "${toolkit_mode}" == command && -z "${toolkit_expected}" ]]; then
    toolkit_expected="adapter-backed: selected text"
  fi
  VINPUT_LIVE_TOOLKIT_WAV="${wav_path}" \
  VINPUT_LIVE_TOOLKIT_PLAYBACK_TARGET="${sink_name}" \
  VINPUT_LIVE_TOOLKIT_AUTO_TRIGGER=1 \
  VINPUT_LIVE_TOOLKIT_OUT_DIR="${out_dir}/gtk4" \
  VINPUT_TOOLKIT_REQUIRE_PARTIAL="${require_partial}" \
  VINPUT_TOOLKIT_EXPECTED_CYCLES="${toolkit_cycles}" \
  VINPUT_TOOLKIT_EXPECTED_COMMIT_SUBSTRING="${toolkit_expected}" \
    scripts/run-ime-gtk4-native-live.sh "${toolkit_mode}"
  jq -s -e --arg mode "${toolkit_mode}" --argjson cycles "${toolkit_cycles}" '
    any(.[];
      .event == "summary" and
      .toolkit == "gtk4" and
      .mode == $mode and
      .partial == true and
      .commit == true and
      .replacement == ($mode == "command") and
      .completed_cycles == $cycles and
      .expected_cycles == $cycles and
      .timed_out == false and
      .ok == true
    )
  ' "${out_dir}/gtk4/${toolkit_mode}.jsonl" >/dev/null
  toolkit_key=F9
  if [[ "${toolkit_mode}" == command ]]; then
    toolkit_key=F10
  fi
  jq -s -e --arg key "${toolkit_key}" --argjson expected "$((toolkit_cycles * 2))" '
    length == $expected and
    all(.[]; .event == "uinput-key" and .key == $key and .ok == true)
  ' "${out_dir}/gtk4/${toolkit_mode}.uinput.jsonl" >/dev/null
  jq -e '
    .event == "window-focus" and
    .backend == "niri" and
    .focused == true and
    .ok == true
  ' "${out_dir}/gtk4/${toolkit_mode}.focus.json" >/dev/null
elif [[ "${probe_kind}" == gnome-text-editor ]]; then
  VINPUT_LIVE_TOOLKIT_WAV="${wav_path}" \
  VINPUT_LIVE_TOOLKIT_PLAYBACK_TARGET="${sink_name}" \
  VINPUT_LIVE_TOOLKIT_OUT_DIR="${out_dir}/gnome-text-editor" \
    scripts/run-ime-gnome-text-editor-live.sh "${toolkit_mode}"
  jq -e --arg mode "${toolkit_mode}" '
    .event == "summary" and
    .application == "gnome-text-editor" and
    .mode == $mode and
    .partial_count > 0 and
    .replacement == ($mode == "command") and
    .saved == true and
    .ok == true
  ' "${out_dir}/gnome-text-editor/${toolkit_mode}.summary.json" >/dev/null
elif [[ "${probe_kind}" == kitty ]]; then
  VINPUT_LIVE_TOOLKIT_WAV="${wav_path}" \
  VINPUT_LIVE_TOOLKIT_PLAYBACK_TARGET="${sink_name}" \
  VINPUT_LIVE_TOOLKIT_OUT_DIR="${out_dir}/kitty" \
    scripts/run-ime-kitty-live.sh "${toolkit_mode}"
  jq -e --arg mode "${toolkit_mode}" '
    .event == "summary" and
    .application == "kitty" and
    .mode == $mode and
    .partial_count > 0 and
    .primary_selection_fallback == ($mode == "command") and
    .written == true and
    .ok == true
  ' "${out_dir}/kitty/${toolkit_mode}.summary.json" >/dev/null
else
  VINPUT_LIVE_TOOLKIT_WAV="${wav_path}" \
  VINPUT_LIVE_TOOLKIT_PLAYBACK_TARGET="${sink_name}" \
  VINPUT_LIVE_TOOLKIT_OUT_DIR="${out_dir}/chromium" \
    scripts/run-ime-chromium-virtual-live.sh "${toolkit_mode}"
  jq -s -e --arg mode "${toolkit_mode}" '
    any(.[];
      .event == "summary" and
      .toolkit == "chromium" and
      .mode == $mode and
      .partial == true and
      .commit == true and
      .replacement == ($mode == "command") and
      .selection_ready == ($mode == "command") and
      .timed_out == false and
      .ok == true)
  ' "${out_dir}/chromium/${toolkit_mode}.jsonl" >/dev/null
  jq -e '
    .event == "renderer-sandbox" and
    .browser_no_sandbox_flag == false and
    .no_new_privs == 1 and
    .seccomp == 2 and
    .cap_eff == "0000000000000000" and
    .nspid_depth >= 2 and
    .ok == true
  ' "${out_dir}/chromium/${toolkit_mode}.sandbox.json" >/dev/null
  if [[ "${toolkit_mode}" == command ]]; then
    primary_restore_path="${out_dir}/chromium/command.primary-before.txt"
    test -f "${primary_restore_path}"
    primary_restore_present="$(
      jq -r 'if .previous_selection_present then 1 else 0 end' \
        "${out_dir}/chromium/command.primary-selection.json"
    )"
    primary_restore_needed=1
  fi
fi

same_daemon_owner=false
if [[ "${probe_kind}" == gtk4 ]]; then
  "${cli_binary}" daemon status --json >"${out_dir}/status-after-probe.json"
  jq -e --argjson virtual_pid "${virtual_pid}" '
    .status == "idle" and
    .runtime_status.active_session == false and
    .owner.ok == true and
    .owner.unix_process_id == $virtual_pid
  ' "${out_dir}/status-after-probe.json" >/dev/null
  same_daemon_owner=true
fi

restore_profile
cmp "${out_dir}/config-before.json" "${config_path}"
if [[ "${backup_existed}" == 1 ]]; then
  cmp "${out_dir}/config-backup-before.json" "${config_path}.bak"
else
  test ! -e "${config_path}.bak"
fi
restored_pid="$(jq -r '.owner.unix_process_id' "${out_dir}/status-current.json")"
restored_capture="$(jq -r '.global.capture_device' "${config_path}")"
if [[ "${restored_capture}" != "${before_capture}" ]]; then
  echo "capture target was not restored" >&2
  exit 1
fi

restore_primary_after_gate

jq -n \
  --arg sink "${sink_name}" \
  --arg source "${source_name}" \
  --arg probe_kind "${probe_kind}" \
  --arg before_capture "${before_capture}" \
  --argjson before_pid "${before_pid}" \
  --argjson virtual_pid "${virtual_pid}" \
  --argjson restored_pid "${restored_pid}" \
  --argjson reload_proven "${reload_proven}" \
  --argjson require_partial "$( [[ "${require_partial}" == "1" ]] && echo true || echo false )" \
  --argjson primary_selection_restored "${primary_restore_proven}" \
  --argjson same_daemon_owner "${same_daemon_owner}" \
  --slurpfile preflight "${out_dir}/preflight.json" \
  '{
    event: "summary",
    probe_kind: $probe_kind,
    route: {sink: $sink, source: $source},
    preflight: $preflight[0],
    before_capture: $before_capture,
    before_pid: $before_pid,
    virtual_pid: $virtual_pid,
    restored_pid: $restored_pid,
    profile_restored: true,
    physical_speaker_or_microphone_used: false,
    reload_before_probe: $reload_proven,
    require_partial: $require_partial,
    primary_selection_restored: $primary_selection_restored,
    same_daemon_owner: $same_daemon_owner,
    ok: true
  }' | tee "${out_dir}/summary.json"
