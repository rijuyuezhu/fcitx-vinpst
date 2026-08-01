#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

for command in bsdtar fakeroot pacman sha256sum; do
  command -v "${command}" >/dev/null
done

initial_package="${1:-}"
upgrade_package="${2:-}"
if [[ -z "${initial_package}" ]]; then
  initial_package="$(find target/tmp/arch-package-smoke/build -maxdepth 1 -type f \
    -name 'fcitx-vinput-rs-*-1-*.pkg.tar.zst' ! -name '*-debug-*' -print -quit)"
fi
if [[ -z "${upgrade_package}" ]]; then
  upgrade_package="$(find target/tmp/arch-package-smoke/build -maxdepth 1 -type f \
    -name 'fcitx-vinput-rs-*-2-*.pkg.tar.zst' ! -name '*-debug-*' -print -quit)"
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
user_config="${pacman_root}/home/test/.config/fcitx-vinput/config.json"

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
  local stderr_path="${transaction_root}/${phase}-future-config.stderr"
  local stdout_path="${transaction_root}/${phase}-future-config.stdout"
  local status
  set +e
  "${pacman_root}/usr/bin/vinput" config validate "${user_config}" --json \
    >"${stdout_path}" 2>"${stderr_path}"
  status=$?
  set -e
  test "${status}" -ne 0
  test ! -s "${stdout_path}"
  grep -Fq \
    'unsupported config schema version 2; this binary supports up to 1' \
    "${stderr_path}"
  assert_user_config_unchanged
}

assert_package_files_present() {
  local package_name="$1"
  local listing
  listing="$(fakeroot pacman "${pacman_args[@]}" -Ql "${package_name}")"
  listing="${listing//${pacman_root}/}"
  for path in \
    /usr/bin/vinput \
    /usr/bin/vinput-daemon \
    /usr/lib/fcitx-vinput/package-remove-handoff \
    /usr/lib/fcitx5/fcitx5-vinput.so \
    /usr/lib/systemd/user/vinput-daemon.service \
    /usr/share/dbus-1/services/org.fcitx.Vinput.service; do
    grep -qx "${package_name} ${path}" <<<"${listing}"
    test -e "${pacman_root}${path}"
  done
  ! grep -qE "^${package_name} /(etc|home)/" <<<"${listing}"
  fakeroot pacman "${pacman_args[@]}" -Qkk "${package_name}" >/dev/null
}

fakeroot pacman "${pacman_args[@]}" -dd --noscriptlet -U "${initial_package}"
test "$(fakeroot pacman "${pacman_args[@]}" -Q fcitx-vinput-rs)" = \
  "fcitx-vinput-rs ${initial_version}"
assert_package_files_present fcitx-vinput-rs
assert_user_config_unchanged
assert_future_config_rejected installed

fakeroot pacman "${pacman_args[@]}" -dd --noscriptlet -U "${upgrade_package}"
test "$(fakeroot pacman "${pacman_args[@]}" -Q fcitx-vinput-rs)" = \
  "fcitx-vinput-rs ${upgrade_version}"
assert_package_files_present fcitx-vinput-rs
assert_user_config_unchanged
assert_future_config_rejected upgraded

fakeroot pacman "${pacman_args[@]}" -dd --noscriptlet -U "${initial_package}"
test "$(fakeroot pacman "${pacman_args[@]}" -Q fcitx-vinput-rs)" = \
  "fcitx-vinput-rs ${initial_version}"
assert_package_files_present fcitx-vinput-rs
assert_user_config_unchanged
assert_future_config_rejected rolled-back

fakeroot pacman "${pacman_args[@]}" -dd --noscriptlet -R fcitx-vinput-rs
! fakeroot pacman "${pacman_args[@]}" -Q fcitx-vinput-rs >/dev/null 2>&1
assert_user_config_unchanged
test "$(cat "${transaction_root}/user-config.sha256")" = \
  "$(sha256sum "${user_config}" | awk '{print $1}')"
if [[ -d "${pacman_root}/usr" ]]; then
  ! find "${pacman_root}/usr" \( -type f -o -type l \) -print -quit |
    grep -q .
fi

echo "Arch package install, upgrade, rollback, and uninstall transaction smoke passed"
