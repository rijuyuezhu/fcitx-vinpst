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
cd "${repo_root}"

daemon="${VINPST_USER_DAEMON:-${repo_root}/target/debug/vinpst-daemon}"
profile="${VINPST_USER_PROFILE:-}"
config="${VINPST_USER_CONFIG:-}"
audio_backend="${VINPST_USER_AUDIO_BACKEND:-}"
configured_backends="${VINPST_USER_CONFIGURED_BACKENDS:-}"
remove_user="${VINPST_USER_REMOVE:-}"
status_user="${VINPST_USER_STATUS:-}"
features=()

case "${profile}" in
  ""|mock)
    ;;
  command-demo)
    config="${VINPST_USER_CONFIG:-${repo_root}/data/e2e-command-demo-config.json}"
    configured_backends="${VINPST_USER_CONFIGURED_BACKENDS:-1}"
    ;;
  configured-pipewire-live)
    config="${VINPST_USER_CONFIG:-${repo_root}/data/e2e-configured-pipewire-live.json}"
    audio_backend="${VINPST_USER_AUDIO_BACKEND:-pipewire}"
    configured_backends="${VINPST_USER_CONFIGURED_BACKENDS:-1}"
    features+=(--features pipewire-backend)
    ;;
  *)
    echo "unsupported VINPST_USER_PROFILE: ${profile}" >&2
    echo "supported profiles: mock, command-demo, configured-pipewire-live" >&2
    exit 2
    ;;
esac

cargo build -q -p vinpst-cli -p vinpst-daemon "${features[@]}"

if [[ "${remove_user}" == "1" || "${remove_user}" == "true" ]]; then
  target/debug/vinpst activation-service --remove-user
  exit 0
fi

if [[ "${status_user}" == "1" || "${status_user}" == "true" ]]; then
  target/debug/vinpst activation-service --user-status
  exit 0
fi

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

target/debug/vinpst "${args[@]}"
doctor_args=(doctor)
if [[ -n "${config}" ]]; then
  doctor_args+=(--config "${config}")
fi
target/debug/vinpst "${doctor_args[@]}"
