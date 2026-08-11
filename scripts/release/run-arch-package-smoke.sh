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

usage() {
  cat <<'EOF'
usage: run-arch-package-smoke.sh [--source-archive ARCHIVE]

Without --source-archive, creates a deterministic archive from the current
checkout. Release workflows should pass the single archive produced by the
source job so the Arch package consumes those exact bytes.
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
    echo "Arch source archive must be a regular file: ${input_source_archive}" >&2
    exit 2
  fi
  input_source_archive="$(
    cd "$(dirname "${input_source_archive}")"
    pwd
  )/$(basename "${input_source_archive}")"
fi

version="$(
  cargo metadata --no-deps --format-version 1 |
    jq -r '.packages[] | select(.name == "vinpst-cli") | .version'
)"
test -n "${version}"
source_dir="fcitx-vinpst-${version}"
stage_root="${repo_root}/target/tmp/arch-package-smoke"
build_root="${stage_root}/build"
source_cache="${stage_root}/sources"
package_root="${stage_root}/package-root"
source_archive="${source_cache}/fcitx-vinpst-${version}.tar.gz"
asset_cache="${repo_root}/target/tmp/arch-package-assets"
runtime_bundle="$(
  scripts/release/runtime_bundles.py packaging/arch/runtime-bundles.json
)"
sherpa_version="$(jq -er '.sherpa_onnx_version' <<<"${runtime_bundle}")"
sherpa_archive_name="$(jq -er '.sherpa_onnx_archive' <<<"${runtime_bundle}")"
sherpa_sha256="$(jq -er '.sherpa_onnx_sha256' <<<"${runtime_bundle}")"
sherpa_license_sha256="$(jq -er '.sherpa_onnx_license_sha256' <<<"${runtime_bundle}")"
onnxruntime_version="$(jq -er '.onnxruntime_version' <<<"${runtime_bundle}")"
onnxruntime_license_sha256="$(jq -er '.onnxruntime_license_sha256' <<<"${runtime_bundle}")"
sherpa_archive="${asset_cache}/${sherpa_archive_name}"
legacy_sherpa_archive="${repo_root}/target/sherpa-onnx-prebuilt/${sherpa_archive_name}"
sherpa_license="${asset_cache}/sherpa-onnx-LICENSE-${sherpa_version}"
onnxruntime_license="${asset_cache}/onnxruntime-LICENSE-${onnxruntime_version}"

rm -rf "${stage_root}"
mkdir -p "${build_root}" "${source_cache}" "${package_root}" "${asset_cache}"

fetch_asset() {
  local url="$1"
  local output="$2"
  if [[ ! -s "${output}" ]]; then
    local temporary="${output}.tmp"
    rm -f "${temporary}"
    curl --retry 3 --retry-all-errors -fsSL "${url}" -o "${temporary}"
    mv "${temporary}" "${output}"
  fi
}

if [[ ! -s "${sherpa_archive}" && -s "${legacy_sherpa_archive}" ]]; then
  cp "${legacy_sherpa_archive}" "${sherpa_archive}"
fi
fetch_asset \
  "https://github.com/k2-fsa/sherpa-onnx/releases/download/v${sherpa_version}/${sherpa_archive_name}" \
  "${sherpa_archive}"
fetch_asset \
  "https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/v${sherpa_version}/LICENSE" \
  "${sherpa_license}"
fetch_asset \
  "https://raw.githubusercontent.com/microsoft/onnxruntime/v${onnxruntime_version}/LICENSE" \
  "${onnxruntime_license}"
cp "${sherpa_archive}" "${source_cache}/"
cp "${sherpa_license}" \
  "${source_cache}/sherpa-onnx-LICENSE-${sherpa_version}"
cp "${onnxruntime_license}" \
  "${source_cache}/onnxruntime-LICENSE-${onnxruntime_version}"

sha256sum -c <<EOF
${sherpa_sha256}  ${source_cache}/${sherpa_archive_name}
${sherpa_license_sha256}  ${source_cache}/sherpa-onnx-LICENSE-${sherpa_version}
${onnxruntime_license_sha256}  ${source_cache}/onnxruntime-LICENSE-${onnxruntime_version}
EOF

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
  >"${stage_root}/source-archive.sha256"

