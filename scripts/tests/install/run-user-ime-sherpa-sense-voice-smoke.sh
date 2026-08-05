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

tmp_dir="$(mktemp -d)"
stub_bin="${tmp_dir}/bin"
home_dir="${tmp_dir}/home"
out_dir="${tmp_dir}/out"
runtime_bin="${out_dir}/runtime-bin"
runtime_source_dir="${out_dir}/runtime-source"
profile="${VINPST_TEST_SHERPA_PROFILE:-sherpa-sense-voice-live}"
case "${profile}" in
  sherpa-native-live)
    config_name="sherpa-native-live.json"
    typed_metadata="1"
    command_adapter=""
    ;;
  sherpa-native-command-live)
    config_name="sherpa-native-command-live.json"
    typed_metadata="1"
    command_adapter="1"
    ;;
  sherpa-sense-voice-live)
    config_name="sherpa-sense-voice-live.json"
    typed_metadata=""
    command_adapter=""
    ;;
  *)
    echo "unsupported VINPST_TEST_SHERPA_PROFILE: ${profile}" >&2
    exit 2
    ;;
esac

cleanup() {
  rm -rf "${tmp_dir}"
}
trap cleanup EXIT

mkdir -p "${stub_bin}" "${home_dir}" "${out_dir}" "${runtime_bin}" "${runtime_source_dir}"
printf 'stub sherpa runtime\n' >"${runtime_source_dir}/libsherpa-onnx-c-api.so"
printf 'stub sherpa cxx runtime\n' >"${runtime_source_dir}/libsherpa-onnx-cxx-api.so"
printf 'stub onnx runtime\n' >"${runtime_source_dir}/libonnxruntime.so"

