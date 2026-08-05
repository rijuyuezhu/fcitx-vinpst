#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="${script_dir}"
while [[ ! -f "${repo_root}/Cargo.toml" || ! -d "${repo_root}/scripts" ]]; do
  parent="$(dirname "${repo_root}")"
  if [[ "${parent}" == "${repo_root}" ]]; then
    echo "cannot locate repository root from ${script_dir}" >&2
    exit 1
  fi
  repo_root="${parent}"
done
export VINPST_SHERPA_EXPECT_FAMILY="${VINPST_SHERPA_EXPECT_FAMILY:-sense_voice}"
export VINPST_SHERPA_SMOKE_DIR="${VINPST_SHERPA_SMOKE_DIR:-target/tmp/sherpa-sense-voice-local-smoke}"
exec "${repo_root}/scripts/tests/asr/run-sherpa-offline-local-smoke.sh"
