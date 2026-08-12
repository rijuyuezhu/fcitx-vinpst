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
assert manifest["app-id"] == "org.fcitx.Fcitx5.Addon.Vinpst"
assert manifest["runtime"] == "org.fcitx.Fcitx5"
assert manifest["runtime-version"] == "stable"
assert manifest["sdk"] == "org.kde.Sdk//6.10"
assert manifest["sdk-extensions"] == [
    "org.freedesktop.Sdk.Extension.rust-stable",
    "org.freedesktop.Sdk.Extension.llvm20",
]
assert manifest["build-extension"] is True
assert manifest["build-options"]["prefix"] == "/app/addons/Vinpst"
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
assert product["name"] == "fcitx-vinpst"
assert product["sources"][0]["type"] == "dir"
assert product["build-options"]["env"]["CARGO_NET_OFFLINE"] == "true"
assert product["build-options"]["env"]["SHERPA_ONNX_LIB_DIR"] == "/app/addons/Vinpst/lib"
commands = "\n".join(product["build-commands"])
for needle in (
    "cargo build --frozen --release",
    "pipewire-backend,sherpa-onnx-backend",
    "vinpst-gui",
    "VINPST_FCITX_MODULE_INSTALL_DIR=lib/fcitx5",
    "VINPST_SYSTEMD_USER_UNIT_DIR=share/systemd/user",
    "package-revision",
):
    assert needle in commands, needle
assert "patchelf" not in commands
cargo_sources = product["sources"][1:]
archives = [entry for entry in cargo_sources if entry.get("type") == "archive"]
checksums = [entry for entry in cargo_sources if entry.get("dest-filename") == ".cargo-checksum.json"]
assert len(archives) == len(checksums) == 504
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
release_version="$(scripts/release/check-release-metadata.sh --print-version)"
scripts/release/create-source-archive.sh "${source_archive}" "${release_version}" >/dev/null
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

runtime_source_dir="${work_dir}/runtime-sources"
mkdir -p "${runtime_source_dir}"
python3 - "${runtime_source_dir}" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

source_dir = Path(sys.argv[1])
manifest = json.loads(Path("packaging/arch/runtime-bundles.json").read_text())
bundle = next(
    entry for entry in manifest["bundles"] if entry["id"] == manifest["default_bundle"]
)
fixtures = {
    bundle["sherpa_onnx_archive"]: b"archive fixture",
    "sherpa-onnx-LICENSE": b"sherpa license fixture",
    "onnxruntime-LICENSE": b"onnxruntime license fixture",
}
for name, content in fixtures.items():
    (source_dir / name).write_bytes(content)
digests = {name: hashlib.sha256(content).hexdigest() for name, content in fixtures.items()}
bundle["sherpa_onnx_sha256"] = digests[bundle["sherpa_onnx_archive"]]
bundle["sherpa_onnx_license_sha256"] = digests["sherpa-onnx-LICENSE"]
bundle["onnxruntime_license_sha256"] = digests["onnxruntime-LICENSE"]
manifest["bundles"] = [bundle]
(source_dir / "runtime-bundles.json").write_text(json.dumps(manifest), encoding="utf-8")
PY
scripts/release/render-flatpak-manifest.py \
  --source-dir "${repo_root}" \
  --runtime-manifest "${runtime_source_dir}/runtime-bundles.json" \
  --runtime-source-dir "${runtime_source_dir}" \
  --output "${work_dir}/local-runtime-manifest.json"
python3 - "${work_dir}/local-runtime-manifest.json" <<'PY'
import json
import sys
from pathlib import Path

manifest = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
sources = manifest["modules"][0]["sources"]
assert all("path" in source and "url" not in source for source in sources)
assert sources[0]["type"] == "archive"
assert sources[1]["dest-filename"] == "sherpa-onnx-LICENSE"
assert sources[2]["dest-filename"] == "onnxruntime-LICENSE"
PY

cargo_source_dir="${work_dir}/cargo-sources"
cargo_cache_dir="${work_dir}/cargo-cache/registry-cache"
mkdir -p "${cargo_source_dir}" "${cargo_cache_dir}"
python3 - "${cargo_source_dir}" "${cargo_cache_dir}" <<'PY'
import hashlib
import json
import sys
from pathlib import Path
from urllib.parse import urlparse