scripts/release/render-arch-pkgbuild.py \
  --version "${version}" \
  --source-url "file://${source_archive}" \
  --source-sha256 "${source_sha256}" \
  --source-dir "${source_dir}" \
  --output "${build_root}/PKGBUILD"

bash -n "${build_root}/PKGBUILD"
bash -n "${build_root}/fcitx-vinpst.install"
cmp packaging/arch/fcitx-vinpst.install \
  "${build_root}/fcitx-vinpst.install"
(
  cd "${build_root}"
  SRCDEST="${source_cache}" makepkg --printsrcinfo >.SRCINFO
  SRCDEST="${source_cache}" makepkg --verifysource --noconfirm
  SRCDEST="${source_cache}" makepkg --nodeps --noconfirm --force
)

package_archive="$(find "${build_root}" -maxdepth 1 -type f \
  -name 'fcitx-vinpst-*.pkg.tar.zst' ! -name '*-debug-*' -print -quit)"
test -n "${package_archive}"
packaged_install="${stage_root}/packaged.INSTALL"
bsdtar -xOf "${package_archive}" .INSTALL >"${packaged_install}"
grep -q '^post_install()' "${packaged_install}"
grep -q '^post_upgrade()' "${packaged_install}"
grep -q '^post_remove()' "${packaged_install}"
grep -q '^pre_remove()' "${packaged_install}"
grep -q 'vinpst daemon handoff' "${packaged_install}"
grep -q 'package-upgrade-handoff' "${packaged_install}"
grep -q 'package-remove-handoff' "${packaged_install}"
grep -q 'intentionally preserved' "${packaged_install}"
bsdtar -xf "${package_archive}" -C "${package_root}"

required_files=(
  usr/bin/vinpst
  usr/bin/vinpst-daemon
  usr/bin/vinpst-gui
  usr/lib/fcitx-vinpst/package-session-common.sh
  usr/lib/fcitx-vinpst/package-upgrade-handoff
  usr/lib/fcitx-vinpst/package-remove-handoff
  usr/lib/fcitx-vinpst/libsherpa-onnx-c-api.so
  usr/lib/fcitx-vinpst/libonnxruntime.so
  usr/lib/fcitx5/fcitx5-vinpst.so
  usr/lib/systemd/user/vinpst-daemon.service
  usr/share/applications/vinpst-gui.desktop
  usr/share/dbus-1/services/org.fcitx.Vinpst.service
  usr/share/fcitx5/addon/vinpst.conf
  usr/share/icons/hicolor/128x128/apps/vinpst-gui.png
  usr/share/icons/hicolor/16x16/apps/vinpst-gui.png
  usr/share/icons/hicolor/512x512/apps/vinpst-gui.png
  usr/share/fcitx-vinpst/default-config.json
  usr/share/fcitx-vinpst/vad/silero_vad.onnx
  usr/share/licenses/fcitx-vinpst/LICENSE
  usr/share/licenses/fcitx-vinpst/silero-vad-LICENSE
  usr/share/licenses/fcitx-vinpst/sherpa-onnx-LICENSE
  usr/share/licenses/fcitx-vinpst/onnxruntime-LICENSE
)
for relative in "${required_files[@]}"; do
  test -f "${package_root}/${relative}"
done
! test -e "${package_root}/usr/lib/fcitx-vinpst/libsherpa-onnx-cxx-api.so"
! find "${package_root}" -type f -print0 | xargs -0 grep -IlF "${build_root}/src" | grep -q .

test "$(patchelf --print-rpath "${package_root}/usr/bin/vinpst")" = \
  '$ORIGIN/../lib/fcitx-vinpst'
test "$(patchelf --print-rpath "${package_root}/usr/bin/vinpst-daemon")" = \
  '$ORIGIN/../lib/fcitx-vinpst'
test "$(patchelf --print-rpath "${package_root}/usr/lib/fcitx-vinpst/libsherpa-onnx-c-api.so")" = \
  '$ORIGIN'

