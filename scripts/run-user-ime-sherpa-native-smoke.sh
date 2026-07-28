#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

VINPUT_TEST_SHERPA_PROFILE=sherpa-native-live \
  scripts/run-user-ime-sherpa-sense-voice-smoke.sh
