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

for command in bsdtar fakeroot jq pacman sha256sum; do
  command -v "${command}" >/dev/null
done

initial_package="${1:-}"
upgrade_package="${2:-}"
if [[ -z "${initial_package}" ]]; then
  initial_package="$(find target/tmp/arch-package-smoke/build -maxdepth 1 -type f \
    -name 'fcitx-vinpst-*-1-*.pkg.tar.zst' ! -name '*-debug-*' -print -quit)"
fi
if [[ -z "${upgrade_package}" ]]; then
  upgrade_package="$(find target/tmp/arch-package-smoke/build -maxdepth 1 -type f \
    -name 'fcitx-vinpst-*-2-*.pkg.tar.zst' ! -name '*-debug-*' -print -quit)"
fi

if [[ ! -f "${initial_package}" || ! -f "${upgrade_package}" ]]; then
  echo "initial and upgrade package archives are required; run just arch-package-smoke first" >&2
  exit 1
fi

package_version() {
  bsdtar -xOf "$1" .PKGINFO |
    awk -F ' = ' '$1 == "pkgver" { print $2; exit }'
}

initial_version="$(package_version "${initial_package}")"
upgrade_version="$(package_version "${upgrade_package}")"
test -n "${initial_version}"
test -n "${upgrade_version}"
test "${initial_version}" != "${upgrade_version}"

transaction_root="${repo_root}/target/tmp/arch-package-transaction-smoke"
pacman_root="${transaction_root}/root"
database_path="${transaction_root}/database"
cache_path="${transaction_root}/cache"
config_path="${transaction_root}/pacman.conf"
log_path="${transaction_root}/pacman.log"
user_config="${pacman_root}/home/test/.config/fcitx-vinpst/config.json"

rm -rf "${transaction_root}"
mkdir -p "${pacman_root}" "${database_path}" "${cache_path}" \
  "$(dirname "${user_config}")"
printf '%s\n' \
  '[options]' \
  'Architecture = auto' \
  'SigLevel = Never' \
  'LocalFileSigLevel = Never' >"${config_path}"
cat >"${user_config}" <<'EOF'
{
  "version": 2,
  "future_state": {
    "sentinel": "preserve-incompatible-user-config"
  }
}
EOF
user_config_sha256="$(sha256sum "${user_config}" | awk '{print $1}')"
printf '%s\n' "${user_config_sha256}" >"${transaction_root}/user-config.sha256"

pacman_args=(
  --root "${pacman_root}"
  --dbpath "${database_path}"
  --cachedir "${cache_path}"
  --config "${config_path}"
  --logfile "${log_path}"
  --noconfirm
)

assert_user_config_unchanged() {
  test "$(sha256sum "${user_config}" | awk '{print $1}')" = \
    "${user_config_sha256}"
}

assert_future_config_rejected() {
  local phase="$1"
  local installed_default="${pacman_root}/usr/share/fcitx-vinpst/default-config.json"
  local probe_path="${transaction_root}/${phase}-future-config.json"
  local current_stdout="${transaction_root}/${phase}-current-config.stdout"
  local current_stderr="${transaction_root}/${phase}-current-config.stderr"
  local future_stdout="${transaction_root}/${phase}-future-config.stdout"
  local future_stderr="${transaction_root}/${phase}-future-config.stderr"
  local status

  "${pacman_root}/usr/bin/vinpst" config validate "${installed_default}" --json \
    >"${current_stdout}" 2>"${current_stderr}"
  test -s "${current_stdout}"
  test ! -s "${current_stderr}"

  jq '.version = 2' "${installed_default}" >"${probe_path}"
  set +e
  "${pacman_root}/usr/bin/vinpst" config validate "${probe_path}" --json \
    >"${future_stdout}" 2>"${future_stderr}"
  status=$?
  set -e
  test "${status}" -ne 0
  test ! -s "${future_stdout}"
  test -s "${future_stderr}"
  assert_user_config_unchanged
}

assert_package_files_present() {
  local package_name="$1"
  local listing
  listing="$(fakeroot pacman "${pacman_args[@]}" -Ql "${package_name}")"
  listing="${listing//${pacman_root}/}"
  for path in \
    /usr/bin/vinpst \
    /usr/bin/vinpst-daemon \
    /usr/bin/vinpst-gui \
    /usr/lib/fcitx-vinpst/package-session-common.sh \
    /usr/lib/fcitx-vinpst/package-upgrade-handoff \
    /usr/lib/fcitx-vinpst/package-remove-handoff \
    /usr/lib/fcitx5/fcitx5-vinpst.so \
    /usr/lib/systemd/user/vinpst-daemon.service \
    /usr/share/applications/vinpst-gui.desktop \
    /usr/share/dbus-1/services/org.fcitx.Vinpst.service; do
    grep -qx "${package_name} ${path}" <<<"${listing}"
    test -e "${pacman_root}${path}"
  done
  ! grep -qE "^${package_name} /(etc|home)/" <<<"${listing}"
  fakeroot pacman "${pacman_args[@]}" -Qkk "${package_name}" >/dev/null
}

fakeroot pacman "${pacman_args[@]}" -dd --noscriptlet -U "${initial_package}"
test "$(fakeroot pacman "${pacman_args[@]}" -Q fcitx-vinpst)" = \
  "fcitx-vinpst ${initial_version}"
assert_package_files_present fcitx-vinpst
assert_user_config_unchanged
assert_future_config_rejected installed

fakeroot pacman "${pacman_args[@]}" -dd --noscriptlet -U "${upgrade_package}"
test "$(fakeroot pacman "${pacman_args[@]}" -Q fcitx-vinpst)" = \
  "fcitx-vinpst ${upgrade_version}"
assert_package_files_present fcitx-vinpst
assert_user_config_unchanged
assert_future_config_rejected upgraded

fakeroot pacman "${pacman_args[@]}" -dd --noscriptlet -U "${initial_package}"
test "$(fakeroot pacman "${pacman_args[@]}" -Q fcitx-vinpst)" = \
  "fcitx-vinpst ${initial_version}"
assert_package_files_present fcitx-vinpst
assert_user_config_unchanged
assert_future_config_rejected rolled-back

fakeroot pacman "${pacman_args[@]}" -dd --noscriptlet -R fcitx-vinpst
! fakeroot pacman "${pacman_args[@]}" -Q fcitx-vinpst >/dev/null 2>&1
assert_user_config_unchanged
test "$(cat "${transaction_root}/user-config.sha256")" = \
  "$(sha256sum "${user_config}" | awk '{print $1}')"
if [[ -d "${pacman_root}/usr" ]]; then
  ! find "${pacman_root}/usr" \( -type f -o -type l \) -print -quit |
    grep -q .
fi

echo "Arch package install, upgrade, rollback, and uninstall transaction smoke passed"
