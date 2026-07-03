#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

stub_bin="${tmp_dir}/bin"
home_dir="${tmp_dir}/home"
out_dir="${tmp_dir}/out"
mkdir -p "${stub_bin}" "${home_dir}" "${out_dir}"

cat >"${stub_bin}/fcitx5" <<'SH'
#!/usr/bin/env bash
exit 0
SH
chmod +x "${stub_bin}/fcitx5"

cat >"${stub_bin}/fcitx5-remote" <<'SH'
#!/usr/bin/env bash
case "${1:-}" in
  --check)
    [[ "${VINPUT_STUB_FCITX_RUNNING:-1}" == "1" ]]
    ;;
  -a)
    printf 'unix:path=/tmp/vinput-fcitx-test\n'
    ;;
  -q)
    printf '1\n'
    ;;
  -n)
    printf 'keyboard-us\n'
    ;;
  *)
    exit 0
    ;;
esac
SH
chmod +x "${stub_bin}/fcitx5-remote"

cat >"${stub_bin}/gdbus" <<'SH'
#!/usr/bin/env bash
args=" $* "
if [[ "${args}" == *" org.freedesktop.DBus.GetNameOwner org.fcitx.Vinput "* ]]; then
  if [[ -n "${VINPUT_STUB_BUS_OWNER:-}" ]]; then
    printf "('%s',)\n" "${VINPUT_STUB_BUS_OWNER}"
    exit 0
  fi
  printf 'Error: GDBus.Error:org.freedesktop.DBus.Error.NameHasNoOwner: Name org.fcitx.Vinput has no owner\n' >&2
  exit 1
fi

if [[ "${args}" == *" org.fcitx.Vinput.Service.GetRuntimeStatus"* ]]; then
  case "${VINPUT_STUB_RUNTIME_STATUS:-ok}" in
    ok)
      printf "('{\"ok\":true}',)\n"
      exit 0
      ;;
    unknown-method)
      printf 'Error: GDBus.Error:org.freedesktop.DBus.Error.UnknownMethod: No such method GetRuntimeStatus\n' >&2
      exit 1
      ;;
    fail)
      printf 'Error: simulated runtime failure\n' >&2
      exit 1
      ;;
  esac
fi

printf 'unexpected gdbus call: %s\n' "$*" >&2
exit 1
SH
chmod +x "${stub_bin}/gdbus"

base_env=(
  PATH="${stub_bin}:${PATH}"
  HOME="${home_dir}"
  XDG_DATA_HOME="${home_dir}/.local/share"
  VINPUT_LIVE_SKIP_USER_STATUS=1
  VINPUT_LIVE_SKIP_FCITX_ENV_CHECK=1
)

expect_failure() {
  local name="$1"
  local expected="$2"
  shift 2
  local output="${out_dir}/${name}.log"
  set +e
  "$@" >"${output}" 2>&1
  local status=$?
  set -e
  if [[ "${status}" == "0" ]]; then
    cat "${output}" >&2
    echo "expected ${name} to fail" >&2
    exit 1
  fi
  if ! grep -Fq "${expected}" "${output}"; then
    cat "${output}" >&2
    echo "expected ${name} output to contain: ${expected}" >&2
    exit 1
  fi
}

expect_output() {
  local name="$1"
  local expected="$2"
  local output="${out_dir}/${name}.log"
  if ! grep -Fq "${expected}" "${output}"; then
    cat "${output}" >&2
    echo "expected ${name} output to contain: ${expected}" >&2
    exit 1
  fi
}

probe="${repo_root}/scripts/run-ime-fcitx-live-probe.sh"

expect_failure no-dbus \
  'FAIL[user-dbus-session-missing]' \
  env -u DBUS_SESSION_BUS_ADDRESS "${base_env[@]}" "${probe}"

expect_failure no-fcitx \
  'FAIL[fcitx5-not-running]' \
  env "${base_env[@]}" DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/vinput-test-bus VINPUT_STUB_FCITX_RUNNING=0 "${probe}"

expect_failure missing-install \
  'FAIL[addon-module-missing]' \
  env "${base_env[@]}" DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/vinput-test-bus "${probe}"
expect_output missing-install 'FAIL[addon-metadata-missing]'
expect_output missing-install 'FAIL[daemon-missing]'
expect_output missing-install 'FAIL[activation-service-missing]'
expect_output missing-install 'FAIL[fcitx-env-wrapper-missing]'
expect_output missing-install 'FAIL[fcitx-autostart-missing]'
expect_output missing-install 'FAIL[runtime-status-skipped]'

install -Dm755 /bin/true "${home_dir}/.local/bin/vinput-daemon"
install -Dm644 /dev/stdin "${home_dir}/.local/share/fcitx5/addon/vinput.conf" <<'CONF'
Name=Vinput
Type=SharedLibrary
Library=fcitx5-vinput
CONF
install -Dm644 /dev/null "${home_dir}/.local/lib/fcitx5/fcitx5-vinput.so"
install -Dm644 /dev/stdin "${home_dir}/.local/share/dbus-1/services/org.fcitx.Vinput.service" <<'SERVICE'
[D-BUS Service]
Name=org.fcitx.Vinput
Exec=/tmp/old-vinput-daemon --dbus
SERVICE
install -Dm644 /dev/stdin "${home_dir}/.local/share/fcitx-vinput/fcitx-vinput.env" <<ENV
export FCITX_ADDON_DIRS="${home_dir}/.local/lib/fcitx5:/usr/lib/fcitx5"
export XDG_DATA_HOME="${home_dir}/.local/share"
ENV
install -Dm755 /dev/stdin "${home_dir}/.local/share/fcitx-vinput/fcitx5-with-vinput-env.sh" <<ENVSH
#!/usr/bin/env sh
. '${home_dir}/.local/share/fcitx-vinput/fcitx-vinput.env'
exec "\${VINPUT_FCITX5_BIN:-fcitx5}" "\$@"
ENVSH
install -Dm644 /dev/stdin "${home_dir}/.config/autostart/org.fcitx.Fcitx5.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Fcitx 5 with fcitx-vinput
Exec=${home_dir}/.local/share/fcitx-vinput/fcitx5-with-vinput-env.sh
Terminal=false
X-fcitx-vinput-managed=true
DESKTOP

expect_failure stale-bus \
  'FAIL[activation-service-old-daemon]' \
  env "${base_env[@]}" \
    DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/vinput-test-bus \
    VINPUT_STUB_BUS_OWNER=:1.77 \
    VINPUT_STUB_RUNTIME_STATUS=unknown-method \
    "${probe}"
expect_output stale-bus 'FAIL[runtime-status-unavailable]'
expect_output stale-bus 'FAIL[stale-bus-owner]'

printf 'ime-fcitx-live-probe smoke passed\n'
