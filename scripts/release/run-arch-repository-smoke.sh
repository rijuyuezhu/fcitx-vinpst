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
export LC_ALL=C

for command in bsdtar fakeroot pacman repo-add sha256sum; do
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

stage_root="${repo_root}/target/tmp/arch-repository-smoke"
repository_root="${stage_root}/repository"
pacman_root="${stage_root}/root"
database_path="${stage_root}/database"
cache_path="${stage_root}/cache"
config_path="${stage_root}/pacman.conf"
log_path="${stage_root}/pacman.log"
user_config="${pacman_root}/home/test/.config/fcitx-vinpst/config.json"
repository_name="vinpst-local"
repository_database="${repository_root}/${repository_name}.db.tar.gz"

rm -rf "${stage_root}"
mkdir -p \
  "${repository_root}" \
  "${pacman_root}" \
  "${database_path}" \
  "${cache_path}" \
  "$(dirname "${user_config}")"

initial_repository_package="${repository_root}/$(basename "${initial_package}")"
upgrade_repository_package="${repository_root}/$(basename "${upgrade_package}")"
cp "${initial_package}" "${initial_repository_package}"
cp "${upgrade_package}" "${upgrade_repository_package}"
repo-add "${repository_database}" "${initial_repository_package}"

printf '%s\n' \
  '[options]' \
  'Architecture = x86_64' \
  'SigLevel = Never' \
  'LocalFileSigLevel = Never' \
  '' \
  "[${repository_name}]" \
  'SigLevel = Never' \
  "Server = file://${repository_root}" >"${config_path}"
printf '%s\n' '{"sentinel":"preserve-user-config"}' >"${user_config}"
user_config_sha256="$(sha256sum "${user_config}" | awk '{print $1}')"

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

assert_repository_version() {
  local expected_version="$1"
  local info
  info="$(fakeroot pacman "${pacman_args[@]}" -Si fcitx-vinpst)"
  grep -qx "Repository      : ${repository_name}" <<<"${info}"
  grep -qx 'Name            : fcitx-vinpst' <<<"${info}"
  grep -qx "Version         : ${expected_version}" <<<"${info}"
  grep -qx 'Architecture    : x86_64' <<<"${info}"
  grep -qx 'Provides        : fcitx5-vinpst=0.1.0' <<<"${info}"
  grep -qx 'Conflicts With  : fcitx5-vinpst' <<<"${info}"
}

assert_installed_version() {
  local expected_version="$1"
  test "$(fakeroot pacman "${pacman_args[@]}" -Q fcitx-vinpst)" = \
    "fcitx-vinpst ${expected_version}"
  fakeroot pacman "${pacman_args[@]}" -Qkk fcitx-vinpst >/dev/null
  test -x "${pacman_root}/usr/bin/vinpst"
  test -x "${pacman_root}/usr/bin/vinpst-daemon"
  test -x "${pacman_root}/usr/bin/vinpst-gui"
  test -f "${pacman_root}/usr/share/applications/vinpst-gui.desktop"
  assert_user_config_unchanged
}

fakeroot pacman "${pacman_args[@]}" -Syy
assert_repository_version "${initial_version}"
fakeroot pacman "${pacman_args[@]}" -Sdd --noscriptlet fcitx-vinpst
assert_installed_version "${initial_version}"
test -f "${cache_path}/$(basename "${initial_package}")"

repo-add "${repository_database}" "${upgrade_repository_package}"
repository_entries="$(bsdtar -tf "${repository_database}")"
grep -qx "fcitx-vinpst-${upgrade_version}/" <<<"${repository_entries}"
! grep -q "fcitx-vinpst-${initial_version}/" <<<"${repository_entries}"

fakeroot pacman "${pacman_args[@]}" -Syy
assert_repository_version "${upgrade_version}"
fakeroot pacman "${pacman_args[@]}" -Sdd --noscriptlet fcitx-vinpst
assert_installed_version "${upgrade_version}"
test -f "${cache_path}/$(basename "${upgrade_package}")"

fakeroot pacman "${pacman_args[@]}" -Rdd --noscriptlet fcitx-vinpst
! fakeroot pacman "${pacman_args[@]}" -Q fcitx-vinpst >/dev/null 2>&1
assert_user_config_unchanged
if [[ -d "${pacman_root}/usr" ]]; then
  ! find "${pacman_root}/usr" \( -type f -o -type l \) -print -quit |
    grep -q .
fi

echo "Arch local repository install and upgrade smoke passed"
