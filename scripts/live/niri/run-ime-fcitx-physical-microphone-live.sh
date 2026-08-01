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

probe="${repo_root}/scripts/live/niri/probes/fcitx-live-client-probe.py"
out_dir="${VINPUT_LIVE_PHYSICAL_MIC_OUT_DIR:-${repo_root}/target/tmp/ime-fcitx-physical-microphone-live}"
profile_path="${VINPUT_LIVE_FCITX_ADDON_CONFIG:-${HOME}/.local/share/fcitx-vinput/sherpa-native-command-live.json}"
service_path="${HOME}/.local/share/dbus-1/services/org.fcitx.Vinput.service"
addon_config="${HOME}/.config/fcitx5/conf/vinput.conf"
cli_binary="${repo_root}/target/debug/vinput"
recording_ms="${VINPUT_LIVE_PHYSICAL_MIC_RECORDING_MS:-20000}"
start_delay_ms="${VINPUT_LIVE_PHYSICAL_MIC_START_DELAY_MS:-8000}"
result_timeout_ms="${VINPUT_LIVE_PHYSICAL_MIC_RESULT_TIMEOUT_MS:-20000}"

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

cleanup() {
  local exit_code=$?
  set +e
  restore_idle
  find scripts -type d -name __pycache__ -prune -exec rm -rf {} +
  trap - EXIT INT TERM
  exit "${exit_code}"
}
trap cleanup EXIT INT TERM

for command in fcitx5-remote gdbus grep jq pgrep python3 ruff sed wpctl; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required physical-microphone command is missing: ${command}" >&2
    exit 2
  fi
done
for required in "${probe}" "${profile_path}" "${service_path}" "${addon_config}" "${cli_binary}"; do
  if [[ ! -e "${required}" ]]; then
    echo "required physical-microphone path is missing: ${required}" >&2
    exit 2
  fi
done
if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  echo "DBUS_SESSION_BUS_ADDRESS is not set" >&2
  exit 2
fi
if ! [[ "${recording_ms}" =~ ^[0-9]+$ ]] || ((recording_ms < 5000)); then
  echo "VINPUT_LIVE_PHYSICAL_MIC_RECORDING_MS must be an integer >= 5000" >&2
  exit 2
fi
if ! [[ "${start_delay_ms}" =~ ^[0-9]+$ ]] || ((start_delay_ms < 1000)); then
  echo "VINPUT_LIVE_PHYSICAL_MIC_START_DELAY_MS must be an integer >= 1000" >&2
  exit 2
fi
if ! [[ "${result_timeout_ms}" =~ ^[0-9]+$ ]] || ((result_timeout_ms < 1000)); then
  echo "VINPUT_LIVE_PHYSICAL_MIC_RESULT_TIMEOUT_MS must be an integer >= 1000" >&2
  exit 2
fi
if ! fcitx5-remote --check >/dev/null 2>&1; then
  echo "Fcitx5 is not running in the current desktop session" >&2
  exit 2
fi

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
cp -a "${profile_path}" "${out_dir}/profile-before.json"
cp -a "${service_path}" "${out_dir}/service-before.service"
cp -a "${addon_config}" "${out_dir}/addon-config-before.conf"
"${cli_binary}" daemon status --json >"${out_dir}/status-before.json"
wpctl inspect @DEFAULT_AUDIO_SOURCE@ >"${out_dir}/default-source.txt"

capture_device="$(jq -r '.global.capture_device' "${profile_path}")"
source_name="$(sed -n 's/^  \* node.name = "\(.*\)"$/\1/p' "${out_dir}/default-source.txt")"
source_description="$(sed -n 's/^  \* node.description = "\(.*\)"$/\1/p' "${out_dir}/default-source.txt")"
source_class="$(sed -n 's/^  \* media.class = "\(.*\)"$/\1/p' "${out_dir}/default-source.txt")"
device_api="$(sed -n 's/^    device.api = "\(.*\)"$/\1/p' "${out_dir}/default-source.txt")"

if [[ "${capture_device}" != "default" ]]; then
  echo "physical-microphone gate expects capture_device=default, found ${capture_device}" >&2
  exit 2
fi
if [[ "${source_class}" != "Audio/Source" || "${device_api}" != "alsa" ]]; then
  cat "${out_dir}/default-source.txt" >&2
  echo "default source is not a physical ALSA Audio/Source" >&2
  exit 2
fi
if [[ "${source_name}" != alsa_input.* ]] ||
  [[ "${source_name,,}" == *monitor* ]] ||
  [[ "${source_name,,}" == *virtual* ]] ||
  [[ "${source_name,,}" == *vinput* ]]; then
  echo "default source does not look like a physical microphone: ${source_name}" >&2
  exit 2
