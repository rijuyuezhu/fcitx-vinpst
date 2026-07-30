#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"
export LC_ALL=C

if [[ "$#" -ne 3 ]]; then
  echo "usage: $0 BUNDLE GPG_HOME FINGERPRINT" >&2
  exit 2
fi
for command in gpg mktemp realpath; do
  command -v "${command}" >/dev/null
done

bundle="$(realpath -e "$1")"
signing_home="$(realpath -e "$2")"
fingerprint="${3^^}"
if [[ ! "${fingerprint}" =~ ^([0-9A-F]{40}|[0-9A-F]{64})$ ]]; then
  echo "release signature error: fingerprint must be 40 or 64 uppercase hexadecimal characters" >&2
  exit 1
fi
if [[ ! -d "${bundle}" || -L "${bundle}" ]]; then
  echo "release signature error: bundle must be a regular directory" >&2
  exit 1
fi
if [[ ! -d "${signing_home}" || -L "${signing_home}" ]]; then
  echo "release signature error: GPG home must be a regular directory" >&2
  exit 1
fi

scripts/release_manifest.py verify "${bundle}"
secret_fingerprint="$(
  {
    gpg --homedir "${signing_home}" --batch --with-colons \
      --list-secret-keys "${fingerprint}" 2>/dev/null || true
  } |
    awk -F: '$1 == "sec" { want = 1; next } want && $1 == "fpr" { print $10; exit }'
)"
if [[ "${secret_fingerprint}" != "${fingerprint}" ]]; then
  echo "release signature error: exact secret signing key is unavailable" >&2
  exit 1
fi

temporary_signature="$(
  mktemp "$(dirname "${bundle}")/.manifest.json.sig.XXXXXX"
)"
cleanup() {
  rm -f "${temporary_signature}"
}
trap cleanup EXIT

gpg --homedir "${signing_home}" --batch --yes \
  --local-user "${fingerprint}" --detach-sign \
  --output "${temporary_signature}" "${bundle}/manifest.json"
gpg --homedir "${signing_home}" --batch --no-auto-key-retrieve --verify \
  "${temporary_signature}" "${bundle}/manifest.json" >/dev/null 2>&1
test -s "${temporary_signature}"
mv -f "${temporary_signature}" "${bundle}/manifest.json.sig"
chmod 644 "${bundle}/manifest.json.sig"
trap - EXIT

scripts/release_manifest.py verify "${bundle}"
echo "Release manifest signed: ${bundle}/manifest.json.sig"
