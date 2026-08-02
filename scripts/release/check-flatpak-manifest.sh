#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="${script_dir}"
while [[ ! -f "${repo_root}/Cargo.toml" || ! -d "${repo_root}/packaging" ]]; do
  parent="$(dirname "${repo_root}")"
  if [[ "${parent}" == "${repo_root}" ]]; then
    echo "cannot locate repository root from ${script_dir}" >&2
    exit 1
  fi
  repo_root="${parent}"
done
cd "${repo_root}"

for command in cmp python3; do
  command -v "${command}" >/dev/null || {
    echo "missing Flatpak metadata check tool: ${command}" >&2
    exit 1
  }
done

work_dir="${repo_root}/target/tmp/flatpak-manifest-check"
rm -rf "${work_dir}"
mkdir -p "${work_dir}"

scripts/release/generate-flatpak-cargo-sources.py \
  Cargo.lock \
  --output "${work_dir}/cargo-sources.json"
cmp packaging/flatpak/cargo-sources.json "${work_dir}/cargo-sources.json"

scripts/release/render-flatpak-manifest.py \
  --source-dir "${repo_root}" \
  --revision 1 \
  --output "${work_dir}/manifest.json"

python3 - "${work_dir}/manifest.json" <<'PY'
import json
import re
import sys
from pathlib import Path

manifest = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
assert manifest["app-id"] == "org.fcitx.Fcitx5.Addon.Vinput"
assert manifest["runtime"] == "org.fcitx.Fcitx5"
assert manifest["runtime-version"] == "stable"
assert manifest["sdk"] == "org.kde.Sdk//6.10"
assert manifest["sdk-extensions"] == [
    "org.freedesktop.Sdk.Extension.rust-stable",
    "org.freedesktop.Sdk.Extension.llvm20",
]
assert manifest["build-extension"] is True
assert manifest["build-options"]["prefix"] == "/app/addons/Vinput"
assert manifest["build-options"]["env"]["LIBCLANG_PATH"] == "/usr/lib/sdk/llvm20/lib"
assert manifest["build-options"]["env"]["BINDGEN_EXTRA_CLANG_ARGS"] == (
    "-isystem /usr/lib/sdk/llvm20/lib/clang/20/include"
)
assert manifest["build-options"]["env"]["RUSTFLAGS"] == (
    "-C link-arg=-Wl,-rpath,$ORIGIN/../lib"
)
assert len(manifest["modules"]) == 2
runtime, product = manifest["modules"]
assert runtime["name"] == "sherpa-onnx-runtime"
assert product["name"] == "fcitx-vinput-rs"
assert product["sources"][0]["type"] == "dir"
assert product["build-options"]["env"]["CARGO_NET_OFFLINE"] == "true"
assert product["build-options"]["env"]["SHERPA_ONNX_LIB_DIR"] == "/app/addons/Vinput/lib"
commands = "\n".join(product["build-commands"])
for needle in (
    "cargo build --frozen --release",
    "pipewire-backend,sherpa-onnx-backend",
    "vinput-gui",
    "VINPUT_FCITX_MODULE_INSTALL_DIR=lib/fcitx5",
    "VINPUT_SYSTEMD_USER_UNIT_DIR=share/systemd/user",
    "package-revision",
):
    assert needle in commands, needle
assert "patchelf" not in commands
cargo_sources = product["sources"][1:]
archives = [entry for entry in cargo_sources if entry.get("type") == "archive"]
checksums = [entry for entry in cargo_sources if entry.get("dest-filename") == ".cargo-checksum.json"]
assert len(archives) == len(checksums) == 502
assert cargo_sources[-1]["dest-filename"] == "config.toml"
destinations = [entry["dest"] for entry in archives]
assert len(destinations) == len(set(destinations))
for entry in archives:
    assert re.fullmatch(r"[0-9a-f]{64}", entry["sha256"])
    assert entry["url"].startswith("https://static.crates.io/crates/")
serialized = json.dumps(manifest, sort_keys=True)
assert "@" not in serialized
PY

source_archive="${work_dir}/source.tar.gz"
scripts/release/create-source-archive.sh "${source_archive}" 0.1.0 >/dev/null
source_sha256="$(sha256sum "${source_archive}" | awk '{print $1}')"
scripts/release/render-flatpak-manifest.py \
  --source-archive "${source_archive}" \
  --source-sha256 "${source_sha256}" \
  --revision 2 \
  --output "${work_dir}/archive-manifest.json"
python3 - "${work_dir}/archive-manifest.json" "${source_sha256}" <<'PY'
import json
import sys
from pathlib import Path
manifest = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
source = manifest["modules"][1]["sources"][0]
assert source["type"] == "archive"
assert source["sha256"] == sys.argv[2]
assert any("'2'" in command for command in manifest["modules"][1]["build-commands"])
PY

expect_failure() {
  local label="$1"
  shift
  if "$@" >"${work_dir}/${label}.stdout" 2>"${work_dir}/${label}.stderr"; then
    echo "expected Flatpak metadata failure: ${label}" >&2
    exit 1
  fi
}

expect_failure bad-source-digest \
  scripts/release/render-flatpak-manifest.py \
    --source-archive "${source_archive}" \
    --source-sha256 "$(printf '0%.0s' {1..64})" \
    --output "${work_dir}/bad-digest.json"
expect_failure unsafe-revision \
  scripts/release/render-flatpak-manifest.py \
    --source-dir "${repo_root}" \
    --revision '../escape' \
    --output "${work_dir}/unsafe-revision.json"
expect_failure unknown-runtime \
  scripts/release/render-flatpak-manifest.py \
    --source-dir "${repo_root}" \
    --runtime-bundle missing \
    --output "${work_dir}/unknown-runtime.json"

cp Cargo.lock "${work_dir}/Cargo.lock"
python3 - "${work_dir}/Cargo.lock" <<'PY'
from pathlib import Path
path = Path(__import__('sys').argv[1])
text = path.read_text(encoding='utf-8')
text = text.replace(
    'source = "registry+https://github.com/rust-lang/crates.io-index"',
    'source = "git+https://example.invalid/repository#deadbeef"',
    1,
)
path.write_text(text, encoding='utf-8')
PY
expect_failure git-cargo-source \
  scripts/release/generate-flatpak-cargo-sources.py \
    "${work_dir}/Cargo.lock" \
    --output "${work_dir}/git-sources.json"

printf 'Flatpak manifest metadata check passed\n'
