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
# shellcheck source=scripts/release/gpg-session-common.sh
source "${script_dir}/gpg-session-common.sh"

for command in bsdtar fakeroot gpg gpgconf pacman pacman-key python3 repo-add sha256sum; do
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
package_base_version="${initial_version%-*}"
test -n "${initial_version}"
test -n "${upgrade_version}"
test -n "${package_base_version}"
test "${initial_version}" != "${upgrade_version}"

stage_root="${repo_root}/target/tmp/arch-signing-smoke"
signing_home="${stage_root}/signing-home"
trusted_keyring="${stage_root}/trusted-keyring"
repository_root="${stage_root}/repository"
repository_name="vinpst-signed"
repository_database="${repository_root}/${repository_name}.db.tar.gz"
public_key="${stage_root}/public-key.asc"
user_config_relative="home/test/.config/fcitx-vinpst/config.json"

cleanup() {
  gpg_session_stop "${signing_home}"
}
trap cleanup EXIT
cleanup
rm -rf "${stage_root}"
mkdir -p "${signing_home}" "${trusted_keyring}" "${repository_root}"
chmod 700 "${signing_home}" "${trusted_keyring}"

initial_repository_package="${repository_root}/$(basename "${initial_package}")"
upgrade_repository_package="${repository_root}/$(basename "${upgrade_package}")"
cp "${initial_package}" "${initial_repository_package}"
cp "${upgrade_package}" "${upgrade_repository_package}"

gpg --homedir "${signing_home}" --batch --passphrase '' \
  --quick-generate-key \
  'Vinpst Package Smoke <package-smoke@example.invalid>' \
  ed25519 sign 1d
fingerprint="$(
  gpg --homedir "${signing_home}" --batch --with-colons --list-secret-keys |
    awk -F: '$1 == "fpr" { print $10; exit }'
)"
test -n "${fingerprint}"
gpg --homedir "${signing_home}" --batch --armor --export "${fingerprint}" \
  >"${public_key}"

sign_package() {
  local package="$1"
  gpg --homedir "${signing_home}" --batch --yes --detach-sign \
    --output "${package}.sig" "${package}"
  gpg --homedir "${signing_home}" --batch --verify \
    "${package}.sig" "${package}" >/dev/null 2>&1
}

sign_package "${initial_repository_package}"
sign_package "${upgrade_repository_package}"
GNUPGHOME="${signing_home}" repo-add \
  --include-sigs --sign --key "${fingerprint}" \
  "${repository_database}" "${initial_repository_package}"
test -f "${repository_database}.sig"
test -f "${repository_root}/${repository_name}.files.tar.gz.sig"

fakeroot pacman-key --gpgdir "${trusted_keyring}" --init >/dev/null
fakeroot pacman-key --gpgdir "${trusted_keyring}" --add "${public_key}"
fakeroot pacman-key --gpgdir "${trusted_keyring}" --lsign-key "${fingerprint}"
fakeroot pacman-key --gpgdir "${trusted_keyring}" --verify \
  "${repository_database}.sig" >/dev/null

write_pacman_config() {
  local output="$1"
  local keyring="$2"
  local repository="$3"
  printf '%s\n' \
    '[options]' \
    'Architecture = x86_64' \
    "GPGDir = ${keyring}" \
    'SigLevel = Required DatabaseRequired' \
    'LocalFileSigLevel = Required' \
    '' \
    "[${repository_name}]" \
    'SigLevel = Required DatabaseRequired' \
    "Server = file://${repository}" >"${output}"
}

prepare_pacman_root() {
  local name="$1"
  local keyring="$2"
  local repository="$3"
  local root="${stage_root}/${name}-root"
  local database="${stage_root}/${name}-database"
  local cache="${stage_root}/${name}-cache"
  local config="${stage_root}/${name}.conf"
  local log="${stage_root}/${name}.log"
  rm -rf "${root}" "${database}" "${cache}"
  mkdir -p "${root}/$(dirname "${user_config_relative}")" "${database}" "${cache}"
  printf '%s\n' '{"sentinel":"preserve-user-config"}' \
    >"${root}/${user_config_relative}"
  write_pacman_config "${config}" "${keyring}" "${repository}"
  printf '%s\0' \
    "${root}" "${database}" "${cache}" "${config}" "${log}"
}