cat >"${stub_bin}/cargo" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${VINPST_STUB_CARGO_CALLS:?}"
: "${VINPST_USER_CLI_BINARY:?}"
: "${VINPST_USER_DAEMON_BINARY:?}"
mkdir -p "$(dirname "${VINPST_USER_CLI_BINARY}")" "$(dirname "${VINPST_USER_DAEMON_BINARY}")"
cat >"${VINPST_USER_CLI_BINARY}" <<'VINPST'
#!/usr/bin/env bash
set -euo pipefail
printf 'cli LD_LIBRARY_PATH=%s args=%s\n' "${LD_LIBRARY_PATH:-}" "$*" >>"${VINPST_STUB_CALLS:?}"
case "${1:-}" in
  activation-service)
    service_dir="${XDG_DATA_HOME:-${HOME}/.local/share}/dbus-1/services"
    mkdir -p "${service_dir}"
    if [[ " $* " == *" --remove-user "* ]]; then
      rm -f "${service_dir}/org.fcitx.Vinpst.service"
      printf '{"activation":"removed"}\n'
      exit 0
    fi
    daemon=""
    args=("$@")
    for ((index = 0; index < ${#args[@]}; index++)); do
      if [[ "${args[$index]}" == "--daemon" && $((index + 1)) -lt ${#args[@]} ]]; then
        daemon="${args[$((index + 1))]}"
      fi
    done
    cat >"${service_dir}/org.fcitx.Vinpst.service" <<SERVICE
[D-BUS Service]
Name=org.fcitx.Vinpst
Exec=${daemon:-vinpst-daemon} --dbus
SERVICE
    printf '{"activation":"ok"}\n'
    ;;
  doctor)
    printf '{"doctor":"ok"}\n'
    ;;
  *)
    printf '{"ok":true}\n'
    ;;
esac
VINPST
chmod +x "${VINPST_USER_CLI_BINARY}"
cat >"${VINPST_USER_DAEMON_BINARY}" <<'DAEMON'
#!/usr/bin/env bash
set -euo pipefail
printf 'daemon LD_LIBRARY_PATH=%s args=%s\n' "${LD_LIBRARY_PATH:-}" "$*" >>"${VINPST_STUB_CALLS:?}"
printf '{"runtime":"ok"}\n'
DAEMON
chmod +x "${VINPST_USER_DAEMON_BINARY}"
SH
chmod +x "${stub_bin}/cargo"

cat >"${stub_bin}/cmake" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "--build" ]]; then
  exit 0
fi
build_dir=""
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    -B)
      shift
      build_dir="$1"
      ;;
  esac
  shift || true
done
if [[ -z "${build_dir}" ]]; then
  echo "stub cmake requires -B" >&2
  exit 2
fi
mkdir -p "${build_dir}"
printf 'stub module\n' >"${build_dir}/fcitx5-vinpst.so"
mkdir -p "${build_dir}/locale/zh_CN/LC_MESSAGES"
printf 'stub zh_CN catalog\n' >"${build_dir}/locale/zh_CN/LC_MESSAGES/fcitx5-vinpst.mo"
cat >"${build_dir}/vinpst-addon.conf" <<'CONF'
Name=Vinpst
Type=SharedLibrary
Library=fcitx5-vinpst
CONF
SH
chmod +x "${stub_bin}/cmake"

model_dir="${out_dir}/sense-voice-model"
mkdir -p "${model_dir}"
printf 'onnx\n' >"${model_dir}/model.int8.onnx"
printf '<blank> 0\n' >"${model_dir}/tokens.txt"
printf 'hello 1.0\n' >"${model_dir}/hotwords.txt"
native_wav_path="${out_dir}/native-validation.wav"
printf 'RIFFstub WAV fixture\n' >"${native_wav_path}"
if [[ "${typed_metadata}" == "1" ]]; then
  printf 'encoder\n' >"${model_dir}/encoder.onnx"
  printf 'decoder\n' >"${model_dir}/decoder.onnx"
  printf 'joiner\n' >"${model_dir}/joiner.onnx"
  cat >"${model_dir}/vinpst-model.json" <<'JSON'
{
  "backend": "sherpa-streaming",
  "family": "transducer",
  "runtime": "online",
  "model": {
    "tokens": "tokens.txt",
    "transducer": {
      "encoder": "encoder.onnx",
      "decoder": "decoder.onnx",
      "joiner": "joiner.onnx"
    }
  }
}
JSON
fi

calls_log="${out_dir}/vinpst-calls.log"
cargo_calls_log="${out_dir}/cargo-calls.log"

PATH="${stub_bin}:${PATH}" \
HOME="${home_dir}" \
XDG_DATA_HOME="${home_dir}/.local/share" \
VINPST_STUB_CALLS="${calls_log}" \
VINPST_STUB_CARGO_CALLS="${cargo_calls_log}" \
VINPST_USER_CLI_BINARY="${runtime_bin}/vinpst" \
VINPST_USER_DAEMON_BINARY="${runtime_bin}/vinpst-daemon" \
VINPST_USER_SHERPA_RUNTIME_LIB_DIR="${runtime_source_dir}" \
VINPST_USER_PROFILE="${profile}" \
VINPST_USER_AUDIO_BACKEND=mock \
VINPST_USER_SHERPA_MODEL="${model_dir}" \
VINPST_USER_SHERPA_HOTWORDS_FILE=hotwords.txt \
VINPST_USER_SHERPA_TIMEOUT_MS=7000 \
VINPST_USER_NATIVE_WAV="${native_wav_path}" \
scripts/install/install-user-ime.sh >"${out_dir}/install.log" 2>&1

config_path="${home_dir}/.local/share/fcitx-vinpst/${config_name}"
service_path="${home_dir}/.local/share/dbus-1/services/org.fcitx.Vinpst.service"
vad_model_path="${home_dir}/.local/share/fcitx-vinpst/vad/silero_vad.onnx"
vad_license_path="${home_dir}/.local/share/fcitx-vinpst/vad/LICENSE"
runtime_lib_dir="${home_dir}/.local/share/fcitx-vinpst/runtime/lib"
env_path="${home_dir}/.local/share/fcitx-vinpst/fcitx-vinpst.env"
daemon_wrapper_path="${home_dir}/.local/share/fcitx-vinpst/vinpst-daemon-with-vinpst-env.sh"
locale_catalog_path="${home_dir}/.local/share/locale/zh_CN/LC_MESSAGES/fcitx5-vinpst.mo"

for path in "${config_path}" "${service_path}" "${vad_model_path}" "${vad_license_path}" "${env_path}" "${daemon_wrapper_path}" "${locale_catalog_path}" "${runtime_lib_dir}/libsherpa-onnx-c-api.so" "${runtime_lib_dir}/libsherpa-onnx-cxx-api.so" "${runtime_lib_dir}/libonnxruntime.so"; do
  if [[ ! -e "${path}" ]]; then
    cat "${out_dir}/install.log" >&2
    echo "missing expected file: ${path}" >&2
    exit 1
  fi
done

cmp data/vad/silero_vad.onnx "${vad_model_path}"
cmp data/vad/LICENSE "${vad_license_path}"
cmp "${runtime_source_dir}/libsherpa-onnx-c-api.so" "${runtime_lib_dir}/libsherpa-onnx-c-api.so"
cmp "${runtime_source_dir}/libsherpa-onnx-cxx-api.so" "${runtime_lib_dir}/libsherpa-onnx-cxx-api.so"
cmp "${runtime_source_dir}/libonnxruntime.so" "${runtime_lib_dir}/libonnxruntime.so"
if ! grep -Fq -- "export VINPST_SHERPA_RUNTIME_LIB_DIR=\"${runtime_lib_dir}\"" "${env_path}"; then
  cat "${env_path}" >&2
  echo "environment file did not expose the installed runtime bundle" >&2
  exit 1
fi
if ! grep -Fq -- "export LD_LIBRARY_PATH=\"${runtime_lib_dir}" "${env_path}"; then
  cat "${env_path}" >&2
  echo "environment file did not prepend the installed runtime bundle" >&2
  exit 1
fi
if ! grep -Fq -- "Exec=${daemon_wrapper_path} --dbus" "${service_path}"; then
  cat "${service_path}" >&2
  echo "activation service did not use the daemon environment wrapper" >&2
  exit 1
fi
if ! grep -Fq -- ". ${env_path}" "${daemon_wrapper_path}"; then
  cat "${daemon_wrapper_path}" >&2
  echo "daemon wrapper did not source the generated environment" >&2
  exit 1
fi
VINPST_STUB_CALLS="${calls_log}" "${daemon_wrapper_path}" --wrapper-probe >/dev/null
if ! grep -Fq -- "LD_LIBRARY_PATH=${runtime_lib_dir}" "${calls_log}" ||
   ! grep -Fq -- "args=--wrapper-probe" "${calls_log}"; then
  cat "${calls_log}" >&2
  echo "daemon wrapper did not launch with the installed runtime library path" >&2
  exit 1
fi

python3 - "${config_path}" "${model_dir}" "${typed_metadata}" "${command_adapter}" <<'PY'
import json
import pathlib
import sys

config = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
model_dir = pathlib.Path(sys.argv[2]).resolve()
typed_metadata = bool(sys.argv[3])
command_adapter = bool(sys.argv[4])
provider = config["asr"]["providers"][0]
vad = config["asr"]["vad"]
assert config["asr"]["active_provider"] == "sherpa-onnx", config
assert vad == {
    "enabled": not typed_metadata,
    "threshold": 0.45,
    "min_speech_duration": 0.15,
    "min_silence_duration": 0.5,
    "speech_pad_ms": 300,
}, vad
assert provider["id"] == "sherpa-onnx", provider
assert provider["type"] == "local", provider
assert provider["model"] == str(model_dir), provider
assert provider["hotwords_file"] == str(model_dir / "hotwords.txt"), provider
assert provider["timeout_ms"] == 7000, provider
assert config["scenes"]["active_scene"] == "raw", config
if command_adapter:
    assert config["llm"]["adapters"] == [
        {
            "id": "native-command-live-adapter",
            "command": "python3",
            "args": config["llm"]["adapters"][0]["args"],
        }
    ], config
    assert "adapter-backed:" in config["llm"]["adapters"][0]["args"][1], config
    command_scene = next(
        scene for scene in config["scenes"]["definitions"] if scene["id"] == "__command__"
    )
    assert command_scene["candidate_count"] == 1, command_scene
else:
    assert "llm" not in config, config
PY

if ! grep -Fq -- '--features pipewire-backend,sherpa-onnx-backend' "${cargo_calls_log}"; then
  cat "${cargo_calls_log}" >&2
  echo "cargo build did not enable the sherpa and pipewire features" >&2
  exit 1
fi
if ! grep -Fq -- '--audio-backend mock' "${calls_log}"; then
  cat "${calls_log}" >&2
  echo "activation call did not preserve requested audio backend" >&2
  exit 1
fi
if ! grep -Fq -- '--configured-backends' "${calls_log}"; then
  cat "${calls_log}" >&2
  echo "activation call did not enable configured backends" >&2
  exit 1
fi
if ! grep -Fq -- "--config ${config_path}" "${calls_log}"; then
  cat "${calls_log}" >&2
  echo "activation call did not point at generated sherpa config" >&2
  exit 1
fi
if ! grep -Fq -- "--daemon-arg=--wav --daemon-arg ${native_wav_path}" "${calls_log}"; then
  cat "${calls_log}" >&2
  echo "activation call did not preserve the deterministic native WAV" >&2
  exit 1
fi
if ! grep -Fq -- "LD_LIBRARY_PATH=${runtime_lib_dir}" "${calls_log}" ||
   ! grep -Fq -- "args=--configured-backends --config ${config_path} runtime-status" "${calls_log}"; then
  cat "${calls_log}" >&2
  echo "install did not run runtime-status validation against generated sherpa config" >&2
  exit 1
fi

: >"${cargo_calls_log}"
: >"${calls_log}"
PATH="${stub_bin}:${PATH}" \
HOME="${home_dir}" \
XDG_DATA_HOME="${home_dir}/.local/share" \
VINPST_STUB_CALLS="${calls_log}" \
VINPST_STUB_CARGO_CALLS="${cargo_calls_log}" \
VINPST_USER_CLI_BINARY="${runtime_bin}/vinpst" \
VINPST_USER_DAEMON_BINARY="${runtime_bin}/vinpst-daemon" \
VINPST_USER_SHERPA_RUNTIME_LIB_DIR="${runtime_source_dir}" \
VINPST_USER_STATUS=1 \
scripts/install/install-user-ime.sh >"${out_dir}/status.log" 2>&1

if ! grep -Fq -- '-p vinpst-cli --features pipewire-backend,sherpa-onnx-backend' "${cargo_calls_log}"; then
  cat "${cargo_calls_log}" >&2
  echo "status build did not enable the native sherpa CLI diagnostics" >&2
  exit 1
fi
if ! grep -Fq -- "cli LD_LIBRARY_PATH=${runtime_lib_dir}" "${calls_log}" ||
   ! grep -Fq -- "args=doctor --config ${config_path}" "${calls_log}"; then
  cat "${calls_log}" >&2
  echo "status call did not run native doctor with the installed runtime bundle" >&2
  exit 1
fi
if ! grep -Fq -- "LD_LIBRARY_PATH=${runtime_lib_dir}" "${calls_log}" ||
   ! grep -Fq -- "args=--configured-backends --config ${config_path} runtime-status" "${calls_log}"; then
  cat "${calls_log}" >&2
  echo "status call did not run runtime-status validation against generated sherpa config" >&2
  exit 1
fi

: >"${cargo_calls_log}"
: >"${calls_log}"
PATH="${stub_bin}:${PATH}" \
HOME="${home_dir}" \
XDG_DATA_HOME="${home_dir}/.local/share" \
VINPST_STUB_CALLS="${calls_log}" \
VINPST_STUB_CARGO_CALLS="${cargo_calls_log}" \
VINPST_USER_CLI_BINARY="${runtime_bin}/vinpst" \
VINPST_USER_DAEMON_BINARY="${runtime_bin}/vinpst-daemon" \
VINPST_USER_SHERPA_RUNTIME_LIB_DIR="${runtime_source_dir}" \
VINPST_USER_PROFILE="${profile}" \
VINPST_USER_STATUS=1 \
VINPST_USER_RUNTIME_STATUS=0 \
scripts/install/install-user-ime.sh >"${out_dir}/status-skip-runtime.log" 2>&1

if grep -Fq -- "runtime-status" "${calls_log}"; then
  cat "${calls_log}" >&2
  echo "VINPST_USER_RUNTIME_STATUS=0 should skip runtime-status validation" >&2
  exit 1
fi

rm -f "${runtime_lib_dir}/libonnxruntime.so"
set +e
PATH="${stub_bin}:${PATH}" \
HOME="${home_dir}" \
XDG_DATA_HOME="${home_dir}/.local/share" \
VINPST_STUB_CALLS="${calls_log}" \
VINPST_STUB_CARGO_CALLS="${cargo_calls_log}" \
VINPST_USER_CLI_BINARY="${runtime_bin}/vinpst" \
VINPST_USER_DAEMON_BINARY="${runtime_bin}/vinpst-daemon" \
VINPST_USER_SHERPA_RUNTIME_LIB_DIR="${runtime_source_dir}" \
VINPST_USER_PROFILE="${profile}" \
VINPST_USER_STATUS=1 \
scripts/install/install-user-ime.sh >"${out_dir}/status-missing-runtime.log" 2>&1
missing_runtime_status=$?
set -e
if [[ "${missing_runtime_status}" -eq 0 ]]; then
  cat "${out_dir}/status-missing-runtime.log" >&2
  echo "status unexpectedly accepted a missing installed runtime library" >&2
  exit 1
fi
if ! grep -Fq -- "installed native sherpa runtime library is missing: ${runtime_lib_dir}/libonnxruntime.so" "${out_dir}/status-missing-runtime.log"; then
  cat "${out_dir}/status-missing-runtime.log" >&2
  echo "missing runtime failure did not identify the exact library" >&2
  exit 1
fi

PATH="${stub_bin}:${PATH}" \
HOME="${home_dir}" \
XDG_DATA_HOME="${home_dir}/.local/share" \
VINPST_STUB_CALLS="${calls_log}" \
VINPST_STUB_CARGO_CALLS="${cargo_calls_log}" \
VINPST_USER_CLI_BINARY="${runtime_bin}/vinpst" \
VINPST_USER_DAEMON_BINARY="${runtime_bin}/vinpst-daemon" \
VINPST_USER_SHERPA_RUNTIME_LIB_DIR="${runtime_source_dir}" \
VINPST_USER_PROFILE="${profile}" \
VINPST_USER_REMOVE=1 \
scripts/install/install-user-ime.sh >"${out_dir}/remove.log" 2>&1
for removed in "${service_path}" "${runtime_lib_dir}" "${env_path}" "${daemon_wrapper_path}" "${locale_catalog_path}"; do
  if [[ -e "${removed}" ]]; then
    cat "${out_dir}/remove.log" >&2
    echo "remove left native install artifact: ${removed}" >&2
    exit 1
  fi
done

printf 'user-ime-sherpa profile %s smoke passed\n' "${profile}"
