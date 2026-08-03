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

for command in bsdtar gpg gpgconf jq repo-add zstd; do
  command -v "${command}" >/dev/null
done

repo_add_signed() {
  local signing_home="$1"
  local fingerprint="$2"
  local database="$3"
  local package="$4"
  local args=(--sign --key "${fingerprint}")
  if repo-add --help 2>&1 | grep -q -- '--include-sigs'; then
    args=(--include-sigs "${args[@]}")
  fi
  GNUPGHOME="${signing_home}" repo-add "${args[@]}" "${database}" "${package}"
}

stage_root="${repo_root}/target/tmp/release-candidate-check"
artifacts="${stage_root}/artifacts"
signing_home="${stage_root}/signing-home"
public_key="${stage_root}/public-key.asc"
gate="${stage_root}/release-gate"
candidate="${stage_root}/release-candidate"
cleanup() {
  gpg_session_stop "${signing_home}"
}
trap cleanup EXIT
cleanup
rm -rf "${stage_root}"
mkdir -p "${artifacts}" "${signing_home}"
chmod 700 "${signing_home}"

gpg --homedir "${signing_home}" --batch --passphrase '' \
  --quick-generate-key \
  'Vinput Candidate Check <candidate-check@example.invalid>' \
  ed25519 sign 1d >/dev/null 2>&1
fingerprint="$(
  gpg --homedir "${signing_home}" --batch --with-colons --list-secret-keys |
    awk -F: '$1 == "fpr" { print $10; exit }'
)"
test -n "${fingerprint}"
gpg --homedir "${signing_home}" --batch --armor --export "${fingerprint}" \
  >"${public_key}"

make_package() {
  local pkgrel="$1"
  local root="${stage_root}/package-${pkgrel}"
  local package="${artifacts}/fcitx-vinput-rs-0.1.0-${pkgrel}-x86_64.pkg.tar.zst"
  mkdir -p "${root}"
  cat >"${root}/.PKGINFO" <<EOF
pkgname = fcitx-vinput-rs
pkgbase = fcitx-vinput-rs
pkgver = 0.1.0-${pkgrel}
pkgdesc = Minimal release candidate fixture
url = https://example.invalid/
builddate = 0
packager = Test
size = 1
arch = x86_64
license = MIT
EOF
  printf 'fixture-%s\n' "${pkgrel}" >"${root}/payload"
  bsdtar -C "${root}" -cf - .PKGINFO payload |
    zstd -q -o "${package}"
  gpg --homedir "${signing_home}" --batch --yes \
    --local-user "${fingerprint}" --detach-sign "${package}"
  printf '%s\n' "${package}"
}

package_one="$(make_package 1)"
package_two="$(make_package 2)"
printf 'source fixture\n' >"${artifacts}/source.txt"
tar -C "${artifacts}" -czf "${artifacts}/fcitx-vinput-rs-0.1.0.tar.gz" source.txt
printf 'pkgname=fcitx-vinput-rs\n' >"${artifacts}/PKGBUILD"
printf '%s\n' 'pkgbase = fcitx-vinput-rs' >"${artifacts}/.SRCINFO"
printf '%s\n' 'post_install() { :; }' >"${artifacts}/fcitx-vinput-rs.install"

repository="${stage_root}/gate-repository"
mkdir "${repository}"
cp "${package_two}" "${package_two}.sig" "${repository}/"
(
  cd "${repository}"
  repo_add_signed "${signing_home}" "${fingerprint}" \
    vinput-signed.db.tar.gz "$(basename "${package_two}")"
)

