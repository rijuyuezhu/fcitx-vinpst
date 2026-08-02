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
usage: build-deb-package.sh --distribution LABEL --output DIR [--release N ...]
EOF
}

distribution=""
output_dir=""
runtime_bundle=""
releases=()
while (($# > 0)); do
  case "$1" in
  --distribution)
    distribution="${2:-}"
    shift 2
    ;;
  --output)
    output_dir="${2:-}"
    shift 2
    ;;
  --release)
    releases+=("${2:-}")
    shift 2
    ;;
  --runtime-bundle)
    runtime_bundle="${2:-}"
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

if [[ ! "${distribution}" =~ ^[a-z0-9][a-z0-9.-]*$ ]]; then
  echo "invalid Debian distribution label: ${distribution@Q}" >&2
  exit 2
fi
if [[ -z "${output_dir}" ]]; then
  echo "--output is required" >&2
  exit 2
fi
if ((${#releases[@]} == 0)); then
  releases=(1)
fi
for release in "${releases[@]}"; do
  if [[ ! "${release}" =~ ^[0-9][0-9A-Za-z.+~]*$ ]]; then
    echo "invalid Debian release: ${release@Q}" >&2
    exit 2
  fi
done

for command in cargo cmake curl dpkg dpkg-deb jq ninja patchelf pkg-config python3 sha256sum tar; do
  command -v "${command}" >/dev/null || {
    echo "missing Debian package build tool: ${command}" >&2
    exit 1
  }
done

version="$({
  cargo metadata --no-deps --format-version 1
} | jq -r '.packages[] | select(.name == "vinput-cli") | .version')"
if [[ ! "${version}" =~ ^[0-9][0-9A-Za-z.+~-]*$ ]]; then
  echo "invalid workspace version: ${version@Q}" >&2
  exit 1
fi

architecture="$(dpkg --print-architecture)"
if [[ "${architecture}" != "amd64" ]]; then
  echo "the checked Debian baseline currently supports amd64 only, got ${architecture}" >&2
  exit 1
fi

runtime_json="$({
  PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="${repo_root}/scripts/release" python3 - \
    "${repo_root}/packaging/arch/runtime-bundles.json" "${runtime_bundle}"
} <<'PY'
import json
import sys
from pathlib import Path

from runtime_bundles import load_runtime_bundle

requested = sys.argv[2] or None
print(json.dumps(load_runtime_bundle(Path(sys.argv[1]), requested), sort_keys=True))
PY
)"
package_arch="$(jq -r '.package_arch' <<<"${runtime_json}")"
rust_target="$(jq -r '.rust_target' <<<"${runtime_json}")"
sherpa_version="$(jq -r '.sherpa_onnx_version' <<<"${runtime_json}")"
sherpa_archive_name="$(jq -r '.sherpa_onnx_archive' <<<"${runtime_json}")"
sherpa_archive_root="$(jq -r '.sherpa_onnx_archive_root' <<<"${runtime_json}")"
sherpa_sha256="$(jq -r '.sherpa_onnx_sha256' <<<"${runtime_json}")"
sherpa_license_sha256="$(jq -r '.sherpa_onnx_license_sha256' <<<"${runtime_json}")"
onnxruntime_version="$(jq -r '.onnxruntime_version' <<<"${runtime_json}")"
onnxruntime_license_sha256="$(jq -r '.onnxruntime_license_sha256' <<<"${runtime_json}")"
if [[ "${package_arch}" != "x86_64" || "${rust_target}" != "x86_64-unknown-linux-gnu" ]]; then
  echo "Debian amd64 requires the x86_64 runtime bundle" >&2
  exit 1
fi

stage_root="${repo_root}/target/tmp/deb-package-build/${distribution}"
cache_root="${repo_root}/target/tmp/deb-package-cache/${distribution}"
asset_cache="${repo_root}/target/tmp/deb-package-assets"
runtime_root="${stage_root}/runtime"
cargo_home="${cache_root}/cargo-home"
cargo_target="${cache_root}/cargo-target"
cmake_build="${cache_root}/cmake-build"
mkdir -p "${stage_root}" "${cache_root}" "${asset_cache}" "${output_dir}"
rm -rf "${runtime_root}"
mkdir -p "${runtime_root}"

