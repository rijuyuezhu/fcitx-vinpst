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
native_runtime_dir="${VINPUT_USER_SHERPA_RUNTIME_DIR:-${data_home}/fcitx-vinput/runtime}"
native_runtime_lib_dir="${native_runtime_dir}/lib"
daemon_env_wrapper="${config_dir}/vinput-daemon-with-vinput-env.sh"
activation_service_path="${data_home}/dbus-1/services/org.fcitx.Vinput.service"
runtime_activation_service_path=""
if [[ -n "${XDG_RUNTIME_DIR:-}" ]]; then
  runtime_activation_service_path="${XDG_RUNTIME_DIR}/dbus-1/services/org.fcitx.Vinput.service"
fi
runtime_activation_mode="${VINPUT_USER_RUNTIME_ACTIVATION:-auto}"

daemon_path="${VINPUT_USER_DAEMON:-${bin_dir}/vinput-daemon}"
activation_daemon_path="${daemon_path}"
cli_binary="${VINPUT_USER_CLI_BINARY:-target/debug/vinput}"
daemon_binary="${VINPUT_USER_DAEMON_BINARY:-target/debug/vinput-daemon}"
module_path="${lib_dir}/fcitx5-vinput.so"
addon_conf_path="${addon_dir}/vinput.conf"
build_dir="target/cpp/fcitx5-user-ime"
locale_catalog_source="${build_dir}/locale/zh_CN/LC_MESSAGES/fcitx5-vinput.mo"
locale_catalog_path="${data_home}/locale/zh_CN/LC_MESSAGES/fcitx5-vinput.mo"
command_asr_wav_helper_path="${VINPUT_USER_COMMAND_ASR_WAV_HELPER:-${bin_dir}/vinput-command-asr-wav-helper}"
vad_dir="${data_home}/fcitx-vinput/vad"
vad_model_path="${vad_dir}/silero_vad.onnx"
vad_license_path="${vad_dir}/LICENSE"


is_native_sherpa_profile() {
  case "${profile}" in
    sherpa-native-live|sherpa-native-command-live|sherpa-sense-voice-live)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

profile_cli_features() {
  case "${profile}" in
    configured-pipewire-live|real-command-asr-wav)
      printf '%s\n' 'pipewire-backend'
      ;;
    sherpa-native-live|sherpa-native-command-live|sherpa-sense-voice-live)
      printf '%s\n' 'pipewire-backend,sherpa-onnx-backend'
      ;;
  esac
}

profile_daemon_features() {
  case "${profile}" in
    configured-pipewire-live|real-command-asr-wav)
      printf '%s\n' 'pipewire-backend'
      ;;
    sherpa-native-live|sherpa-native-command-live|sherpa-sense-voice-live)
      printf '%s\n' 'pipewire-backend,sherpa-onnx-backend'
      ;;
  esac
}

cargo_build_vinput_cli() {
  local cargo_features
  cargo_features="$(profile_cli_features)"
  if [[ -n "${cargo_features}" ]]; then
    cargo build -q -p vinput-cli --features "${cargo_features}"
  else
    cargo build -q -p vinput-cli
  fi
}

cargo_build_vinput_daemon() {
  local cargo_features
  cargo_features="$(profile_daemon_features)"
  if [[ -n "${cargo_features}" ]]; then
    cargo build -q -p vinput-daemon --features "${cargo_features}"
  else
    cargo build -q -p vinput-daemon
  fi
}

cargo_build_vinput_binaries() {
  cargo_build_vinput_cli
  cargo_build_vinput_daemon
}

shell_quote() {
  python3 - "$1" <<'PY'
import shlex
import sys
print(shlex.quote(sys.argv[1]))
PY
}

home_matches_current_account() {
  python3 - "${home_dir}" <<'PY'
import os
import pwd
import sys

requested = os.path.realpath(sys.argv[1])
account = os.path.realpath(pwd.getpwuid(os.getuid()).pw_dir)
raise SystemExit(0 if requested == account else 1)
PY
}

runtime_activation_enabled() {
  case "${runtime_activation_mode}" in
    1|true|yes)
      return 0
      ;;
    0|false|no)
      return 1
      ;;
    auto)
      [[ -n "${runtime_activation_service_path}" ]] && home_matches_current_account
      ;;
    *)
      echo "VINPUT_USER_RUNTIME_ACTIVATION must be auto, true, or false" >&2
      exit 2
      ;;
  esac
}

