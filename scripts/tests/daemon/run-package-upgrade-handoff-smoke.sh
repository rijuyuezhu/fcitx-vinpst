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

for command in dbus-run-session gdbus jq timeout; do
  command -v "${command}" >/dev/null
done

cargo build -q -p vinput-cli --bin vinput -p vinput-daemon --bin vinput-daemon

root="${repo_root}/target/tmp/package-upgrade-handoff-smoke"
rm -rf "${root}"
mkdir -p "${root}/data-home" "${root}/data-dirs"

XDG_DATA_HOME="${root}/data-home" \
XDG_DATA_DIRS="${root}/data-dirs" \
VINPUT_UPGRADE_ROOT="${root}" \
  timeout 30s dbus-run-session -- bash -euo pipefail <<'INNER'
root="${VINPUT_UPGRADE_ROOT}"
uid="$(id -u)"
runtime_root="$(mktemp -d "${TMPDIR:-/tmp}/vinput-upgrade-runtime.XXXXXX")"
runtime_dir="${runtime_root}/${uid}"
test_home="${root}/home"
daemon_pid=""
cleanup() {
  if [[ -n "${daemon_pid}" ]]; then
    kill "${daemon_pid}" 2>/dev/null || true
    wait "${daemon_pid}" 2>/dev/null || true
  fi
  rm -rf "${runtime_root}"
}
trap cleanup EXIT
mkdir -p "${runtime_dir}" "${test_home}" "${root}/config"
bus_path="${DBUS_SESSION_BUS_ADDRESS#unix:path=}"
bus_path="${bus_path%%,*}"
ln -s "${bus_path}" "${runtime_dir}/bus"

cat >"${root}/runuser" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
test "$1" = -u
shift 2
test "$1" = --
shift
exec "$@"
SH
cat >"${root}/getent" <<SH
#!/usr/bin/env bash
set -euo pipefail
test "\$1" = passwd
test "\$2" = "${uid}"
printf '%s\n' 'vinput-test:x:${uid}:${uid}:Vinput Test:${test_home}:/bin/bash'
SH
cat >"${root}/must-not-run" <<'SH'
#!/usr/bin/env bash
echo "vinput was called without a live owner" >&2
exit 91
SH
cat >"${root}/vinput-success" <<SH
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "\$*" >"${root}/vinput.args"
printf '%s\n' "\${HOME}" >"${root}/vinput.home"
printf '%s\n' "\${XDG_RUNTIME_DIR}" >"${root}/vinput.runtime"
printf '%s\n' "\${DBUS_SESSION_BUS_ADDRESS}" >"${root}/vinput.bus"
printf '%s\n' '{"ok":true,"action":"handoff"}'
SH
cat >"${root}/vinput-failure" <<'SH'
#!/usr/bin/env bash
printf '%s\n' '{"ok":false,"error":"fixture handoff failure"}'
exit 42
SH
chmod +x \
  "${root}/runuser" \
  "${root}/getent" \
  "${root}/must-not-run" \
  "${root}/vinput-success" \
  "${root}/vinput-failure"

common_env=(
  VINPUT_UPGRADE_RUNTIME_ROOT="${runtime_root}"
  VINPUT_UPGRADE_RUNUSER="${root}/runuser"
  VINPUT_UPGRADE_GETENT="${root}/getent"
  VINPUT_UPGRADE_SYSTEMCTL=/usr/bin/true
  VINPUT_UPGRADE_KILL=/usr/bin/true
)

env "${common_env[@]}" \
  VINPUT_UPGRADE_VINPUT="${root}/must-not-run" \
  scripts/release/package-upgrade-handoff.sh >"${root}/no-owner.log"
grep -Fq 'checked 1 live user session(s), 0 active owner(s)' \
  "${root}/no-owner.log"
test ! -e "${root}/vinput.args"

XDG_CONFIG_HOME="${root}/config" \
  target/debug/vinput-daemon --dbus >"${root}/daemon.log" 2>&1 &
daemon_pid=$!
for _ in $(seq 1 100); do
  if gdbus call --session \
    --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.NameHasOwner org.fcitx.Vinput |
    grep -Fq true; then
    break
  fi
  sleep 0.05
done
gdbus call --session \
  --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.NameHasOwner org.fcitx.Vinput |
  grep -Fq true

env "${common_env[@]}" \
  VINPUT_UPGRADE_VINPUT="${root}/vinput-success" \
  scripts/release/package-upgrade-handoff.sh >"${root}/owner.log"
grep -Fq 'checked 1 live user session(s), 1 active owner(s)' "${root}/owner.log"
test "$(cat "${root}/vinput.args")" = 'daemon handoff --json'
test "$(cat "${root}/vinput.home")" = "${test_home}"
test "$(cat "${root}/vinput.runtime")" = "${runtime_dir}"
test "$(cat "${root}/vinput.bus")" = "unix:path=${runtime_dir}/bus"
kill -0 "${daemon_pid}"

if env "${common_env[@]}" \
  VINPUT_UPGRADE_VINPUT="${root}/vinput-failure" \
  scripts/release/package-upgrade-handoff.sh \
  >"${root}/failure.log" 2>"${root}/failure.err"; then
  echo "failing user handoff unexpectedly succeeded" >&2
  exit 1
fi
grep -Fq 'failed to hand off vinput daemon upgrade' "${root}/failure.err"
grep -Fq 'vinput upgrade handoff failed for 1 session(s)' "${root}/failure.err"
kill -0 "${daemon_pid}"
INNER

echo "package upgrade cross-user dispatch smoke passed"
