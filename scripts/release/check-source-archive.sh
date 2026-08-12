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

check_root="${repo_root}/target/tmp/source-archive-check"
archive_one="${check_root}/fcitx-vinpst-0.1.0-one.tar.gz"
archive_two="${check_root}/fcitx-vinpst-0.1.0-two.tar.gz"
rm -rf "${check_root}"
mkdir -p "${check_root}"

scripts/release/create-source-archive.sh "${archive_one}" 0.1.0 >/dev/null
scripts/release/create-source-archive.sh "${archive_two}" 0.1.0 >/dev/null
cmp "${archive_one}" "${archive_two}"

tar -tzf "${archive_one}" >"${check_root}/listing"
grep -Eq '^fcitx-vinpst-0.1.0/(\./)?Cargo.toml$' "${check_root}/listing"
grep -Eq '^fcitx-vinpst-0.1.0/(\./)?Cargo.lock$' "${check_root}/listing"
grep -Eq '^fcitx-vinpst-0.1.0/(\./)?scripts/release/create-source-archive.sh$' \
  "${check_root}/listing"
grep -Eq '^fcitx-vinpst-0.1.0/(\./)?scripts/release/check-release-metadata.sh$' \
  "${check_root}/listing"
grep -Eq '^fcitx-vinpst-0.1.0/(\./)?scripts/release/publish-github-release.sh$' \
  "${check_root}/listing"
if grep -Eq '(^|/)(\.git|target|dist|__pycache__|\.ruff_cache|\.cache)(/|$)' \
  "${check_root}/listing"; then
  echo "source archive includes excluded build or VCS state" >&2
  exit 1
fi
if grep -Eq '\.py[co]$|packaging/arch/PKGBUILD$' "${check_root}/listing"; then
  echo "source archive includes generated packaging or Python cache files" >&2
  exit 1
fi

extracted_root="${check_root}/extracted"
extracted_source="$(scripts/release/extract-source-archive.py \
  --archive "${archive_one}" \
  --version 0.1.0 \
  --output-root "${extracted_root}")"
[[ "${extracted_source}" == "${extracted_root}/fcitx-vinpst-0.1.0" ]]
test -f "${extracted_source}/Cargo.toml"
test -f "${extracted_source}/Cargo.lock"
test -x "${extracted_source}/scripts/release/create-source-archive.sh"
test -x "${extracted_source}/scripts/release/check-release-metadata.sh"
test -x "${extracted_source}/scripts/release/publish-github-release.sh"

source_sha256="$(sha256sum "${archive_one}" | awk '{print $1}')"
extracted_manifest="${extracted_source}/target/tmp/source-archive-flatpak-manifest.json"
(
  cd "${extracted_source}"
  cargo metadata --no-deps --format-version 1 \
    | jq -e '.packages[] | select(.name == "vinpst-cli" and .version == "0.1.0")' \
      >/dev/null
  scripts/release/check-deb-package.sh >/dev/null
  mkdir -p "$(dirname "${extracted_manifest}")"
  scripts/release/render-flatpak-manifest.py \
    --source-archive "${archive_one}" \
    --source-sha256 "${source_sha256}" \
    --revision 1 \
    --output "${extracted_manifest}"
)
jq -e --arg digest "${source_sha256}" \
  '[.. | objects | select(.type? == "archive" and .sha256? == $digest)] | length == 1' \
  "${extracted_manifest}" >/dev/null

if scripts/release/extract-source-archive.py \
  --archive "${archive_one}" \
  --version 0.1.0 \
  --output-root "${extracted_root}" \
  >"${check_root}/existing-output.out" 2>&1; then
  echo "source archive extractor replaced an existing destination" >&2
  exit 1
fi
grep -q 'destination already exists' "${check_root}/existing-output.out"

outside_output="${repo_root}/../fcitx-vinpst-source-extract-outside"
rm -rf "${outside_output}"
if scripts/release/extract-source-archive.py \
  --archive "${archive_one}" \
  --version 0.1.0 \
  --output-root "${outside_output}" \
  >"${check_root}/outside-output.out" 2>&1; then
  echo "source archive extractor accepted an output outside target or dist" >&2
  exit 1