publish_runtime_activation_service() {
  if ! runtime_activation_enabled; then
    return 0
  fi
  if [[ ! -f "${activation_service_path}" ]]; then
    echo "persistent user activation service is missing: ${activation_service_path}" >&2
    exit 2
  fi
  install -Dm644 "${activation_service_path}" "${runtime_activation_service_path}"
}

remove_runtime_activation_service() {
  if [[ -n "${runtime_activation_service_path}" ]]; then
    rm -f "${runtime_activation_service_path}"
  fi
}

require_installed_sherpa_runtime() {
  local required
  for required in libsherpa-onnx-c-api.so libonnxruntime.so; do
    if [[ ! -f "${native_runtime_lib_dir}/${required}" ]]; then
      echo "installed native sherpa runtime library is missing: ${native_runtime_lib_dir}/${required}" >&2
      echo "reinstall with VINPUT_USER_PROFILE=sherpa-native-live and a validated VINPUT_USER_SHERPA_RUNTIME_LIB_DIR" >&2
      exit 2
    fi
  done
}

with_native_runtime() {
  if is_native_sherpa_profile; then
    require_installed_sherpa_runtime
    LD_LIBRARY_PATH="${native_runtime_lib_dir}${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}" "$@"
  else
    "$@"
  fi
}

install_sherpa_runtime_libraries() {
  local source_dir="${VINPUT_USER_SHERPA_RUNTIME_LIB_DIR:-target/debug}"
  local required
  for required in libsherpa-onnx-c-api.so libonnxruntime.so; do
    if [[ ! -f "${source_dir}/${required}" ]]; then
      echo "native sherpa runtime library is missing: ${source_dir}/${required}" >&2
      echo "set VINPUT_USER_SHERPA_RUNTIME_LIB_DIR to the bundle that passed local smoke" >&2
      exit 2
    fi
  done

  rm -rf "${native_runtime_lib_dir}"
  mkdir -p "${native_runtime_lib_dir}"
  local runtime_files=()
  shopt -s nullglob
  runtime_files=(
    "${source_dir}"/libsherpa-onnx*.so*
    "${source_dir}"/libonnxruntime.so*
  )
  shopt -u nullglob
  local source
  for source in "${runtime_files[@]}"; do
    install -Dm755 "${source}" "${native_runtime_lib_dir}/$(basename "${source}")"
  done
}

