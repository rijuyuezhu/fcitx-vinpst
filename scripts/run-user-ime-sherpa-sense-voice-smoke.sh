#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

tmp_dir="$(mktemp -d)"
stub_bin="${tmp_dir}/bin"
home_dir="${tmp_dir}/home"
out_dir="${tmp_dir}/out"
backup_dir="${tmp_dir}/target-backup"

cleanup() {
  mkdir -p target/debug
  for binary in vinput vinput-daemon; do
    if [[ -e "${backup_dir}/${binary}" ]]; then
      cp -a "${backup_dir}/${binary}" "target/debug/${binary}"
    else
      rm -f "target/debug/${binary}"
    fi
  done
  rm -rf "${tmp_dir}"
}
trap cleanup EXIT

mkdir -p "${stub_bin}" "${home_dir}" "${out_dir}" "${backup_dir}"
for binary in vinput vinput-daemon; do
  if [[ -e "target/debug/${binary}" ]]; then
    cp -a "target/debug/${binary}" "${backup_dir}/${binary}"
  fi
done

cat >"${stub_bin}/cargo" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${VINPUT_STUB_CARGO_CALLS:?}"
mkdir -p target/debug
cat >target/debug/vinput <<'VINPUT'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${VINPUT_STUB_CALLS:?}"
case "${1:-}" in
  activation-service)
    service_dir="${XDG_DATA_HOME:-${HOME}/.local/share}/dbus-1/services"
    mkdir -p "${service_dir}"
    daemon=""
    args=("$@")
    for ((index = 0; index < ${#args[@]}; index++)); do
      if [[ "${args[$index]}" == "--daemon" && $((index + 1)) -lt ${#args[@]} ]]; then
        daemon="${args[$((index + 1))]}"
      fi
    done
    cat >"${service_dir}/org.fcitx.Vinput.service" <<SERVICE
[D-BUS Service]
Name=org.fcitx.Vinput
Exec=${daemon:-vinput-daemon} --dbus
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
VINPUT
chmod +x target/debug/vinput
cat >target/debug/vinput-daemon <<'DAEMON'
#!/usr/bin/env sh
exit 0
DAEMON
chmod +x target/debug/vinput-daemon
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
printf 'stub module\n' >"${build_dir}/fcitx5-vinput.so"
cat >"${build_dir}/vinput-addon.conf" <<'CONF'
Name=Vinput
Type=SharedLibrary
Library=fcitx5-vinput
CONF
SH
chmod +x "${stub_bin}/cmake"

model_dir="${out_dir}/sense-voice-model"
mkdir -p "${model_dir}"
printf 'onnx\n' >"${model_dir}/model.int8.onnx"
printf '<blank> 0\n' >"${model_dir}/tokens.txt"
printf 'hello 1.0\n' >"${model_dir}/hotwords.txt"

calls_log="${out_dir}/vinput-calls.log"
cargo_calls_log="${out_dir}/cargo-calls.log"

PATH="${stub_bin}:${PATH}" \
HOME="${home_dir}" \
XDG_DATA_HOME="${home_dir}/.local/share" \
VINPUT_STUB_CALLS="${calls_log}" \
VINPUT_STUB_CARGO_CALLS="${cargo_calls_log}" \
VINPUT_USER_PROFILE=sherpa-sense-voice-live \
VINPUT_USER_AUDIO_BACKEND=mock \
VINPUT_USER_SHERPA_MODEL="${model_dir}" \
VINPUT_USER_SHERPA_HOTWORDS_FILE=hotwords.txt \
VINPUT_USER_SHERPA_TIMEOUT_MS=7000 \
scripts/install-user-ime.sh >"${out_dir}/install.log" 2>&1

config_path="${home_dir}/.local/share/fcitx-vinput/sherpa-sense-voice-live.json"
service_path="${home_dir}/.local/share/dbus-1/services/org.fcitx.Vinput.service"

for path in "${config_path}" "${service_path}"; do
  if [[ ! -e "${path}" ]]; then
    cat "${out_dir}/install.log" >&2
    echo "missing expected file: ${path}" >&2
    exit 1
  fi
done

python3 - "${config_path}" "${model_dir}" <<'PY'
import json
import pathlib
import sys

config = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
model_dir = pathlib.Path(sys.argv[2]).resolve()
provider = config["asr"]["providers"][0]
assert config["asr"]["active_provider"] == "sherpa-onnx", config
assert provider["id"] == "sherpa-onnx", provider
assert provider["type"] == "local", provider
assert provider["model"] == str(model_dir), provider
assert provider["hotwords_file"] == str(model_dir / "hotwords.txt"), provider
assert provider["timeout_ms"] == 7000, provider
assert config["scenes"]["active_scene"] == "raw", config
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

: >"${cargo_calls_log}"
: >"${calls_log}"
rm -f target/debug/vinput
PATH="${stub_bin}:${PATH}" \
HOME="${home_dir}" \
XDG_DATA_HOME="${home_dir}/.local/share" \
VINPUT_STUB_CALLS="${calls_log}" \
VINPUT_STUB_CARGO_CALLS="${cargo_calls_log}" \
VINPUT_USER_PROFILE=sherpa-sense-voice-live \
VINPUT_USER_STATUS=1 \
scripts/install-user-ime.sh >"${out_dir}/status.log" 2>&1

if ! grep -Fq -- '--features pipewire-backend,sherpa-onnx-backend' "${cargo_calls_log}"; then
  cat "${cargo_calls_log}" >&2
  echo "status build did not enable the sherpa and pipewire features" >&2
  exit 1
fi
if ! grep -Fq -- "doctor --config ${config_path}" "${calls_log}"; then
  cat "${calls_log}" >&2
  echo "status call did not run doctor against generated sherpa config" >&2
  exit 1
fi

printf 'user-ime-sherpa-sense-voice smoke passed\n'
