#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"
cd "${repo_root}"

VINPST_TEST_PIPEWIRE_CONTEXT=1 \
VINPST_TEST_PIPEWIRE_ENUMERATE=1 \
VINPST_TEST_PIPEWIRE_RECORD=1 \
  cargo test -p vinpst-audio --features pipewire-backend pipewire_ -- --nocapture
