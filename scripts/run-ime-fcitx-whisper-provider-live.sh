#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

resolve_path() {
  if [[ "$1" == /* ]]; then
    printf '%s\n' "$1"
  else
    printf '%s/%s\n' "${repo_root}" "$1"
  fi
}

whisper_root="${VINPUT_WHISPER_CPP_ROOT:-target/third-party/whisper.cpp-v1.9.1}"
whisper_binary="${VINPUT_WHISPER_CPP_BINARY:-${whisper_root}/build/bin/whisper-cli}"
whisper_source="${whisper_root}/src"
whisper_binary_abs="$(resolve_path "${whisper_binary}")"
whisper_source_abs="$(resolve_path "${whisper_source}")"
whisper_model="${VINPUT_WHISPER_MODEL:-${HOME}/.local/share/voxtype/models/ggml-base.bin}"
whisper_language="${VINPUT_WHISPER_LANGUAGE:-zh}"
recognition_wav="${VINPUT_WHISPER_WAV:-target/models/onnx-zf-ctc-zh-sm-int8-stream/test_wavs/0.wav}"
preflight_out="${VINPUT_WHISPER_PROVIDER_PREFLIGHT_OUT_DIR:-target/tmp/whisper-cpp-provider-preflight}"
live_out="${VINPUT_WHISPER_PROVIDER_OUT_DIR:-target/tmp/ime-fcitx-whisper-provider-live}"

VINPUT_WHISPER_CPP_ROOT="${whisper_root}" \
VINPUT_WHISPER_CPP_BINARY="${whisper_binary}" \
VINPUT_WHISPER_MODEL="${whisper_model}" \
VINPUT_WHISPER_LANGUAGE="${whisper_language}" \
VINPUT_WHISPER_WAV="${recognition_wav}" \
VINPUT_WHISPER_OUT_DIR="${preflight_out}" \
  scripts/run-whisper-cpp-asr-live.sh

VINPUT_LIVE_EXTERNAL_RECOGNIZER=whisper-cpp \
VINPUT_LIVE_EXTERNAL_PROVIDER_ID=external-whisper \
VINPUT_LIVE_EXTERNAL_MODEL_ID=whisper-cpp-base-multilingual \
VINPUT_LIVE_EXTERNAL_WAV="${recognition_wav}" \
VINPUT_LIVE_WHISPER_BINARY="${whisper_binary_abs}" \
VINPUT_LIVE_WHISPER_SOURCE="${whisper_source_abs}" \
VINPUT_LIVE_WHISPER_MODEL="${whisper_model}" \
VINPUT_LIVE_WHISPER_LANGUAGE="${whisper_language}" \
VINPUT_LIVE_CROSS_PROVIDER_OUT_DIR="${live_out}" \
  scripts/run-ime-fcitx-cross-provider-live.sh
