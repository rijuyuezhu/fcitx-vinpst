#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"
export LC_ALL=C

for command in gpg jq python3; do
  command -v "${command}" >/dev/null
done

stage_root="${repo_root}/target/tmp/release-signature-check"
artifacts_root="${stage_root}/artifacts"
bundle="${stage_root}/bundle"
signing_home="${stage_root}/signing-home"
wrong_home="${stage_root}/wrong-home"
public_key="${stage_root}/public-key.asc"
wrong_public_key="${stage_root}/wrong-public-key.asc"
rm -rf "${stage_root}"
mkdir -p "${artifacts_root}" "${signing_home}" "${wrong_home}"
chmod 700 "${signing_home}" "${wrong_home}"
printf 'package payload\n' >"${artifacts_root}/package.pkg.tar.zst"
printf 'repository payload\n' >"${artifacts_root}/repository.db.tar.gz"

scripts/release_manifest.py assemble \
  --package-name fcitx-vinput-rs \
  --version 0.1.0 \
  --architecture x86_64 \
  --output-dir "${bundle}" \
  --artifact "package=${artifacts_root}/package.pkg.tar.zst" \
  --artifact "repository-database=${artifacts_root}/repository.db.tar.gz"

generate_key() {
  local home="$1"
  local identity="$2"
  local output="$3"
  gpg --homedir "${home}" --batch --passphrase '' \
    --quick-generate-key "${identity}" ed25519 sign 1d >/dev/null 2>&1
  local fingerprint
  fingerprint="$(
    gpg --homedir "${home}" --batch --with-colons --list-secret-keys |
      awk -F: '$1 == "fpr" { print $10; exit }'
  )"
  test -n "${fingerprint}"
  gpg --homedir "${home}" --batch --armor --export "${fingerprint}" >"${output}"
  printf '%s\n' "${fingerprint}"
}

fingerprint="$(
  generate_key "${signing_home}" \
    'Vinput Release Signature Check <release-signature@example.invalid>' \
    "${public_key}"
)"
wrong_fingerprint="$(
  generate_key "${wrong_home}" \
    'Vinput Wrong Signature Check <wrong-signature@example.invalid>' \
    "${wrong_public_key}"
)"
test "${fingerprint}" != "${wrong_fingerprint}"

scripts/sign-release-manifest.sh "${bundle}" "${signing_home}" "${fingerprint}"
test "$(stat -c '%a' "${bundle}/manifest.json.sig")" = 644
scripts/verify-release-bundle-signature.sh \
  "${bundle}" "${public_key}" "${fingerprint}"
scripts/release_manifest.py verify "${bundle}"

test "$(find "${bundle}" -maxdepth 1 -type f | wc -l)" -eq 5
! grep -q 'manifest.json.sig' "${bundle}/SHA256SUMS"
jq -e '[.artifacts[].name] | index("manifest.json.sig") == null' \
  "${bundle}/manifest.json" >/dev/null
! find "${bundle}" -maxdepth 1 -type f -printf '%f\n' |
  grep -Ei '(secret|private|trustdb|revocation)'

expect_failure() {
  local name="$1"
  local expected="$2"
  shift 2
  set +e
  "$@" >"${stage_root}/${name}.out" 2>&1
  local status=$?
  set -e
  test "${status}" -ne 0
  grep -qi "${expected}" "${stage_root}/${name}.out"
}

missing_bundle="${stage_root}/missing-signature"
cp -a "${bundle}" "${missing_bundle}"
rm "${missing_bundle}/manifest.json.sig"
expect_failure missing-signature 'manifest.json.sig is missing' \
  scripts/verify-release-bundle-signature.sh \
  "${missing_bundle}" "${public_key}" "${fingerprint}"

expect_failure wrong-fingerprint 'fingerprint does not match' \
  scripts/verify-release-bundle-signature.sh \
  "${bundle}" "${public_key}" "${wrong_fingerprint}"

expect_failure wrong-key 'detached manifest signature verification failed' \
  scripts/verify-release-bundle-signature.sh \
  "${bundle}" "${wrong_public_key}" "${wrong_fingerprint}"

inside_key_bundle="${stage_root}/inside-key"
cp -a "${bundle}" "${inside_key_bundle}"
cp "${public_key}" "${inside_key_bundle}/public-key.asc"
expect_failure inside-key 'must come from outside the bundle' \
  scripts/verify-release-bundle-signature.sh \
  "${inside_key_bundle}" "${inside_key_bundle}/public-key.asc" "${fingerprint}"

manifest_tamper_bundle="${stage_root}/manifest-tamper"
cp -a "${bundle}" "${manifest_tamper_bundle}"
python3 - "${manifest_tamper_bundle}/manifest.json" <<'PY'
from pathlib import Path
import json
import sys

path = Path(sys.argv[1])
data = json.loads(path.read_text())
data["package"]["version"] = "9.9.9"
path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
PY
expect_failure manifest-tamper 'detached manifest signature verification failed' \
  scripts/verify-release-bundle-signature.sh \
  "${manifest_tamper_bundle}" "${public_key}" "${fingerprint}"

signature_tamper_bundle="${stage_root}/signature-tamper"
cp -a "${bundle}" "${signature_tamper_bundle}"
python3 - "${signature_tamper_bundle}/manifest.json.sig" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
data = bytearray(path.read_bytes())
data[len(data) // 2] ^= 0x01
path.write_bytes(data)
PY
expect_failure signature-tamper 'detached manifest signature verification failed' \
  scripts/verify-release-bundle-signature.sh \
  "${signature_tamper_bundle}" "${public_key}" "${fingerprint}"

artifact_tamper_bundle="${stage_root}/artifact-tamper"
cp -a "${bundle}" "${artifact_tamper_bundle}"
printf 'tamper\n' >>"${artifact_tamper_bundle}/package.pkg.tar.zst"
expect_failure artifact-tamper 'artifact size mismatch' \
  scripts/verify-release-bundle-signature.sh \
  "${artifact_tamper_bundle}" "${public_key}" "${fingerprint}"

expect_failure unavailable-secret-key 'exact secret signing key is unavailable' \
  scripts/sign-release-manifest.sh \
  "${bundle}" "${signing_home}" "${wrong_fingerprint}"

# Re-signing is explicit and atomic; rebuilding with --force removes the old signature.
scripts/sign-release-manifest.sh "${bundle}" "${signing_home}" "${fingerprint}"
scripts/release_manifest.py assemble \
  --package-name fcitx-vinput-rs \
  --version 0.1.0 \
  --architecture x86_64 \
  --output-dir "${bundle}" \
  --artifact "package=${artifacts_root}/package.pkg.tar.zst" \
  --artifact "repository-database=${artifacts_root}/repository.db.tar.gz" \
  --force
! test -e "${bundle}/manifest.json.sig"
expect_failure rebuilt-unsigned 'manifest.json.sig is missing' \
  scripts/verify-release-bundle-signature.sh \
  "${bundle}" "${public_key}" "${fingerprint}"

echo "Release manifest signature check passed"
