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

version="$(
  cargo metadata --no-deps --format-version 1 |
    jq -r '.packages[] | select(.name == "vinput-cli") | .version'
)"
test -n "${version}"

check_root="${repo_root}/target/tmp/arch-pkgbuild-check"
rm -rf "${check_root}"
mkdir -p "${check_root}"

scripts/release/render-arch-pkgbuild.py \
  --version "${version}" \
  --source-url file:///tmp/fcitx-vinput-rs-source.tar.gz \
  --source-sha256 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef \
  --source-dir "fcitx-vinput-rs-${version}" \
  --output "${check_root}/PKGBUILD"

(
  cd "${check_root}"
  "${repo_root}/scripts/release/render-arch-pkgbuild.py" \
    --version "${version}" \
    --source-url file:///tmp/fcitx-vinput-rs-source.tar.gz \
    --source-sha256 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef \
    --source-dir "fcitx-vinput-rs-${version}" \
    --output nested/PKGBUILD
)
cmp packaging/arch/fcitx-vinput-rs.install \
  "${check_root}/nested/fcitx-vinput-rs.install"

bash -n "${check_root}/PKGBUILD"
bash -n "${check_root}/fcitx-vinput-rs.install"
cmp packaging/arch/fcitx-vinput-rs.install \
  "${check_root}/fcitx-vinput-rs.install"
(
  cd "${check_root}"
  makepkg --printsrcinfo >.SRCINFO
)

srcinfo="${check_root}/.SRCINFO"
grep -qx $'\t'"pkgver = ${version}" "${srcinfo}"
grep -qx $'\tarch = x86_64' "${srcinfo}"
grep -qx $'\t'"provides = fcitx5-vinput=${version}" "${srcinfo}"
grep -qx $'\tconflicts = fcitx5-vinput' "${srcinfo}"
grep -qx $'\toptions = !debug' "${srcinfo}"
grep -qx $'\toptions = !lto' "${srcinfo}"
grep -qx $'\tinstall = fcitx-vinput-rs.install' "${srcinfo}"
grep -qx $'\tdepends = coreutils' "${srcinfo}"
grep -qx $'\tdepends = fontconfig' "${srcinfo}"
grep -qx $'\tdepends = glib2' "${srcinfo}"
grep -qx $'\tdepends = libpipewire' "${srcinfo}"
grep -qx $'\tdepends = libx11' "${srcinfo}"
grep -qx $'\tdepends = libxkbcommon' "${srcinfo}"
grep -qx $'\tdepends = systemd' "${srcinfo}"
grep -qx $'\tdepends = systemd-libs' "${srcinfo}"
grep -qx $'\tdepends = util-linux' "${srcinfo}"
grep -qx $'\tdepends = wayland' "${srcinfo}"
grep -qx $'\tsha256sums = 650d3da32694fa48e6e018f7087e4840aace56b3187a294a18ba3b9f51e80943' "${srcinfo}"
grep -qx $'\tsha256sums = cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30' "${srcinfo}"
grep -qx $'\tsha256sums = 2f07c72751aed99790b8a4869cf2311df85a860b22ded05fa22803587a48922c' "${srcinfo}"

