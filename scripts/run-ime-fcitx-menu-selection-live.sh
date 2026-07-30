#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

out_dir="${VINPUT_LIVE_MENU_SELECTION_OUT_DIR:-target/tmp/ime-fcitx-menu-selection-live}"
trigger_key="${VINPUT_LIVE_SCENE_MENU_KEY:-F7}"
probe="scripts/fcitx-live-menu-selection-probe.py"

for command in python3 fcitx5-remote gdbus; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required command is missing: ${command}" >&2
    exit 2
  fi
done
if ! fcitx5-remote --check >/dev/null 2>&1; then
  echo "Fcitx5 is not running in this session" >&2
  exit 1
fi
if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  echo "DBUS_SESSION_BUS_ADDRESS is not set" >&2
  exit 1
fi
if ! python3 - <<'PY'
import gi

gi.require_version("FcitxG", "1.0")
gi.require_version("Gdk", "4.0")
from gi.repository import FcitxG, Gdk  # noqa: F401, E402
PY
then
  echo "python GObject bindings for FcitxG/Gdk4 are required" >&2
  exit 1
fi

status="$(gdbus call --session \
  --dest org.fcitx.Vinput \
  --object-path /org/fcitx/Vinput \
  --method org.fcitx.Vinput.Service.GetStatus 2>/dev/null || true)"
if [[ "${status}" != *"'idle'"* ]]; then
  echo "org.fcitx.Vinput must be idle before scene-menu selection: ${status}" >&2
  exit 1
fi

rm -rf "${out_dir}"
mkdir -p "${out_dir}"

echo "Fcitx scene-menu selection live probe" >&2
echo "Select the first non-active scene with a real Fcitx Enter event, then restore the original scene." >&2
python3 "${probe}" --trigger-key "${trigger_key}" | tee "${out_dir}/scene-selection.jsonl"
