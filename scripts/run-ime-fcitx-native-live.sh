#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

wav_path="${VINPUT_LIVE_NATIVE_WAV:-}"
selected_text="${VINPUT_LIVE_SELECTED_TEXT:-selected text}"
modes="${VINPUT_LIVE_NATIVE_MODES:-normal,command}"
env_file="${VINPUT_LIVE_ENV_FILE:-${HOME}/.local/share/fcitx-vinput/fcitx-vinput.env}"
out_dir="${VINPUT_LIVE_NATIVE_OUT_DIR:-target/tmp/ime-fcitx-native-live}"
probe="scripts/fcitx-live-client-probe.py"

call_service() {
  gdbus call --session \
    --dest org.fcitx.Vinput \
    --object-path /org/fcitx/Vinput \
    --method "org.fcitx.Vinput.Service.$1" "${@:2}"
}

restore_idle() {
  local current_status
  current_status="$(call_service GetStatus 2>/dev/null || true)"
  if [[ "${current_status}" == *"'recording'"* ]]; then
    call_service StopRecording "" >/dev/null 2>&1 || true
  fi
}

trap restore_idle EXIT

if [[ -z "${wav_path}" ]]; then
  echo "set VINPUT_LIVE_NATIVE_WAV to a validated speech WAV" >&2
  exit 2
fi
if [[ ! -f "${wav_path}" ]]; then
  echo "live native WAV does not exist: ${wav_path}" >&2
  exit 2
fi
if [[ -f "${env_file}" ]]; then
  # shellcheck disable=SC1090
  . "${env_file}"
fi

for command in python3 pw-play fcitx5-remote gdbus; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required live-probe command is missing: ${command}" >&2
    exit 2
  fi
done
python3 - <<'PY'
import gi

gi.require_version("FcitxG", "1.0")
gi.require_version("Gdk", "4.0")
from gi.repository import FcitxG, Gdk  # noqa: F401
PY

if ! fcitx5-remote --check; then
  echo "Fcitx5 is not running in the current desktop session" >&2
  exit 2
fi
status="$(call_service GetStatus 2>/dev/null || true)"
if [[ "${status}" != *"'idle'"* ]]; then
  echo "org.fcitx.Vinput must be idle before the live probe: ${status:-unavailable}" >&2
  exit 2
fi

rm -rf "${out_dir}"
mkdir -p "${out_dir}"

IFS=',' read -r -a requested_modes <<<"${modes}"
for mode in "${requested_modes[@]}"; do
  case "${mode}" in
    normal|command)
      ;;
    *)
      echo "unsupported VINPUT_LIVE_NATIVE_MODES entry: ${mode}" >&2
      exit 2
      ;;
  esac
  echo "Running real Fcitx ${mode} native live probe..."
  set -o pipefail
  timeout 40s python3 "${probe}" \
    --mode "${mode}" \
    --wav "${wav_path}" \
    --selected-text "${selected_text}" \
    | tee "${out_dir}/${mode}.jsonl"
done

printf 'real Fcitx native live probes passed; evidence: %s\n' "${out_dir}"
