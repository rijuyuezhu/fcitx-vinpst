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
source_dir="fcitx-vinput-rs-${version}"
stage_root="${repo_root}/target/tmp/arch-package-smoke"
build_root="${stage_root}/build"
source_cache="${stage_root}/sources"
package_root="${stage_root}/package-root"
source_archive="${source_cache}/fcitx-vinput-rs-${version}.tar.gz"
asset_cache="${repo_root}/target/tmp/arch-package-assets"
sherpa_archive="${asset_cache}/sherpa-onnx-v1.13.3-linux-x64-shared-lib.tar.bz2"
legacy_sherpa_archive="${repo_root}/target/sherpa-onnx-prebuilt/sherpa-onnx-v1.13.3-linux-x64-shared-lib.tar.bz2"

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
  https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.13.3/sherpa-onnx-v1.13.3-linux-x64-shared-lib.tar.bz2 \
  "${sherpa_archive}"
fetch_asset \
  https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/v1.13.3/LICENSE \
  "${asset_cache}/sherpa-onnx-LICENSE"
fetch_asset \
  https://raw.githubusercontent.com/microsoft/onnxruntime/v1.24.4/LICENSE \
  "${asset_cache}/onnxruntime-LICENSE"
cp "${sherpa_archive}" "${source_cache}/"
cp "${asset_cache}/sherpa-onnx-LICENSE" \
  "${source_cache}/sherpa-onnx-LICENSE-1.13.3"
cp "${asset_cache}/onnxruntime-LICENSE" \
  "${source_cache}/onnxruntime-LICENSE-1.24.4"

sha256sum -c <<EOF
650d3da32694fa48e6e018f7087e4840aace56b3187a294a18ba3b9f51e80943  ${source_cache}/sherpa-onnx-v1.13.3-linux-x64-shared-lib.tar.bz2
cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30  ${source_cache}/sherpa-onnx-LICENSE-1.13.3
2f07c72751aed99790b8a4869cf2311df85a860b22ded05fa22803587a48922c  ${source_cache}/onnxruntime-LICENSE-1.24.4
EOF

tar \
  --exclude=.git \
  --exclude=target \
  --exclude='__pycache__' \
  --exclude='*.py[co]' \
  --exclude='packaging/arch/PKGBUILD' \
  --transform "s,^,${source_dir}/," \
  -czf "${source_archive}" \
  .
source_sha256="$(sha256sum "${source_archive}" | awk '{print $1}')"

scripts/release/render-arch-pkgbuild.py \
  --version "${version}" \
  --source-url "file://${source_archive}" \
  --source-sha256 "${source_sha256}" \
  --source-dir "${source_dir}" \
  --output "${build_root}/PKGBUILD"

bash -n "${build_root}/PKGBUILD"
bash -n "${build_root}/fcitx-vinput-rs.install"
cmp packaging/arch/fcitx-vinput-rs.install \
  "${build_root}/fcitx-vinput-rs.install"
(
  cd "${build_root}"
  SRCDEST="${source_cache}" makepkg --printsrcinfo >.SRCINFO
  SRCDEST="${source_cache}" makepkg --verifysource --noconfirm
  SRCDEST="${source_cache}" makepkg --nodeps --noconfirm --force
)

package_archive="$(find "${build_root}" -maxdepth 1 -type f \
  -name 'fcitx-vinput-rs-*.pkg.tar.zst' ! -name '*-debug-*' -print -quit)"
test -n "${package_archive}"
packaged_install="${stage_root}/packaged.INSTALL"
bsdtar -xOf "${package_archive}" .INSTALL >"${packaged_install}"
grep -q '^post_install()' "${packaged_install}"
grep -q '^post_upgrade()' "${packaged_install}"
grep -q '^post_remove()' "${packaged_install}"
grep -q '^pre_remove()' "${packaged_install}"
grep -q 'vinput daemon handoff' "${packaged_install}"
grep -q 'package-upgrade-handoff' "${packaged_install}"
grep -q 'package-remove-handoff' "${packaged_install}"
grep -q 'intentionally preserved' "${packaged_install}"
bsdtar -xf "${package_archive}" -C "${package_root}"

