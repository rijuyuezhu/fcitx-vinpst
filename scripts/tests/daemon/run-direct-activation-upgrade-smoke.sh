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

for command in dbus-run-session gdbus readlink sed stat timeout; do
  command -v "${command}" >/dev/null
done

cargo build -q -p vinpst-daemon --bin vinpst-daemon

root="${repo_root}/target/tmp/direct-activation-upgrade-smoke"
home_dir="${root}/home"
data_home="${root}/share"
config_home="${root}/config"
runtime_dir="${root}/runtime"
bin_dir="${root}/bin"
daemon_path="${bin_dir}/vinpst-daemon"
service_file="${data_home}/dbus-1/services/org.fcitx.Vinpst.service"

rm -rf "${root}"
mkdir -p \
  "${home_dir}" \
  "${config_home}" \
  "${runtime_dir}" \
  "${bin_dir}" \
  "$(dirname "${service_file}")"
chmod 700 "${runtime_dir}"
install -Dm755 target/debug/vinpst-daemon "${daemon_path}"

cat >"${service_file}" <<EOF
[D-BUS Service]
Name=org.fcitx.Vinpst
Exec=${daemon_path} --dbus --exit-when-executable-replaced
EOF

grep -qx 'Name=org.fcitx.Vinpst' "${service_file}"
grep -qx "Exec=${daemon_path} --dbus --exit-when-executable-replaced" "${service_file}"
! grep -q '^SystemdService=' "${service_file}"

HOME="${home_dir}" \
XDG_DATA_HOME="${data_home}" \
XDG_DATA_DIRS="${data_home}:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
XDG_CONFIG_HOME="${config_home}" \
XDG_RUNTIME_DIR="${runtime_dir}" \
VINPST_UPGRADE_DAEMON="${daemon_path}" \
VINPST_UPGRADE_SOURCE="${repo_root}/target/debug/vinpst-daemon" \
  timeout 20s dbus-run-session -- bash -euo pipefail <<'INNER'
name_has_owner() {
  gdbus call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.NameHasOwner \
    org.fcitx.Vinpst 2>/dev/null | grep -q true
}

owner_pid() {
  gdbus call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.GetConnectionUnixProcessID \
    org.fcitx.Vinpst 2>/dev/null | sed -n 's/.*uint32 \([0-9][0-9]*\).*/\1/p'
}

get_status() {
  gdbus call --session \
    --dest org.fcitx.Vinpst \
    --object-path /org/fcitx/Vinpst \
    --method org.fcitx.Vinpst.Service.GetStatus 2>/dev/null
}

activate_and_read_pid() {
  local reply pid
  for _ in $(seq 1 100); do
    reply="$(get_status || true)"
    if [[ "${reply}" == "('idle',)" ]]; then
      pid="$(owner_pid || true)"
      if [[ "${pid}" =~ ^[0-9]+$ ]]; then
        printf '%s\n' "${pid}"
        return 0
      fi
    fi
    sleep 0.05
  done
  return 1
}

stop_owner() {
  local pid
  if ! name_has_owner; then
    return 0
  fi
  pid="$(owner_pid || true)"
  if [[ "${pid}" =~ ^[0-9]+$ ]]; then
    kill -TERM "${pid}" 2>/dev/null || true
  fi
  for _ in $(seq 1 100); do
    name_has_owner || return 0
    sleep 0.02
  done
  if [[ "${pid}" =~ ^[0-9]+$ ]]; then
    kill -KILL "${pid}" 2>/dev/null || true
  fi
}
trap stop_owner EXIT

first_pid="$(activate_and_read_pid)"
kill -0 "${first_pid}"
test "$(readlink -f "/proc/${first_pid}/exe")" = "$(readlink -f "${VINPST_UPGRADE_DAEMON}")"
first_identity="$(stat -Lc '%d:%i' "${VINPST_UPGRADE_DAEMON}")"

replacement="${VINPST_UPGRADE_DAEMON}.replacement"
install -m755 "${VINPST_UPGRADE_SOURCE}" "${replacement}"
mv -f "${replacement}" "${VINPST_UPGRADE_DAEMON}"
second_identity="$(stat -Lc '%d:%i' "${VINPST_UPGRADE_DAEMON}")"
test "${second_identity}" != "${first_identity}"

owner_released=false
for _ in $(seq 1 100); do
  if ! name_has_owner; then
    owner_released=true
    break
  fi
  sleep 0.05
done
test "${owner_released}" = true

second_pid="$(activate_and_read_pid)"
test "${second_pid}" != "${first_pid}"
kill -0 "${second_pid}"
test "$(readlink -f "/proc/${second_pid}/exe")" = "$(readlink -f "${VINPST_UPGRADE_DAEMON}")"
INNER

rm -rf "${root}"
echo "direct activation upgrade smoke passed"
