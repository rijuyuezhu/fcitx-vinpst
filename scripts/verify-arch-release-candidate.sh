#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"
export LC_ALL=C

if [[ "$#" -ne 3 ]]; then
  echo "usage: $0 CANDIDATE_BUNDLE PUBLIC_KEY EXPECTED_FINGERPRINT" >&2
  exit 2
fi
for command in bsdtar cmp gpg jq mktemp realpath; do
  command -v "${command}" >/dev/null
done

bundle="$(realpath -e "$1")"
public_key="$(realpath -e "$2")"
fingerprint="${3^^}"
scripts/verify-release-bundle-signature.sh \
  "${bundle}" "${public_key}" "${fingerprint}"

manifest="${bundle}/manifest.json"
expected_roles='[
  "arch-install-script",
  "arch-pkgbuild",
  "arch-srcinfo",
  "package",
  "package-signature",
  "repository-database",
  "repository-database-signature",
  "repository-files",
  "repository-files-signature",
  "signing-public-key",
  "source-archive"
]'
if ! jq -e --argjson expected "${expected_roles}" '
  ([.artifacts[].role] | sort) == ($expected | sort) and
  ([.artifacts[].role] | all(test("test"; "i") | not)) and
  ([.artifacts[].name] | all(test("test|synthetic"; "i") | not))
' "${manifest}" >/dev/null; then
  echo "release candidate error: artifact roles do not match the production candidate policy" >&2
  exit 1
fi

artifact_path() {
  local role="$1"
  local count name
  count="$(jq --arg role "${role}" '[.artifacts[] | select(.role == $role)] | length' "${manifest}")"
  if [[ "${count}" != 1 ]]; then
    echo "release candidate error: role ${role} must identify exactly one artifact" >&2
    exit 1
  fi
  name="$(jq -r --arg role "${role}" '.artifacts[] | select(.role == $role) | .name' "${manifest}")"
  printf '%s/%s\n' "${bundle}" "${name}"
}

package_name="$(jq -r '.package.name' "${manifest}")"
base_version="$(jq -r '.package.version' "${manifest}")"
architecture="$(jq -r '.package.architecture' "${manifest}")"
package="$(artifact_path package)"
package_signature="$(artifact_path package-signature)"
repository_database="$(artifact_path repository-database)"
repository_database_signature="$(artifact_path repository-database-signature)"
repository_files="$(artifact_path repository-files)"
repository_files_signature="$(artifact_path repository-files-signature)"
bundled_public_key="$(artifact_path signing-public-key)"

cmp "${bundled_public_key}" "${public_key}"
package_info="$(bsdtar -xOf "${package}" .PKGINFO)"
package_info_field() {
  local field="$1"
  awk -F ' = ' -v field="${field}" '$1 == field { print $2; exit }' <<<"${package_info}"
}
full_version="$(package_info_field pkgver)"
test "$(package_info_field pkgname)" = "${package_name}"
test "$(package_info_field arch)" = "${architecture}"
pkgrel="${full_version##*-}"
if [[ "${full_version%-*}" != "${base_version}" || ! "${pkgrel}" =~ ^[1-9][0-9]*$ ]]; then
  echo "release candidate error: package version ${full_version} does not match ${base_version}-<pkgrel>" >&2
  exit 1
fi

verification_home="$(mktemp -d)"
cleanup() {
  rm -rf "${verification_home}"
}
trap cleanup EXIT
chmod 700 "${verification_home}"
gpg --homedir "${verification_home}" --batch --import "${public_key}" >/dev/null 2>&1
imported_fingerprint="$(
  gpg --homedir "${verification_home}" --batch --with-colons --list-keys |
    awk -F: '$1 == "pub" { want = 1; next } want && $1 == "fpr" { print $10; exit }'
)"
test "${imported_fingerprint}" = "${fingerprint}"
for pair in \
  "${package_signature}|${package}" \
  "${repository_database_signature}|${repository_database}" \
  "${repository_files_signature}|${repository_files}"; do
  signature="${pair%%|*}"
  signed_file="${pair#*|}"
  gpg --homedir "${verification_home}" --batch --no-auto-key-retrieve \
    --verify "${signature}" "${signed_file}" >/dev/null 2>&1
done

mapfile -t desc_entries < <(bsdtar -tf "${repository_database}" | grep '/desc$')
if [[ "${#desc_entries[@]}" -ne 1 ]]; then
  echo "release candidate error: repository database must contain exactly one package" >&2
  exit 1
fi
repository_desc="$(bsdtar -xOf "${repository_database}" "${desc_entries[0]}")"
repository_field() {
  local field="$1"
  awk -v marker="%${field}%" '$0 == marker { getline; print; exit }' <<<"${repository_desc}"
}
test "$(repository_field NAME)" = "${package_name}"
test "$(repository_field VERSION)" = "${full_version}"
test "$(repository_field ARCH)" = "${architecture}"
test "$(repository_field FILENAME)" = "$(basename "${package}")"

mapfile -t files_desc_entries < <(bsdtar -tf "${repository_files}" | grep '/desc$')
if [[ "${#files_desc_entries[@]}" -ne 1 ]]; then
  echo "release candidate error: repository files index must contain exactly one package" >&2
  exit 1
fi
files_desc="$(bsdtar -xOf "${repository_files}" "${files_desc_entries[0]}")"
grep -qx "${full_version}" < <(
  awk '$0 == "%VERSION%" { getline; print; exit }' <<<"${files_desc}"
)

echo "Arch release candidate verified: ${package_name} ${full_version} ${architecture}"
