#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

tmp_dir="$(mktemp -d)"
stub_bin="${tmp_dir}/bin"
home_dir="${tmp_dir}/home"
out_dir="${tmp_dir}/out"
runtime_bin="${out_dir}/runtime-bin"

cleanup() {
  rm -rf "${tmp_dir}"
}
trap cleanup EXIT

mkdir -p "${stub_bin}" "${home_dir}" "${out_dir}" "${runtime_bin}"

cat >"${stub_bin}/cargo" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
: "${VINPUT_USER_CLI_BINARY:?}"
: "${VINPUT_USER_DAEMON_BINARY:?}"
mkdir -p "$(dirname "${VINPUT_USER_CLI_BINARY}")" "$(dirname "${VINPUT_USER_DAEMON_BINARY}")"
cat >"${VINPUT_USER_CLI_BINARY}" <<'VINPUT'
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
chmod +x "${VINPUT_USER_CLI_BINARY}"
cat >"${VINPUT_USER_DAEMON_BINARY}" <<'DAEMON'
#!/usr/bin/env sh
exit 0
DAEMON
chmod +x "${VINPUT_USER_DAEMON_BINARY}"
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

calls_log="${out_dir}/vinput-calls.log"
fake_asr="${out_dir}/fake-real-asr.py"
cat >"${fake_asr}" <<'PY'
#!/usr/bin/env python3
import os
import wave

with wave.open(os.environ["VINPUT_ASR_WAV"]) as handle:
    print("real helper frames %d" % handle.getnframes())
PY
chmod +x "${fake_asr}"
external_command="python3 ${fake_asr}"

PATH="${stub_bin}:${PATH}" \
HOME="${home_dir}" \
XDG_DATA_HOME="${home_dir}/.local/share" \
VINPUT_STUB_CALLS="${calls_log}" \
VINPUT_USER_CLI_BINARY="${runtime_bin}/vinput" \
VINPUT_USER_DAEMON_BINARY="${runtime_bin}/vinput-daemon" \
VINPUT_USER_PROFILE=real-command-asr-wav \
VINPUT_USER_AUDIO_BACKEND=mock \
VINPUT_USER_COMMAND_ASR_WAV_COMMAND="${external_command}" \
VINPUT_USER_COMMAND_ASR_WAV_TIMEOUT_MS=5000 \
scripts/install-user-ime.sh >"${out_dir}/install.log" 2>&1

config_path="${home_dir}/.local/share/fcitx-vinput/real-command-asr-wav.json"
helper_path="${home_dir}/.local/bin/vinput-command-asr-wav-helper"
service_path="${home_dir}/.local/share/dbus-1/services/org.fcitx.Vinput.service"

for path in "${config_path}" "${helper_path}" "${service_path}"; do
  if [[ ! -e "${path}" ]]; then
    cat "${out_dir}/install.log" >&2
    echo "missing expected file: ${path}" >&2
    exit 1
  fi
done
if [[ ! -x "${helper_path}" ]]; then
  echo "helper is not executable: ${helper_path}" >&2
  exit 1
fi

python3 - "${config_path}" "${helper_path}" "${external_command}" <<'PY'
import json
import pathlib
import sys

config = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
provider = config["asr"]["providers"][0]
assert config["asr"]["active_provider"] == "real-command-asr-wav", config
assert provider["command"] == sys.argv[2], provider
assert provider["args"] == ["--timeout-ms", "5000", "--", "sh", "-c", "$VINPUT_REAL_ASR_COMMAND"], provider
assert provider["env"] == {"VINPUT_REAL_ASR_COMMAND": sys.argv[3]}, provider
assert provider["timeout_ms"] == 7000, provider
assert config["scenes"]["active_scene"] == "raw", config
PY

request='{"provider_id":"real-command-asr-wav","timeout_ms":5000,"pcm":{"sample_rate_hz":16000,"channels":1},"context":{"mode":"normal","selected_text":null},"samples":[0,1000,-1000,2000,-2000,3000]}'
helper_output="$(python3 - "${config_path}" "${request}" <<'PY'
import json
import os
import subprocess
import sys

config = json.load(open(sys.argv[1], encoding="utf-8"))
request = sys.argv[2]
provider = config["asr"]["providers"][0]
env = os.environ.copy()
env.update(provider.get("env", {}))
completed = subprocess.run(
    [provider["command"], *provider["args"]],
    input=request,
    text=True,
    capture_output=True,
    check=True,
    env=env,
)
print(completed.stdout.strip())
PY
)"
python3 - "${helper_output}" <<'PY'
import json
import sys
payload = json.loads(sys.argv[1])
assert payload == {"text": "real helper frames 6"}, payload
PY

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
  echo "activation call did not point at generated config" >&2
  exit 1
fi

printf 'user-ime-real-command-asr-wav smoke passed\n'
