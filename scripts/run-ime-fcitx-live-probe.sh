#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

if [[ "${VINPUT_LIVE_INSTALL_COMMAND_DEMO:-}" == "1" || "${VINPUT_LIVE_INSTALL_COMMAND_DEMO:-}" == "true" ]]; then
  VINPUT_USER_PROFILE=command-demo scripts/install-user-ime.sh
fi

missing=0
require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    missing=1
  fi
}

require_cmd fcitx5
require_cmd fcitx5-remote
require_cmd gdbus
if [[ "${missing}" != 0 ]]; then
  exit 2
fi

if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  echo "DBUS_SESSION_BUS_ADDRESS is not set; run this inside a desktop user session." >&2
  exit 2
fi

if ! fcitx5-remote --check >/dev/null 2>&1; then
  echo "Fcitx5 is not running on the current session bus." >&2
  echo "Start or restart Fcitx5 after installing the addon, then retry." >&2
  exit 2
fi

echo "Fcitx5 is running."
echo "Fcitx DBus address: $(fcitx5-remote -a 2>/dev/null || true)"
echo "Current input method group: $(fcitx5-remote -q 2>/dev/null || true)"
echo "Current input method: $(fcitx5-remote -n 2>/dev/null || true)"

scripts/install-user-ime.sh >/tmp/vinput-ime-live-status.log 2>&1 || {
  cat /tmp/vinput-ime-live-status.log >&2
  echo "User IME install/status check failed." >&2
  exit 1
}
cat /tmp/vinput-ime-live-status.log

echo "Probing org.fcitx.Vinput activation and runtime status..."
gdbus call --session \
  --dest org.fcitx.Vinput \
  --object-path /org/fcitx/Vinput \
  --method org.fcitx.Vinput.Service.GetRuntimeStatus

echo "Live probe complete. Trigger keys are controlled by VINPUT_FCITX_NORMAL_TRIGGER and VINPUT_FCITX_COMMAND_TRIGGER before launching Fcitx5."
