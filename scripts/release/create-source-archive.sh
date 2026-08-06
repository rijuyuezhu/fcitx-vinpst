#!/usr/bin/env bash
set -euo pipefail

if (($# < 1 || $# > 2)); then
  echo "usage: create-source-archive.sh OUTPUT [VERSION]" >&2
  exit 2
fi

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

output="$1"
version="${2:-}"
if [[ -z "${version}" ]]; then
  version="$(cargo metadata --no-deps --format-version 1 \
    | jq -r '.packages[] | select(.name == "vinpst-cli") | .version')"
fi
if [[ ! "${version}" =~ ^[0-9][0-9A-Za-z.+~-]*$ ]]; then
  echo "invalid source archive version: ${version@Q}" >&2
  exit 2
fi

output_parent="$(dirname "${output}")"
mkdir -p "${output_parent}"
output_parent="$(cd "${output_parent}" && pwd)"
output="${output_parent}/$(basename "${output}")"
case "${output}" in
"${repo_root}"/target/* | "${repo_root}"/dist/*) ;;
*)
  echo "source archive output must be under target/ or dist/: ${output}" >&2
  exit 2
  ;;
esac

source_dir="fcitx-vinpst-${version}"
temporary="${output}.tmp.$$"
listing="${temporary}.list"
rm -f "${temporary}" "${listing}"
trap 'rm -f "${temporary}" "${listing}"' EXIT

export TZ=UTC
if [[ -z "$(git status --porcelain=v1 --untracked-files=normal \
  --ignore-submodules=dirty)" ]]; then
  git archive \
    --format=tar \
    --prefix="${source_dir}/" \
    HEAD \
    | gzip -n -9 >"${temporary}"
else
  git ls-files --cached --others --exclude-standard -z \
    | while IFS= read -r -d '' path; do
        if [[ -L "${path}" ]]; then
          echo "source archive input must not be a symbolic link: ${path}" >&2
          exit 1
        fi
        if [[ -f "${path}" ]]; then
          printf '%s\0' "${path}"
        fi
      done \
    | LC_ALL=C sort -z \
    | tar \
      --null \
      --no-recursion \
      --sort=name \
      --files-from=- \
      --mtime='UTC 1970-01-01' \
      --owner=0 \
      --group=0 \
      --numeric-owner \
      --transform "s,^,${source_dir}/," \
      -cf - \
    | gzip -n -9 >"${temporary}"
fi

test -s "${temporary}"
tar -tzf "${temporary}" >"${listing}"
grep -Eq "^${source_dir}/(\\./)?Cargo.toml$" "${listing}"
mv "${temporary}" "${output}"
rm -f "${listing}"
trap - EXIT
printf '%s\n' "${output}"
