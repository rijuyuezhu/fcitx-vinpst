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

menus="${VINPST_LIVE_MENU_MODES:-scene,asr}"
scene_key="${VINPST_LIVE_SCENE_MENU_KEY:-F7}"
asr_key="${VINPST_LIVE_ASR_MENU_KEY:-F8}"
out_dir="${VINPST_LIVE_MENU_OUT_DIR:-target/tmp/ime-fcitx-menu-live}"
probe="scripts/live/niri/probes/fcitx-live-menu-probe.py"

call_service() {
  gdbus call --session \
    --dest org.fcitx.Vinpst \
    --object-path /org/fcitx/Vinpst \
    --method "org.fcitx.Vinpst.Service.$1" "${@:2}"
}

for command in python3 fcitx5-remote gdbus; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required menu-probe command is missing: ${command}" >&2
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
  echo "org.fcitx.Vinpst must be idle before the menu probe: ${status:-unavailable}" >&2
  exit 2
fi

rm -rf "${out_dir}"
mkdir -p "${out_dir}"

IFS=',' read -r -a requested_menus <<<"${menus}"
for menu in "${requested_menus[@]}"; do
  case "${menu}" in
  scene)
    trigger_key="${scene_key}"
    ;;
  asr)
    trigger_key="${asr_key}"
    ;;
  *)
    echo "unsupported VINPST_LIVE_MENU_MODES entry: ${menu}" >&2
    exit 2
    ;;
  esac
  echo "Running real Fcitx ${menu} menu live probe with ${trigger_key}..."
  set -o pipefail
  timeout 15s python3 "${probe}" \
    --menu "${menu}" \
    --trigger-key "${trigger_key}" \
    | tee "${out_dir}/${menu}.jsonl"
done

printf 'real Fcitx menu live probes passed; evidence: %s\n' "${out_dir}"
