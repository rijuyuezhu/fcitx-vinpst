#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

wav_path="${VINPUT_LIVE_NATIVE_WAV:-}"
selected_text="${VINPUT_LIVE_SELECTED_TEXT:-selected text}"
modes="${VINPUT_LIVE_NATIVE_MODES:-normal,command}"
focus_switch="${VINPUT_LIVE_NATIVE_FOCUS_SWITCH:-0}"
owner_loss="${VINPUT_LIVE_NATIVE_OWNER_LOSS:-0}"
expected_text_adapter="${VINPUT_LIVE_EXPECTED_TEXT_ADAPTER:-}"
expected_commit_prefix="${VINPUT_LIVE_EXPECTED_COMMIT_PREFIX:-}"
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
if [[ -n "${expected_text_adapter}" ]]; then
  runtime_status="$(call_service GetRuntimeStatus)"
  python3 - "${runtime_status}" "${expected_text_adapter}" <<'PY'
import ast
import json
import sys

payload = ast.literal_eval(sys.argv[1])[0]
status = json.loads(payload)
expected = sys.argv[2]
adapter_ids = status.get("text_adapters", {}).get("adapter_ids", [])
if expected not in adapter_ids:
    raise SystemExit(
        f"expected text adapter {expected!r} is not configured; found {adapter_ids!r}"
    )
PY
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
  if [[ "${focus_switch}" != "0" && "${mode}" != "normal" ]]; then
    echo "VINPUT_LIVE_NATIVE_FOCUS_SWITCH supports normal mode only" >&2
    exit 2
  fi
  if [[ "${owner_loss}" != "0" && "${mode}" != "normal" ]]; then
    echo "VINPUT_LIVE_NATIVE_OWNER_LOSS supports normal mode only" >&2
    exit 2
  fi
  if [[ "${focus_switch}" != "0" && "${owner_loss}" != "0" ]]; then
    echo "focus-switch and owner-loss are separate live cases" >&2
    exit 2
  fi
  if [[ -n "${expected_commit_prefix}" && "${mode}" != "command" ]]; then
    echo "VINPUT_LIVE_EXPECTED_COMMIT_PREFIX supports command mode only" >&2
    exit 2
  fi
  echo "Running real Fcitx ${mode} native live probe..."
  probe_args=(
    --mode "${mode}"
    --wav "${wav_path}"
    --selected-text "${selected_text}"
  )
  if [[ "${focus_switch}" != "0" ]]; then
    probe_args+=(--focus-switch)
  fi
  if [[ "${owner_loss}" != "0" ]]; then
    probe_args+=(--owner-loss)
  fi
  if [[ -n "${expected_commit_prefix}" ]]; then
    probe_args+=(
      --expected-commit-prefix "${expected_commit_prefix}"
      --allow-direct-command-commit
    )
  fi
  set -o pipefail
  timeout 40s python3 "${probe}" "${probe_args[@]}" \
    | tee "${out_dir}/${mode}.jsonl"
done

printf 'real Fcitx native live probes passed; evidence: %s\n' "${out_dir}"
