#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 [--print-version] [--tag vVERSION]" >&2
  exit 2
}

print_version=false
tag=""
while (($#)); do
  case "$1" in
    --print-version)
      [[ "${print_version}" == false ]] || usage
      print_version=true
      shift
      ;;
    --tag)
      (($# >= 2)) || usage
      [[ -z "${tag}" ]] || usage
      tag="$2"
      shift 2
      ;;
    *)
      usage
      ;;
  esac
done

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
cd "${repo_root}"

for command in cargo jq python3; do
  command -v "${command}" >/dev/null 2>&1 || {
    echo "required release metadata command is missing: ${command}" >&2
    exit 1
  }
done

metadata="$(cargo metadata --locked --no-deps --format-version 1)"
mapfile -t package_versions < <(
  jq -r '
    . as $metadata |
    .packages[] |
    select(.id as $id | $metadata.workspace_members | index($id)) |
    [.name, .version] |
    @tsv
  ' <<<"${metadata}" | LC_ALL=C sort
)
((${#package_versions[@]} > 0)) || {
  echo "workspace contains no release packages" >&2
  exit 1
}

mapfile -t versions < <(
  printf '%s\n' "${package_versions[@]}" |
    cut -f2 |
    LC_ALL=C sort -u
)
if ((${#versions[@]} != 1)); then
  printf 'workspace package versions are inconsistent:\n' >&2
  printf '  %s\n' "${package_versions[@]}" >&2
  exit 1
fi
version="${versions[0]}"
[[ "${version}" =~ ^[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)?$ ]] || {
  echo "workspace release version is invalid: ${version}" >&2
  exit 1
}

if [[ -n "${tag}" && "${tag}" != "v${version}" ]]; then
  echo "release tag ${tag} does not match workspace version ${version}" >&2
  exit 1
fi

# Render each native package metadata format with the resolved workspace version.
# This proves that the checked templates accept the same version before any
# source archive or distribution package is built.
mkdir -p "${repo_root}/target/tmp"
render_root="$(mktemp -d "${repo_root}/target/tmp/release-metadata-check.XXXXXX")"
trap 'rm -rf "${render_root}"' EXIT
zero_sha256="$(printf '0%.0s' {1..64})"
source_name="fcitx-vinpst-${version}.tar.gz"
source_dir="fcitx-vinpst-${version}"

python3 scripts/release/render-arch-pkgbuild.py \
  --version "${version}" \
  --pkgrel 1 \
  --source-url "https://example.invalid/${source_name}" \
  --source-sha256 "${zero_sha256}" \
  --source-dir "${source_dir}" \
  --output "${render_root}/PKGBUILD"
[[ "$(sed -n 's/^pkgver=//p' "${render_root}/PKGBUILD")" == "${version}" ]]
[[ "$(sed -n 's/^pkgrel=//p' "${render_root}/PKGBUILD")" == 1 ]]

python3 scripts/release/render-deb-control.py \
  --version "${version}" \
  --release 1 \
  --architecture amd64 \
  --output "${render_root}/debian-control"
[[ "$(sed -n 's/^Version: //p' "${render_root}/debian-control")" == "${version}-1" ]]

python3 scripts/release/render-rpm-spec.py \
  --version "${version}" \
  --release 1 \
  --source-name "${source_name}" \
  --source-sha256 "${zero_sha256}" \
  --source-dir "${source_dir}" \
  --output "${render_root}/fcitx-vinpst.spec"
[[ "$(sed -n 's/^Version:[[:space:]]*//p' "${render_root}/fcitx-vinpst.spec")" == "${version}" ]]
[[ "$(sed -n 's/^Release:[[:space:]]*\([^%]*\).*/\1/p' "${render_root}/fcitx-vinpst.spec")" == 1 ]]

if [[ "${print_version}" == true ]]; then
  printf '%s\n' "${version}"
else
  printf 'Release metadata check passed: %s across %d workspace packages and native package templates\n' \
    "${version}" "${#package_versions[@]}"
fi
