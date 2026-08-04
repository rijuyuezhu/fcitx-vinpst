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

if [[ "$#" -ne 3 ]]; then
  echo "usage: $0 BUNDLE PUBLIC_KEY EXPECTED_FINGERPRINT" >&2
  exit 2
fi
for command in gpg gpgconf mktemp realpath; do
  command -v "${command}" >/dev/null
done

bundle="$(realpath -e "$1")"
public_key="$(realpath -e "$2")"
expected_fingerprint="${3^^}"
if [[ ! "${expected_fingerprint}" =~ ^([0-9A-F]{40}|[0-9A-F]{64})$ ]]; then
  echo "release signature error: expected fingerprint must be 40 or 64 uppercase hexadecimal characters" >&2
  exit 1
fi
if [[ ! -d "${bundle}" || -L "${bundle}" ]]; then
  echo "release signature error: bundle must be a regular directory" >&2
  exit 1
fi
if [[ ! -f "${public_key}" || -L "${public_key}" ]]; then
  echo "release signature error: public key must be a regular file" >&2
  exit 1
fi
case "${public_key}" in
  "${bundle}"/*)
    echo "release signature error: public key must come from outside the bundle" >&2
    exit 1
    ;;
esac
signature="${bundle}/manifest.json.sig"
manifest="${bundle}/manifest.json"
if [[ ! -f "${signature}" || -L "${signature}" ]]; then
  echo "release signature error: manifest.json.sig is missing or not a regular file" >&2
  exit 1
fi
if [[ ! -f "${manifest}" || -L "${manifest}" ]]; then
  echo "release signature error: manifest.json is missing or not a regular file" >&2
  exit 1
fi

verification_home="$(mktemp -d)"
cleanup() {
  gpg_session_stop "${verification_home}"
  rm -rf "${verification_home}"
}
trap cleanup EXIT
chmod 700 "${verification_home}"
gpg --homedir "${verification_home}" --batch --import "${public_key}" \
  >/dev/null 2>&1
mapfile -t imported_fingerprints < <(
  gpg --homedir "${verification_home}" --batch --with-colons --list-keys |
    awk -F: '$1 == "pub" { want = 1; next } want && $1 == "fpr" { print $10; want = 0 }'
)
if [[ "${#imported_fingerprints[@]}" -ne 1 || \
      "${imported_fingerprints[0]}" != "${expected_fingerprint}" ]]; then
  echo "release signature error: public key fingerprint does not match the expected trust root" >&2
  exit 1
fi

status_output="${verification_home}/verify.status"
error_output="${verification_home}/verify.stderr"
if ! gpg --homedir "${verification_home}" --batch --no-auto-key-retrieve \
  --status-fd=1 --verify "${signature}" "${manifest}" \
  >"${status_output}" 2>"${error_output}"; then
  cat "${error_output}" >&2
  echo "release signature error: detached manifest signature verification failed" >&2
  exit 1
fi
awk -v expected="${expected_fingerprint}" '
  $1 == "[GNUPG:]" && $2 == "VALIDSIG" && ($3 == expected || $NF == expected) {
    valid = 1
  }
  END { exit(valid ? 0 : 1) }
' "${status_output}" || {
  echo "release signature error: valid signature did not match the expected trust root" >&2
  exit 1
}

scripts/release/release_manifest.py verify "${bundle}"
echo "Release bundle signature verified: ${expected_fingerprint}"