fetch_asset() {
  local url="$1"
  local output="$2"
  if [[ ! -s "${output}" ]]; then
    local temporary="${output}.tmp"
    rm -f "${temporary}"
    curl \
      --retry 5 \
      --retry-all-errors \
      --connect-timeout 30 \
      --max-time 120 \
      -fsSL "${url}" -o "${temporary}"
    mv "${temporary}" "${output}"
  fi
}

sherpa_archive="${asset_cache}/${sherpa_archive_name}"
sherpa_license="${asset_cache}/sherpa-onnx-LICENSE-${sherpa_version}"
onnxruntime_license="${asset_cache}/onnxruntime-LICENSE-${onnxruntime_version}"
fetch_asset \
  "https://github.com/k2-fsa/sherpa-onnx/releases/download/v${sherpa_version}/${sherpa_archive_name}" \
  "${sherpa_archive}"
fetch_asset \
  "https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/v${sherpa_version}/LICENSE" \
  "${sherpa_license}"
fetch_asset \
  "https://raw.githubusercontent.com/microsoft/onnxruntime/v${onnxruntime_version}/LICENSE" \
  "${onnxruntime_license}"
printf '%s  %s\n' "${sherpa_sha256}" "${sherpa_archive}" | sha256sum -c -
printf '%s  %s\n' "${sherpa_license_sha256}" "${sherpa_license}" | sha256sum -c -
printf '%s  %s\n' "${onnxruntime_license_sha256}" "${onnxruntime_license}" | sha256sum -c -
tar -xjf "${sherpa_archive}" -C "${runtime_root}"
runtime_libdir="${runtime_root}/${sherpa_archive_root}/lib"
test -f "${runtime_libdir}/libsherpa-onnx-c-api.so"
test -f "${runtime_libdir}/libonnxruntime.so"

export CARGO_HOME="${cargo_home}"
export CARGO_TARGET_DIR="${cargo_target}"
export SHERPA_ONNX_LIB_DIR="${runtime_libdir}"
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=${repo_root}=."
export CFLAGS="${CFLAGS:-} -ffile-prefix-map=${repo_root}=. -fdebug-prefix-map=${repo_root}=."
export CXXFLAGS="${CXXFLAGS:-} -ffile-prefix-map=${repo_root}=. -fdebug-prefix-map=${repo_root}=."

if [[ "${VINPUT_DEB_CARGO_OFFLINE:-0}" == "1" ]]; then
  export CARGO_NET_OFFLINE=true
  cargo fetch --locked --offline --target "${rust_target}"
else
  cargo fetch --locked --target "${rust_target}"
  export CARGO_NET_OFFLINE=true
fi
cargo build --frozen --release \
  -p vinput-cli --features pipewire-backend,sherpa-onnx-backend \
  -p vinput-daemon --features pipewire-backend,sherpa-onnx-backend \
  -p vinput-gui

install_libdir="lib/$(dpkg-architecture -qDEB_HOST_MULTIARCH)"
module_dir="${install_libdir}/fcitx5"
cmake -S cpp/fcitx5-addon -B "${cmake_build}" -G Ninja \
  -DBUILD_TESTING=OFF \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX=/usr \
  -DCMAKE_INSTALL_LIBDIR="${install_libdir}" \
  -DVINPUT_DAEMON_EXECUTABLE=/usr/bin/vinput-daemon \
  -DVINPUT_DAEMON_ARGS='--dbus --configured-backends --audio-backend pipewire' \
  -DVINPUT_FCITX_BRIDGE_ENABLE_TESTS=OFF \
  -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON \
  -DVINPUT_FCITX_MODULE_INSTALL_DIR="${module_dir}" \
  -DVINPUT_FCITX_RUNTIME_BUILD_LOCALEDIR='' \
  -DVINPUT_SYSTEMD_USER_UNIT_DIR=lib/systemd/user
cmake --build "${cmake_build}" --target fcitx5_vinput_addon --parallel "$(nproc)"

