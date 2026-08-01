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

whisper_tag="v1.9.1"
whisper_commit="f049fff95a089aa9969deb009cdd4892b3e74916"
whisper_root="${VINPUT_WHISPER_CPP_ROOT:-target/third-party/whisper.cpp-v1.9.1}"
source_dir="${whisper_root}/src"
build_dir="${whisper_root}/build"
binary="${VINPUT_WHISPER_CPP_BINARY:-${build_dir}/bin/whisper-cli}"
default_model="${HOME}/.local/share/voxtype/models/ggml-base.bin"
model="${VINPUT_WHISPER_MODEL:-${default_model}}"
wav="${VINPUT_WHISPER_WAV:-target/models/onnx-zf-ctc-zh-sm-int8-stream/test_wavs/0.wav}"
language="${VINPUT_WHISPER_LANGUAGE:-zh}"
threads="${VINPUT_WHISPER_THREADS:-8}"
timeout_seconds="${VINPUT_WHISPER_TIMEOUT_SECONDS:-120}"
out_dir="${VINPUT_WHISPER_OUT_DIR:-target/tmp/whisper-cpp-asr-live}"
expected_model_sha256="${VINPUT_WHISPER_EXPECTED_MODEL_SHA256:-}"

if [[ "${model}" == "${default_model}" && -z "${expected_model_sha256}" ]]; then
  expected_model_sha256="60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe"
fi

for command in cmake git jq ninja sha256sum timeout; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "required whisper.cpp live command is missing: ${command}" >&2
    exit 2
  fi
done
for value in "${threads}" "${timeout_seconds}"; do
  if [[ ! "${value}" =~ ^[1-9][0-9]*$ ]]; then
    echo "whisper.cpp numeric options must be positive integers: ${value}" >&2
    exit 2
  fi
done
for path in "${model}" "${wav}"; do
  if [[ ! -f "${path}" ]]; then
    echo "whisper.cpp live input is missing: ${path}" >&2
    exit 2
  fi
done

if [[ ! -d "${source_dir}/.git" ]] ||
  [[ "$(git -C "${source_dir}" rev-parse HEAD 2>/dev/null || true)" != "${whisper_commit}" ]]; then
  rm -rf "${whisper_root}"
  mkdir -p "$(dirname "${whisper_root}")"
  git clone --depth 1 --branch "${whisper_tag}" \
    https://github.com/ggerganov/whisper.cpp.git "${source_dir}"
fi
actual_commit="$(git -C "${source_dir}" rev-parse HEAD)"
if [[ "${actual_commit}" != "${whisper_commit}" ]]; then
  echo "whisper.cpp checkout mismatch: ${actual_commit}" >&2
  exit 1
fi

if [[ ! -x "${binary}" ]]; then
  cmake -S "${source_dir}" -B "${build_dir}" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DWHISPER_BUILD_TESTS=OFF \
    -DWHISPER_BUILD_EXAMPLES=ON \
    -DWHISPER_BUILD_SERVER=OFF
  cmake --build "${build_dir}" --target whisper-cli --parallel
fi
if [[ ! -x "${binary}" ]]; then
  echo "whisper.cpp build did not produce an executable: ${binary}" >&2
  exit 1
fi

rm -rf "${out_dir}"
mkdir -p "${out_dir}"
binary_sha256="$(sha256sum "${binary}" | awk '{print $1}')"
model_sha256="$(sha256sum "${model}" | awk '{print $1}')"
wav_sha256="$(sha256sum "${wav}" | awk '{print $1}')"
if [[ -n "${expected_model_sha256}" && "${model_sha256}" != "${expected_model_sha256}" ]]; then
  echo "Whisper model checksum mismatch: ${model_sha256}" >&2
  exit 1
fi

start_ms="$(date +%s%3N)"
LD_LIBRARY_PATH="$(dirname "${binary}")${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}" \
  timeout "${timeout_seconds}s" \
  "${binary}" \
    --no-gpu \
    --threads "${threads}" \
    --language "${language}" \
    --no-timestamps \
    --no-prints \
    --model "${model}" \
    --file "${wav}" \
    >"${out_dir}/recognition.txt" \
    2>"${out_dir}/whisper.stderr"
end_ms="$(date +%s%3N)"

text="$(tr '\n' ' ' <"${out_dir}/recognition.txt" | sed -E 's/[[:space:]]+/ /g; s/^ //; s/ $//')"
if [[ -z "${text}" ]]; then
  cat "${out_dir}/whisper.stderr" >&2
  echo "whisper.cpp produced no recognized text" >&2
  exit 1
fi

jq -n \
  --arg tag "${whisper_tag}" \
  --arg commit "${actual_commit}" \
  --arg binary "${binary}" \
  --arg binary_sha256 "${binary_sha256}" \
  --arg model "${model}" \
  --arg model_sha256 "${model_sha256}" \
  --arg wav "${wav}" \
  --arg wav_sha256 "${wav_sha256}" \
  --arg language "${language}" \
  --arg text "${text}" \
  --argjson elapsed_ms "$((end_ms - start_ms))" \
  '{
    event: "summary",
    recognizer: "whisper.cpp",
    independent_recognizer: true,
    version: $tag,
    commit: $commit,
    binary: $binary,
    binary_sha256: $binary_sha256,
    model: $model,
    model_sha256: $model_sha256,
    wav: $wav,
    wav_sha256: $wav_sha256,
    language: $language,
    elapsed_ms: $elapsed_ms,
    text: $text,
    ok: true
  }' | tee "${out_dir}/summary.json"