scripts/release/release_manifest.py assemble \
  --package-name fcitx-vinput-rs \
  --version 0.1.0 \
  --architecture x86_64 \
  --output-dir "${gate}" \
  --artifact "source-archive=${artifacts}/fcitx-vinput-rs-0.1.0.tar.gz" \
  --artifact "arch-pkgbuild=${artifacts}/PKGBUILD" \
  --artifact "arch-srcinfo=${artifacts}/.SRCINFO" \
  --artifact "arch-install-script=${artifacts}/fcitx-vinput-rs.install" \
  --artifact "package-pkgrel1=${package_one}" \
  --artifact "package-signature-pkgrel1=${package_one}.sig" \
  --artifact "package-pkgrel2-test=${package_two}" \
  --artifact "package-signature-pkgrel2-test=${package_two}.sig" \
  --artifact "repository-database=${repository}/vinput-signed.db.tar.gz" \
  --artifact "repository-database-signature=${repository}/vinput-signed.db.tar.gz.sig" \
  --artifact "repository-files=${repository}/vinput-signed.files.tar.gz" \
  --artifact "repository-files-signature=${repository}/vinput-signed.files.tar.gz.sig" \
  --artifact "signing-public-key-test=${public_key}"
scripts/release/sign-release-manifest.sh "${gate}" "${signing_home}" "${fingerprint}"

scripts/release/prepare-arch-release-candidate.sh \
  "${gate}" "${candidate}" "${signing_home}" "${public_key}" "${fingerprint}"
scripts/release/verify-arch-release-candidate.sh \
  "${candidate}" "${public_key}" "${fingerprint}"

test "$(find "${candidate}" -maxdepth 1 -type f | wc -l)" -eq 14
jq -e '
  (.artifacts | length) == 11 and
  ([.artifacts[].role] | all(test("test"; "i") | not)) and
  ([.artifacts[].name] | all(test("0.1.0-2|test|synthetic"; "i") | not))
' "${candidate}/manifest.json" >/dev/null
if find "${candidate}" -maxdepth 1 -type f -printf '%f\n' |
  grep -Eqi '(private|secret|trustdb|revocation|0\.1\.0-2)'; then
  echo "release candidate contains forbidden private or test artifacts" >&2
  exit 1
fi
repository_database="${candidate}/fcitx-vinput-rs.db.tar.gz"
repository_desc="$(bsdtar -xOf "${repository_database}" '*/desc')"
grep -qx '0.1.0-1' < <(
  awk '$0 == "%VERSION%" { getline; print; exit }' <<<"${repository_desc}"
)

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

expect_failure existing-output 'output already exists' \
  scripts/release/prepare-arch-release-candidate.sh \
  "${gate}" "${candidate}" "${signing_home}" "${public_key}" "${fingerprint}"
scripts/release/prepare-arch-release-candidate.sh --force \
  "${gate}" "${candidate}" "${signing_home}" "${public_key}" "${fingerprint}"
scripts/release/verify-arch-release-candidate.sh \
  "${candidate}" "${public_key}" "${fingerprint}"

expect_failure gate-is-not-candidate 'production candidate policy' \
  scripts/release/verify-arch-release-candidate.sh \
  "${gate}" "${public_key}" "${fingerprint}"
expect_failure output-inside-gate 'must not be the signed gate bundle' \
  scripts/release/prepare-arch-release-candidate.sh \
  "${gate}" "${gate}/candidate" "${signing_home}" "${public_key}" "${fingerprint}"

invalid_output="${stage_root}/invalid-output"
mkdir "${invalid_output}"
printf 'preserve\n' >"${invalid_output}/sentinel"
expect_failure invalid-force 'manifest.json.sig is missing' \
  scripts/release/prepare-arch-release-candidate.sh --force \
  "${gate}" "${invalid_output}" "${signing_home}" "${public_key}" "${fingerprint}"
test -f "${invalid_output}/sentinel"

mutated_candidate="${stage_root}/mutated-candidate"
cp -a "${candidate}" "${mutated_candidate}"
printf 'tamper\n' >>"${mutated_candidate}/fcitx-vinput-rs-0.1.0-1-x86_64.pkg.tar.zst"
expect_failure mutated-candidate 'artifact size mismatch' \
  scripts/release/verify-arch-release-candidate.sh \
  "${mutated_candidate}" "${public_key}" "${fingerprint}"

echo "Arch release candidate check passed"
