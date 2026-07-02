#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

daemon="${VINPUT_USER_DAEMON:-${repo_root}/target/debug/vinput-daemon}"
config="${VINPUT_USER_CONFIG:-}"
audio_backend="${VINPUT_USER_AUDIO_BACKEND:-}"
configured_backends="${VINPUT_USER_CONFIGURED_BACKENDS:-}"

cargo build -q -p vinput-cli -p vinput-daemon

args=(activation-service --daemon "${daemon}" --user)
if [[ -n "${config}" || "${configured_backends}" == "1" || "${configured_backends}" == "true" ]]; then
  args+=(--configured-backends)
fi
if [[ -n "${config}" ]]; then
  args+=(--config "${config}")
fi
if [[ -n "${audio_backend}" ]]; then
  args+=(--audio-backend "${audio_backend}")
fi

target/debug/vinput "${args[@]}"
doctor_args=(doctor)
if [[ -n "${config}" ]]; then
  doctor_args+=(--config "${config}")
fi
target/debug/vinput "${doctor_args[@]}"