write_daemon_env_wrapper() {
  local quoted_env_file
  local quoted_daemon
  quoted_env_file="$(shell_quote "${env_file}")"
  quoted_daemon="$(shell_quote "${daemon_path}")"
  mkdir -p "$(dirname "${daemon_env_wrapper}")"
  cat >"${daemon_env_wrapper}" <<EOF
#!/usr/bin/env sh
# Generated by fcitx-vinput-rs for D-Bus activation with the native runtime bundle.
. ${quoted_env_file}
exec ${quoted_daemon} "\$@"
EOF
  chmod 755 "${daemon_env_wrapper}"
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


write_sherpa_native_config() {
  local output_path="$1"
  local model_dir="$2"
  local hotwords_file="$3"
  local timeout_ms="$4"
  local command_adapter="$5"
  mkdir -p "$(dirname "${output_path}")"
  python3 - "${output_path}" "${model_dir}" "${hotwords_file}" "${timeout_ms}" "${command_adapter}" <<'PY'
import json
import pathlib
import sys

output_path = pathlib.Path(sys.argv[1])
model_dir = pathlib.Path(sys.argv[2]).expanduser().resolve()
hotwords_arg = sys.argv[3].strip()
timeout_arg = sys.argv[4].strip()
command_adapter = sys.argv[5] == "1"
if not model_dir.is_dir():
    raise SystemExit(f"VINPUT_USER_SHERPA_MODEL must be an existing model directory: {model_dir}")

metadata_path = model_dir / "vinput-model.json"
runtime = "offline"
if metadata_path.is_file():
    try:
        metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"failed to read {metadata_path}: {error}") from error
    family = str(metadata.get("family") or metadata.get("model_type") or "").strip()
    runtime = str(metadata.get("runtime") or "").strip()
    backend = str(metadata.get("backend") or "").strip()
    if not family:
        raise SystemExit(f"{metadata_path} must declare a non-empty family or model_type")
    if runtime not in {"offline", "online"} and backend not in {
        "sherpa-offline",
        "sherpa-streaming",
    }:
        raise SystemExit(
            f"{metadata_path} must select a native sherpa offline or online runtime"
        )
    if runtime not in {"offline", "online"}:
        runtime = "online" if backend == "sherpa-streaming" else "offline"
else:
    # Preserve the metadata-free SenseVoice compatibility layout.
    if not any((model_dir / name).is_file() for name in ("model.int8.onnx", "model.onnx")):
        raise SystemExit(
            f"{model_dir} has no vinput-model.json and must contain "
            "model.int8.onnx or model.onnx for the SenseVoice compatibility path"
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

scenes = [{"id": "raw", "label": "Raw", "candidate_count": 0}]
config = {
    "version": 1,
    "asr": {
        "active_provider": "sherpa-onnx",
        "normalize_audio": False,
        "input_gain": 1.0,
        "vad": {
            "enabled": runtime == "offline",
            "threshold": 0.45,
            "min_speech_duration": 0.15,
            "min_silence_duration": 0.5,
            "speech_pad_ms": 300,
        },
        "providers": [provider],
    },
    "scenes": {
        "active_scene": "raw",
        "definitions": scenes,
    },
}
if command_adapter:
    adapter_program = (
        "import json,sys; req=json.load(sys.stdin); "
        "selected=(req.get('selected_text') or '').strip(); "
        "raw=(req.get('raw_text') or '').strip(); "
        "text=f'adapter-backed: {selected} | command: {raw}'; "
        "print(json.dumps({'text': text}, ensure_ascii=False))"
    )
    config["llm"] = {
        "adapters": [
            {
                "id": "native-command-live-adapter",
                "command": "python3",
                "args": ["-c", adapter_program],
            }
        ]
    }
    scenes.append(
        {
            "id": "__command__",
            "label": "Command",
            "prompt": "Apply the recognized command to the selected text.",
            "candidate_count": 1,
        }
    )
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
    sherpa-native-live)
      printf '%s\n' "${VINPUT_USER_CONFIG:-${config_dir}/sherpa-native-live.json}"
      ;;
    sherpa-native-command-live)
      printf '%s\n' "${VINPUT_USER_CONFIG:-${config_dir}/sherpa-native-command-live.json}"
      ;;
    sherpa-sense-voice-live)
      printf '%s\n' "${VINPUT_USER_CONFIG:-${config_dir}/sherpa-sense-voice-live.json}"
      ;;
  esac
}

activation_service_config_path() {
  if [[ ! -f "${activation_service_path}" ]]; then
    return 0
  fi
  python3 - "${activation_service_path}" <<'PY'
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
      is_native_sherpa_profile
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
  if [[ ! -x "${daemon_path}" ]]; then
    echo "installed daemon is missing or not executable: ${daemon_path}" >&2
    exit 2
  fi
  echo "Runtime status validation:"
  with_native_runtime "${daemon_path}" --configured-backends --config "${runtime_config}" runtime-status
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
  rm -f "${fcitx_env_wrapper}" "${daemon_env_wrapper}" "${env_file}"
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
    status_config="$(activation_service_config_path)"
  fi

  local args=(doctor)
  if [[ -n "${status_config}" && -f "${status_config}" ]]; then
    args+=(--config "${status_config}")
  fi
  with_native_runtime "${cli_binary}" "${args[@]}"
}

if [[ "${remove_user}" == "1" || "${remove_user}" == "true" ]]; then
  rm -f "${module_path}" "${addon_conf_path}" "${locale_catalog_path}"
  if [[ -z "${VINPUT_USER_COMMAND_ASR_WAV_HELPER:-}" ]]; then
    rm -f "${command_asr_wav_helper_path}"
  fi
  rm -f "${vad_model_path}" "${vad_license_path}"
  rm -f "${activation_service_path}"
  remove_runtime_activation_service
  rm -rf "${native_runtime_dir}"
  remove_fcitx_env_integration
  echo "Removed user IME files if present:"
  echo "  ${module_path}"
  echo "  ${addon_conf_path}"
  echo "  ${locale_catalog_path}"
  echo "  ${env_file}"
  echo "  ${fcitx_env_wrapper}"
  echo "  ${daemon_env_wrapper}"
  echo "  ${native_runtime_dir}"
  echo "  ${fcitx_autostart_file}"
  echo "  ${command_asr_wav_helper_path}"
  echo "  ${vad_model_path}"
  exit 0
fi

