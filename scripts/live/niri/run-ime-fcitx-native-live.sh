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
selected_text="${VINPUT_LIVE_SELECTED_TEXT-selected text}"
modes="${VINPUT_LIVE_NATIVE_MODES:-normal,command}"
focus_switch="${VINPUT_LIVE_NATIVE_FOCUS_SWITCH:-0}"
owner_loss="${VINPUT_LIVE_NATIVE_OWNER_LOSS:-0}"
primary_selection_fallback="${VINPUT_LIVE_PRIMARY_SELECTION_FALLBACK:-0}"
require_partial="${VINPUT_LIVE_REQUIRE_PARTIAL:-1}"
expected_text_adapter="${VINPUT_LIVE_EXPECTED_TEXT_ADAPTER:-}"
expected_commit_prefix="${VINPUT_LIVE_EXPECTED_COMMIT_PREFIX:-}"
expect_unchanged_on_error="${VINPUT_LIVE_EXPECT_UNCHANGED_ON_ERROR:-0}"
clear_primary_selection="${VINPUT_LIVE_CLEAR_PRIMARY_SELECTION:-0}"
candidate_delay_ms="${VINPUT_LIVE_CANDIDATE_DELAY_MS:-200}"
playback_target="${VINPUT_LIVE_PLAYBACK_TARGET:-}"
env_file="${VINPUT_LIVE_ENV_FILE:-${HOME}/.local/share/fcitx-vinput/fcitx-vinput.env}"
out_dir="${VINPUT_LIVE_NATIVE_OUT_DIR:-target/tmp/ime-fcitx-native-live}"
probe="scripts/live/niri/probes/fcitx-live-client-probe.py"
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
  local current_status
  current_status="$(call_service GetStatus 2>/dev/null || true)"
  if [[ "${current_status}" == *"'recording'"* ]]; then
    call_service StopRecording "" >/dev/null 2>&1 || true
  fi
}

restore_primary_selection() {
  local restored_path
  if [[ "${primary_snapshot_ready}" == "0" ]]; then
    return 0
  fi
  if [[ -n "${primary_owner_pid}" ]]; then
    kill -TERM "${primary_owner_pid}" 2>/dev/null || true
    wait "${primary_owner_pid}" 2>/dev/null || true
    primary_owner_pid=""
  fi
  if [[ "${primary_before_present}" == "1" ]]; then
    wl-copy --primary --type 'text/plain;charset=utf-8' \
      <"${out_dir}/primary-selection-before.txt" >/dev/null 2>&1
    restored_path="${out_dir}/primary-selection-restored.txt"
    for _ in $(seq 1 50); do
      if timeout 1s wl-paste --primary --no-newline \
        >"${restored_path}" 2>/dev/null &&
        cmp -s "${out_dir}/primary-selection-before.txt" "${restored_path}"; then
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
      return 0
    fi
    sleep 0.05
  done
  echo "failed to clear the temporary Wayland primary selection" >&2
  return 1
}

cleanup() {
  local exit_code=$?
  trap - EXIT
  set +e
  restore_idle
  if ! restore_primary_selection; then
    exit_code=1
  fi
  exit "${exit_code}"
}
trap cleanup EXIT

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
if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
  runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
  mapfile -t wayland_sockets < <(
    find "${runtime_dir}" -maxdepth 1 -type s -name 'wayland-*' -printf '%f\n' 2>/dev/null
  )
  if [[ "${#wayland_sockets[@]}" == 1 ]]; then
    export WAYLAND_DISPLAY="${wayland_sockets[0]}"
  fi
fi

for command in python3 pw-play fcitx5-remote gdbus timeout; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required live-probe command is missing: ${command}" >&2
    exit 2
  fi
done
if [[ "${primary_selection_fallback}" != "0" || "${clear_primary_selection}" != "0" ]]; then
  for command in wl-copy wl-paste; do
    if ! command -v "${command}" >/dev/null 2>&1; then
      echo "required primary-selection command is missing: ${command}" >&2
      exit 2
    fi
  done
fi
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

if [[ "${primary_selection_fallback}" != "0" && "${clear_primary_selection}" != "0" ]]; then
  echo "primary-selection fallback and clearing are mutually exclusive" >&2
  exit 2
