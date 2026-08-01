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

for command in bsdtar cmp gpg jq python3 sha256sum; do
  command -v "${command}" >/dev/null
done

source_archive="${1:-}"
initial_package="${2:-}"
upgrade_package="${3:-}"
if [[ -z "${source_archive}" ]]; then
  source_archive="$(find target/tmp/arch-package-smoke/sources -maxdepth 1 -type f \
    -name 'fcitx-vinput-rs-*.tar.gz' -print -quit)"
fi
if [[ -z "${initial_package}" ]]; then
  initial_package="$(find target/tmp/arch-package-smoke/build -maxdepth 1 -type f \
    -name 'fcitx-vinput-rs-*-1-*.pkg.tar.zst' ! -name '*-debug-*' -print -quit)"
fi
if [[ -z "${upgrade_package}" ]]; then
  upgrade_package="$(find target/tmp/arch-package-smoke/build -maxdepth 1 -type f \
    -name 'fcitx-vinput-rs-*-2-*.pkg.tar.zst' ! -name '*-debug-*' -print -quit)"
fi
for artifact in "${source_archive}" "${initial_package}" "${upgrade_package}"; do
  test -f "${artifact}"
done

package_version() {
  bsdtar -xOf "$1" .PKGINFO |
    awk -F ' = ' '$1 == "pkgver" { print $2; exit }'
}

initial_version="$(package_version "${initial_package}")"
upgrade_version="$(package_version "${upgrade_package}")"
version="${initial_version%-*}"
test "${initial_version}" = "${version}-1"
test "${upgrade_version}" = "${version}-2"

build_root="$(dirname "${initial_package}")"
signing_root="${repo_root}/target/tmp/arch-signing-smoke"
signing_home="${signing_root}/signing-home"
signed_repository="${signing_root}/repository"
repository_name="vinput-signed"
signed_initial_package="${signed_repository}/$(basename "${initial_package}")"
signed_upgrade_package="${signed_repository}/$(basename "${upgrade_package}")"
repository_database="${signed_repository}/${repository_name}.db.tar.gz"
repository_files="${signed_repository}/${repository_name}.files.tar.gz"
public_key="${signing_root}/public-key.asc"
fingerprint="$(
  gpg --homedir "${signing_home}" --batch --with-colons --list-secret-keys |
    awk -F: '$1 == "fpr" { print $10; exit }'
)"
test -n "${fingerprint}"

required_artifacts=(
  "${build_root}/PKGBUILD"
  "${build_root}/.SRCINFO"
  "${build_root}/fcitx-vinput-rs.install"
  "${signed_initial_package}"
  "${signed_initial_package}.sig"
  "${signed_upgrade_package}"
  "${signed_upgrade_package}.sig"
  "${repository_database}"
  "${repository_database}.sig"
  "${repository_files}"
  "${repository_files}.sig"
  "${public_key}"
)
for artifact in "${required_artifacts[@]}"; do
  test -f "${artifact}"
  test ! -L "${artifact}"
done
cmp "${initial_package}" "${signed_initial_package}"
cmp "${upgrade_package}" "${signed_upgrade_package}"

stage_root="${repo_root}/target/tmp/arch-release-bundle-smoke"
bundle="${stage_root}/fcitx-vinput-rs-${version}-x86_64-release-gate"
rm -rf "${stage_root}"
mkdir -p "${stage_root}"

scripts/release/release_manifest.py assemble \
  --package-name fcitx-vinput-rs \
  --version "${version}" \
  --architecture x86_64 \
  --output-dir "${bundle}" \
  --artifact "source-archive=${source_archive}" \
  --artifact "arch-pkgbuild=${build_root}/PKGBUILD" \
  --artifact "arch-srcinfo=${build_root}/.SRCINFO" \
  --artifact "arch-install-script=${build_root}/fcitx-vinput-rs.install" \
  --artifact "package-pkgrel1=${signed_initial_package}" \
  --artifact "package-signature-pkgrel1=${signed_initial_package}.sig" \
  --artifact "package-pkgrel2-test=${signed_upgrade_package}" \
  --artifact "package-signature-pkgrel2-test=${signed_upgrade_package}.sig" \
  --artifact "repository-database=${repository_database}" \
  --artifact "repository-database-signature=${repository_database}.sig" \
  --artifact "repository-files=${repository_files}" \
  --artifact "repository-files-signature=${repository_files}.sig" \
  --artifact "signing-public-key-test=${public_key}"

scripts/release/release_manifest.py verify "${bundle}"
scripts/release/sign-release-manifest.sh "${bundle}" "${signing_home}" "${fingerprint}"
scripts/release/verify-release-bundle-signature.sh \
  "${bundle}" "${public_key}" "${fingerprint}"
(
  cd "${bundle}"
  sha256sum -c SHA256SUMS
)

expected_roles='[
  "arch-install-script",
  "arch-pkgbuild",
  "arch-srcinfo",
  "package-pkgrel1",
  "package-pkgrel2-test",
  "package-signature-pkgrel1",
  "package-signature-pkgrel2-test",
  "repository-database",
  "repository-database-signature",
  "repository-files",
  "repository-files-signature",
  "signing-public-key-test",
  "source-archive"
]'
jq -e \
  --arg version "${version}" \
  --argjson roles "${expected_roles}" '
    .schema_version == 1 and
    .package == {
      "architecture": "x86_64",
      "name": "fcitx-vinput-rs",
      "version": $version
    } and
    (.artifacts | length) == 13 and
    ([.artifacts[].name] == ([.artifacts[].name] | sort)) and
    ([.artifacts[].role] | sort) == ($roles | sort) and
    ([.artifacts[].sha256] | all(test("^[0-9a-f]{64}$"))) and
    ([.artifacts[].size] | all(. > 0))
  ' "${bundle}/manifest.json" >/dev/null

