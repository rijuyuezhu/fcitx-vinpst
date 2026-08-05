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

download_checked() {
  local url="$1"
  local destination="$2"
  local expected_sha256="$3"
  local actual_sha256
  local attempt
  local temporary="${destination}.partial"

  if [[ -f "${destination}" ]]; then
    actual_sha256="$(sha256sum "${destination}" | awk '{print $1}')"
    if [[ "${actual_sha256}" == "${expected_sha256}" ]]; then
      return 0
    fi
    rm -f "${destination}"
  fi

  for attempt in 1 2 3 4 5; do
    rm -f "${temporary}"
    if curl \
      --retry 3 \
      --retry-all-errors \
      --retry-delay 2 \
      --connect-timeout 30 \
      --max-time 300 \
      --speed-limit 32768 \
      --speed-time 30 \
      --proto '=https' \
      --tlsv1.2 \
      -fsSL "${url}" \
      -o "${temporary}"; then
      actual_sha256="$(sha256sum "${temporary}" | awk '{print $1}')"
      if [[ "${actual_sha256}" == "${expected_sha256}" ]]; then
        mv "${temporary}" "${destination}"
        return 0
      fi
      echo "Flatpak runtime source digest mismatch from ${url}" >&2
    fi
    sleep "$((attempt * 5))"
  done
  rm -f "${temporary}"
  echo "failed to download checked Flatpak runtime source: ${url}" >&2
  return 1
}

download_checked \
  "https://github.com/k2-fsa/sherpa-onnx/releases/download/v${sherpa_version}/${sherpa_archive}" \
  "${runtime_source_dir}/${sherpa_archive}" \
  "${sherpa_sha256}"
download_checked \
  "https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/v${sherpa_version}/LICENSE" \
  "${runtime_source_dir}/sherpa-onnx-LICENSE" \
  "${sherpa_license_sha256}"
download_checked \
  "https://raw.githubusercontent.com/microsoft/onnxruntime/v${onnxruntime_version}/LICENSE" \
  "${runtime_source_dir}/onnxruntime-LICENSE" \
  "${onnxruntime_license_sha256}"

cargo_cache_dir="${CARGO_HOME:-${HOME}/.cargo}/registry/cache"
cargo_prefetch_args=(
  --sources "${repo_root}/packaging/flatpak/cargo-sources.json"
  --output-dir "${cargo_source_dir}"
  --jobs "${VINPST_FLATPAK_CARGO_DOWNLOAD_JOBS:-16}"
  --attempts "${VINPST_FLATPAK_CARGO_DOWNLOAD_ATTEMPTS:-5}"
)
if [[ -d "${cargo_cache_dir}" && ! -L "${cargo_cache_dir}" ]]; then
  cargo_prefetch_args+=(--cache-dir "${cargo_cache_dir}")
fi
scripts/release/prefetch-flatpak-cargo-sources.py "${cargo_prefetch_args[@]}"

curl --retry 10 --retry-all-errors --connect-timeout 30 --max-time 300 \
  --proto '=https' --tlsv1.2 -fsSL \
  https://dl.flathub.org/repo/flathub.flatpakrepo \
  -o "${work_dir}/flathub.flatpakrepo"

version="$(cargo metadata --no-deps --format-version 1 \
  | jq -r '.packages[] | select(.name == "vinpst-cli") | .version')"
scripts/release/create-source-archive.sh "${source_archive}" "${version}" >/dev/null
source_sha256="$(sha256sum "${source_archive}" | awk '{print $1}')"
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