if [[ "${status_user}" == "1" || "${status_user}" == "true" ]]; then
  if [[ -z "${VINPUT_USER_PROFILE:-}" && -x "${daemon_env_wrapper}" && -d "${native_runtime_lib_dir}" ]]; then
    config_path="$(activation_service_config_path)"
    if [[ -z "${config_path}" ]]; then
      if [[ -f "${config_dir}/sherpa-native-command-live.json" ]]; then
        config_path="${config_dir}/sherpa-native-command-live.json"
      elif [[ -f "${config_dir}/sherpa-native-live.json" ]]; then
        config_path="${config_dir}/sherpa-native-live.json"
      elif [[ -f "${config_dir}/sherpa-sense-voice-live.json" ]]; then
        config_path="${config_dir}/sherpa-sense-voice-live.json"
      fi
    fi
    case "${config_path}" in
      */sherpa-native-command-live.json)
        profile="sherpa-native-command-live"
        ;;
      */sherpa-sense-voice-live.json)
        profile="sherpa-sense-voice-live"
        ;;
      *)
        profile="sherpa-native-live"
        ;;
    esac
  fi
  echo "User IME install status:"
  printf '  module: %s (%s)\n' "${module_path}" "$([[ -f "${module_path}" ]] && echo present || echo missing)"
  printf '  addon metadata: %s (%s)\n' "${addon_conf_path}" "$([[ -f "${addon_conf_path}" ]] && echo present || echo missing)"
  printf '  zh_CN locale catalog: %s (%s)\n' "${locale_catalog_path}" "$([[ -f "${locale_catalog_path}" ]] && echo present || echo missing)"
  printf '  daemon: %s (%s)\n' "${daemon_path}" "$([[ -x "${daemon_path}" ]] && echo executable || echo missing)"
  printf '  command ASR WAV helper: %s (%s)\n' "${command_asr_wav_helper_path}" "$([[ -x "${command_asr_wav_helper_path}" ]] && echo executable || echo missing)"
  printf '  Silero VAD model: %s (%s)\n' "${vad_model_path}" "$([[ -f "${vad_model_path}" ]] && echo present || echo missing)"
  printf '  environment file: %s (%s)\n' "${env_file}" "$([[ -f "${env_file}" ]] && echo present || echo missing)"
  printf '  Fcitx env wrapper: %s (%s)\n' "${fcitx_env_wrapper}" "$([[ -x "${fcitx_env_wrapper}" ]] && echo executable || echo missing)"
  if is_native_sherpa_profile; then
    printf '  daemon env wrapper: %s (%s)\n' "${daemon_env_wrapper}" "$([[ -x "${daemon_env_wrapper}" ]] && echo executable || echo missing)"
    printf '  native runtime libs: %s (%s)\n' "${native_runtime_lib_dir}" "$([[ -f "${native_runtime_lib_dir}/libsherpa-onnx-c-api.so" && -f "${native_runtime_lib_dir}/libonnxruntime.so" ]] && echo present || echo missing)"
  fi
  printf '  Fcitx autostart: %s (%s)\n' "${fcitx_autostart_file}" "$([[ -f "${fcitx_autostart_file}" ]] && echo present || echo missing)"
  doctor_status
  run_runtime_status_validation
  exit 0
fi

audio_backend="${VINPUT_USER_AUDIO_BACKEND:-}"
daemon_args=()
configured_backends="${VINPUT_USER_CONFIGURED_BACKENDS:-}"
install_sherpa_vad=""
install_sherpa_runtime=""

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
  sherpa-native-live|sherpa-native-command-live|sherpa-sense-voice-live)
    configured_backends="1"
    install_sherpa_vad="1"
    install_sherpa_runtime="1"
    audio_backend="${audio_backend:-pipewire}"
    config_path="$(profile_default_config_path)"
    sherpa_model_dir="${VINPUT_USER_SHERPA_MODEL:-}"
    if [[ -z "${sherpa_model_dir}" ]]; then
      echo "VINPUT_USER_SHERPA_MODEL is required for ${profile}" >&2
      echo 'example: VINPUT_USER_SHERPA_MODEL=/path/to/registry-model VINPUT_USER_PROFILE=sherpa-native-live scripts/install-user-ime.sh' >&2
      exit 2
    fi
    command_adapter=""
    if [[ "${profile}" == "sherpa-native-command-live" ]]; then
      command_adapter="1"
    fi
    write_sherpa_native_config "${config_path}" "${sherpa_model_dir}" "${VINPUT_USER_SHERPA_HOTWORDS_FILE:-}" "${VINPUT_USER_SHERPA_TIMEOUT_MS:-}" "${command_adapter}"
    native_wav_path="${VINPUT_USER_NATIVE_WAV:-}"
    if [[ -n "${native_wav_path}" ]]; then
      if [[ ! -f "${native_wav_path}" ]]; then
        echo "VINPUT_USER_NATIVE_WAV must be an existing WAV file: ${native_wav_path}" >&2
        exit 2
      fi
      native_wav_path="$(realpath "${native_wav_path}")"
      daemon_args+=(--daemon-arg=--wav --daemon-arg "${native_wav_path}")
    fi
    ;;
  *)
    echo "unsupported VINPUT_USER_PROFILE: ${profile}" >&2
    echo "supported profiles: mock, command-demo, configured-pipewire-live, real-command-asr-wav, sherpa-native-live, sherpa-native-command-live, sherpa-sense-voice-live" >&2
    exit 2
    ;;
