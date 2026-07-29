#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

model_dir="${VINPUT_SHERPA_MODEL:-}"
wav_path="${VINPUT_SHERPA_WAV:-}"
expected_text="${VINPUT_SHERPA_EXPECT_TEXT:-}"
runtime_lib_dir="${VINPUT_SHERPA_RUNTIME_LIB_DIR:-${repo_root}/target/debug}"
frontend_bin="${VINPUT_NATIVE_ACTIVATION_FRONTEND_BIN:-}"
out_dir="${VINPUT_USER_NATIVE_ACTIVATION_SMOKE_DIR:-target/tmp/user-ime-sherpa-native-activation-smoke}"
if [[ "${out_dir}" = /* ]]; then
  out_dir_abs="${out_dir}"
else
  out_dir_abs="${repo_root}/${out_dir}"
fi
home_dir="${out_dir_abs}/home"
data_home="${home_dir}/.local/share"
config_home="${home_dir}/.config"
install_log="${out_dir_abs}/install.log"
status_output="${out_dir_abs}/daemon-status.json"
start_output="${out_dir_abs}/recording-start.json"
stop_output="${out_dir_abs}/recording-stop.json"
service_file="${data_home}/dbus-1/services/org.fcitx.Vinput.service"
daemon_path="${home_dir}/.local/bin/vinput-daemon"
daemon_wrapper="${data_home}/fcitx-vinput/vinput-daemon-with-vinput-env.sh"
installed_runtime_lib_dir="${data_home}/fcitx-vinput/runtime/lib"
rustup_home="${RUSTUP_HOME:-$(rustup show home)}"
cargo_home="${CARGO_HOME:-${HOME}/.cargo}"

if [[ -z "${model_dir}" ]]; then
  echo "VINPUT_SHERPA_MODEL is required and must point at a supported local model directory" >&2
  exit 2
fi
if [[ -z "${wav_path}" ]]; then
  echo "VINPUT_SHERPA_WAV is required and must point at an uncompressed PCM16 WAV file" >&2
  exit 2
fi
if [[ -z "${expected_text}" ]]; then
  echo "VINPUT_SHERPA_EXPECT_TEXT is required for an exact activation recognition assertion" >&2
  exit 2
fi

if [[ ! -d "${model_dir}" ]]; then
  echo "VINPUT_SHERPA_MODEL must be an existing directory: ${model_dir}" >&2
  exit 2
fi
if [[ ! -f "${wav_path}" ]]; then
  echo "VINPUT_SHERPA_WAV must be an existing file: ${wav_path}" >&2
  exit 2
fi
if [[ ! -d "${runtime_lib_dir}" ]]; then
  echo "VINPUT_SHERPA_RUNTIME_LIB_DIR must be an existing directory: ${runtime_lib_dir}" >&2
  exit 2
fi
model_dir="$(realpath "${model_dir}")"
wav_path="$(realpath "${wav_path}")"
runtime_lib_dir="$(realpath "${runtime_lib_dir}")"
if [[ -n "${frontend_bin}" ]]; then
  if [[ ! -x "${frontend_bin}" ]]; then
    echo "VINPUT_NATIVE_ACTIVATION_FRONTEND_BIN must be executable: ${frontend_bin}" >&2
    exit 2
  fi
  frontend_bin="$(realpath "${frontend_bin}")"
fi
if [[ ! -f "${model_dir}/vinput-model.json" ]]; then
  echo "native activation smoke requires typed metadata: ${model_dir}/vinput-model.json" >&2
  exit 2
fi
for library in libsherpa-onnx-c-api.so libonnxruntime.so; do
  if [[ ! -f "${runtime_lib_dir}/${library}" ]]; then
    echo "native activation smoke runtime library is missing: ${runtime_lib_dir}/${library}" >&2
    exit 2
  fi
done

rm -rf "${out_dir_abs}"
mkdir -p "${home_dir}" "${config_home}"

common_env=(
  HOME="${home_dir}"
  XDG_DATA_HOME="${data_home}"
  XDG_CONFIG_HOME="${config_home}"
  RUSTUP_HOME="${rustup_home}"
  CARGO_HOME="${cargo_home}"
  VINPUT_USER_PROFILE=sherpa-native-live
  VINPUT_USER_AUDIO_BACKEND=mock
  VINPUT_USER_SHERPA_MODEL="${model_dir}"
  VINPUT_USER_SHERPA_RUNTIME_LIB_DIR="${runtime_lib_dir}"
  VINPUT_USER_NATIVE_WAV="${wav_path}"
)

env "${common_env[@]}" scripts/install-user-ime.sh >"${install_log}" 2>&1

for required in "${service_file}" "${daemon_path}" "${daemon_wrapper}" \
  "${installed_runtime_lib_dir}/libsherpa-onnx-c-api.so" \
  "${installed_runtime_lib_dir}/libonnxruntime.so"; do
  if [[ ! -e "${required}" ]]; then
    cat "${install_log}" >&2
    echo "native activation install is missing: ${required}" >&2
    exit 1
  fi
done

grep -Fq -- "Exec=${daemon_wrapper} --dbus" "${service_file}"
grep -Fq -- "--configured-backends" "${service_file}"
grep -Fq -- "--wav ${wav_path}" "${service_file}"

cleanup_install() {
  env "${common_env[@]}" VINPUT_USER_REMOVE=1 scripts/install-user-ime.sh >/dev/null 2>&1 || true
}
trap cleanup_install EXIT

HOME="${home_dir}" \
XDG_DATA_HOME="${data_home}" \
XDG_CONFIG_HOME="${config_home}" \
VINPUT_ACTIVATION_EXPECTED_TEXT="${expected_text}" \
VINPUT_ACTIVATION_EXPECTED_DAEMON="${daemon_path}" \
VINPUT_ACTIVATION_EXPECTED_WAV="${wav_path}" \
VINPUT_ACTIVATION_STATUS_OUTPUT="${status_output}" \
VINPUT_ACTIVATION_START_OUTPUT="${start_output}" \
VINPUT_ACTIVATION_STOP_OUTPUT="${stop_output}" \
VINPUT_ACTIVATION_FRONTEND_BIN="${frontend_bin}" \
  timeout 120s dbus-run-session -- bash -euo pipefail <<'INNER'
if [[ -n "${VINPUT_ACTIVATION_FRONTEND_BIN}" ]]; then
  VINPUT_NATIVE_FRONTEND_EXPECTED_TEXT="${VINPUT_ACTIVATION_EXPECTED_TEXT}" \
    "${VINPUT_ACTIVATION_FRONTEND_BIN}"
fi

target/debug/vinput daemon status --json >"${VINPUT_ACTIVATION_STATUS_OUTPUT}"
python3 - \
  "${VINPUT_ACTIVATION_STATUS_OUTPUT}" \
  "${VINPUT_ACTIVATION_EXPECTED_DAEMON}" \
  "${VINPUT_ACTIVATION_EXPECTED_WAV}" <<'PY'
import json
import pathlib
import sys

status = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
expected_daemon = sys.argv[2]
expected_wav = sys.argv[3]
owner = status["owner"]
cmdline = owner["process"]["cmdline"]
assert owner["ok"] is True, owner
assert owner["process"]["exe"] == expected_daemon, owner
assert cmdline[0] == expected_daemon, cmdline
assert "--configured-backends" in cmdline, cmdline
wav_index = cmdline.index("--wav")
assert cmdline[wav_index + 1] == expected_wav, cmdline
asr = status["asr_backend"]
assert asr["effective_provider_id"] == "sherpa-onnx", asr
assert asr["has_effective_backend"] is True, asr
assert asr["last_error"] == "", asr
print(f"activated native daemon: {expected_daemon}")
PY

if [[ -z "${VINPUT_ACTIVATION_FRONTEND_BIN}" ]]; then
  target/debug/vinput recording start --json >"${VINPUT_ACTIVATION_START_OUTPUT}"
  target/debug/vinput recording stop --json >"${VINPUT_ACTIVATION_STOP_OUTPUT}"
  python3 - \
    "${VINPUT_ACTIVATION_STOP_OUTPUT}" \
    "${VINPUT_ACTIVATION_EXPECTED_TEXT}" <<'PY'
import json
import pathlib
import sys

outer = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
payload = json.loads(outer["payload_json"])
actual = payload["commit_text"]
expected = sys.argv[2]
if actual != expected:
    raise SystemExit(f"unexpected native activation recognition: {actual!r}")
print(f"native activation recognition: {actual}")
PY
fi
INNER

cleanup_install
trap - EXIT
for removed in "${service_file}" "${daemon_wrapper}" "${installed_runtime_lib_dir}"; do
  if [[ -e "${removed}" ]]; then
    echo "native activation cleanup left artifact: ${removed}" >&2
    exit 1
  fi
done

echo "user IME native D-Bus activation smoke passed"
