#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

profile="${VINPUT_USER_PROFILE:-mock}"
remove_user="${VINPUT_USER_REMOVE:-}"
status_user="${VINPUT_USER_STATUS:-}"
config_path=""
home_dir="${HOME:?HOME must be set for user IME installation}"
data_home="${XDG_DATA_HOME:-${home_dir}/.local/share}"
bin_dir="${VINPUT_USER_BIN_DIR:-${home_dir}/.local/bin}"
lib_dir="${VINPUT_USER_FCITX_LIB_DIR:-${home_dir}/.local/lib/fcitx5}"
addon_dir="${VINPUT_USER_FCITX_ADDON_DIR:-${data_home}/fcitx5/addon}"
config_dir="${VINPUT_USER_CONFIG_DIR:-${data_home}/fcitx-vinput}"
config_home="${XDG_CONFIG_HOME:-${home_dir}/.config}"
autostart_dir="${VINPUT_USER_AUTOSTART_DIR:-${config_home}/autostart}"
env_file="${config_dir}/fcitx-vinput.env"
fcitx_env_wrapper="${config_dir}/fcitx5-with-vinput-env.sh"
fcitx_autostart_file="${autostart_dir}/org.fcitx.Fcitx5.desktop"

daemon_path="${VINPUT_USER_DAEMON:-${bin_dir}/vinput-daemon}"
module_path="${lib_dir}/fcitx5-vinput.so"
addon_conf_path="${addon_dir}/vinput.conf"
build_dir="target/cpp/fcitx5-user-ime"
command_asr_wav_helper_path="${VINPUT_USER_COMMAND_ASR_WAV_HELPER:-${bin_dir}/vinput-command-asr-wav-helper}"


profile_cargo_features() {
  case "${profile}" in
    configured-pipewire-live|real-command-asr-wav)
      printf '%s\n' 'pipewire-backend'
      ;;
    sherpa-sense-voice-live)
      printf '%s\n' 'pipewire-backend,sherpa-onnx-backend'
      ;;
  esac
}

cargo_build_vinput_cli() {
  local cargo_features
  cargo_features="$(profile_cargo_features)"
  if [[ -n "${cargo_features}" ]]; then
    cargo build -q -p vinput-cli --features "${cargo_features}"
  else
    cargo build -q -p vinput-cli
  fi
}

cargo_build_vinput_binaries() {
  local cargo_features
  cargo_features="$(profile_cargo_features)"
  if [[ -n "${cargo_features}" ]]; then
    cargo build -q -p vinput-cli -p vinput-daemon --features "${cargo_features}"
  else
    cargo build -q -p vinput-cli -p vinput-daemon
  fi
}

shell_quote() {
  python3 - "$1" <<'PY'
import shlex
import sys
print(shlex.quote(sys.argv[1]))
PY
}

