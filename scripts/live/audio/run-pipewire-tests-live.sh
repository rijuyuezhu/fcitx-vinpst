#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"
cd "${repo_root}"

VINPUT_TEST_PIPEWIRE_CONTEXT=1 \
VINPUT_TEST_PIPEWIRE_ENUMERATE=1 \
VINPUT_TEST_PIPEWIRE_RECORD=1 \
  cargo test -p vinput-audio --features pipewire-backend pipewire_ -- --nocapture
