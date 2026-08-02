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

run_target() {
  local base_image="$1"
  local label="$2"
  if [[ ! "${label}" =~ ^[a-z0-9][a-z0-9.-]*$ ]]; then
    echo "invalid Debian smoke label: ${label@Q}" >&2
    exit 2
  fi

  local docker_tag="fcitx-vinput-rs-deb-${label}:local"
  local output_dir="target/tmp/deb-package-smoke/${label}"
  local -a build_args=(--build-arg "BASE_IMAGE=${base_image}")
  if [[ -n "${VINPUT_DEB_APT_MIRROR:-}" ]]; then
    build_args+=(--build-arg "APT_MIRROR=${VINPUT_DEB_APT_MIRROR}")
  fi
  if [[ -n "${VINPUT_DEB_APT_SECURITY_MIRROR:-}" ]]; then
    build_args+=(--build-arg "APT_SECURITY_MIRROR=${VINPUT_DEB_APT_SECURITY_MIRROR}")
  fi
  if [[ -n "${VINPUT_DEB_RUSTUP_DIST_SERVER:-}" ]]; then
    build_args+=(--build-arg "RUSTUP_DIST_SERVER=${VINPUT_DEB_RUSTUP_DIST_SERVER}")
  fi
  if [[ -n "${VINPUT_DEB_RUSTUP_UPDATE_ROOT:-}" ]]; then
    build_args+=(--build-arg "RUSTUP_UPDATE_ROOT=${VINPUT_DEB_RUSTUP_UPDATE_ROOT}")
  fi
  docker build \
    "${build_args[@]}" \
    --file packaging/debian/Dockerfile \
    --tag "${docker_tag}" \
    packaging/debian
  docker run --rm \
    --volume "${repo_root}:/workspace" \
    --workdir /workspace \
    --env RUSTUP_TOOLCHAIN=stable \
    --env "VINPUT_DEB_CARGO_OFFLINE=${VINPUT_DEB_CARGO_OFFLINE:-0}" \
    --env "VINPUT_HOST_UID=$(id -u)" \
    --env "VINPUT_HOST_GID=$(id -g)" \
    "${docker_tag}" \
    bash -lc "
      set -euo pipefail
      cleanup() {
        chown -R \"\${VINPUT_HOST_UID}:\${VINPUT_HOST_GID}\" \
          /workspace/target/tmp/deb-package-build \
          /workspace/target/tmp/deb-package-cache \
          /workspace/target/tmp/deb-package-assets \
          /workspace/${output_dir} 2>/dev/null || true
      }
      trap cleanup EXIT
      scripts/release/run-deb-package-smoke-inner.sh '${label}' '/workspace/${output_dir}'
    "
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