required_files=(
  usr/bin/vinput
  usr/bin/vinput-daemon
  usr/bin/vinput-gui
  usr/lib/fcitx-vinput/package-session-common.sh
  usr/lib/fcitx-vinput/package-upgrade-handoff
  usr/lib/fcitx-vinput/package-remove-handoff
  usr/lib/fcitx-vinput/libsherpa-onnx-c-api.so
  usr/lib/fcitx-vinput/libonnxruntime.so
  usr/lib/fcitx5/fcitx5-vinput.so
  usr/lib/systemd/user/vinput-daemon.service
  usr/share/applications/vinput-gui.desktop
  usr/share/dbus-1/services/org.fcitx.Vinput.service
  usr/share/fcitx5/addon/vinput.conf
  usr/share/icons/hicolor/128x128/apps/vinput-gui.png
  usr/share/icons/hicolor/16x16/apps/vinput-gui.png
  usr/share/icons/hicolor/512x512/apps/vinput-gui.png
  usr/share/fcitx-vinput/default-config.json
  usr/share/fcitx-vinput/vad/silero_vad.onnx
  usr/share/licenses/fcitx-vinput-rs/silero-vad-LICENSE
  usr/share/licenses/fcitx-vinput-rs/sherpa-onnx-LICENSE
  usr/share/licenses/fcitx-vinput-rs/onnxruntime-LICENSE
)
for relative in "${required_files[@]}"; do
  test -f "${package_root}/${relative}"
done
! test -e "${package_root}/usr/lib/fcitx-vinput/libsherpa-onnx-cxx-api.so"
! find "${package_root}" -type f -print0 | xargs -0 grep -IlF "${build_root}/src" | grep -q .

test "$(patchelf --print-rpath "${package_root}/usr/bin/vinput")" = \
  '$ORIGIN/../lib/fcitx-vinput'
test "$(patchelf --print-rpath "${package_root}/usr/bin/vinput-daemon")" = \
  '$ORIGIN/../lib/fcitx-vinput'
test "$(patchelf --print-rpath "${package_root}/usr/lib/fcitx-vinput/libsherpa-onnx-c-api.so")" = \
  '$ORIGIN'

for binary in "${package_root}/usr/bin/vinput" "${package_root}/usr/bin/vinput-daemon"; do
  linkage="$(ldd "${binary}")"
  ! grep -q 'not found' <<<"${linkage}"
  grep -q "${package_root}/usr/bin/../lib/fcitx-vinput/libsherpa-onnx-c-api.so" \
    <<<"${linkage}"
done
gui_linkage="$(ldd "${package_root}/usr/bin/vinput-gui")"
! grep -q 'not found' <<<"${gui_linkage}"
ldd "${package_root}/usr/lib/fcitx-vinput/libsherpa-onnx-c-api.so" |
  grep -q "${package_root}/usr/lib/fcitx-vinput/libonnxruntime.so"

"${package_root}/usr/bin/vinput" --version | grep -q "${version}"
"${package_root}/usr/bin/vinput-daemon" --help >/dev/null
"${package_root}/usr/bin/vinput-gui" --version | grep -q "${version}"

isolated_config="${stage_root}/empty-config"
mkdir -p "${isolated_config}"
XDG_CONFIG_HOME="${isolated_config}" \
  "${package_root}/usr/bin/vinput-daemon" --configured-backends print-config |
  jq -e '.active_provider == "sherpa-onnx" and .active_scene == "__raw__"' >/dev/null
XDG_CONFIG_HOME="${isolated_config}" \
  "${package_root}/usr/bin/vinput-gui" --check --offline |
  jq -e '.ok and .application == "vinput-gui" and .daemon.skipped' >/dev/null

grep -qx 'Exec=vinput-gui' \
  "${package_root}/usr/share/applications/vinput-gui.desktop"
grep -qx 'Icon=vinput-gui' \
  "${package_root}/usr/share/applications/vinput-gui.desktop"

grep -qx 'SystemdService=vinput-daemon.service' \
  "${package_root}/usr/share/dbus-1/services/org.fcitx.Vinput.service"
grep -qx 'ExecStart=/usr/bin/vinput-daemon --dbus --configured-backends --audio-backend pipewire --exit-when-executable-replaced' \
  "${package_root}/usr/lib/systemd/user/vinput-daemon.service"
grep -qx 'Restart=on-failure' \
  "${package_root}/usr/lib/systemd/user/vinput-daemon.service"
grep -qx 'Exec=/usr/bin/vinput-daemon --dbus --configured-backends --audio-backend pipewire --exit-when-executable-replaced' \
  "${package_root}/usr/share/dbus-1/services/org.fcitx.Vinput.service"

grep -qx $'\t'"provides = fcitx5-vinput=${version}" "${build_root}/.SRCINFO"
grep -qx $'\tconflicts = fcitx5-vinput' "${build_root}/.SRCINFO"

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
  -name "fcitx-vinput-rs-${version}-2-*.pkg.tar.zst" \
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
