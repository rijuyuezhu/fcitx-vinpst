#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

profile="${VINPUT_USER_PROFILE:-mock}"
remove_user="${VINPUT_USER_REMOVE:-}"
status_user="${VINPUT_USER_STATUS:-}"
home_dir="${HOME:?HOME must be set for user IME installation}"
data_home="${XDG_DATA_HOME:-${home_dir}/.local/share}"
bin_dir="${VINPUT_USER_BIN_DIR:-${home_dir}/.local/bin}"
lib_dir="${VINPUT_USER_FCITX_LIB_DIR:-${home_dir}/.local/lib/fcitx5}"
addon_dir="${VINPUT_USER_FCITX_ADDON_DIR:-${data_home}/fcitx5/addon}"
config_dir="${VINPUT_USER_CONFIG_DIR:-${data_home}/fcitx-vinput}"
env_file="${config_dir}/fcitx-vinput.env"

daemon_path="${VINPUT_USER_DAEMON:-${bin_dir}/vinput-daemon}"
module_path="${lib_dir}/fcitx5-vinput.so"
addon_conf_path="${addon_dir}/vinput.conf"
build_dir="target/cpp/fcitx5-user-ime"

doctor_status() {
  if [[ ! -x target/debug/vinput ]]; then
    cargo build -q -p vinput-cli
  fi

  local status_config="${config_path:-}"
  if [[ -z "${status_config}" ]]; then
    case "${profile}" in
      command-demo)
        status_config="${VINPUT_USER_CONFIG:-${config_dir}/e2e-command-demo-config.json}"
        ;;
      configured-pipewire-live)
        status_config="${VINPUT_USER_CONFIG:-${config_dir}/e2e-configured-pipewire-live.json}"
        ;;
    esac
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
  cargo build -q -p vinput-cli
  target/debug/vinput activation-service --remove-user
  echo "Removed user IME files if present:"
  echo "  ${module_path}"
  echo "  ${addon_conf_path}"
  echo "  ${env_file}"
  exit 0
fi

if [[ "${status_user}" == "1" || "${status_user}" == "true" ]]; then
  echo "User IME install status:"
  printf '  module: %s (%s)\n' "${module_path}" "$([[ -f "${module_path}" ]] && echo present || echo missing)"
  printf '  addon metadata: %s (%s)\n' "${addon_conf_path}" "$([[ -f "${addon_conf_path}" ]] && echo present || echo missing)"
  printf '  daemon: %s (%s)\n' "${daemon_path}" "$([[ -x "${daemon_path}" ]] && echo executable || echo missing)"
  doctor_status
  exit 0
fi

features=()
config_path=""
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
    features+=(--features pipewire-backend)
    configured_backends="1"
    audio_backend="${audio_backend:-pipewire}"
    config_path="${VINPUT_USER_CONFIG:-${config_dir}/e2e-configured-pipewire-live.json}"
    install -Dm644 data/e2e-configured-pipewire-live.json "${config_path}"
    ;;
  *)
    echo "unsupported VINPUT_USER_PROFILE: ${profile}" >&2
    echo "supported profiles: mock, command-demo, configured-pipewire-live" >&2
    exit 2
    ;;
esac

cargo build -q -p vinput-cli -p vinput-daemon "${features[@]}"
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

cat <<EOF
Installed user IME files:
  daemon: ${daemon_path}
  addon module: ${module_path}
  addon metadata: ${addon_conf_path}
  environment file: ${env_file}

Restart Fcitx5, then use the retained addon triggers:
  Right Ctrl press/release: start/stop normal dictation
  F10 press/release: start/stop command dictation using selected text

Override trigger keys before launching Fcitx5 if needed:
  VINPUT_FCITX_NORMAL_TRIGGER=F8
  VINPUT_FCITX_COMMAND_TRIGGER=F9

For most desktop sessions, source this before launching Fcitx5:
  . ${env_file}
EOF

doctor_status
