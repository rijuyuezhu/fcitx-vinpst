#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
check_root="${repo_root}/target/tmp/aur-pkgbuild-check"
rm -rf "${check_root}"
mkdir -p "${check_root}"

package_sha256='0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef'
rendered="${check_root}/PKGBUILD"
"${repo_root}/scripts/release/render-aur-pkgbuild.py" \
  --version 9.8.7 \
  --package-sha256 "${package_sha256}" \
  --output "${rendered}"

bash -n "${rendered}"
(
  pkgname='' pkgver='' pkgrel='' install=''
  arch=() provides=() conflicts=() options=() sha256sums_x86_64=() source_x86_64=() noextract=()
  # shellcheck disable=SC1090
  source "${rendered}"
  [[ "${pkgname}" == fcitx-vinpst-bin ]]
  [[ "${pkgver}" == 9.8.7 ]]
  [[ "${pkgrel}" == 1 ]]
  [[ "${arch[*]}" == x86_64 ]]
  [[ " ${provides[*]} " == *' fcitx-vinpst '* ]]
  [[ " ${conflicts[*]} " == *' fcitx-vinpst '* ]]
  [[ " ${options[*]} " == *' !strip '* ]]
  [[ "${install}" == fcitx-vinpst.install ]]
  [[ "${sha256sums_x86_64[*]}" == "${package_sha256}" ]]
  [[ "${source_x86_64[*]}" == *'/releases/download/v9.8.7/fcitx-vinpst-9.8.7-1-x86_64.pkg.tar.zst'* ]]
  [[ "${noextract[*]}" == fcitx-vinpst-9.8.7-1-x86_64.pkg.tar.zst ]]
  declare -F package >/dev/null
)

if "${repo_root}/scripts/release/render-aur-pkgbuild.py" \
  --version 9.8.7 \
  --package-sha256 invalid \
  --output "${check_root}/invalid-PKGBUILD" \
  >"${check_root}/invalid.out" 2>"${check_root}/invalid.err"; then
  echo "AUR PKGBUILD renderer accepted an invalid package digest" >&2
  exit 1
fi
grep -Fq 'SHA-256 must be 64 lowercase hexadecimal characters' "${check_root}/invalid.err"

printf 'AUR PKGBUILD check passed\n'