test "$(wc -l <"${bundle}/SHA256SUMS")" -eq 13
test "$(find "${bundle}" -maxdepth 1 -type f | wc -l)" -eq 16
test "$(stat -c '%a' "${bundle}/manifest.json.sig")" = 644
! grep -q 'manifest.json.sig' "${bundle}/SHA256SUMS"
jq -e '[.artifacts[].name] | index("manifest.json.sig") == null' \
  "${bundle}/manifest.json" >/dev/null
! find "${bundle}" -maxdepth 1 -type f -printf '%f\n' |
  grep -Eq '(\.old($|\.)|private|secret|trustdb|revocation)'

source_listing="${stage_root}/source-archive.list"
tar -tzf "${bundle}/$(basename "${source_archive}")" >"${source_listing}"
grep -Eq "^fcitx-vinput-rs-${version}/(\./)?scripts/release/release_manifest.py$" \
  "${source_listing}"
grep -Eq "^fcitx-vinput-rs-${version}/(\./)?scripts/release/sign-release-manifest.sh$" \
  "${source_listing}"
grep -Eq "^fcitx-vinput-rs-${version}/(\./)?scripts/release/verify-release-bundle-signature.sh$" \
  "${source_listing}"
grep -Eq "^fcitx-vinput-rs-${version}/(\./)?scripts/release/prepare-arch-release-candidate.sh$" \
  "${source_listing}"
grep -Eq "^fcitx-vinput-rs-${version}/(\./)?scripts/release/verify-arch-release-candidate.sh$" \
  "${source_listing}"

verify_home="${stage_root}/verify-home"
mkdir "${verify_home}"
chmod 700 "${verify_home}"
gpg --homedir "${verify_home}" --batch --import \
  "${bundle}/$(basename "${public_key}")" >/dev/null 2>&1
gpg --homedir "${verify_home}" --batch --verify \
  "${bundle}/$(basename "${signed_initial_package}.sig")" \
  "${bundle}/$(basename "${signed_initial_package}")" >/dev/null 2>&1
gpg --homedir "${verify_home}" --batch --verify \
  "${bundle}/$(basename "${signed_upgrade_package}.sig")" \
  "${bundle}/$(basename "${signed_upgrade_package}")" >/dev/null 2>&1
gpg --homedir "${verify_home}" --batch --verify \
  "${bundle}/$(basename "${repository_database}.sig")" \
  "${bundle}/$(basename "${repository_database}")" >/dev/null 2>&1
gpg --homedir "${verify_home}" --batch --verify \
  "${bundle}/$(basename "${repository_files}.sig")" \
  "${bundle}/$(basename "${repository_files}")" >/dev/null 2>&1

extra_bundle="${stage_root}/extra-bundle"
cp -a "${bundle}" "${extra_bundle}"
printf 'must not ship\n' >"${extra_bundle}/unexpected.key"
set +e
scripts/release/verify-release-bundle-signature.sh \
  "${extra_bundle}" "${public_key}" "${fingerprint}" \
  >"${stage_root}/extra.out" 2>&1
extra_status=$?
set -e
test "${extra_status}" -ne 0
grep -q 'inventory mismatch' "${stage_root}/extra.out"

tampered_bundle="${stage_root}/tampered-bundle"
cp -a "${bundle}" "${tampered_bundle}"
python3 - "${tampered_bundle}/$(basename "${signed_initial_package}")" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
data = bytearray(path.read_bytes())
data[len(data) // 2] ^= 0x01
path.write_bytes(data)
PY
set +e
scripts/release/verify-release-bundle-signature.sh \
  "${tampered_bundle}" "${public_key}" "${fingerprint}" \
  >"${stage_root}/tampered.out" 2>&1
tampered_status=$?
set -e
test "${tampered_status}" -ne 0
grep -q 'artifact digest mismatch' "${stage_root}/tampered.out"

signature_tampered_bundle="${stage_root}/signature-tampered-bundle"
cp -a "${bundle}" "${signature_tampered_bundle}"
python3 - "${signature_tampered_bundle}/manifest.json.sig" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
data = bytearray(path.read_bytes())
data[len(data) // 2] ^= 0x01
path.write_bytes(data)
PY
set +e
scripts/release/verify-release-bundle-signature.sh \
  "${signature_tampered_bundle}" "${public_key}" "${fingerprint}" \
  >"${stage_root}/signature-tampered.out" 2>&1
signature_tampered_status=$?
set -e
test "${signature_tampered_status}" -ne 0
grep -q 'detached manifest signature verification failed' \
  "${stage_root}/signature-tampered.out"

candidate="${stage_root}/fcitx-vinput-rs-${version}-x86_64-release-candidate"
scripts/release/prepare-arch-release-candidate.sh \
  "${bundle}" "${candidate}" "${signing_home}" "${public_key}" "${fingerprint}"
scripts/release/verify-arch-release-candidate.sh \
  "${candidate}" "${public_key}" "${fingerprint}"
test "$(find "${candidate}" -maxdepth 1 -type f | wc -l)" -eq 14
jq -e '
  (.artifacts | length) == 11 and
  ([.artifacts[].role] | all(test("test"; "i") | not)) and
  ([.artifacts[].name] | all(test("0.1.0-2|test|synthetic"; "i") | not))
' "${candidate}/manifest.json" >/dev/null
! find "${candidate}" -maxdepth 1 -type f -printf '%f\n' |
  grep -Eq '(\.old($|\.)|private|secret|trustdb|revocation|0\.1\.0-2)'

echo "Arch release artifact bundle smoke passed: ${bundle}"
