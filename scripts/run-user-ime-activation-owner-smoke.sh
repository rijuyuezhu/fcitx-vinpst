#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

out_dir="${repo_root}/target/tmp/user-ime-activation-owner-smoke"
home_dir="${out_dir}/home"
data_home="${out_dir}/share"
config_home="${out_dir}/config"
bin_dir="${out_dir}/bin"
lib_dir="${out_dir}/lib/fcitx5"
status_output="${out_dir}/daemon-status.json"
service_file="${data_home}/dbus-1/services/org.fcitx.Vinput.service"
daemon_path="${bin_dir}/vinput-daemon"
rustup_home="${RUSTUP_HOME:-$(rustup show home)}"
cargo_home="${CARGO_HOME:-${HOME}/.cargo}"

rm -rf "${out_dir}"
mkdir -p "${home_dir}"

common_env=(
  HOME="${home_dir}"
  XDG_DATA_HOME="${data_home}"
  XDG_CONFIG_HOME="${config_home}"
  RUSTUP_HOME="${rustup_home}"
  CARGO_HOME="${cargo_home}"
  VINPUT_USER_BIN_DIR="${bin_dir}"
  VINPUT_USER_FCITX_LIB_DIR="${lib_dir}"
  VINPUT_USER_PROFILE=command-demo
)

env "${common_env[@]}" scripts/install-user-ime.sh >"${out_dir}/install.log" 2>&1

test -x "${daemon_path}"
test -f "${service_file}"
grep -Fq -- "Exec=${daemon_path} --dbus" "${service_file}"
grep -Fq -- "--configured-backends" "${service_file}"
grep -Fq -- "--wav ${data_home}/fcitx-vinput/e2e-command-demo.wav" "${service_file}"

HOME="${home_dir}" \
XDG_DATA_HOME="${data_home}" \
XDG_DATA_DIRS="${data_home}:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
VINPUT_OWNER_SMOKE_STATUS="${status_output}" \
VINPUT_OWNER_SMOKE_DAEMON="${daemon_path}" \
  timeout 20s dbus-run-session -- bash -euo pipefail <<'INNER'
target/debug/vinput daemon status --json >"${VINPUT_OWNER_SMOKE_STATUS}"
python3 - "${VINPUT_OWNER_SMOKE_STATUS}" "${VINPUT_OWNER_SMOKE_DAEMON}" <<'PY'
import json
import pathlib
import sys

status = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
expected_daemon = sys.argv[2]
owner = status["owner"]
assert owner["ok"] is True, owner
assert owner["process"]["exe"] == expected_daemon, owner
assert owner["process"]["cmdline"][0] == expected_daemon, owner
assert status["status"] == "idle", status
asr = status["asr_backend"]
assert asr["effective_provider_id"] == "demo-command-asr", asr
assert asr["has_effective_backend"] is True, asr
assert asr["last_error"] == "", asr
print(f"activation owner: {expected_daemon}")
PY
INNER

env "${common_env[@]}" VINPUT_USER_REMOVE=1 scripts/install-user-ime.sh >/dev/null
if [[ -e "${service_file}" ]]; then
  echo "activation owner cleanup left service: ${service_file}" >&2
  exit 1
fi
rm -rf "${out_dir}"

echo "user IME activation owner smoke passed"
