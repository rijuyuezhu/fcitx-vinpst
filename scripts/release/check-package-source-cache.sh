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

check_root="${repo_root}/target/tmp/package-source-cache-check"
rm -rf "${check_root}"
mkdir -p "${check_root}"

cached_asset="${check_root}/cached-asset"
printf 'verified package source cache fixture\n' >"${cached_asset}"
cached_sha256="$(sha256sum "${cached_asset}" | awk '{print $1}')"

# A valid local cache entry must be accepted without touching the network.
scripts/release/fetch-checked-asset.sh \
  https://example.invalid/never-requested \
  "${cached_asset}" \
  "${cached_sha256}"
grep -qx 'verified package source cache fixture' "${cached_asset}"

ln -s cached-asset "${check_root}/symlink-asset"
if scripts/release/fetch-checked-asset.sh \
  https://example.invalid/never-requested \
  "${check_root}/symlink-asset" \
  "${cached_sha256}" 2>/dev/null; then
  echo "checked asset helper accepted a symlink destination" >&2
  exit 1
fi
[[ -L "${check_root}/symlink-asset" ]]
grep -qx 'verified package source cache fixture' "${cached_asset}"

if scripts/release/fetch-checked-asset.sh \
  https://example.invalid/never-requested \
  "${check_root}/invalid-digest" \
  NOT-A-SHA256 2>/dev/null; then
  echo "checked asset helper accepted an invalid digest" >&2
  exit 1
fi
[[ ! -e "${check_root}/invalid-digest" ]]

if scripts/release/fetch-checked-asset.sh \
  http://example.invalid/never-requested \
  "${check_root}/insecure-url" \
  "${cached_sha256}" 2>/dev/null; then
  echo "checked asset helper accepted a non-HTTPS URL" >&2
  exit 1
fi
[[ ! -e "${check_root}/insecure-url" ]]

echo "Package source cache check passed"
