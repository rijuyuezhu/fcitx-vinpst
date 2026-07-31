#!/usr/bin/env bash
set -euo pipefail

if [[ "${VINPUT_RUN_SYSTEMD_UPGRADE_LIVE:-}" != 1 ]]; then
  echo "set VINPUT_RUN_SYSTEMD_UPGRADE_LIVE=1 to run the user-systemd upgrade gate" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

for command in cargo gdbus install mv python3 readlink sed stat systemctl; do
  command -v "${command}" >/dev/null
done

runtime_dir="${XDG_RUNTIME_DIR:?XDG_RUNTIME_DIR is required}"
data_home="${XDG_DATA_HOME:-${HOME}/.local/share}"
activation_file="${data_home}/dbus-1/services/org.fcitx.Vinput.service"
unit_name="vinput-daemon-upgrade-live.service"
unit_dir="${runtime_dir}/systemd/user"
unit_file="${unit_dir}/${unit_name}"
root="${repo_root}/target/tmp/systemd-upgrade-live"
daemon_path="${root}/bin/vinput-daemon"
summary_path="${root}/summary.json"
activation_backup="${activation_file}.vinput-systemd-upgrade-live.$$"

cargo build -q -p vinput-daemon --bin vinput-daemon
rm -rf "${root}"
mkdir -p "${root}/bin" "${unit_dir}"
install -m755 target/debug/vinput-daemon "${daemon_path}"

test -f "${activation_file}"
test ! -e "${activation_backup}"

name_has_owner() {
  gdbus call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.NameHasOwner \
    org.fcitx.Vinput 2>/dev/null | grep -q true
}

owner_pid() {
  gdbus call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.GetConnectionUnixProcessID \
    org.fcitx.Vinput 2>/dev/null | sed -n 's/.*uint32 \([0-9][0-9]*\).*/\1/p'
}

reload_dbus_activation() {
  gdbus call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.ReloadConfig >/dev/null
}

get_status() {
  gdbus call --session \
    --dest org.fcitx.Vinput \
    --object-path /org/fcitx/Vinput \
    --method org.fcitx.Vinput.Service.GetStatus 2>/dev/null
}

wait_for_no_owner() {
  for _ in $(seq 1 200); do
    name_has_owner || return 0
    sleep 0.05
  done
  return 1
}

wait_for_owner() {
  local expected_pid="${1:-}"
  local pid reply
  for _ in $(seq 1 200); do
    reply="$(get_status || true)"
    pid="$(owner_pid || true)"
    if [[ "${reply}" == "('idle',)" && "${pid}" =~ ^[0-9]+$ ]]; then
      if [[ -z "${expected_pid}" || "${pid}" == "${expected_pid}" ]]; then
        printf '%s\n' "${pid}"
        return 0
      fi
    fi
    sleep 0.05
  done
  return 1
}

original_pid="$(owner_pid)"
[[ "${original_pid}" =~ ^[0-9]+$ ]]
test "$(get_status)" = "('idle',)"
original_exe="$(readlink -f "/proc/${original_pid}/exe")"
tr '\0' '\n' <"/proc/${original_pid}/cmdline" >"${root}/original-cmdline.txt"
original_cmd0="$(head -n 1 "${root}/original-cmdline.txt")"

restored=false
cleanup() {
  local status=$?
  trap - EXIT
  set +e

  systemctl --user stop "${unit_name}" >/dev/null 2>&1
  rm -f "${unit_file}"
  systemctl --user daemon-reload >/dev/null 2>&1
  systemctl --user reset-failed "${unit_name}" >/dev/null 2>&1

  if [[ -e "${activation_backup}" && ! -e "${activation_file}" ]]; then
    mv "${activation_backup}" "${activation_file}"
  fi
  reload_dbus_activation >/dev/null 2>&1

  if [[ "${original_pid}" =~ ^[0-9]+$ ]]; then
    restored_pid="$(wait_for_owner || true)"
    if [[ "${restored_pid}" =~ ^[0-9]+$ ]]; then
      restored_exe="$(readlink -f "/proc/${restored_pid}/exe" 2>/dev/null || true)"
      restored_cmd0="$(tr '\0' '\n' <"/proc/${restored_pid}/cmdline" 2>/dev/null | head -n 1)"
      if [[ "${restored_exe}" == "${original_exe}" && "${restored_cmd0}" == "${original_cmd0}" ]]; then
        restored=true
      fi
    fi
  fi

  if [[ "${restored}" != true ]]; then
    echo "failed to restore the original vinput D-Bus activation owner" >&2
    status=1
  fi
  exit "${status}"
}
trap cleanup EXIT

