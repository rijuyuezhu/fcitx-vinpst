#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

for command in dbus-run-session jq readlink timeout; do
  command -v "${command}" >/dev/null
done

stage_root="${repo_root}/target/tmp/daemon-handoff-diagnostics-smoke"
mismatch_root="${stage_root}/mismatch"
deleted_root="${stage_root}/deleted"
rm -rf "${stage_root}"
mkdir -p "${mismatch_root}/bin" "${mismatch_root}/config" \
  "${deleted_root}/bin" "${deleted_root}/config"

cargo build -q -p vinput-cli --bin vinput -p vinput-daemon --bin vinput-daemon

install -Dm755 target/debug/vinput-daemon "${mismatch_root}/bin/vinput-daemon-old"
install -Dm755 target/debug/vinput "${deleted_root}/bin/vinput"
install -Dm755 target/debug/vinput-daemon "${deleted_root}/bin/vinput-daemon"

VINPUT_HANDOFF_CLI="${repo_root}/target/debug/vinput" \
VINPUT_HANDOFF_DAEMON="${mismatch_root}/bin/vinput-daemon-old" \
VINPUT_HANDOFF_CONFIG_HOME="${mismatch_root}/config" \
VINPUT_HANDOFF_STATUS="${mismatch_root}/status.json" \
  timeout 20s dbus-run-session -- bash -euo pipefail <<'INNER'
"${VINPUT_HANDOFF_DAEMON}" --dbus >"${VINPUT_HANDOFF_STATUS}.daemon.log" 2>&1 &
daemon_pid=$!
cleanup() {
  kill "${daemon_pid}" 2>/dev/null || true
  wait "${daemon_pid}" 2>/dev/null || true
}
trap cleanup EXIT

ready=0
for _ in $(seq 1 100); do
  if XDG_CONFIG_HOME="${VINPUT_HANDOFF_CONFIG_HOME}" \
    "${VINPUT_HANDOFF_CLI}" daemon status --json >"${VINPUT_HANDOFF_STATUS}" 2>/dev/null; then
    ready=1
    break
  fi
  sleep 0.05
done
test "${ready}" = 1
INNER

expected_mismatch="$(readlink -f target/debug/vinput-daemon)"
owner_mismatch="$(readlink -f "${mismatch_root}/bin/vinput-daemon-old")"
jq -e \
  --arg expected "${expected_mismatch}" \
  --arg owner "${owner_mismatch}" \
  '.handoff.expected_executable == $expected
   and .handoff.owner_executable == $owner
   and .handoff.owner_executable_deleted == false
   and .handoff.path_matches == false
   and .handoff.restart_recommended == true
   and .handoff.reason == "owner-executable-path-mismatch"
   and .handoff.automatic_restart_performed == false
   and .handoff.next_step == "run vinput daemon handoff"' \
  "${mismatch_root}/status.json" >/dev/null

VINPUT_HANDOFF_CLI="${deleted_root}/bin/vinput" \
VINPUT_HANDOFF_DAEMON="${deleted_root}/bin/vinput-daemon" \
VINPUT_HANDOFF_REPLACEMENT="${repo_root}/target/debug/vinput-daemon" \
VINPUT_HANDOFF_CONFIG_HOME="${deleted_root}/config" \
VINPUT_HANDOFF_STATUS="${deleted_root}/status.json" \
  timeout 20s dbus-run-session -- bash -euo pipefail <<'INNER'
"${VINPUT_HANDOFF_DAEMON}" --dbus >"${VINPUT_HANDOFF_STATUS}.daemon.log" 2>&1 &
daemon_pid=$!
cleanup() {
  kill "${daemon_pid}" 2>/dev/null || true
  wait "${daemon_pid}" 2>/dev/null || true
}
trap cleanup EXIT

ready=0
for _ in $(seq 1 100); do
  if XDG_CONFIG_HOME="${VINPUT_HANDOFF_CONFIG_HOME}" \
    "${VINPUT_HANDOFF_CLI}" daemon status --json >"${VINPUT_HANDOFF_STATUS}.before" 2>/dev/null; then
    ready=1
    break
  fi
  sleep 0.05
done
test "${ready}" = 1
jq -e \
  '.handoff.path_matches == true
   and .handoff.owner_executable_deleted == false
   and .handoff.restart_recommended == false' \
  "${VINPUT_HANDOFF_STATUS}.before" >/dev/null

rm -f "${VINPUT_HANDOFF_DAEMON}"
install -Dm755 "${VINPUT_HANDOFF_REPLACEMENT}" "${VINPUT_HANDOFF_DAEMON}"
XDG_CONFIG_HOME="${VINPUT_HANDOFF_CONFIG_HOME}" \
  "${VINPUT_HANDOFF_CLI}" daemon status --json >"${VINPUT_HANDOFF_STATUS}"
XDG_CONFIG_HOME="${VINPUT_HANDOFF_CONFIG_HOME}" \
  "${VINPUT_HANDOFF_CLI}" daemon status >"${VINPUT_HANDOFF_STATUS}.txt"
INNER

expected_deleted="$(readlink -f "${deleted_root}/bin/vinput-daemon")"
jq -e \
  --arg expected "${expected_deleted}" \
  '.handoff.expected_executable == $expected
   and .handoff.normalized_owner_executable == $expected
   and (.handoff.owner_executable | endswith(" (deleted)"))
   and .handoff.owner_executable_deleted == true
   and .handoff.path_matches == true
   and .handoff.restart_recommended == true
   and .handoff.reason == "owner-executable-deleted"
   and .handoff.automatic_restart_performed == false
   and .handoff.next_step == "run vinput daemon handoff"' \
  "${deleted_root}/status.json" >/dev/null
grep -qx 'handoff_owner_exe_deleted: true' "${deleted_root}/status.json.txt"
grep -qx 'handoff_path_matches: true' "${deleted_root}/status.json.txt"
grep -qx 'handoff_restart_recommended: true' "${deleted_root}/status.json.txt"
grep -qx 'handoff_reason: owner-executable-deleted' "${deleted_root}/status.json.txt"
grep -qx 'handoff_next_step: run vinput daemon handoff' "${deleted_root}/status.json.txt"

echo "daemon handoff diagnostics smoke passed"