write_command_asr_wav_helper_config() {
  local output_path="$1"
  local helper_path="$2"
  local external_command="$3"
  local helper_timeout_ms="$4"
  local provider_timeout_ms="$5"
  mkdir -p "$(dirname "${output_path}")"
  python3 - "${output_path}" "${helper_path}" "${external_command}" "${helper_timeout_ms}" "${provider_timeout_ms}" <<'PY'
import json
import pathlib
import sys

output_path = pathlib.Path(sys.argv[1])
helper_path = sys.argv[2]
external_command = sys.argv[3]
helper_timeout_ms = int(sys.argv[4])
provider_timeout_arg = sys.argv[5].strip()
provider_timeout_ms = (
    int(provider_timeout_arg) if provider_timeout_arg else helper_timeout_ms + 2000
)
if helper_timeout_ms <= 0:
    raise SystemExit("VINPUT_USER_COMMAND_ASR_WAV_TIMEOUT_MS must be positive")
if provider_timeout_ms <= 0:
    raise SystemExit("VINPUT_USER_COMMAND_ASR_WAV_PROVIDER_TIMEOUT_MS must be positive")
if provider_timeout_ms <= helper_timeout_ms:
    provider_timeout_ms = helper_timeout_ms + 1000

config = {
    "version": 1,
    "asr": {
        "active_provider": "real-command-asr-wav",
        "normalize_audio": False,
        "input_gain": 1.0,
        "providers": [
            {
                "id": "real-command-asr-wav",
                "type": "command",
                "command": helper_path,
                "args": [
                    "--timeout-ms",
                    str(helper_timeout_ms),
                    "--",
                    "sh",
                    "-c",
                    "$VINPUT_REAL_ASR_COMMAND",
                ],
                "env": {"VINPUT_REAL_ASR_COMMAND": external_command},
                "timeout_ms": provider_timeout_ms,
            }
        ],
    },
    "scenes": {
        "active_scene": "raw",
        "definitions": [{"id": "raw", "label": "Raw", "candidate_count": 0}],
    },
}
output_path.write_text(json.dumps(config, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
PY
}


write_sherpa_sense_voice_config() {
  local output_path="$1"
  local model_dir="$2"
  local hotwords_file="$3"
  local timeout_ms="$4"
  mkdir -p "$(dirname "${output_path}")"
  python3 - "${output_path}" "${model_dir}" "${hotwords_file}" "${timeout_ms}" <<'PY'
import json
import pathlib
import sys

output_path = pathlib.Path(sys.argv[1])
model_dir = pathlib.Path(sys.argv[2]).expanduser().resolve()
hotwords_arg = sys.argv[3].strip()
timeout_arg = sys.argv[4].strip()
if not model_dir.is_dir():
    raise SystemExit(f"VINPUT_USER_SHERPA_MODEL must be an existing model directory: {model_dir}")
if not any((model_dir / name).is_file() for name in ("model.int8.onnx", "model.onnx")):
    raise SystemExit(
        "VINPUT_USER_SHERPA_MODEL must contain model.int8.onnx or model.onnx "
        f"for the current SenseVoice backend: {model_dir}"
    )
if not (model_dir / "tokens.txt").is_file():
    raise SystemExit(f"VINPUT_USER_SHERPA_MODEL must contain tokens.txt: {model_dir}")
provider = {
    "id": "sherpa-onnx",
    "type": "local",
    "model": str(model_dir),
}
if hotwords_arg:
    hotwords = pathlib.Path(hotwords_arg).expanduser()
    if not hotwords.is_absolute():
        hotwords = model_dir / hotwords
    hotwords = hotwords.resolve()
    if not hotwords.is_file():
        raise SystemExit(f"VINPUT_USER_SHERPA_HOTWORDS_FILE must be a regular file: {hotwords}")
    provider["hotwords_file"] = str(hotwords)
if timeout_arg:
    timeout_ms = int(timeout_arg)
    if timeout_ms <= 0:
        raise SystemExit("VINPUT_USER_SHERPA_TIMEOUT_MS must be positive")
    provider["timeout_ms"] = timeout_ms

config = {
    "version": 1,
    "asr": {
        "active_provider": "sherpa-onnx",
        "normalize_audio": False,
        "input_gain": 1.0,
        "providers": [provider],
    },
    "scenes": {
        "active_scene": "raw",
        "definitions": [{"id": "raw", "label": "Raw", "candidate_count": 0}],
    },
}
output_path.write_text(json.dumps(config, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
PY
}



profile_default_config_path() {
  case "${profile}" in
    command-demo)
      printf '%s\n' "${VINPUT_USER_CONFIG:-${config_dir}/e2e-command-demo-config.json}"
      ;;
    configured-pipewire-live)
      printf '%s\n' "${VINPUT_USER_CONFIG:-${config_dir}/e2e-configured-pipewire-live.json}"
      ;;
    real-command-asr-wav)
      printf '%s\n' "${VINPUT_USER_CONFIG:-${config_dir}/real-command-asr-wav.json}"
      ;;
    sherpa-sense-voice-live)
      printf '%s\n' "${VINPUT_USER_CONFIG:-${config_dir}/sherpa-sense-voice-live.json}"
      ;;
  esac
}