source_dir = Path(sys.argv[1])
cache_dir = Path(sys.argv[2])
sources = json.loads(Path("packaging/flatpak/cargo-sources.json").read_text())
fixtures = []
archives = [entry for entry in sources if entry.get("type") == "archive"][:2]
for index, archive in enumerate(archives):
    content = f"crate fixture {index}\n".encode()
    digest = hashlib.sha256(content).hexdigest()
    filename = Path(urlparse(archive["url"]).path).name
    archive = dict(archive)
    archive["sha256"] = digest
    checksum = {
        "type": "inline",
        "contents": json.dumps(
            {"package": digest, "files": {}},
            sort_keys=True,
            separators=(",", ":"),
        ),
        "dest": archive["dest"],
        "dest-filename": ".cargo-checksum.json",
    }
    (source_dir / filename).write_bytes(content)
    (cache_dir / filename).write_bytes(content)
    fixtures.extend([archive, checksum])
fixtures.append(sources[-1])
(source_dir / "cargo-sources.json").write_text(
    json.dumps(fixtures), encoding="utf-8"
)
PY
scripts/release/render-flatpak-manifest.py \
  --source-dir "${repo_root}" \
  --cargo-sources-manifest "${cargo_source_dir}/cargo-sources.json" \
  --cargo-source-dir "${cargo_source_dir}" \
  --output "${work_dir}/local-cargo-manifest.json"
python3 - "${work_dir}/local-cargo-manifest.json" <<'PY'
import json
import sys
from pathlib import Path

manifest = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
sources = manifest["modules"][1]["sources"][1:]
archives = [source for source in sources if source.get("type") == "archive"]
assert len(archives) == 2
assert all("path" in source and "url" not in source for source in archives)
assert all(source["archive-type"] == "tar-gzip" for source in archives)
assert len([source for source in sources if source.get("dest-filename") == ".cargo-checksum.json"]) == 2
assert sources[-1]["dest-filename"] == "config.toml"
PY

expect_failure() {
  local label="$1"
  shift
  if "$@" >"${work_dir}/${label}.stdout" 2>"${work_dir}/${label}.stderr"; then
    echo "expected Flatpak metadata failure: ${label}" >&2
    exit 1
  fi
}

runtime_archive="$(python3 - "${runtime_source_dir}/runtime-bundles.json" <<'PY'
import json
import sys
from pathlib import Path

manifest = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
print(manifest["bundles"][0]["sherpa_onnx_archive"])
PY
)"
cp "${runtime_source_dir}/${runtime_archive}" \
  "${runtime_source_dir}/${runtime_archive}.original"
printf 'tampered\n' >>"${runtime_source_dir}/${runtime_archive}"
expect_failure bad-runtime-source-digest \
  scripts/release/render-flatpak-manifest.py \
    --source-dir "${repo_root}" \
    --runtime-manifest "${runtime_source_dir}/runtime-bundles.json" \
    --runtime-source-dir "${runtime_source_dir}" \
    --output "${work_dir}/bad-runtime-digest.json"
grep -Fq 'runtime source digest mismatch' \
  "${work_dir}/bad-runtime-source-digest.stderr"
mv "${runtime_source_dir}/${runtime_archive}.original" \
  "${runtime_source_dir}/${runtime_archive}"

runtime_source_link="${work_dir}/runtime-sources-link"
ln -s "${runtime_source_dir}" "${runtime_source_link}"
expect_failure runtime-source-dir-symlink \
  scripts/release/render-flatpak-manifest.py \
    --source-dir "${repo_root}" \
    --runtime-manifest "${runtime_source_dir}/runtime-bundles.json" \
    --runtime-source-dir "${runtime_source_link}" \
    --output "${work_dir}/runtime-source-link.json"
grep -Fq 'must not be a symbolic link' \
  "${work_dir}/runtime-source-dir-symlink.stderr"

cargo_archive="$(python3 - "${cargo_source_dir}/cargo-sources.json" <<'PY'
import json
import sys
from pathlib import Path
from urllib.parse import urlparse

sources = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
archive = next(source for source in sources if source.get("type") == "archive")
print(Path(urlparse(archive["url"]).path).name)
PY
)"
cp "${cargo_source_dir}/${cargo_archive}" \
  "${cargo_source_dir}/${cargo_archive}.original"
printf 'tampered\n' >>"${cargo_source_dir}/${cargo_archive}"
expect_failure bad-cargo-source-digest \
  scripts/release/render-flatpak-manifest.py \
    --source-dir "${repo_root}" \
    --cargo-sources-manifest "${cargo_source_dir}/cargo-sources.json" \
    --cargo-source-dir "${cargo_source_dir}" \
    --output "${work_dir}/bad-cargo-digest.json"
grep -Fq 'Cargo source digest mismatch' \
  "${work_dir}/bad-cargo-source-digest.stderr"
