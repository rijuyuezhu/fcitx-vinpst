#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"
cd "${repo_root}"

VINPUT_LIVE_NATIVE_MODES=command \
VINPUT_LIVE_PRIMARY_SELECTION_FALLBACK=1 \
VINPUT_LIVE_SELECTED_TEXT='primary fallback fixture' \
VINPUT_LIVE_EXPECTED_TEXT_ADAPTER=native-command-live-adapter \
VINPUT_LIVE_EXPECTED_COMMIT_PREFIX='adapter-backed:' \
VINPUT_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-primary-selection-live \
  scripts/live/niri/run-ime-fcitx-virtual-source-live.sh