runtime_status_requested() {
  case "${VINPUT_USER_RUNTIME_STATUS:-}" in
    1|true|yes|on)
      return 0
      ;;
    0|false|no|off)
      return 1
      ;;
    "")
      [[ "${profile}" == "sherpa-sense-voice-live" ]]
      ;;
    *)
      echo "unsupported VINPUT_USER_RUNTIME_STATUS: ${VINPUT_USER_RUNTIME_STATUS}" >&2
      echo "supported values: 1, true, yes, on, 0, false, no, off" >&2
      exit 2
      ;;
  esac
}

run_runtime_status_validation() {
  if ! runtime_status_requested; then
    return 0
  fi
  local runtime_config="${config_path:-}"
  if [[ -z "${runtime_config}" ]]; then
    runtime_config="$(profile_default_config_path)"
  fi
  if [[ -z "${runtime_config}" || ! -f "${runtime_config}" ]]; then
    echo "VINPUT_USER_RUNTIME_STATUS requested but config is missing: ${runtime_config}" >&2
    exit 2
  fi
  cargo_build_vinput_binaries
  echo "Runtime status validation:"
  target/debug/vinput-daemon --configured-backends --config "${runtime_config}" runtime-status
}

write_fcitx_env_integration() {
  local quoted_env_file
  quoted_env_file="$(shell_quote "${env_file}")"
  mkdir -p "$(dirname "${fcitx_env_wrapper}")" "$(dirname "${fcitx_autostart_file}")"
  cat >"${fcitx_env_wrapper}" <<EOF
#!/usr/bin/env sh
# Generated by fcitx-vinput-rs. Source the same environment as the live probe.
. ${quoted_env_file}
exec "\${VINPUT_FCITX5_BIN:-fcitx5}" "\$@"
EOF
  chmod 755 "${fcitx_env_wrapper}"
  cat >"${fcitx_autostart_file}" <<EOF
[Desktop Entry]
Type=Application
Name=Fcitx 5 with fcitx-vinput
Comment=Start Fcitx5 with the user-installed fcitx-vinput addon environment
Exec=${fcitx_env_wrapper}
Terminal=false
X-GNOME-Autostart-enabled=true
X-fcitx-vinput-managed=true
EOF
}

remove_fcitx_env_integration() {
  rm -f "${fcitx_env_wrapper}" "${env_file}"
  if [[ -f "${fcitx_autostart_file}" ]] && grep -qx 'X-fcitx-vinput-managed=true' "${fcitx_autostart_file}"; then
    rm -f "${fcitx_autostart_file}"
  fi
}

doctor_status() {
  cargo_build_vinput_cli

  local status_config="${config_path:-}"
  if [[ -z "${status_config}" ]]; then
    status_config="$(profile_default_config_path)"
  fi
  if [[ -z "${status_config}" ]]; then
    local service_file="${data_home}/dbus-1/services/org.fcitx.Vinput.service"
    if [[ -f "${service_file}" ]]; then
      status_config="$(python3 - "${service_file}" <<'PY'
import shlex
import sys

path = sys.argv[1]
for line in open(path, encoding="utf-8"):
    if not line.startswith("Exec="):
        continue
    parts = shlex.split(line.removeprefix("Exec=").strip())
    for index, part in enumerate(parts):
        if part == "--config" and index + 1 < len(parts):
            print(parts[index + 1])
            raise SystemExit(0)
raise SystemExit(0)
PY
)"
    fi
  fi

  local args=(doctor)
  if [[ -n "${status_config}" && -f "${status_config}" ]]; then
    args+=(--config "${status_config}")
  fi
  target/debug/vinput "${args[@]}"
}

if [[ "${remove_user}" == "1" || "${remove_user}" == "true" ]]; then
  rm -f "${module_path}" "${addon_conf_path}"
  if [[ -z "${VINPUT_USER_COMMAND_ASR_WAV_HELPER:-}" ]]; then
    rm -f "${command_asr_wav_helper_path}"
  fi
  remove_fcitx_env_integration
  cargo_build_vinput_cli
  target/debug/vinput activation-service --remove-user
  echo "Removed user IME files if present:"
  echo "  ${module_path}"
  echo "  ${addon_conf_path}"
  echo "  ${env_file}"
  echo "  ${fcitx_env_wrapper}"
  echo "  ${fcitx_autostart_file}"
  echo "  ${command_asr_wav_helper_path}"
  exit 0