fi
if [[ "${primary_selection_fallback}" != "0" || "${clear_primary_selection}" != "0" ]]; then
  if [[ "${modes}" != "command" ]]; then
    echo "primary-selection setup requires command-only mode" >&2
    exit 2
  fi
  if timeout 2s wl-paste --primary --no-newline \
    >"${out_dir}/primary-selection-before.txt" 2>/dev/null; then
    primary_before_present=1
  else
    : >"${out_dir}/primary-selection-before.txt"
  fi
  primary_snapshot_ready=1
  if [[ "${primary_selection_fallback}" != "0" ]]; then
    wl-copy --primary --foreground --type 'text/plain;charset=utf-8' \
      < <(printf '%s' "${selected_text}") &
    primary_owner_pid=$!
    primary_ready=0
    for _ in $(seq 1 100); do
      if ! kill -0 "${primary_owner_pid}" 2>/dev/null; then
        break
      fi
      if current_primary="$(timeout 1s wl-paste --primary --no-newline 2>/dev/null)" &&
        [[ "${current_primary}" == "${selected_text}" ]]; then
        primary_ready=1
        break
      fi
      sleep 0.05
    done
    if [[ "${primary_ready}" != "1" ]]; then
      echo "temporary Wayland primary selection did not become readable" >&2
      exit 1
    fi
    sleep 0.2
    python3 - "${selected_text}" "${primary_before_present}" \
      >"${out_dir}/primary-selection-setup.json" <<'PY'
import json
import sys

print(
    json.dumps(
        {
            "event": "primary-selection-ready",
            "text": sys.argv[1],
            "previous_text_present": sys.argv[2] == "1",
            "ok": True,
        },
        ensure_ascii=False,
    )
)
PY
  else
    wl-copy --primary --clear
    primary_cleared=0
    for _ in $(seq 1 50); do
      if ! timeout 1s wl-paste --primary --no-newline >/dev/null 2>&1; then
        primary_cleared=1
        break
      fi
      sleep 0.05
    done
    if [[ "${primary_cleared}" != "1" ]]; then
      echo "Wayland primary selection did not clear" >&2
      exit 1
    fi
    printf '{"event":"primary-selection-cleared","previous_text_present":%s,"ok":true}\n' \
      "$( [[ "${primary_before_present}" == "1" ]] && echo true || echo false )" \
      >"${out_dir}/primary-selection-setup.json"
  fi
fi

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
  if [[ "${primary_selection_fallback}" != "0" && "${mode}" != "command" ]]; then
    echo "VINPUT_LIVE_PRIMARY_SELECTION_FALLBACK supports command mode only" >&2
    exit 2
  fi
  if [[ "${clear_primary_selection}" != "0" && "${mode}" != "command" ]]; then
    echo "VINPUT_LIVE_CLEAR_PRIMARY_SELECTION supports command mode only" >&2
    exit 2
  fi
  if [[ "${expect_unchanged_on_error}" != "0" && "${mode}" != "command" ]]; then
    echo "VINPUT_LIVE_EXPECT_UNCHANGED_ON_ERROR supports command mode only" >&2
    exit 2
  fi
  if [[ "${expect_unchanged_on_error}" != "0" && -n "${expected_commit_prefix}" ]]; then
    echo "error-preservation and expected-commit modes are mutually exclusive" >&2
    exit 2
  fi
  echo "Running real Fcitx ${mode} native live probe..."
  probe_args=(
    --mode "${mode}"
    --wav "${wav_path}"
    --selected-text "${selected_text}"
    --candidate-delay-ms "${candidate_delay_ms}"
  )
  if [[ "${require_partial}" == "0" ]]; then
    probe_args+=(--no-require-partial)
  fi
  if [[ -n "${playback_target}" ]]; then
    probe_args+=(--playback-target "${playback_target}")
  fi
  if [[ "${focus_switch}" != "0" ]]; then
    probe_args+=(--focus-switch)
  fi
  if [[ "${owner_loss}" != "0" ]]; then
    probe_args+=(--owner-loss)
  fi
  if [[ "${primary_selection_fallback}" != "0" ]]; then
    probe_args+=(--primary-selection-fallback)
  fi
  if [[ -n "${expected_commit_prefix}" ]]; then
    probe_args+=(
      --expected-commit-prefix "${expected_commit_prefix}"
      --allow-direct-command-commit
    )
  fi
  if [[ "${expect_unchanged_on_error}" != "0" ]]; then
    probe_args+=(--expect-unchanged-on-error)
  fi
  set -o pipefail
  timeout 40s python3 "${probe}" "${probe_args[@]}" \
    | tee "${out_dir}/${mode}.jsonl"
done

printf 'real Fcitx native live probes passed; evidence: %s\n' "${out_dir}"
