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

force=0
if [[ "${1:-}" == "--force" ]]; then
  force=1
  shift
fi
if [[ "$#" -ne 5 ]]; then
  echo "usage: $0 [--force] GATE_BUNDLE OUTPUT_DIR SIGNING_HOME PUBLIC_KEY FINGERPRINT" >&2
  exit 2
fi
for command in bsdtar cmp gpg jq mktemp realpath repo-add; do
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

gate="$(realpath -e "$1")"
raw_output="$2"
if [[ -L "${raw_output}" ]]; then
  echo "release candidate error: output must not be a symlink" >&2
  exit 1
fi
output="$(realpath -m "${raw_output}")"
signing_home="$(realpath -e "$3")"
public_key="$(realpath -e "$4")"
fingerprint="${5^^}"
case "${output}" in
  "${gate}"|"${gate}"/*)
    echo "release candidate error: output must not be the signed gate bundle or its descendant" >&2
    exit 1
    ;;
esac

scripts/release/verify-release-bundle-signature.sh \
  "${gate}" "${public_key}" "${fingerprint}"
manifest="${gate}/manifest.json"
expected_gate_roles='[
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
if ! jq -e --argjson expected "${expected_gate_roles}" \
  '([.artifacts[].role] | sort) == ($expected | sort)' "${manifest}" >/dev/null; then
  echo "release candidate error: gate artifact roles do not match the release-gate policy" >&2
  exit 1
fi

artifact_path() {
  local role="$1"
  local count name
  count="$(jq --arg role "${role}" '[.artifacts[] | select(.role == $role)] | length' "${manifest}")"
  if [[ "${count}" != 1 ]]; then
    echo "release candidate error: gate role ${role} must identify exactly one artifact" >&2
    exit 1
  fi
  name="$(jq -r --arg role "${role}" '.artifacts[] | select(.role == $role) | .name' "${manifest}")"
  printf '%s/%s\n' "${gate}" "${name}"
}

source_archive="$(artifact_path source-archive)"
pkgbuild="$(artifact_path arch-pkgbuild)"
srcinfo="$(artifact_path arch-srcinfo)"
install_script="$(artifact_path arch-install-script)"
package="$(artifact_path package-pkgrel1)"
package_signature="$(artifact_path package-signature-pkgrel1)"
package_name="$(jq -r '.package.name' "${manifest}")"
base_version="$(jq -r '.package.version' "${manifest}")"
architecture="$(jq -r '.package.architecture' "${manifest}")"
full_version="$(
  bsdtar -xOf "${package}" .PKGINFO |
    awk -F ' = ' '$1 == "pkgver" { print $2; exit }'
)"
test "${full_version}" = "${base_version}-1"

status_file="$(mktemp)"
cleanup_status() {
  rm -f "${status_file}"
}
trap cleanup_status EXIT
if ! gpg --homedir "${signing_home}" --batch --no-auto-key-retrieve \
  --status-fd=1 --verify "${package_signature}" "${package}" \
  >"${status_file}" 2>/dev/null; then
  echo "release candidate error: selected package signature is invalid" >&2
  exit 1
fi
awk -v expected="${fingerprint}" '
  $1 == "[GNUPG:]" && $2 == "VALIDSIG" && ($3 == expected || $NF == expected) { valid = 1 }
  END { exit(valid ? 0 : 1) }
' "${status_file}" || {
  echo "release candidate error: selected package signer does not match the expected fingerprint" >&2
  exit 1
}
rm -f "${status_file}"
trap - EXIT

if [[ -e "${output}" ]]; then
  if [[ "${force}" != 1 ]]; then
    echo "release candidate error: output already exists; pass --force to replace a valid candidate" >&2
    exit 1
  fi
  scripts/release/verify-arch-release-candidate.sh \
    "${output}" "${public_key}" "${fingerprint}"
fi

output_parent="$(dirname "${output}")"
mkdir -p "${output_parent}"
workspace="$(mktemp -d "${output_parent}/.release-candidate-work.XXXXXX")"
cleanup() {
  rm -rf "${workspace}"
}
trap cleanup EXIT
repository="${workspace}/repository"
mkdir "${repository}"
cp "${package}" "${package_signature}" "${repository}/"
repository_database="${repository}/${package_name}.db.tar.gz"
(
  cd "${repository}"
  repo_add_signed "${signing_home}" "${fingerprint}" \
    "${repository_database}" "$(basename "${package}")"
)
repository_files="${repository}/${package_name}.files.tar.gz"
for artifact in \
  "${repository_database}" \
  "${repository_database}.sig" \
  "${repository_files}" \
  "${repository_files}.sig"; do
  test -f "${artifact}"
done
candidate_public_key="${workspace}/${package_name}-signing-key.asc"
cp "${public_key}" "${candidate_public_key}"

assemble_args=(
  assemble
  --package-name "${package_name}"
  --version "${base_version}"
  --architecture "${architecture}"
  --output-dir "${output}"
  --artifact "source-archive=${source_archive}"
  --artifact "arch-pkgbuild=${pkgbuild}"
  --artifact "arch-srcinfo=${srcinfo}"
  --artifact "arch-install-script=${install_script}"
  --artifact "package=${package}"
  --artifact "package-signature=${package_signature}"
  --artifact "repository-database=${repository_database}"
  --artifact "repository-database-signature=${repository_database}.sig"
  --artifact "repository-files=${repository_files}"
  --artifact "repository-files-signature=${repository_files}.sig"
  --artifact "signing-public-key=${candidate_public_key}"
)
if [[ "${force}" == 1 ]]; then
  assemble_args+=(--force)
fi
scripts/release/release_manifest.py "${assemble_args[@]}"
scripts/release/sign-release-manifest.sh "${output}" "${signing_home}" "${fingerprint}"
scripts/release/verify-arch-release-candidate.sh \
  "${output}" "${public_key}" "${fingerprint}"

echo "Arch release candidate prepared: ${output}"