fi
test ! -e "${outside_output}"
grep -q 'output must be under target/ or dist/' "${check_root}/outside-output.out"

ln -s "${archive_one}" "${check_root}/archive-link.tar.gz"
if scripts/release/extract-source-archive.py \
  --archive "${check_root}/archive-link.tar.gz" \
  --version 0.1.0 \
  --output-root "${check_root}/linked-output" \
  >"${check_root}/archive-link.out" 2>&1; then
  echo "source archive extractor accepted a symbolic-link archive" >&2
  exit 1
fi
grep -q 'must be a regular file' "${check_root}/archive-link.out"

input_symlink="${repo_root}/source-archive-symlink-fixture"
rm -f "${input_symlink}"
ln -s README.md "${input_symlink}"
if scripts/release/create-source-archive.sh \
  "${check_root}/symlink-input.tar.gz" 0.1.0 \
  >"${check_root}/symlink-input.out" 2>&1; then
  rm -f "${input_symlink}"
  echo "source archive creator accepted an unignored symbolic-link input" >&2
  exit 1
fi
rm -f "${input_symlink}"
grep -q 'input must not be a symbolic link' "${check_root}/symlink-input.out"

python3 - "${check_root}" <<'PY'
import io
import sys
import tarfile
from pathlib import Path

root = Path(sys.argv[1])


def write_archive(name: str, members: list[tuple[str, bytes | None, str]]) -> None:
    with tarfile.open(root / name, "w:gz") as archive:
        for path, body, kind in members:
            entry = tarfile.TarInfo(path)
            if kind == "symlink":
                entry.type = tarfile.SYMTYPE
                entry.linkname = "Cargo.toml"
                archive.addfile(entry)
            else:
                assert body is not None
                entry.size = len(body)
                archive.addfile(entry, io.BytesIO(body))


required = [
    ("fcitx-vinpst-0.1.0/Cargo.toml", b"[workspace]\n", "file"),
    ("fcitx-vinpst-0.1.0/Cargo.lock", b"", "file"),
]
write_archive(
    "traversal.tar.gz",
    required + [("fcitx-vinpst-0.1.0/../../escape", b"bad", "file")],
)
write_archive(
    "symlink-member.tar.gz",
    required + [("fcitx-vinpst-0.1.0/link", None, "symlink")],
)
write_archive(
    "wrong-root.tar.gz",
    [("other/Cargo.toml", b"[workspace]\n", "file")],
)
PY

for fixture in traversal symlink-member wrong-root; do
  if scripts/release/extract-source-archive.py \
    --archive "${check_root}/${fixture}.tar.gz" \
    --version 0.1.0 \
    --output-root "${check_root}/${fixture}-output" \
    >"${check_root}/${fixture}.out" 2>&1; then
    echo "source archive extractor accepted unsafe fixture: ${fixture}" >&2
    exit 1
  fi
done
grep -q 'unsafe source archive member path' "${check_root}/traversal.out"
grep -q 'unsupported source archive member type' "${check_root}/symlink-member.out"
grep -q 'outside fcitx-vinpst-0.1.0/' "${check_root}/wrong-root.out"

if scripts/release/create-source-archive.sh \
  "${repo_root}/README-source.tar.gz" 0.1.0 >"${check_root}/unsafe.out" 2>&1; then
  echo "source archive creator accepted an unsafe output location" >&2
  exit 1
fi
grep -q 'output must be under target/ or dist/' "${check_root}/unsafe.out"

if scripts/release/create-source-archive.sh \
  "${check_root}/bad-version.tar.gz" 'invalid version' \
  >"${check_root}/bad-version.out" 2>&1; then
  echo "source archive creator accepted an invalid version" >&2
  exit 1
fi
grep -q 'invalid source archive version' "${check_root}/bad-version.out"

echo "Source archive check passed"
