#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

version="$(
  cargo metadata --no-deps --format-version 1 |
    jq -r '.packages[] | select(.name == "vinput-cli") | .version'
)"
test -n "${version}"

check_root="${repo_root}/target/tmp/arch-pkgbuild-check"
rm -rf "${check_root}"
mkdir -p "${check_root}"

scripts/render-arch-pkgbuild.py \
  --version "${version}" \
  --source-url file:///tmp/fcitx-vinput-rs-source.tar.gz \
  --source-sha256 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef \
  --source-dir "fcitx-vinput-rs-${version}" \
  --output "${check_root}/PKGBUILD"

bash -n "${check_root}/PKGBUILD"
(
  cd "${check_root}"
  makepkg --printsrcinfo >.SRCINFO
)

srcinfo="${check_root}/.SRCINFO"
grep -qx $'\t'"pkgver = ${version}" "${srcinfo}"
grep -qx $'\tarch = x86_64' "${srcinfo}"
grep -qx $'\t'"provides = fcitx5-vinput=${version}" "${srcinfo}"
grep -qx $'\tconflicts = fcitx5-vinput' "${srcinfo}"
grep -qx $'\toptions = !debug' "${srcinfo}"
grep -qx $'\toptions = !lto' "${srcinfo}"
grep -qx $'\tdepends = libpipewire' "${srcinfo}"
grep -qx $'\tdepends = systemd-libs' "${srcinfo}"
grep -qx $'\tsha256sums = 650d3da32694fa48e6e018f7087e4840aace56b3187a294a18ba3b9f51e80943' "${srcinfo}"
grep -qx $'\tsha256sums = cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30' "${srcinfo}"
grep -qx $'\tsha256sums = 2f07c72751aed99790b8a4869cf2311df85a860b22ded05fa22803587a48922c' "${srcinfo}"

echo "Arch PKGBUILD metadata check passed"
