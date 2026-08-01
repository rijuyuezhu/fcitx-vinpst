#!/usr/bin/env bash
set -euo pipefail

mode="${1:-normal}"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"
cd "${repo_root}"

VINPUT_LIVE_VIRTUAL_PROBE_KIND=kitty \
VINPUT_LIVE_TOOLKIT_MODE="${mode}" \
VINPUT_LIVE_VIRTUAL_OUT_DIR="target/tmp/ime-kitty-virtual-source-live/${mode}" \
  scripts/live/niri/run-ime-fcitx-virtual-source-live.sh
