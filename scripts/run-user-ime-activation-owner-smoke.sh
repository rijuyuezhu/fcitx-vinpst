#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

out_dir="${repo_root}/target/tmp/user-ime-activation-owner-smoke"
home_dir="${out_dir}/home"
data_home="${out_dir}/share"
config_home="${out_dir}/config"
runtime_dir="${out_dir}/runtime"
bin_dir="${out_dir}/bin"
lib_dir="${out_dir}/lib/fcitx5"
status_output="${out_dir}/daemon-status.json"
service_file="${data_home}/dbus-1/services/org.fcitx.Vinput.service"
runtime_service_file="${runtime_dir}/dbus-1/services/org.fcitx.Vinput.service"
daemon_path="${bin_dir}/vinput-daemon"
config_path="${data_home}/fcitx-vinput/e2e-command-demo-config.json"
rustup_home="${RUSTUP_HOME:-$(rustup show home)}"
cargo_home="${CARGO_HOME:-${HOME}/.cargo}"

smoke_daemon_pids() {
  local proc_cmdline pid index
  local -a argv
  for proc_cmdline in /proc/[0-9]*/cmdline; do
    [[ -r "${proc_cmdline}" ]] || continue
    pid="${proc_cmdline#/proc/}"
    pid="${pid%/cmdline}"
    argv=()
    mapfile -d '' -t argv <"${proc_cmdline}" || true
    [[ ${#argv[@]} -gt 0 && "${argv[0]}" == "${daemon_path}" ]] || continue
    for ((index = 0; index + 1 < ${#argv[@]}; index++)); do
      if [[ "${argv[index]}" == "--config" && "${argv[index + 1]}" == "${config_path}" ]]; then
        printf '%s\n' "${pid}"
        break
      fi
    done
  done
}

stop_stale_smoke_daemons() {
  local pid
  local -a pids
  mapfile -t pids < <(smoke_daemon_pids)
  for pid in "${pids[@]}"; do
    kill -TERM "${pid}" 2>/dev/null || true
  done
  for _ in $(seq 1 50); do
    mapfile -t pids < <(smoke_daemon_pids)
    [[ ${#pids[@]} == 0 ]] && return 0
    sleep 0.02
  done
  for pid in "${pids[@]}"; do
    kill -KILL "${pid}" 2>/dev/null || true
  done
}

trap stop_stale_smoke_daemons EXIT
stop_stale_smoke_daemons
rm -rf "${out_dir}"
mkdir -p "${home_dir}" "${runtime_dir}"
chmod 700 "${runtime_dir}"

common_env=(
  HOME="${home_dir}"
  XDG_DATA_HOME="${data_home}"
  XDG_CONFIG_HOME="${config_home}"
  XDG_RUNTIME_DIR="${runtime_dir}"
  RUSTUP_HOME="${rustup_home}"
  CARGO_HOME="${cargo_home}"
  VINPUT_USER_BIN_DIR="${bin_dir}"
  VINPUT_USER_FCITX_LIB_DIR="${lib_dir}"
  VINPUT_USER_PROFILE=command-demo
  VINPUT_USER_RUNTIME_ACTIVATION=1
)

env "${common_env[@]}" scripts/install-user-ime.sh >"${out_dir}/install.log" 2>&1

test -x "${daemon_path}"
test -f "${service_file}"
test -f "${runtime_service_file}"
grep -Fq -- "Exec=${daemon_path} --dbus" "${service_file}"
grep -Fq -- "Exec=${daemon_path} --dbus" "${runtime_service_file}"
grep -Fq -- "--configured-backends" "${service_file}"
grep -Fq -- "--wav ${data_home}/fcitx-vinput/e2e-command-demo.wav" "${service_file}"

HOME="${home_dir}" \
XDG_DATA_HOME="${data_home}" \
XDG_DATA_DIRS="${data_home}:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
XDG_RUNTIME_DIR="${runtime_dir}" \
VINPUT_OWNER_SMOKE_STATUS="${status_output}" \
VINPUT_OWNER_SMOKE_DAEMON="${daemon_path}" \
  timeout 20s dbus-run-session -- bash -euo pipefail <<'INNER'
stop_activation_owner() {
  if [[ ! -f "${VINPUT_OWNER_SMOKE_STATUS}" ]]; then
    return 0
  fi
  local owner_pid owner_exe owner_cmd0 expected_exe
  owner_pid="$(jq -r '.owner.unix_process_id // empty' "${VINPUT_OWNER_SMOKE_STATUS}")"
  if [[ ! "${owner_pid}" =~ ^[0-9]+$ ]]; then
    return 0
  fi
  owner_exe="$(readlink "/proc/${owner_pid}/exe" 2>/dev/null || true)"
  owner_cmd0="$(tr '\0' '\n' <"/proc/${owner_pid}/cmdline" 2>/dev/null | head -n 1)"
  expected_exe="$(realpath "${VINPUT_OWNER_SMOKE_DAEMON}")"
  if [[ "${owner_cmd0}" != "${VINPUT_OWNER_SMOKE_DAEMON}" ]] ||
    [[ "${owner_exe}" != "${expected_exe}" && "${owner_exe}" != "${expected_exe} (deleted)" ]]; then
    return 0
  fi
  kill "${owner_pid}" 2>/dev/null || true
  for _ in $(seq 1 50); do
    if ! kill -0 "${owner_pid}" 2>/dev/null; then
      return 0
    fi
    sleep 0.02
  done
  kill -KILL "${owner_pid}" 2>/dev/null || true
}
trap stop_activation_owner EXIT

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

mapfile -t remaining_smoke_pids < <(smoke_daemon_pids)
if [[ ${#remaining_smoke_pids[@]} != 0 ]]; then
  echo "activation owner smoke leaked daemon PIDs: ${remaining_smoke_pids[*]}" >&2
  exit 1
fi

env "${common_env[@]}" VINPUT_USER_REMOVE=1 scripts/install-user-ime.sh >/dev/null
if [[ -e "${service_file}" ]]; then
  echo "activation owner cleanup left service: ${service_file}" >&2
  exit 1
fi
rm -rf "${out_dir}"
trap - EXIT

echo "user IME activation owner smoke passed"
