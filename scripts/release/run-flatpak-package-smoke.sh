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

image="${VINPUT_FLATPAK_BUILDER_IMAGE:-fcitx-vinput-rs-flatpak-builder:local}"
for command in curl docker jq sha256sum; do
  command -v "${command}" >/dev/null || {
    echo "missing Flatpak package smoke host tool: ${command}" >&2
    exit 1
  }
done

work_dir="${repo_root}/target/tmp/flatpak-package-smoke"
source_archive="${work_dir}/fcitx-vinput-rs-source.tar.gz"
container_id_file="${work_dir}/builder.cid"

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
  "${work_dir}/fcitx-vinput-rs.flatpak" \
  "${work_dir}/summary.json" \
  "${work_dir}/flathub.flatpakrepo" \
  "${container_id_file}"

curl --retry 10 --retry-all-errors --connect-timeout 30 --max-time 300 \
  --proto '=https' --tlsv1.2 -fsSL \
  https://dl.flathub.org/repo/flathub.flatpakrepo \
  -o "${work_dir}/flathub.flatpakrepo"

version="$(cargo metadata --no-deps --format-version 1 \
  | jq -r '.packages[] | select(.name == "vinput-cli") | .version')"
scripts/release/create-source-archive.sh "${source_archive}" "${version}" >/dev/null
source_sha256="$(sha256sum "${source_archive}" | awk '{print $1}')"
scripts/release/render-flatpak-manifest.py \
  --source-archive "${source_archive}" \
  --source-sha256 "${source_sha256}" \
  --revision 1 \
  --output "${work_dir}/manifest.json"

if [[ -n "${VINPUT_FLATPAK_BUILDER_IMAGE:-}" ]]; then
  docker pull "${image}"
else
  docker build \
    --build-arg "APT_MIRROR=${VINPUT_FLATPAK_APT_MIRROR:-}" \
    --build-arg "APT_SECURITY_MIRROR=${VINPUT_FLATPAK_APT_SECURITY_MIRROR:-}" \
    --tag "${image}" \
    --file packaging/flatpak/Dockerfile \
    packaging/flatpak
fi
docker run --rm --privileged \
  --cidfile "${container_id_file}" \
  --security-opt label=disable \
  --env "VINPUT_FLATPAK_REMOTE_URL=${VINPUT_FLATPAK_REMOTE_URL:-}" \
  --env "VINPUT_FLATPAK_RETRY_ATTEMPTS=${VINPUT_FLATPAK_RETRY_ATTEMPTS:-5}" \
  --volume "${repo_root}:/workspace" \
  --workdir /workspace \
  --entrypoint /bin/bash \
  "${image}" \
  scripts/release/run-flatpak-package-smoke-inner.sh \
    /workspace/target/tmp/flatpak-package-smoke/manifest.json \
    /workspace/target/tmp/flatpak-package-smoke

cleanup
trap - EXIT

test -s "${work_dir}/fcitx-vinput-rs.flatpak"
test -s "${work_dir}/summary.json"
printf 'Flatpak package smoke completed: %s\n' \
  "${work_dir}/fcitx-vinput-rs.flatpak"
