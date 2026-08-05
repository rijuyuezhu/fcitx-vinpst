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

for command in jq python3 sha256sum; do
  command -v "${command}" >/dev/null
done

stage_root="${repo_root}/target/tmp/release-manifest-check"
artifacts_root="${stage_root}/artifacts"
bundle="${stage_root}/bundle"
rm -rf "${stage_root}"
mkdir -p "${artifacts_root}"

printf 'source archive\n' >"${artifacts_root}/fcitx-vinpst-0.1.0.tar.gz"
printf 'package one\n' >"${artifacts_root}/fcitx-vinpst-0.1.0-1-x86_64.pkg.tar.zst"
printf 'signature one\n' >"${artifacts_root}/fcitx-vinpst-0.1.0-1-x86_64.pkg.tar.zst.sig"
printf 'repository database\n' >"${artifacts_root}/vinpst-signed.db.tar.gz"
printf 'repository signature\n' >"${artifacts_root}/vinpst-signed.db.tar.gz.sig"
printf 'public key\n' >"${artifacts_root}/public-key.asc"

assemble_bundle() {
  local output="$1"
  shift
  scripts/release/release_manifest.py assemble \
    --package-name fcitx-vinpst \
    --version 0.1.0 \
    --architecture x86_64 \
    --output-dir "${output}" \
    --artifact "source-archive=${artifacts_root}/fcitx-vinpst-0.1.0.tar.gz" \
    --artifact "package-pkgrel1=${artifacts_root}/fcitx-vinpst-0.1.0-1-x86_64.pkg.tar.zst" \
    --artifact "package-signature-pkgrel1=${artifacts_root}/fcitx-vinpst-0.1.0-1-x86_64.pkg.tar.zst.sig" \
    --artifact "repository-database=${artifacts_root}/vinpst-signed.db.tar.gz" \
    --artifact "repository-database-signature=${artifacts_root}/vinpst-signed.db.tar.gz.sig" \
    --artifact "signing-public-key=${artifacts_root}/public-key.asc" \
    "$@"
}

python3 -c 'compile(open("scripts/release/release_manifest.py", encoding="utf-8").read(), "scripts/release/release_manifest.py", "exec")'
assemble_bundle "${bundle}"
scripts/release/release_manifest.py verify "${bundle}"
(
  cd "${bundle}"
  sha256sum -c SHA256SUMS
)

jq -e '
  .schema_version == 1 and
  .package == {
    "architecture": "x86_64",
    "name": "fcitx-vinpst",
    "version": "0.1.0"
  } and
  (.artifacts | length) == 6 and
  ([.artifacts[].name] == ([.artifacts[].name] | sort)) and
  ([.artifacts[].role] | unique | length) == 6 and
  .checksum_file.name == "SHA256SUMS" and
  (.checksum_file.sha256 | test("^[0-9a-f]{64}$"))
' "${bundle}/manifest.json" >/dev/null

test "$(wc -l <"${bundle}/SHA256SUMS")" -eq 6
test "$(find "${bundle}" -maxdepth 1 -type f | wc -l)" -eq 8

# --force may replace only a previously valid generated bundle.
assemble_bundle "${bundle}" --force
scripts/release/release_manifest.py verify "${bundle}"

expect_verify_failure() {
  local name="$1"
  local expected="$2"
  local candidate="${stage_root}/${name}"
  cp -a "${bundle}" "${candidate}"
  shift 2
  "$@" "${candidate}"
  set +e
  scripts/release/release_manifest.py verify "${candidate}" \
    >"${stage_root}/${name}.out" 2>&1
  local status=$?
  set -e
  test "${status}" -ne 0
  grep -q "${expected}" "${stage_root}/${name}.out"
}

add_extra_file() {
  printf 'unexpected release content\n' >"$1/unexpected.bin"
}
expect_verify_failure extra-file 'inventory mismatch' add_extra_file

mutate_artifact() {
  python3 - "$1/fcitx-vinpst-0.1.0-1-x86_64.pkg.tar.zst" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
data = bytearray(path.read_bytes())
data[len(data) // 2] ^= 0x01
path.write_bytes(data)
PY
}
expect_verify_failure mutated-artifact 'artifact digest mismatch' mutate_artifact

replace_with_symlink() {
  rm "$1/public-key.asc"
  ln -s fcitx-vinpst-0.1.0.tar.gz "$1/public-key.asc"
}
expect_verify_failure symlink-artifact 'missing or not a regular file' replace_with_symlink

add_nested_directory() {
  mkdir "$1/nested"
}
expect_verify_failure nested-directory 'must be flat' add_nested_directory

corrupt_manifest_schema() {
  python3 - "$1/manifest.json" <<'PY'
from pathlib import Path
import json
import sys

path = Path(sys.argv[1])
data = json.loads(path.read_text())
data["unexpected"] = True
path.write_text(json.dumps(data))
PY
}
expect_verify_failure manifest-fields 'manifest fields mismatch' corrupt_manifest_schema

arbitrary="${stage_root}/arbitrary"
mkdir "${arbitrary}"
printf 'do not delete\n' >"${arbitrary}/sentinel"
set +e
assemble_bundle "${arbitrary}" --force >"${stage_root}/force-arbitrary.out" 2>&1
force_status=$?
set -e
test "${force_status}" -ne 0
test -f "${arbitrary}/sentinel"
grep -q 'manifest.json' "${stage_root}/force-arbitrary.out"

inside_bundle="${bundle}/inside.pkg.tar.zst"
printf 'inside bundle\n' >"${inside_bundle}"
set +e
scripts/release/release_manifest.py assemble \
  --package-name fcitx-vinpst \
  --version 0.1.0 \
  --architecture x86_64 \
  --output-dir "${bundle}" \
  --artifact "inside=${inside_bundle}" \
  --force >"${stage_root}/inside-output.out" 2>&1
inside_status=$?
set -e
test "${inside_status}" -ne 0
test -f "${inside_bundle}"
grep -q 'must not be inside the output directory' "${stage_root}/inside-output.out"
rm "${inside_bundle}"
scripts/release/release_manifest.py verify "${bundle}"

set +e
scripts/release/release_manifest.py assemble \
  --package-name fcitx-vinpst \
  --version 0.1.0 \
  --architecture x86_64 \
  --output-dir "${stage_root}/duplicate-role" \
  --artifact "duplicate=${artifacts_root}/fcitx-vinpst-0.1.0.tar.gz" \
  --artifact "duplicate=${artifacts_root}/public-key.asc" \
  >"${stage_root}/duplicate-role.out" 2>&1
duplicate_status=$?
set -e
test "${duplicate_status}" -ne 0
! test -e "${stage_root}/duplicate-role"
grep -q 'duplicate artifact role' "${stage_root}/duplicate-role.out"

echo "Release manifest check passed"
