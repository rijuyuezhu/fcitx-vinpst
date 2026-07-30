#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

mode="${1:-${VINPUT_LIVE_TOOLKIT_MODE:-normal}}"
case "${mode}" in
normal | command) ;;
*)
  echo "mode must be normal or command" >&2
  exit 2
  ;;
esac

out_dir="${VINPUT_LIVE_TOOLKIT_OUT_DIR:-target/tmp/ime-chromium-native-live}"
browser="${VINPUT_CHROMIUM_BIN:-}"
if [[ -z "${browser}" ]]; then
  for candidate in google-chrome-unstable google-chrome chromium; do
    if command -v "${candidate}" >/dev/null 2>&1; then
      browser="$(command -v "${candidate}")"
      break
    fi
  done
fi
if [[ -z "${browser}" || ! -x "${browser}" ]]; then
  echo "Chrome or Chromium is required; set VINPUT_CHROMIUM_BIN when needed" >&2
  exit 1
fi
command -v python3 >/dev/null 2>&1 || {
  echo "python3 is required" >&2
  exit 1
}
command -v gdbus >/dev/null 2>&1 || {
  echo "gdbus is required to monitor daemon status and recognition partials" >&2
  exit 1
}
command -v fcitx5-remote >/dev/null 2>&1 || {
  echo "fcitx5-remote is required" >&2
  exit 1
}
if ! fcitx5-remote --check >/dev/null 2>&1; then
  echo "Fcitx5 is not running in this session" >&2
  exit 1
fi
if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  echo "DBUS_SESSION_BUS_ADDRESS is not set" >&2
  exit 1
fi

mkdir -p "${out_dir}"
echo "Chromium live probe (${mode})" >&2
echo "Use the real Fcitx shortcut in the focused browser field; no browser key events are synthesized." >&2
python3 scripts/chromium-live-toolkit-probe.py \
  --mode "${mode}" \
  --browser "${browser}" \
  --out-dir "${out_dir}"