mv "${activation_file}" "${activation_backup}"
reload_dbus_activation
kill -TERM "${original_pid}"
wait_for_no_owner

cat >"${unit_file}" <<EOF
[Unit]
Description=fcitx-vinput user-systemd upgrade live gate

[Service]
Type=dbus
BusName=org.fcitx.Vinput
ExecStart=${daemon_path} --dbus --exit-when-executable-replaced
Restart=on-failure
RestartSec=100ms
EOF

systemctl --user daemon-reload
systemctl --user start "${unit_name}"
first_pid="$(systemctl --user show --property MainPID --value "${unit_name}")"
[[ "${first_pid}" =~ ^[0-9]+$ ]]
test "$(wait_for_owner "${first_pid}")" = "${first_pid}"
first_identity="$(stat -Lc '%d:%i' "${daemon_path}")"
test "$(stat -Lc '%d:%i' "/proc/${first_pid}/exe")" = "${first_identity}"

replacement="${daemon_path}.replacement"
install -m755 target/debug/vinput-daemon "${replacement}"
mv -f "${replacement}" "${daemon_path}"
second_identity="$(stat -Lc '%d:%i' "${daemon_path}")"
test "${second_identity}" != "${first_identity}"

second_pid=""
for _ in $(seq 1 200); do
  candidate="$(systemctl --user show --property MainPID --value "${unit_name}" 2>/dev/null || true)"
  if [[ "${candidate}" =~ ^[0-9]+$ && "${candidate}" != 0 && "${candidate}" != "${first_pid}" ]]; then
    if [[ "$(get_status || true)" == "('idle',)" && "$(owner_pid || true)" == "${candidate}" ]]; then
      second_pid="${candidate}"
      break
    fi
  fi
  sleep 0.05
done
[[ "${second_pid}" =~ ^[0-9]+$ ]]
test "$(stat -Lc '%d:%i' "/proc/${second_pid}/exe")" = "${second_identity}"
restart_count="$(systemctl --user show --property NRestarts --value "${unit_name}")"
test "${restart_count}" -ge 1

systemctl --user stop "${unit_name}"
wait_for_no_owner
rm -f "${unit_file}"
systemctl --user daemon-reload
systemctl --user reset-failed "${unit_name}" >/dev/null 2>&1 || true

mv "${activation_backup}" "${activation_file}"
reload_dbus_activation
restored_pid="$(wait_for_owner)"
restored_exe="$(readlink -f "/proc/${restored_pid}/exe")"
restored_cmd0="$(tr '\0' '\n' <"/proc/${restored_pid}/cmdline" | head -n 1)"
test "${restored_exe}" = "${original_exe}"
test "${restored_cmd0}" = "${original_cmd0}"
restored=true

python3 - \
  "${summary_path}" \
  "${unit_name}" \
  "${original_pid}" \
  "${first_pid}" \
  "${second_pid}" \
  "${restored_pid}" \
  "${restart_count}" \
  "${original_exe}" <<'PY'
import json
import pathlib
import sys

(
    output,
    unit_name,
    original_pid,
    first_pid,
    second_pid,
    restored_pid,
    restart_count,
    original_exe,
) = sys.argv[1:]
summary = {
    "ok": True,
    "unit_name": unit_name,
    "original_owner_pid": int(original_pid),
    "first_systemd_pid": int(first_pid),
    "second_systemd_pid": int(second_pid),
    "restored_owner_pid": int(restored_pid),
    "systemd_restart_count": int(restart_count),
    "original_owner_executable": original_exe,
    "activation_file_restored": True,
    "original_owner_restored": True,
}
pathlib.Path(output).write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
PY

trap - EXIT
echo "user-systemd executable replacement live gate passed"