for release in "${releases[@]}"; do
  package_root="${stage_root}/root-${release}"
  rm -rf "${package_root}"
  mkdir -p "${package_root}/DEBIAN"

  install -Dm755 "${cargo_target}/release/vinput" "${package_root}/usr/bin/vinput"
  install -Dm755 "${cargo_target}/release/vinput-daemon" "${package_root}/usr/bin/vinput-daemon"
  install -Dm755 "${cargo_target}/release/vinput-gui" "${package_root}/usr/bin/vinput-gui"
  install -Dm644 scripts/release/package-session-common.sh \
    "${package_root}/usr/lib/fcitx-vinput/package-session-common.sh"
  install -Dm755 scripts/release/package-upgrade-handoff.sh \
    "${package_root}/usr/lib/fcitx-vinput/package-upgrade-handoff"
  install -Dm755 scripts/release/package-remove-handoff.sh \
    "${package_root}/usr/lib/fcitx-vinput/package-remove-handoff"
  install -Dm755 "${runtime_libdir}/libsherpa-onnx-c-api.so" \
    "${package_root}/usr/lib/fcitx-vinput/libsherpa-onnx-c-api.so"
  install -Dm755 "${runtime_libdir}/libonnxruntime.so" \
    "${package_root}/usr/lib/fcitx-vinput/libonnxruntime.so"
  patchelf --set-rpath '$ORIGIN/../lib/fcitx-vinput' "${package_root}/usr/bin/vinput"
  patchelf --set-rpath '$ORIGIN/../lib/fcitx-vinput' "${package_root}/usr/bin/vinput-daemon"
  patchelf --set-rpath '$ORIGIN' \
    "${package_root}/usr/lib/fcitx-vinput/libsherpa-onnx-c-api.so"

  DESTDIR="${package_root}" cmake --install "${cmake_build}"
  install -Dm644 data/vinput-gui.desktop \
    "${package_root}/usr/share/applications/vinput-gui.desktop"
  for size in 16 22 24 32 48 64 128 256 512; do
    install -Dm644 "data/icons/hicolor/${size}x${size}/apps/vinput-gui.png" \
      "${package_root}/usr/share/icons/hicolor/${size}x${size}/apps/vinput-gui.png"
  done
  install -Dm644 data/default-config.json \
    "${package_root}/usr/share/fcitx-vinput/default-config.json"
  install -Dm644 data/vad/silero_vad.onnx \
    "${package_root}/usr/share/fcitx-vinput/vad/silero_vad.onnx"
  install -Dm644 packaging/debian/copyright \
    "${package_root}/usr/share/doc/fcitx-vinput-rs/copyright"
  install -Dm644 LICENSE \
    "${package_root}/usr/share/doc/fcitx-vinput-rs/LICENSE"
  install -Dm644 data/vad/LICENSE \
    "${package_root}/usr/share/doc/fcitx-vinput-rs/silero-vad-LICENSE"
  install -Dm644 "${sherpa_license}" \
    "${package_root}/usr/share/doc/fcitx-vinput-rs/sherpa-onnx-LICENSE"
  install -Dm644 "${onnxruntime_license}" \
    "${package_root}/usr/share/doc/fcitx-vinput-rs/onnxruntime-LICENSE"

  scripts/release/render-deb-control.py \
    --version "${version}" \
    --release "${release}" \
    --architecture "${architecture}" \
    --output "${package_root}/DEBIAN/control"
  for script in postinst prerm postrm; do
    install -Dm755 "packaging/debian/${script}" "${package_root}/DEBIAN/${script}"
  done
  (
    cd "${package_root}"
    find . -type f ! -path './DEBIAN/*' -print0 \
      | sort -z \
      | xargs -0 md5sum \
      | sed 's#  \./#  #' >DEBIAN/md5sums
  )

  output="${output_dir}/fcitx-vinput-rs_${version}-${release}_${distribution}_${architecture}.deb"
  rm -f "${output}"
  dpkg-deb --root-owner-group --build "${package_root}" "${output}"
  test -s "${output}"
  printf '%s\n' "${output}"
done