for binary in "${package_root}/usr/bin/vinpst" "${package_root}/usr/bin/vinpst-daemon"; do
  linkage="$(ldd "${binary}")"
  ! grep -q 'not found' <<<"${linkage}"
  grep -q "${package_root}/usr/bin/../lib/fcitx-vinpst/libsherpa-onnx-c-api.so" \
    <<<"${linkage}"
done
gui_linkage="$(ldd "${package_root}/usr/bin/vinpst-gui")"
! grep -q 'not found' <<<"${gui_linkage}"
ldd "${package_root}/usr/lib/fcitx-vinpst/libsherpa-onnx-c-api.so" |
  grep -q "${package_root}/usr/lib/fcitx-vinpst/libonnxruntime.so"

"${package_root}/usr/bin/vinpst" --version | grep -q "${version}"
"${package_root}/usr/bin/vinpst-daemon" --help >/dev/null
"${package_root}/usr/bin/vinpst-gui" --version | grep -q "${version}"

isolated_config="${stage_root}/empty-config"
mkdir -p "${isolated_config}"
XDG_CONFIG_HOME="${isolated_config}" \
  "${package_root}/usr/bin/vinpst-daemon" --configured-backends print-config |
  jq -e '.active_provider == "sherpa-onnx" and .active_scene == "__raw__"' >/dev/null
XDG_CONFIG_HOME="${isolated_config}" \
  "${package_root}/usr/bin/vinpst-gui" --check --offline |
  jq -e '.ok and .application == "vinpst-gui" and .daemon.skipped' >/dev/null

grep -qx 'Exec=vinpst-gui' \
  "${package_root}/usr/share/applications/vinpst-gui.desktop"
grep -qx 'Icon=vinpst-gui' \
  "${package_root}/usr/share/applications/vinpst-gui.desktop"

grep -qx 'SystemdService=vinpst-daemon.service' \
  "${package_root}/usr/share/dbus-1/services/org.fcitx.Vinpst.service"
grep -qx 'ExecStart=/usr/bin/vinpst-daemon --dbus --configured-backends --audio-backend pipewire --exit-when-executable-replaced' \
  "${package_root}/usr/lib/systemd/user/vinpst-daemon.service"
grep -qx 'Restart=on-failure' \
  "${package_root}/usr/lib/systemd/user/vinpst-daemon.service"
grep -qx 'Exec=/usr/bin/vinpst-daemon --dbus --configured-backends --audio-backend pipewire --exit-when-executable-replaced' \
  "${package_root}/usr/share/dbus-1/services/org.fcitx.Vinpst.service"

! grep -q $'\tprovides = ' "${build_root}/.SRCINFO"
! grep -q $'\tconflicts = ' "${build_root}/.SRCINFO"
! grep -q $'\treplaces = ' "${build_root}/.SRCINFO"

(
  cd "${build_root}"
  cp PKGBUILD PKGBUILD.pkgrel1
  trap 'mv -f PKGBUILD.pkgrel1 PKGBUILD' EXIT
  "${repo_root}/scripts/release/render-arch-pkgbuild.py" \
    --template "${repo_root}/packaging/arch/PKGBUILD.in" \
    --version "${version}" \
    --pkgrel 2 \
    --source-url "file://${source_archive}" \
    --source-sha256 "${source_sha256}" \
    --source-dir "${source_dir}" \
    --output PKGBUILD
  SRCDEST="${source_cache}" makepkg --repackage --nodeps --noconfirm --force
)
upgrade_package_archive="$(find "${build_root}" -maxdepth 1 -type f \
  -name "fcitx-vinpst-${version}-2-*.pkg.tar.zst" \
  ! -name '*-debug-*' -print -quit)"
test -n "${upgrade_package_archive}"
scripts/release/run-arch-package-transaction-smoke.sh \
  "${package_archive}" "${upgrade_package_archive}"
scripts/release/run-arch-repository-smoke.sh \
  "${package_archive}" "${upgrade_package_archive}"
scripts/release/run-arch-signing-smoke.sh \
  "${package_archive}" "${upgrade_package_archive}"
scripts/release/run-arch-release-bundle-smoke.sh \
  "${source_archive}" "${package_archive}" "${upgrade_package_archive}"

echo "Arch package smoke passed: ${package_archive}"
