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

check_root="${repo_root}/target/tmp/source-archive-check"
archive_one="${check_root}/fcitx-vinpst-0.1.0-one.tar.gz"
archive_two="${check_root}/fcitx-vinpst-0.1.0-two.tar.gz"
rm -rf "${check_root}"
mkdir -p "${check_root}"

scripts/release/create-source-archive.sh "${archive_one}" 0.1.0 >/dev/null
scripts/release/create-source-archive.sh "${archive_two}" 0.1.0 >/dev/null
cmp "${archive_one}" "${archive_two}"

tar -tzf "${archive_one}" >"${check_root}/listing"
grep -Eq '^fcitx-vinpst-0.1.0/(\./)?Cargo.toml$' "${check_root}/listing"
grep -Eq '^fcitx-vinpst-0.1.0/(\./)?Cargo.lock$' "${check_root}/listing"
grep -Eq '^fcitx-vinpst-0.1.0/(\./)?scripts/release/create-source-archive.sh$' \
  "${check_root}/listing"
if grep -Eq '(^|/)(\.git|target|dist|__pycache__|\.ruff_cache|\.cache)(/|$)' \
  "${check_root}/listing"; then
  echo "source archive includes excluded build or VCS state" >&2
  exit 1
fi
if grep -Eq '\.py[co]$|packaging/arch/PKGBUILD$' "${check_root}/listing"; then
  echo "source archive includes generated packaging or Python cache files" >&2
  exit 1
fi

if scripts/release/create-source-archive.sh \
  "${repo_root}/README-source.tar.gz" 0.1.0 >"${check_root}/unsafe.out" 2>&1; then
  echo "source archive creator accepted an unsafe output location" >&2
  exit 1
fi
grep -q 'output must be under target/ or dist/' "${check_root}/unsafe.out"

if scripts/release/create-source-archive.sh \
  "${check_root}/bad-version.tar.gz" 'invalid version' \
  >"${check_root}/bad-version.out" 2>&1; then
  echo "source archive creator accepted an invalid version" >&2
  exit 1
fi
grep -q 'invalid source archive version' "${check_root}/bad-version.out"

echo "Source archive check passed"