fi

if [[ "${status_user}" == "1" || "${status_user}" == "true" ]]; then
  echo "User IME install status:"
  printf '  module: %s (%s)\n' "${module_path}" "$([[ -f "${module_path}" ]] && echo present || echo missing)"
  printf '  addon metadata: %s (%s)\n' "${addon_conf_path}" "$([[ -f "${addon_conf_path}" ]] && echo present || echo missing)"
  printf '  daemon: %s (%s)\n' "${daemon_path}" "$([[ -x "${daemon_path}" ]] && echo executable || echo missing)"
  printf '  command ASR WAV helper: %s (%s)\n' "${command_asr_wav_helper_path}" "$([[ -x "${command_asr_wav_helper_path}" ]] && echo executable || echo missing)"
  printf '  environment file: %s (%s)\n' "${env_file}" "$([[ -f "${env_file}" ]] && echo present || echo missing)"
  printf '  Fcitx env wrapper: %s (%s)\n' "${fcitx_env_wrapper}" "$([[ -x "${fcitx_env_wrapper}" ]] && echo executable || echo missing)"
  printf '  Fcitx autostart: %s (%s)\n' "${fcitx_autostart_file}" "$([[ -f "${fcitx_autostart_file}" ]] && echo present || echo missing)"
  doctor_status
  run_runtime_status_validation
  exit 0
fi

audio_backend="${VINPUT_USER_AUDIO_BACKEND:-}"
daemon_args=()
configured_backends="${VINPUT_USER_CONFIGURED_BACKENDS:-}"

case "${profile}" in
  mock)
    ;;
  command-demo)
    configured_backends="1"
    config_path="${VINPUT_USER_CONFIG:-${config_dir}/e2e-command-demo-config.json}"
    wav_path="${VINPUT_USER_WAV:-${config_dir}/e2e-command-demo.wav}"
    mkdir -p "$(dirname "${config_path}")" "$(dirname "${wav_path}")"
    install -Dm644 data/e2e-command-demo-config.json "${config_path}"
    python3 scripts/write-demo-wav.py "${wav_path}"
    daemon_args+=(--daemon-arg=--wav --daemon-arg "${wav_path}")
    ;;
  configured-pipewire-live)
    configured_backends="1"
    audio_backend="${audio_backend:-pipewire}"
    config_path="${VINPUT_USER_CONFIG:-${config_dir}/e2e-configured-pipewire-live.json}"
    install -Dm644 data/e2e-configured-pipewire-live.json "${config_path}"
    ;;
  real-command-asr-wav)
    configured_backends="1"
    audio_backend="${audio_backend:-pipewire}"
    config_path="${VINPUT_USER_CONFIG:-${config_dir}/real-command-asr-wav.json}"
    external_asr_command="${VINPUT_USER_COMMAND_ASR_WAV_COMMAND:-}"
    if [[ -z "${external_asr_command}" ]]; then
      echo "VINPUT_USER_COMMAND_ASR_WAV_COMMAND is required for real-command-asr-wav" >&2
      echo 'example: VINPUT_USER_COMMAND_ASR_WAV_COMMAND="whisper-cli -m model.bin -f \"$VINPUT_ASR_WAV\"" VINPUT_USER_PROFILE=real-command-asr-wav scripts/install-user-ime.sh' >&2
      exit 2
    fi
    helper_timeout_ms="${VINPUT_USER_COMMAND_ASR_WAV_TIMEOUT_MS:-30000}"
    provider_timeout_ms="${VINPUT_USER_COMMAND_ASR_WAV_PROVIDER_TIMEOUT_MS:-}"
    install -Dm755 scripts/command-asr-wav-helper.py "${command_asr_wav_helper_path}"
    write_command_asr_wav_helper_config "${config_path}" "${command_asr_wav_helper_path}" "${external_asr_command}" "${helper_timeout_ms}" "${provider_timeout_ms}"
    ;;
  sherpa-sense-voice-live)
    configured_backends="1"
    audio_backend="${audio_backend:-pipewire}"
    config_path="${VINPUT_USER_CONFIG:-${config_dir}/sherpa-sense-voice-live.json}"
    sherpa_model_dir="${VINPUT_USER_SHERPA_MODEL:-}"
    if [[ -z "${sherpa_model_dir}" ]]; then
      echo "VINPUT_USER_SHERPA_MODEL is required for sherpa-sense-voice-live" >&2
      echo 'example: VINPUT_USER_SHERPA_MODEL=/path/to/sherpa-onnx-sense-voice... VINPUT_USER_PROFILE=sherpa-sense-voice-live scripts/install-user-ime.sh' >&2
      exit 2
    fi
    write_sherpa_sense_voice_config "${config_path}" "${sherpa_model_dir}" "${VINPUT_USER_SHERPA_HOTWORDS_FILE:-}" "${VINPUT_USER_SHERPA_TIMEOUT_MS:-}"
    ;;
  *)
    echo "unsupported VINPUT_USER_PROFILE: ${profile}" >&2
    echo "supported profiles: mock, command-demo, configured-pipewire-live, real-command-asr-wav, sherpa-sense-voice-live" >&2
    exit 2
    ;;
