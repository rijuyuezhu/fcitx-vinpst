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

usage() {
  cat <<'EOF'
usage: run-flatpak-package-smoke.sh [--source-archive ARCHIVE]

Without --source-archive, creates a deterministic archive from the current
checkout. Release workflows should pass the single archive produced by the
source job so the Flatpak manifest consumes those exact bytes.
EOF
}

input_source_archive=""
while (($# > 0)); do
  case "$1" in
  --source-archive)
    input_source_archive="${2:-}"
    shift 2
    ;;
  -h | --help)
    usage
    exit 0
    ;;
  *)
    echo "unknown argument: $1" >&2
    usage >&2
    exit 2
    ;;
  esac
done
if [[ -n "${input_source_archive}" ]]; then
  if [[ ! -f "${input_source_archive}" || -L "${input_source_archive}" ]]; then
    echo "Flatpak source archive must be a regular file: ${input_source_archive}" >&2
    exit 2
  fi
  input_source_archive="$(cd "$(dirname "${input_source_archive}")" && pwd)/$(basename "${input_source_archive}")"
fi

image="${VINPST_FLATPAK_BUILDER_IMAGE:-fcitx-vinpst-flatpak-builder:local}"
for command in curl docker flock jq python3 sha256sum; do
  command -v "${command}" >/dev/null || {
    echo "missing Flatpak package smoke host tool: ${command}" >&2
    exit 1
  }
done

work_dir="${repo_root}/target/tmp/flatpak-package-smoke"
lock_file="${repo_root}/target/tmp/flatpak-package-smoke.lock"
source_archive="${work_dir}/fcitx-vinpst-source.tar.gz"
runtime_source_dir="${work_dir}/runtime-sources"
cargo_source_dir="${work_dir}/cargo-sources"
package_source_cache="${VINPST_PACKAGE_SOURCE_CACHE:-${repo_root}/target/package-source-cache}"
runtime_asset_cache="${package_source_cache}/runtime-assets"
package_cargo_cache_dir="${package_source_cache}/cargo-home/registry/cache"
container_id_file="${work_dir}/builder.cid"
mkdir -p "$(dirname "${lock_file}")"
exec 9>"${lock_file}"
if ! flock -n 9; then
  echo "another Flatpak package smoke is already using ${work_dir}" >&2
  exit 1
fi

cleanup_container() {
  if [[ -s "${container_id_file}" ]]; then
    docker rm -f "$(cat "${container_id_file}")" >/dev/null 2>&1 || true
    rm -f "${container_id_file}"
  fi
}

cleanup_permissions() {
  docker run --rm \
    --volume "${repo_root}:/workspace" \
    --entrypoint /bin/sh \
    alpine:3.22 \
    -lc "if [ -e /workspace/target/tmp/flatpak-package-smoke ]; then chown -R $(id -u):$(id -g) /workspace/target/tmp/flatpak-package-smoke; fi" \
    >/dev/null 2>&1 || true
}
cleanup_container
cleanup_permissions
cleanup() {
  cleanup_container
  cleanup_permissions
}
trap cleanup EXIT

mkdir -p "${work_dir}"
rm -rf \
  "${work_dir}/repo" \
  "${work_dir}/build-1" \
  "${work_dir}/state" \
  "${work_dir:?}/home" \
  "${work_dir}/fcitx-vinpst.flatpak" \
  "${work_dir}/summary.json" \
  "${work_dir}/flathub.flatpakrepo" \
  "${container_id_file}"

mkdir -p "${runtime_source_dir}"
mkdir -p "${cargo_source_dir}"
mkdir -p "${runtime_asset_cache}" "${package_cargo_cache_dir}"

readarray -t runtime_bundle < <(
  PYTHONDONTWRITEBYTECODE=1 \
    PYTHONPATH="${repo_root}/scripts/release" \
    python3 - <<'PY'
from pathlib import Path

from runtime_bundles import load_runtime_bundle

bundle = load_runtime_bundle(Path("packaging/arch/runtime-bundles.json"), None)
for field in (
    "sherpa_onnx_version",
    "sherpa_onnx_archive",
    "sherpa_onnx_sha256",
    "sherpa_onnx_license_sha256",
    "onnxruntime_version",
    "onnxruntime_license_sha256",
):
    print(bundle[field])
PY
)
if [[ "${#runtime_bundle[@]}" -ne 6 ]]; then
  echo "unexpected Flatpak runtime bundle metadata" >&2
  exit 1
fi
sherpa_version="${runtime_bundle[0]}"
sherpa_archive="${runtime_bundle[1]}"
sherpa_sha256="${runtime_bundle[2]}"
sherpa_license_sha256="${runtime_bundle[3]}"
onnxruntime_version="${runtime_bundle[4]}"
onnxruntime_license_sha256="${runtime_bundle[5]}"

sherpa_cached="${runtime_asset_cache}/${sherpa_archive}"
sherpa_license_cached="${runtime_asset_cache}/sherpa-onnx-LICENSE-${sherpa_version}"
onnxruntime_license_cached="${runtime_asset_cache}/onnxruntime-LICENSE-${onnxruntime_version}"
scripts/release/fetch-checked-asset.sh \
  "https://github.com/k2-fsa/sherpa-onnx/releases/download/v${sherpa_version}/${sherpa_archive}" \
  "${sherpa_cached}" \
  "${sherpa_sha256}"
