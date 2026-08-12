#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 --tag TAG --version VERSION --bundle-dir DIRECTORY --notes-file FILE --repo OWNER/REPO" >&2
  exit 2
}

tag=""
version=""
bundle_dir=""
notes_file=""
repo=""
while (($#)); do
  case "$1" in
    --tag)
      (($# >= 2)) || usage
      tag="$2"
      shift 2
      ;;
    --version)
      (($# >= 2)) || usage
      version="$2"
      shift 2
      ;;
    --bundle-dir)
      (($# >= 2)) || usage
      bundle_dir="$2"
      shift 2
      ;;
    --notes-file)
      (($# >= 2)) || usage
      notes_file="$2"
      shift 2
      ;;
    --repo)
      (($# >= 2)) || usage
      repo="$2"
      shift 2
      ;;
    *)
      usage
      ;;
  esac
done

[[ -n "${tag}" && -n "${version}" && -n "${bundle_dir}" && -n "${notes_file}" && -n "${repo}" ]] || usage
[[ "${tag}" == "v${version}" ]] || {
  echo "release tag ${tag} does not match version ${version}" >&2
  exit 1
}
[[ "${repo}" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || {
  echo "invalid GitHub repository: ${repo}" >&2
  exit 1
}

for command in gh jq python3 sha256sum; do
  command -v "${command}" >/dev/null 2>&1 || {
    echo "required publication command is missing: ${command}" >&2
    exit 1
  }
done

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
bundle_dir="$(realpath "${bundle_dir}")"
notes_file="$(realpath "${notes_file}")"
[[ -d "${bundle_dir}" && ! -L "${bundle_dir}" ]] || {
  echo "release bundle must be a regular directory: ${bundle_dir}" >&2
  exit 1
}
[[ -f "${notes_file}" && ! -L "${notes_file}" && -s "${notes_file}" ]] || {
  echo "release notes must be a non-empty regular file: ${notes_file}" >&2
  exit 1
}
[[ "$(head -n 1 "${notes_file}")" == "# Vinpst ${version}" ]] || {
  echo "release notes heading does not match version ${version}" >&2
  exit 1
}

# Revalidate the exact bytes immediately before any remote mutation. The
# verifier rejects missing, extra, non-regular, symlinked, size-mismatched, or
# digest-mismatched entries.
"${repo_root}/scripts/release/release_manifest.py" verify "${bundle_dir}"
(
  cd "${bundle_dir}"
  sha256sum -c SHA256SUMS
)
manifest_version="$(jq -r '.package.version' "${bundle_dir}/manifest.json")"
manifest_name="$(jq -r '.package.name' "${bundle_dir}/manifest.json")"
[[ "${manifest_name}" == fcitx-vinpst && "${manifest_version}" == "${version}" ]] || {
  echo "release manifest package metadata does not match fcitx-vinpst ${version}" >&2
  exit 1
}

work_root="$(mktemp -d)"
trap 'rm -rf "${work_root}"' EXIT
release_json="${work_root}/release.json"
release_list_json="${work_root}/release-list.json"
release_error="${work_root}/release.error"
release_list_endpoint="repos/${repo}/releases?per_page=100"
release_exists=false

lookup_release() {
  if ! gh api "${release_list_endpoint}" --paginate --slurp \
    >"${release_list_json}" 2>"${release_error}"; then
    cat "${release_error}" >&2
    echo "failed to query GitHub Releases for ${tag}; refusing to create or modify a release" >&2
    exit 1
  fi
  local match_count
  match_count="$(jq --arg tag "${tag}" '[.[][] | select(.tag_name == $tag)] | length' "${release_list_json}")"
  case "${match_count}" in
    0)
      release_exists=false
      : >"${release_json}"
      ;;
    1)
      release_exists=true
      jq --arg tag "${tag}" '.[][] | select(.tag_name == $tag)' "${release_list_json}" >"${release_json}"
      ;;
    *)
      echo "GitHub returned ${match_count} releases for tag ${tag}; refusing ambiguous mutation" >&2
      exit 1
      ;;
  esac
}

# Prove that the repository itself is visible before interpreting an empty
# release list as "no release". Draft releases are intentionally discovered
# through the authenticated release-list endpoint: GitHub's release-by-tag
# endpoint does not expose drafts.
remote_repo="$(gh api "repos/${repo}" --jq '.full_name')"
[[ "${remote_repo}" == "${repo}" ]] || {
  echo "GitHub repository lookup returned unexpected name ${remote_repo@Q}" >&2
  exit 1
}

lookup_release

if [[ "${release_exists}" == true ]]; then
  remote_tag="$(jq -r '.tag_name // empty' "${release_json}")"
  [[ "${remote_tag}" == "${tag}" ]] || {
    echo "GitHub Release lookup returned unexpected tag ${remote_tag@Q}" >&2
    exit 1
  }
  is_draft="$(jq -r '.draft' "${release_json}")"
  [[ "${is_draft}" == true || "${is_draft}" == false ]] || {
    echo "GitHub Release draft state is invalid" >&2
    exit 1
  }
  if [[ "${is_draft}" != true ]]; then
    echo "release ${tag} is already public; refusing to replace published assets" >&2
    exit 1
  fi
  gh release edit "${tag}" \
    --repo "${repo}" \
    --title "Vinpst ${version}" \
    --notes-file "${notes_file}" \
    --draft
else
  gh release create "${tag}" \
    --repo "${repo}" \
    --verify-tag \
    --title "Vinpst ${version}" \
    --notes-file "${notes_file}" \
    --draft
fi

mapfile -d '' -t assets < <(
  find "${bundle_dir}" -mindepth 1 -maxdepth 1 -type f -print0 | LC_ALL=C sort -z
)
((${#assets[@]} > 0)) || {
  echo "release bundle contains no regular files" >&2
  exit 1
}
entry_count="$(find "${bundle_dir}" -mindepth 1 -maxdepth 1 -printf '.' | wc -c)"
((${#assets[@]} == entry_count)) || {
  echo "release bundle contains a non-regular entry" >&2
  exit 1
}

gh release upload "${tag}" \
  --repo "${repo}" \
  "${assets[@]}" \
  --clobber

expected_assets="${work_root}/expected-assets.tsv"
actual_assets="${work_root}/actual-assets.tsv"
for asset in "${assets[@]}"; do
  printf '%s\t%s\tsha256:%s\n' \
    "${asset##*/}" \
    "$(stat -c '%s' "${asset}")" \
    "$(sha256sum "${asset}" | cut -d ' ' -f1)"
done | LC_ALL=C sort >"${expected_assets}"

lookup_release
[[ "${release_exists}" == true ]] || {
  echo "GitHub Release ${tag} disappeared after upload; refusing further mutation" >&2
  exit 1
}
post_upload_draft="$(jq -r '.draft' "${release_json}")"
[[ "${post_upload_draft}" == true ]] || {
  echo "GitHub Release ${tag} is no longer a draft after upload; refusing further mutation" >&2
  exit 1
}
jq -r '.assets[] | [.name, (.size | tostring), (.digest // "")] | @tsv' "${release_json}" |
  LC_ALL=C sort >"${actual_assets}"
if ! diff -u "${expected_assets}" "${actual_assets}"; then
  echo "remote GitHub Release asset names, sizes, or SHA-256 digests do not match the checked local bundle; leaving the release as a draft" >&2
  exit 1
fi

# A second local verification keeps publication fail-closed if anything changed
# while the upload was in progress.
"${repo_root}/scripts/release/release_manifest.py" verify "${bundle_dir}"
(
  cd "${bundle_dir}"
  sha256sum -c SHA256SUMS
)

gh release edit "${tag}" \
  --repo "${repo}" \
  --draft=false \
  --latest
printf 'Published GitHub Release %s with %d checked assets\n' "${tag}" "${#assets[@]}"