read_root_values() {
  local name="$1"
  local keyring="$2"
  local repository="$3"
  mapfile -d '' -t root_values < <(
    prepare_pacman_root "${name}" "${keyring}" "${repository}"
  )
  pacman_root="${root_values[0]}"
  database_path="${root_values[1]}"
  cache_path="${root_values[2]}"
  config_path="${root_values[3]}"
  log_path="${root_values[4]}"
  user_config="${pacman_root}/${user_config_relative}"
  user_config_sha256="$(sha256sum "${user_config}" | awk '{print $1}')"
  pacman_args=(
    --root "${pacman_root}"
    --dbpath "${database_path}"
    --cachedir "${cache_path}"
    --config "${config_path}"
    --logfile "${log_path}"
    --noconfirm
  )
}

assert_user_config_unchanged() {
  test "$(sha256sum "${user_config}" | awk '{print $1}')" = \
    "${user_config_sha256}"
}

assert_signed_repository_version() {
  local expected_version="$1"
  local info
  info="$(fakeroot pacman "${pacman_args[@]}" -Si fcitx-vinpst)"
  grep -qx "Repository      : ${repository_name}" <<<"${info}"
  grep -qx "Version         : ${expected_version}" <<<"${info}"
  grep -qx "Provides        : fcitx5-vinpst=${package_base_version}" <<<"${info}"
  grep -qx 'Validated By    : SHA-256 Sum  Signature' <<<"${info}"
}

assert_installed_version() {
  local expected_version="$1"
  test "$(fakeroot pacman "${pacman_args[@]}" -Q fcitx-vinpst)" = \
    "fcitx-vinpst ${expected_version}"
  fakeroot pacman "${pacman_args[@]}" -Qkk fcitx-vinpst >/dev/null
  assert_user_config_unchanged
}

read_root_values trusted "${trusted_keyring}" "${repository_root}"
fakeroot pacman "${pacman_args[@]}" -Syy
assert_signed_repository_version "${initial_version}"
fakeroot pacman "${pacman_args[@]}" -Sdd --noscriptlet fcitx-vinpst
assert_installed_version "${initial_version}"
test -f "${cache_path}/$(basename "${initial_package}")"

GNUPGHOME="${signing_home}" repo-add \
  --include-sigs --verify --sign --key "${fingerprint}" \
  "${repository_database}" "${upgrade_repository_package}"
fakeroot pacman-key --gpgdir "${trusted_keyring}" --verify \
  "${repository_database}.sig" >/dev/null
fakeroot pacman "${pacman_args[@]}" -Syy
assert_signed_repository_version "${upgrade_version}"
fakeroot pacman "${pacman_args[@]}" -Sdd --noscriptlet fcitx-vinpst
assert_installed_version "${upgrade_version}"
test -f "${cache_path}/$(basename "${upgrade_package}")"
fakeroot pacman "${pacman_args[@]}" -Rdd --noscriptlet fcitx-vinpst
! fakeroot pacman "${pacman_args[@]}" -Q fcitx-vinpst >/dev/null 2>&1
assert_user_config_unchanged

untrusted_keyring="${stage_root}/untrusted-keyring"
mkdir -p "${untrusted_keyring}"
chmod 700 "${untrusted_keyring}"
fakeroot pacman-key --gpgdir "${untrusted_keyring}" --init >/dev/null
printf '%s\n' 'keyserver hkp://127.0.0.1:9' \
  >>"${untrusted_keyring}/gpg.conf"
read_root_values untrusted "${untrusted_keyring}" "${repository_root}"
set +e
fakeroot pacman "${pacman_args[@]}" -Syy \
  >"${stage_root}/untrusted.out" 2>&1
untrusted_status=$?
set -e
test "${untrusted_status}" -ne 0
grep -qi 'invalid or corrupted database (PGP signature)' \
  "${stage_root}/untrusted.out"
! fakeroot pacman "${pacman_args[@]}" -Q fcitx-vinpst >/dev/null 2>&1
assert_user_config_unchanged

tampered_repository="${stage_root}/tampered-repository"
cp -a "${repository_root}" "${tampered_repository}"
tampered_package="${tampered_repository}/$(basename "${upgrade_package}")"
python3 - "${tampered_package}" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
data = bytearray(path.read_bytes())
data[len(data) // 2] ^= 0x01
path.write_bytes(data)
PY
read_root_values tampered "${trusted_keyring}" "${tampered_repository}"
fakeroot pacman "${pacman_args[@]}" -Syy
set +e
fakeroot pacman "${pacman_args[@]}" -Sdd --noscriptlet fcitx-vinpst \
  >"${stage_root}/tampered.out" 2>&1
tampered_status=$?
set -e
test "${tampered_status}" -ne 0
grep -qi 'invalid or corrupted package (PGP signature)' \
  "${stage_root}/tampered.out"
! fakeroot pacman "${pacman_args[@]}" -Q fcitx-vinpst >/dev/null 2>&1
assert_user_config_unchanged

echo "Arch signed repository trust and tamper smoke passed"