scripts/release/fetch-checked-asset.sh \
  "https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/v${sherpa_version}/LICENSE" \
  "${sherpa_license_cached}" \
  "${sherpa_license_sha256}"
scripts/release/fetch-checked-asset.sh \
  "https://raw.githubusercontent.com/microsoft/onnxruntime/v${onnxruntime_version}/LICENSE" \
  "${onnxruntime_license_cached}" \
  "${onnxruntime_license_sha256}"
cp --reflink=auto "${sherpa_cached}" "${runtime_source_dir}/${sherpa_archive}"
cp --reflink=auto "${sherpa_license_cached}" "${runtime_source_dir}/sherpa-onnx-LICENSE"
cp --reflink=auto "${onnxruntime_license_cached}" "${runtime_source_dir}/onnxruntime-LICENSE"

cargo_cache_dir="${CARGO_HOME:-${HOME}/.cargo}/registry/cache"
cargo_prefetch_args=(
  --sources "${repo_root}/packaging/flatpak/cargo-sources.json"
  --output-dir "${cargo_source_dir}"
  --cache-dir "${package_cargo_cache_dir}"
  --jobs "${VINPST_FLATPAK_CARGO_DOWNLOAD_JOBS:-16}"
  --attempts "${VINPST_FLATPAK_CARGO_DOWNLOAD_ATTEMPTS:-5}"
)
if [[ "${cargo_cache_dir}" != "${package_cargo_cache_dir}" && \
  -d "${cargo_cache_dir}" && ! -L "${cargo_cache_dir}" ]]; then
  cargo_prefetch_args+=(--cache-dir "${cargo_cache_dir}")
fi
scripts/release/prefetch-flatpak-cargo-sources.py "${cargo_prefetch_args[@]}"

curl --retry 10 --retry-all-errors --connect-timeout 30 --max-time 300 \
  --proto '=https' --tlsv1.2 -fsSL \
  https://dl.flathub.org/repo/flathub.flatpakrepo \
  -o "${work_dir}/flathub.flatpakrepo"

version="$(cargo metadata --no-deps --format-version 1 \
  | jq -r '.packages[] | select(.name == "vinpst-cli") | .version')"
if [[ -n "${input_source_archive}" ]]; then
  if [[ "${input_source_archive}" != "${source_archive}" ]]; then
    cp --reflink=auto "${input_source_archive}" "${source_archive}"
    cmp "${input_source_archive}" "${source_archive}"
  fi
else
  scripts/release/create-source-archive.sh "${source_archive}" "${version}" >/dev/null
fi
source_sha256="$(sha256sum "${source_archive}" | awk '{print $1}')"
printf '%s  %s\n' "${source_sha256}" "$(basename "${source_archive}")" \
  >"${work_dir}/source-archive.sha256"
scripts/release/render-flatpak-manifest.py \
  --source-archive "${source_archive}" \
  --source-sha256 "${source_sha256}" \
  --runtime-source-dir "${runtime_source_dir}" \
  --cargo-source-dir "${cargo_source_dir}" \
  --revision 1 \
  --output "${work_dir}/manifest.json"

if [[ -n "${VINPST_FLATPAK_BUILDER_IMAGE:-}" ]]; then
  docker pull "${image}"
else
  docker build \
    --build-arg "APT_MIRROR=${VINPST_FLATPAK_APT_MIRROR:-}" \
    --build-arg "APT_SECURITY_MIRROR=${VINPST_FLATPAK_APT_SECURITY_MIRROR:-}" \
    --tag "${image}" \
    --file packaging/flatpak/Dockerfile \
    packaging/flatpak
fi
docker run --rm --privileged \
  --cidfile "${container_id_file}" \
  --security-opt label=disable \
  --env "VINPST_FLATPAK_REMOTE_URL=${VINPST_FLATPAK_REMOTE_URL:-}" \
  --env "VINPST_FLATPAK_RETRY_ATTEMPTS=${VINPST_FLATPAK_RETRY_ATTEMPTS:-5}" \
  --env "VINPST_FLATPAK_DEPENDENCY_TIMEOUT_SECONDS=${VINPST_FLATPAK_DEPENDENCY_TIMEOUT_SECONDS:-900}" \
  --env "VINPST_FLATPAK_BUILD_TIMEOUT_SECONDS=${VINPST_FLATPAK_BUILD_TIMEOUT_SECONDS:-3600}" \
  --env "VINPST_FLATPAK_TRANSACTION_TIMEOUT_SECONDS=${VINPST_FLATPAK_TRANSACTION_TIMEOUT_SECONDS:-600}" \
  --volume "${repo_root}:/workspace" \
  --workdir /workspace \
  --entrypoint /bin/bash \
  "${image}" \
  scripts/release/run-flatpak-package-smoke-inner.sh \
    /workspace/target/tmp/flatpak-package-smoke/manifest.json \
    /workspace/target/tmp/flatpak-package-smoke

cleanup
trap - EXIT

test -s "${work_dir}/fcitx-vinpst.flatpak"
test -s "${work_dir}/summary.json"
printf 'Flatpak package smoke completed: %s\n' \
  "${work_dir}/fcitx-vinpst.flatpak"