fi
if ! grep -Fq -- '--audio-backend pipewire' "${service_path}"; then
  cat "${service_path}" >&2
  echo "activation service is not configured for the PipeWire backend" >&2
  exit 2
fi
if grep -Eq -- ' --wav( |$)|--audio-backend (mock|wav)' "${service_path}"; then
  cat "${service_path}" >&2
  echo "physical-microphone gate refuses mock or WAV-backed activation" >&2
  exit 2
fi
if ! jq -e \
  --arg provider sherpa-onnx \
  '.status == "idle" and
   .asr_backend.reload_in_progress == false and
   .asr_backend.last_error == "" and
   .asr_backend.effective_provider_id == $provider and
   ((.owner.process.cmdline | index("pipewire")) != null)' \
  "${out_dir}/status-before.json" >/dev/null; then
  cat "${out_dir}/status-before.json" >&2
  echo "daemon is not idle on the expected PipeWire/native backend" >&2
  exit 2
fi

ruff check "${probe}"
ruff format --check "${probe}"
python3 -m py_compile "${probe}"

jq -n \
  --arg event ready \
  --arg source_name "${source_name}" \
  --arg source_description "${source_description}" \
  --argjson starts_after_ms "${start_delay_ms}" \
  --argjson recording_ms "${recording_ms}" \
  '{
    event: $event,
    source_name: $source_name,
    source_description: $source_description,
    starts_after_ms: $starts_after_ms,
    recording_ms: $recording_ms,
    instruction: "Speak one clear Chinese sentence into the physical microphone."
  }' | tee "${out_dir}/ready.json"

python3 "${probe}" \
  --mode normal \
  --manual-recording-ms "${recording_ms}" \
  --start-delay-ms "${start_delay_ms}" \
  --result-timeout-ms "${result_timeout_ms}" \
  --require-partial \
  | tee "${out_dir}/physical-microphone.jsonl"

probe_commit="$(jq -r 'select(.event == "summary") | .commit' "${out_dir}/physical-microphone.jsonl")"
partial_count="$(jq -r 'select(.event == "summary") | .partial_count' "${out_dir}/physical-microphone.jsonl")"
if ! jq -s -e \
  'any(.[]; .event == "summary" and .ok == true and .manual_speech == true and .manual_recording_ms > 0 and .require_partial == true and .partial_count > 0 and (.commit | length) > 0)' \
  "${out_dir}/physical-microphone.jsonl" >/dev/null; then
  cat "${out_dir}/physical-microphone.jsonl" >&2
  echo "physical microphone probe did not produce partials and a final commit" >&2
  exit 1
fi

cmp "${out_dir}/profile-before.json" "${profile_path}"
cmp "${out_dir}/service-before.service" "${service_path}"
cmp "${out_dir}/addon-config-before.conf" "${addon_config}"
"${cli_binary}" daemon status --json >"${out_dir}/status-after.json"
if ! jq -e \
  --arg provider "$(jq -r '.asr_backend.effective_provider_id' "${out_dir}/status-before.json")" \
  --arg model "$(jq -r '.asr_backend.effective_model_id' "${out_dir}/status-before.json")" \
  '.status == "idle" and
   .asr_backend.reload_in_progress == false and
   .asr_backend.last_error == "" and
   .asr_backend.effective_provider_id == $provider and
   .asr_backend.effective_model_id == $model and
   .asr_backend.target_provider_id == $provider and
   .asr_backend.target_model_id == $model' \
  "${out_dir}/status-after.json" >/dev/null; then
  cat "${out_dir}/status-after.json" >&2
  echo "physical microphone proof changed daemon backend state" >&2
  exit 1
fi

jq -n \
  --arg event summary \
  --arg source_name "${source_name}" \
  --arg source_description "${source_description}" \
  --arg source_class "${source_class}" \
  --arg device_api "${device_api}" \
  --arg capture_device "${capture_device}" \
  --arg commit "${probe_commit}" \
  --argjson partial_count "${partial_count}" \
  --argjson recording_ms "${recording_ms}" \
  '{
    event: $event,
    source_name: $source_name,
    source_description: $source_description,
    source_class: $source_class,
    device_api: $device_api,
    capture_device: $capture_device,
    recording_ms: $recording_ms,
    partial_count: $partial_count,
    commit: $commit,
    physical_microphone_used: true,
    manual_speech: true,
    playback_used: false,
    profile_unchanged: true,
    service_unchanged: true,
    addon_config_unchanged: true,
    backend_unchanged: true,
    ok: true
  }' | tee "${out_dir}/summary.json"
