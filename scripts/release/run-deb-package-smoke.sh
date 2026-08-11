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
usage: run-deb-package-smoke.sh [--image IMAGE --distribution LABEL]

Without arguments, runs both checked release targets:
  debian:12 / debian12
  ubuntu:24.04 / ubuntu24.04
EOF
}

image=""
distribution=""
while (($# > 0)); do
  case "$1" in
  --image)
    image="${2:-}"
    shift 2
    ;;
  --distribution)
    distribution="${2:-}"
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

command -v docker >/dev/null || {
  echo "Docker is required for the Debian package smoke" >&2
  exit 1
}
command -v rustc >/dev/null || {
  echo "rustc is required on the host for the Debian package smoke" >&2
  exit 1
}
rust_sysroot="$(rustc --print sysroot)"
[[ -x "${rust_sysroot}/bin/rustc" && -x "${rust_sysroot}/bin/cargo" ]] || {
  echo "host Rust sysroot does not contain rustc and cargo: ${rust_sysroot}" >&2
  exit 1
}

package_source_cache="$(scripts/release/resolve-package-source-cache.sh \
  "${VINPST_PACKAGE_SOURCE_CACHE:-${repo_root}/target/package-source-cache}")"

run_target() {
  local base_image="$1"
  local label="$2"
  if [[ ! "${label}" =~ ^[a-z0-9][a-z0-9.-]*$ ]]; then
    echo "invalid Debian smoke label: ${label@Q}" >&2
    exit 2
  fi

  local docker_tag="fcitx-vinpst-deb-${label}:local"
  local output_dir="target/tmp/deb-package-smoke/${label}"
  local -a build_args=(--build-arg "BASE_IMAGE=${base_image}")
  if [[ -n "${VINPST_DEB_APT_MIRROR:-}" ]]; then
    build_args+=(--build-arg "APT_MIRROR=${VINPST_DEB_APT_MIRROR}")
  fi
  if [[ -n "${VINPST_DEB_APT_SECURITY_MIRROR:-}" ]]; then
    build_args+=(--build-arg "APT_SECURITY_MIRROR=${VINPST_DEB_APT_SECURITY_MIRROR}")
  fi
  docker build \
    "${build_args[@]}" \
    --file packaging/debian/Dockerfile \
    --tag "${docker_tag}" \
    packaging/debian
  docker run --rm "${docker_tag}" bash -c '
    set -euo pipefail
    if command -v rustc >/dev/null 2>&1 || command -v cargo >/dev/null 2>&1; then
      echo "Debian package builder unexpectedly contains a Rust toolchain" >&2
      exit 1
    fi
  '
  docker run --rm \
    --volume "${repo_root}:/workspace" \
    --volume "${package_source_cache}:/package-source-cache" \
    --volume "${rust_sysroot}:${rust_sysroot}:ro" \
    --workdir /workspace \
    --env "VINPST_RUST_SYSROOT=${rust_sysroot}" \
    --env "VINPST_DEB_CARGO_OFFLINE=${VINPST_DEB_CARGO_OFFLINE:-0}" \
    --env VINPST_PACKAGE_SOURCE_CACHE=/package-source-cache \
    --env "VINPST_DEB_LABEL=${label}" \
    --env "VINPST_DEB_OUTPUT_DIR=/workspace/${output_dir}" \
    --env "VINPST_HOST_UID=$(id -u)" \
    --env "VINPST_HOST_GID=$(id -g)" \
    "${docker_tag}" \
    bash -lc '
      set -euo pipefail
      export PATH="${VINPST_RUST_SYSROOT}/bin:${PATH}"
      rustc --version
      cargo --version
      cleanup() {
        chown -R "${VINPST_HOST_UID}:${VINPST_HOST_GID}" \
          /workspace/target/tmp/deb-package-build \
          /workspace/target/tmp/deb-package-cache \
          /package-source-cache \
          "${VINPST_DEB_OUTPUT_DIR}" 2>/dev/null || true
      }
      trap cleanup EXIT
      scripts/release/run-deb-package-smoke-inner.sh \
        "${VINPST_DEB_LABEL}" "${VINPST_DEB_OUTPUT_DIR}"
    '
}

if [[ -n "${image}" || -n "${distribution}" ]]; then
  if [[ -z "${image}" || -z "${distribution}" ]]; then
    echo "--image and --distribution must be provided together" >&2
    exit 2
  fi
  run_target "${image}" "${distribution}"
else
  run_target debian:12 debian12
  run_target ubuntu:24.04 ubuntu24.04
fi
