#!/usr/bin/env bash
set -euo pipefail

if (($# != 3)); then
  echo "usage: fetch-checked-asset.sh URL OUTPUT SHA256" >&2
  exit 2
fi

url="$1"
output="$2"
expected_sha256="$3"

if [[ ! "${url}" =~ ^https:// ]]; then
  echo "checked asset URL must use https: ${url}" >&2
  exit 2
fi
if [[ ! "${expected_sha256}" =~ ^[0-9a-f]{64}$ ]]; then
  echo "checked asset digest must be lowercase SHA-256" >&2
  exit 2
fi
if [[ -L "${output}" ]]; then
  echo "checked asset destination must not be a symlink: ${output}" >&2
  exit 1
fi

mkdir -p "$(dirname "${output}")"

asset_matches() {
  [[ -f "${output}" ]] || return 1
  local actual_sha256
  actual_sha256="$(sha256sum "${output}" | awk '{print $1}')"
  [[ "${actual_sha256}" == "${expected_sha256}" ]]
}

if asset_matches; then
  exit 0
fi
rm -f "${output}"

temporary="${output}.partial.$$"
cleanup() {
  rm -f "${temporary}"
}
trap cleanup EXIT

curl \
  --retry 5 \
  --retry-all-errors \
  --retry-delay 2 \
  --connect-timeout 30 \
  --max-time 300 \
  --speed-limit 32768 \
  --speed-time 30 \
  --proto '=https' \
  --tlsv1.2 \
  -fsSL "${url}" \
  -o "${temporary}"

actual_sha256="$(sha256sum "${temporary}" | awk '{print $1}')"
if [[ "${actual_sha256}" != "${expected_sha256}" ]]; then
  echo "checked asset digest mismatch from ${url}" >&2
  exit 1
fi

mv "${temporary}" "${output}"
trap - EXIT
