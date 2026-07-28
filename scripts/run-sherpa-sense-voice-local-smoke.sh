#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export VINPUT_SHERPA_EXPECT_FAMILY="${VINPUT_SHERPA_EXPECT_FAMILY:-sense_voice}"
export VINPUT_SHERPA_SMOKE_DIR="${VINPUT_SHERPA_SMOKE_DIR:-target/tmp/sherpa-sense-voice-local-smoke}"
exec "${repo_root}/scripts/run-sherpa-offline-local-smoke.sh"