mv "${cargo_source_dir}/${cargo_archive}.original" \
  "${cargo_source_dir}/${cargo_archive}"

cargo_source_link="${work_dir}/cargo-sources-link"
ln -s "${cargo_source_dir}" "${cargo_source_link}"
expect_failure cargo-source-dir-symlink \
  scripts/release/render-flatpak-manifest.py \
    --source-dir "${repo_root}" \
    --cargo-sources-manifest "${cargo_source_dir}/cargo-sources.json" \
    --cargo-source-dir "${cargo_source_link}" \
    --output "${work_dir}/cargo-source-link.json"
grep -Fq 'Cargo source directory must not be a symbolic link' \
  "${work_dir}/cargo-source-dir-symlink.stderr"

prefetched_cargo_dir="${work_dir}/prefetched-cargo-sources"
scripts/release/prefetch-flatpak-cargo-sources.py \
  --sources "${cargo_source_dir}/cargo-sources.json" \
  --cache-dir "${cargo_cache_dir}" \
  --output-dir "${prefetched_cargo_dir}" \
  --jobs 2
cmp "${cargo_source_dir}/${cargo_archive}" \
  "${prefetched_cargo_dir}/${cargo_archive}"
printf 'tampered\n' >>"${prefetched_cargo_dir}/${cargo_archive}"
scripts/release/prefetch-flatpak-cargo-sources.py \
  --sources "${cargo_source_dir}/cargo-sources.json" \
  --cache-dir "${cargo_cache_dir}" \
  --output-dir "${prefetched_cargo_dir}" \
  --jobs 2
cmp "${cargo_source_dir}/${cargo_archive}" \
  "${prefetched_cargo_dir}/${cargo_archive}"

download_sources="${work_dir}/download-cargo-sources.json"
python3 - "${cargo_source_dir}/cargo-sources.json" "${download_sources}" <<'PY'
import json
import sys
from pathlib import Path

sources = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
archive = next(source for source in sources if source.get("type") == "archive")
Path(sys.argv[2]).write_text(json.dumps([archive]), encoding="utf-8")
PY
fake_bin="${work_dir}/fake-curl-bin"
empty_cache="${work_dir}/empty-cargo-cache"
downloaded_cargo_dir="${work_dir}/downloaded-cargo-sources"
write_cache_dir="${work_dir}/shared-cargo-write-cache"
offline_cargo_dir="${work_dir}/offline-cargo-sources"
mkdir -p "${fake_bin}" "${empty_cache}"
cat >"${fake_bin}/curl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
destination=""
while (($#)); do
  if [[ "$1" == "-o" ]]; then
    destination="$2"
    shift 2
  else
    shift
  fi
done
[[ -n "${destination}" ]]
cp "${VINPST_FAKE_CURL_SOURCE:?}" "${destination}"
SH
chmod +x "${fake_bin}/curl"
PATH="${fake_bin}:${PATH}" \
  VINPST_FAKE_CURL_SOURCE="${cargo_source_dir}/${cargo_archive}" \
  scripts/release/prefetch-flatpak-cargo-sources.py \
    --sources "${download_sources}" \
    --cache-dir "${empty_cache}" \
    --write-cache-dir "${write_cache_dir}" \
    --output-dir "${downloaded_cargo_dir}" \
    --jobs 1
cmp "${cargo_source_dir}/${cargo_archive}" \
  "${downloaded_cargo_dir}/${cargo_archive}"
cmp "${cargo_source_dir}/${cargo_archive}" \
  "${write_cache_dir}/${cargo_archive}"
scripts/release/prefetch-flatpak-cargo-sources.py \
  --sources "${download_sources}" \
  --write-cache-dir "${write_cache_dir}" \
  --output-dir "${offline_cargo_dir}" \
  --jobs 1 \
  --offline
cmp "${cargo_source_dir}/${cargo_archive}" \
  "${offline_cargo_dir}/${cargo_archive}"

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

lock_file="${repo_root}/target/tmp/flatpak-package-smoke.lock"
mkdir -p "$(dirname "${lock_file}")"
(
  flock -n 9
  if scripts/release/run-flatpak-package-smoke.sh \
    >"${work_dir}/concurrent-smoke.stdout" \
    2>"${work_dir}/concurrent-smoke.stderr"; then
    echo "expected concurrent Flatpak package smoke to fail" >&2
    exit 1
  fi
  grep -Fq 'another Flatpak package smoke is already using' \
    "${work_dir}/concurrent-smoke.stderr"
) 9>"${lock_file}"

printf 'Flatpak manifest metadata check passed\n'