cat >"${check_root}/runtime-bundles.json" <<'EOF'
{
  "schema_version": 1,
  "default_bundle": "fixture-x86_64",
  "bundles": [
    {
      "id": "fixture-x86_64",
      "package_arch": "x86_64",
      "rust_target": "x86_64-unknown-linux-gnu",
      "sherpa_onnx_version": "1.0.0",
      "sherpa_onnx_archive": "fixture-x86_64.tar.bz2",
      "sherpa_onnx_archive_root": "fixture-x86_64",
      "sherpa_onnx_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "sherpa_onnx_license_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "onnxruntime_version": "2.0.0",
      "onnxruntime_license_sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    },
    {
      "id": "fixture-aarch64",
      "package_arch": "aarch64",
      "rust_target": "aarch64-unknown-linux-gnu",
      "sherpa_onnx_version": "9.9.9",
      "sherpa_onnx_archive": "fixture-aarch64.tar.bz2",
      "sherpa_onnx_archive_root": "fixture-aarch64",
      "sherpa_onnx_sha256": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
      "sherpa_onnx_license_sha256": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
      "onnxruntime_version": "8.8.8",
      "onnxruntime_license_sha256": "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
    }
  ]
}
EOF

scripts/release/render-arch-pkgbuild.py \
  --version "${version}" \
  --source-url file:///tmp/fcitx-vinput-rs-source.tar.gz \
  --source-sha256 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef \
  --source-dir "fcitx-vinput-rs-${version}" \
  --runtime-bundles "${check_root}/runtime-bundles.json" \
  --runtime-bundle fixture-aarch64 \
  --output "${check_root}/selected/PKGBUILD"
(
  cd "${check_root}/selected"
  makepkg --printsrcinfo >.SRCINFO
)
selected_srcinfo="${check_root}/selected/.SRCINFO"
grep -qx $'\tarch = aarch64' "${selected_srcinfo}"
grep -qx $'\tsource = fixture-aarch64.tar.bz2::https://github.com/k2-fsa/sherpa-onnx/releases/download/v9.9.9/fixture-aarch64.tar.bz2' "${selected_srcinfo}"
grep -qx $'\tsource = sherpa-onnx-LICENSE-9.9.9::https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/v9.9.9/LICENSE' "${selected_srcinfo}"
grep -qx $'\tsource = onnxruntime-LICENSE-8.8.8::https://raw.githubusercontent.com/microsoft/onnxruntime/v8.8.8/LICENSE' "${selected_srcinfo}"
grep -qx $'\tsha256sums = dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd' "${selected_srcinfo}"
grep -qx $'\tsha256sums = eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee' "${selected_srcinfo}"
grep -qx $'\tsha256sums = ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff' "${selected_srcinfo}"

mkdir -p \
  "${check_root}/selected/fake-bin" \
  "${check_root}/selected/src/fcitx-vinput-rs-${version}"
cat >"${check_root}/selected/fake-bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >"${VINPUT_FAKE_CARGO_LOG:?}"
EOF
chmod 755 "${check_root}/selected/fake-bin/cargo"
(
  cd "${check_root}/selected/src"
  export srcdir="$PWD"
  export VINPUT_FAKE_CARGO_LOG="${check_root}/selected/cargo.log"
  export PATH="${check_root}/selected/fake-bin:${PATH}"
  # shellcheck disable=SC1091
  source ../PKGBUILD
  prepare
)
grep -qx \
  'fetch --locked --target aarch64-unknown-linux-gnu' \
  "${check_root}/selected/cargo.log"

expect_render_failure() {
  local expected="$1"
  local manifest="$2"
  shift 2
  local stderr_path="${check_root}/render-failure.stderr"
  rm -rf "${check_root}/rejected"
  set +e
  scripts/release/render-arch-pkgbuild.py \
    --version "${version}" \
    --source-url file:///tmp/fcitx-vinput-rs-source.tar.gz \
    --source-sha256 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef \
    --source-dir "fcitx-vinput-rs-${version}" \
    --runtime-bundles "${manifest}" \
    --output "${check_root}/rejected/PKGBUILD" \
    "$@" 2>"${stderr_path}"
  status=$?
  set -e
  test "${status}" -ne 0
  grep -Fq "${expected}" "${stderr_path}"
  test ! -e "${check_root}/rejected/PKGBUILD"
}

expect_render_failure \
  'unknown runtime bundle: missing' \
  "${check_root}/runtime-bundles.json" \
  --runtime-bundle missing

python3 - "${check_root}/runtime-bundles.json" "${check_root}" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
root = Path(sys.argv[2])
document = json.loads(path.read_text())

bad_sha = json.loads(json.dumps(document))
bad_sha["bundles"][0]["sherpa_onnx_sha256"] = "not-a-sha256"
(root / "runtime-bundles-bad-sha.json").write_text(json.dumps(bad_sha))

duplicate = json.loads(json.dumps(document))
duplicate["bundles"][1]["id"] = duplicate["bundles"][0]["id"]
(root / "runtime-bundles-duplicate.json").write_text(json.dumps(duplicate))

unsafe = json.loads(json.dumps(document))
unsafe["bundles"][0]["sherpa_onnx_archive_root"] = "$(touch injected)"
(root / "runtime-bundles-unsafe.json").write_text(json.dumps(unsafe))
PY
expect_render_failure \
  'runtime bundle field must be lowercase SHA-256: sherpa_onnx_sha256' \
  "${check_root}/runtime-bundles-bad-sha.json"
expect_render_failure \
  'duplicate runtime bundle id: fixture-x86_64' \
  "${check_root}/runtime-bundles-duplicate.json"
expect_render_failure \
  'runtime bundle field must be a safe token: sherpa_onnx_archive_root' \
  "${check_root}/runtime-bundles-unsafe.json"
test ! -e "${check_root}/injected"

echo "Arch PKGBUILD metadata check passed"
