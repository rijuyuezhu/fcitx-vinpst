#!/usr/bin/env bash
set -euo pipefail

mode="${1:-normal}"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"
cd "${repo_root}"

VINPST_LIVE_VIRTUAL_PROBE_KIND=gtk4 \
VINPST_LIVE_TOOLKIT_MODE="${mode}" \
VINPST_LIVE_VIRTUAL_OUT_DIR="target/tmp/ime-gtk4-virtual-source-live/${mode}" \
  scripts/live/niri/run-ime-fcitx-virtual-source-live.sh