esac

cargo_build_vinput_binaries
install -Dm755 "${daemon_binary}" "${daemon_path}"
if [[ "${install_sherpa_runtime}" == "1" ]]; then
  install_sherpa_runtime_libraries
fi
if [[ "${install_sherpa_vad}" == "1" ]]; then
  install -Dm644 data/vad/silero_vad.onnx "${vad_model_path}"
  install -Dm644 data/vad/LICENSE "${vad_license_path}"
fi

rm -rf "${build_dir}"
cmake -S cpp/fcitx5-addon -B "${build_dir}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON \
  -DVINPUT_DAEMON_EXECUTABLE="${daemon_path}" \
  -DVINPUT_FCITX_RUNTIME_BUILD_LOCALEDIR= \
  -DVINPUT_FCITX_RUNTIME_INSTALL_LOCALEDIR="${data_home}/locale"
cmake --build "${build_dir}" --target fcitx5_vinput_addon --parallel
install -Dm755 "${build_dir}/fcitx5-vinput.so" "${module_path}"
install -Dm644 "${build_dir}/vinput-addon.conf" "${addon_conf_path}"
install -Dm644 "${locale_catalog_source}" "${locale_catalog_path}"
mkdir -p "$(dirname "${env_file}")"
cat >"${env_file}" <<EOF
# Source this before launching Fcitx5 when using the user-installed fcitx-vinput addon.
export FCITX_ADDON_DIRS="${lib_dir}:${FCITX_ADDON_DIRS:-/usr/lib/fcitx5}"
export XDG_DATA_HOME="${data_home}"
EOF
if [[ "${install_sherpa_runtime}" == "1" ]]; then
  cat >>"${env_file}" <<EOF
export VINPUT_SHERPA_RUNTIME_LIB_DIR="${native_runtime_lib_dir}"
export LD_LIBRARY_PATH="${native_runtime_lib_dir}\${LD_LIBRARY_PATH:+:\${LD_LIBRARY_PATH}}"
EOF
  write_daemon_env_wrapper
  activation_daemon_path="${daemon_env_wrapper}"
fi
write_fcitx_env_integration

activation_args=(activation-service --daemon "${activation_daemon_path}" --user)
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
with_native_runtime "${cli_binary}" "${activation_args[@]}"
publish_runtime_activation_service
run_runtime_status_validation

cat <<EOF
Installed user IME files:
  daemon: ${daemon_path}
  addon module: ${module_path}
  addon metadata: ${addon_conf_path}
  zh_CN locale catalog: ${locale_catalog_path}
  environment file: ${env_file}
  Fcitx env wrapper: ${fcitx_env_wrapper}
EOF
if is_native_sherpa_profile; then
  cat <<EOF
  daemon env wrapper: ${daemon_env_wrapper}
  native runtime lib directory: ${native_runtime_lib_dir}
EOF
fi
cat <<EOF
  Fcitx autostart override: ${fcitx_autostart_file}

Restart Fcitx5 with the generated environment, then use the retained addon triggers:
  Right Ctrl press/release: start/stop normal dictation
  F10 press/release: start/stop command dictation using selected text

Override trigger keys before launching Fcitx5 if needed:
  VINPUT_FCITX_NORMAL_TRIGGER=F8
  VINPUT_FCITX_COMMAND_TRIGGER=F9

For the current session, restart through the wrapper:
  ${fcitx_env_wrapper} -dr

For next login, the generated user autostart override starts Fcitx5 with the same environment.
If your desktop ignores XDG autostart, source this before launching Fcitx5 manually:
  . ${env_file}
EOF

doctor_status
