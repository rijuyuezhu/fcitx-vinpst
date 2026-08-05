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

resolve_path() {
  if [[ "$1" == /* ]]; then
    printf '%s\n' "$1"
  else
    printf '%s/%s\n' "${repo_root}" "$1"
  fi
}

whisper_root="${VINPST_WHISPER_CPP_ROOT:-target/third-party/whisper.cpp-v1.9.1}"
whisper_binary="${VINPST_WHISPER_CPP_BINARY:-${whisper_root}/build/bin/whisper-cli}"
whisper_source="${whisper_root}/src"
whisper_binary_abs="$(resolve_path "${whisper_binary}")"
whisper_source_abs="$(resolve_path "${whisper_source}")"
whisper_model="${VINPST_WHISPER_MODEL:-${HOME}/.local/share/voxtype/models/ggml-base.bin}"
whisper_language="${VINPST_WHISPER_LANGUAGE:-zh}"
recognition_wav="${VINPST_WHISPER_WAV:-target/models/onnx-zf-ctc-zh-sm-int8-stream/test_wavs/0.wav}"
preflight_out="${VINPST_WHISPER_PROVIDER_PREFLIGHT_OUT_DIR:-target/tmp/whisper-cpp-provider-preflight}"
live_out="${VINPST_WHISPER_PROVIDER_OUT_DIR:-target/tmp/ime-fcitx-whisper-provider-live}"

VINPST_WHISPER_CPP_ROOT="${whisper_root}" \
VINPST_WHISPER_CPP_BINARY="${whisper_binary}" \
VINPST_WHISPER_MODEL="${whisper_model}" \
VINPST_WHISPER_LANGUAGE="${whisper_language}" \
VINPST_WHISPER_WAV="${recognition_wav}" \
VINPST_WHISPER_OUT_DIR="${preflight_out}" \
  scripts/live/audio/run-whisper-cpp-asr-live.sh

VINPST_LIVE_EXTERNAL_RECOGNIZER=whisper-cpp \
VINPST_LIVE_EXTERNAL_PROVIDER_ID=external-whisper \
VINPST_LIVE_EXTERNAL_MODEL_ID=whisper-cpp-base-multilingual \
VINPST_LIVE_EXTERNAL_WAV="${recognition_wav}" \
VINPST_LIVE_WHISPER_BINARY="${whisper_binary_abs}" \
VINPST_LIVE_WHISPER_SOURCE="${whisper_source_abs}" \
VINPST_LIVE_WHISPER_MODEL="${whisper_model}" \
VINPST_LIVE_WHISPER_LANGUAGE="${whisper_language}" \
VINPST_LIVE_CROSS_PROVIDER_OUT_DIR="${live_out}" \
  scripts/live/niri/run-ime-fcitx-cross-provider-live.sh