esac

cargo_build_vinput_binaries
install -Dm755 target/debug/vinput-daemon "${daemon_path}"

rm -rf "${build_dir}"
cmake -S cpp/fcitx5-addon -B "${build_dir}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON \
  -DVINPUT_DAEMON_EXECUTABLE="${daemon_path}"
cmake --build "${build_dir}" --target fcitx5_vinput_addon --parallel
install -Dm755 "${build_dir}/fcitx5-vinput.so" "${module_path}"
install -Dm644 "${build_dir}/vinput-addon.conf" "${addon_conf_path}"
mkdir -p "$(dirname "${env_file}")"
cat >"${env_file}" <<EOF
# Source this before launching Fcitx5 when using the user-installed fcitx-vinput addon.
export FCITX_ADDON_DIRS="${lib_dir}:${FCITX_ADDON_DIRS:-/usr/lib/fcitx5}"
export XDG_DATA_HOME="${data_home}"
EOF
write_fcitx_env_integration

activation_args=(activation-service --daemon "${daemon_path}" --user)
if [[ "${configured_backends}" == "1" || "${configured_backends}" == "true" || -n "${config_path}" ]]; then
  activation_args+=(--configured-backends)
fi
if [[ -n "${config_path}" ]]; then
  activation_args+=(--config "${config_path}")
fi
if [[ -n "${audio_backend}" ]]; then
  activation_args+=(--audio-backend "${audio_backend}")
fi
activation_args+=("${daemon_args[@]}")
target/debug/vinput "${activation_args[@]}"
run_runtime_status_validation

cat <<EOF
Installed user IME files:
  daemon: ${daemon_path}
  addon module: ${module_path}
  addon metadata: ${addon_conf_path}
  environment file: ${env_file}
  Fcitx env wrapper: ${fcitx_env_wrapper}
  Fcitx autostart override: ${fcitx_autostart_file}

Restart Fcitx5 with the generated environment, then use the retained addon triggers:
  Right Ctrl press/release: start/stop normal dictation
  F10 press/release: start/stop command dictation using selected text

Override trigger keys before launching Fcitx5 if needed:
  VINPUT_FCITX_NORMAL_TRIGGER=F8
  VINPUT_FCITX_COMMAND_TRIGGER=F9

For the current session, restart through the wrapper:
  ${fcitx_env_wrapper} -r

For next login, the generated user autostart override starts Fcitx5 with the same environment.
If your desktop ignores XDG autostart, source this before launching Fcitx5 manually:
  . ${env_file}
EOF

doctor_status
