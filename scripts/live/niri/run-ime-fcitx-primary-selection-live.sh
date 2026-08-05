#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"
cd "${repo_root}"

VINPST_LIVE_NATIVE_MODES=command \
VINPST_LIVE_PRIMARY_SELECTION_FALLBACK=1 \
VINPST_LIVE_SELECTED_TEXT='primary fallback fixture' \
VINPST_LIVE_EXPECTED_TEXT_ADAPTER=native-command-live-adapter \
VINPST_LIVE_EXPECTED_COMMIT_PREFIX='adapter-backed:' \
VINPST_LIVE_VIRTUAL_OUT_DIR=target/tmp/ime-fcitx-primary-selection-live \
  scripts/live/niri/run-ime-fcitx-virtual-source-live.sh
