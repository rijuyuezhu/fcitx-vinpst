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

wav_path="${VINPUT_LIVE_NATIVE_WAV:-}"
env_file="${VINPUT_LIVE_ENV_FILE:-${HOME}/.local/share/fcitx-vinput/fcitx-vinput.env}"
cli_binary="${VINPUT_LIVE_CLI_BINARY:-target/debug/vinput}"
out_dir="${VINPUT_LIVE_RELOAD_OUT_DIR:-target/tmp/ime-fcitx-reload-live}"
recognition_dir="${out_dir}/recognition"

if [[ -z "${wav_path}" || ! -f "${wav_path}" ]]; then
  echo "set VINPUT_LIVE_NATIVE_WAV to a validated speech WAV" >&2
  exit 2
fi
if [[ -f "${env_file}" ]]; then
  # shellcheck disable=SC1090
  . "${env_file}"
fi
for command in jq gdbus; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required reload-probe command is missing: ${command}" >&2
    exit 2
  fi
done
if [[ ! -x "${cli_binary}" ]]; then
  echo "live reload CLI is missing or not executable: ${cli_binary}" >&2
  exit 2
fi

rm -rf "${out_dir}"
mkdir -p "${out_dir}"


"${cli_binary}" daemon status --json >"${out_dir}/before.json"
if ! jq -e '
  .status == "idle" and
  .owner.ok == true and
  .asr_backend.has_effective_backend == true and
  .asr_backend.reload_in_progress == false and
  .asr_backend.last_error == ""
' "${out_dir}/before.json" >/dev/null; then
  echo "ASR backend must be idle and ready before reload" >&2
  cat "${out_dir}/before.json" >&2
  exit 1
fi

before_pid="$(jq -r '.owner.unix_process_id' "${out_dir}/before.json")"
before_provider="$(jq -r '.asr_backend.effective_provider_id' "${out_dir}/before.json")"
before_model="$(jq -r '.asr_backend.effective_model_id' "${out_dir}/before.json")"

"${cli_binary}" daemon reload-asr --json | tee "${out_dir}/reload-call.json"

reload_complete=""
for _ in $(seq 1 300); do
  if "${cli_binary}" daemon status --json >"${out_dir}/after.json" 2>/dev/null &&
    jq -e '
      .status == "idle" and
      .owner.ok == true and
      .asr_backend.has_effective_backend == true and
      .asr_backend.reload_in_progress == false and
      .asr_backend.last_error == ""
    ' "${out_dir}/after.json" >/dev/null; then
    reload_complete="1"
    break
  fi
  sleep 0.1
done
if [[ -z "${reload_complete}" ]]; then
  echo "ASR reload did not return to an idle ready backend" >&2
  cat "${out_dir}/after.json" >&2 || true
  exit 1
fi

if ! jq -e \
  --argjson before_pid "${before_pid}" \
  --arg before_provider "${before_provider}" \
  --arg before_model "${before_model}" '
    .owner.unix_process_id == $before_pid and
    .asr_backend.effective_provider_id == $before_provider and
    .asr_backend.effective_model_id == $before_model
  ' "${out_dir}/after.json" >/dev/null; then
  echo "reload changed the owner, provider, or model unexpectedly" >&2
  cat "${out_dir}/after.json" >&2
  exit 1
fi

VINPUT_LIVE_NATIVE_WAV="${wav_path}" \
VINPUT_LIVE_NATIVE_MODES=normal \
VINPUT_LIVE_NATIVE_OUT_DIR="${recognition_dir}" \
  scripts/live/niri/run-ime-fcitx-native-live.sh

jq -e 'select(.event == "summary" and .ok == true and .partial_count > 0 and (.commit | length) > 0)' \
  "${recognition_dir}/normal.jsonl" >/dev/null

jq -n \
  --argjson owner_pid "${before_pid}" \
  --arg provider "${before_provider}" \
  --arg model "${before_model}" \
  --arg evidence "${recognition_dir}/normal.jsonl" \
  '{
    event: "summary",
    owner_pid: $owner_pid,
    provider: $provider,
    model: $model,
    reload_completed: true,
    post_reload_recognition: true,
    evidence: $evidence,
    ok: true
  }' | tee "${out_dir}/summary.json"
