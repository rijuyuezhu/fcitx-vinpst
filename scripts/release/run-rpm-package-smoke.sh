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
stage_root="${repo_root}/target/tmp/rpm-package-smoke"
cache_root="${repo_root}/target/tmp/rpm-package-cache"
topdir="${stage_root}/rpmbuild"
source_cache="${topdir}/SOURCES"
spec_dir="${topdir}/SPECS"
package_root="${stage_root}/package-root"
rpm_root="${stage_root}/rpm-root"
source_archive="${source_cache}/${source_dir}.tar.gz"
asset_cache="${repo_root}/target/tmp/arch-package-assets"
sherpa_archive="${asset_cache}/sherpa-onnx-v1.13.3-linux-x64-shared-lib.tar.bz2"

rm -rf "${stage_root}"
mkdir -p \
  "${topdir}/BUILD" \
  "${topdir}/BUILDROOT" \
  "${topdir}/RPMS" \
  "${topdir}/SRPMS" \
  "${source_cache}" \
  "${spec_dir}" \
  "${package_root}" \
  "${asset_cache}" \
  "${cache_root}/cargo-home" \
  "${cache_root}/cargo-target"

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
  --transform "s,^,${source_dir}/," \
  -czf "${source_archive}" \
  .
source_sha256="$(sha256sum "${source_archive}" | awk '{print $1}')"

render_spec() {
  local release="$1"
  local output="$2"
  scripts/release/render-rpm-spec.py \
    --version "${version}" \
    --release "${release}" \
    --source-name "${source_dir}.tar.gz" \
    --source-sha256 "${source_sha256}" \
    --source-dir "${source_dir}" \
    --output "${output}"
}

build_rpm() {
  local spec="$1"
  rpmbuild -bb --nodeps \
    --define "_topdir ${topdir}" \
    --define "_smp_build_ncpus $(nproc)" \
    --define "_vinput_cargo_home ${cache_root}/cargo-home" \
    --define "_vinput_cargo_target ${cache_root}/cargo-target" \
    --define "_vinput_cmake_build ${cache_root}/cmake-build" \
    "${spec}"
}

initial_spec="${spec_dir}/fcitx-vinput-rs.spec"
upgrade_spec="${spec_dir}/fcitx-vinput-rs-upgrade.spec"
render_spec 1 "${initial_spec}"
rpmspec -P "${initial_spec}" >"${stage_root}/expanded.spec"
build_rpm "${initial_spec}"
initial_rpm="$(find "${topdir}/RPMS" -type f \
  -name "fcitx-vinput-rs-${version}-1.*.rpm" -print -quit)"
test -n "${initial_rpm}"

render_spec 2 "${upgrade_spec}"
build_rpm "${upgrade_spec}"
upgrade_rpm="$(find "${topdir}/RPMS" -type f \
  -name "fcitx-vinput-rs-${version}-2.*.rpm" -print -quit)"
test -n "${upgrade_rpm}"

verify_db="${stage_root}/verify-rpmdb"
mkdir -p "${verify_db}"
rpm --dbpath "${verify_db}" --initdb
rpm --dbpath "${verify_db}" -K --nosignature "${initial_rpm}"
test "$(rpm --dbpath "${verify_db}" -qp --qf '%{NAME}' "${initial_rpm}")" = fcitx-vinput-rs
test "$(rpm --dbpath "${verify_db}" -qp --qf '%{VERSION}' "${initial_rpm}")" = "${version}"
test "$(rpm --dbpath "${verify_db}" -qp --qf '%{RELEASE}' "${initial_rpm}")" = 1
test "$(rpm --dbpath "${verify_db}" -qp --qf '%{ARCH}' "${initial_rpm}")" = x86_64
test "$(rpm --dbpath "${verify_db}" -qp --qf '%{LICENSE}' "${initial_rpm}")" = \
  'GPL-3.0-or-later AND Apache-2.0 AND MIT'
rpm --dbpath "${verify_db}" -qp --provides "${initial_rpm}" | grep -Eq '^fcitx5-vinput = '
rpm --dbpath "${verify_db}" -qp --conflicts "${initial_rpm}" | grep -qx 'fcitx5-vinput'
rpm --dbpath "${verify_db}" -qp --requires "${initial_rpm}" | grep -qx 'fcitx5'
rpm --dbpath "${verify_db}" -qp --requires "${initial_rpm}" | grep -qx 'pipewire-libs'
rpm --dbpath "${verify_db}" -qp --scripts "${initial_rpm}" >"${stage_root}/scriptlets.txt"
grep -Fq '/usr/lib/fcitx-vinput/package-upgrade-handoff' \
  "${stage_root}/scriptlets.txt"
grep -Fq '/usr/lib/fcitx-vinput/package-remove-handoff' \
  "${stage_root}/scriptlets.txt"
# shellcheck disable=SC2016
grep -Fq '[[ "$1" -gt 1' "${stage_root}/scriptlets.txt"
# shellcheck disable=SC2016
grep -Fq '[[ "$1" -eq 0' "${stage_root}/scriptlets.txt"

(
  cd "${package_root}"
  rpm2cpio "${initial_rpm}" | cpio -idm --quiet
)
required_files=(
  usr/bin/vinput
  usr/bin/vinput-daemon
  usr/bin/vinput-gui
  usr/lib/fcitx-vinput/package-session-common.sh
  usr/lib/fcitx-vinput/package-upgrade-handoff
  usr/lib/fcitx-vinput/package-remove-handoff
  usr/lib/fcitx-vinput/libsherpa-onnx-c-api.so
  usr/lib/fcitx-vinput/libonnxruntime.so
  usr/lib64/fcitx5/fcitx5-vinput.so
  usr/lib/systemd/user/vinput-daemon.service
  usr/share/applications/vinput-gui.desktop
  usr/share/dbus-1/services/org.fcitx.Vinput.service
  usr/share/fcitx5/addon/vinput.conf
  usr/share/icons/hicolor/16x16/apps/vinput-gui.png
  usr/share/icons/hicolor/128x128/apps/vinput-gui.png
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
if test -e "${package_root}/usr/lib/fcitx-vinput/libsherpa-onnx-cxx-api.so"; then
  echo "RPM unexpectedly includes the sherpa C++ API library" >&2
  exit 1
fi
if find "${package_root}" -type f -print0 |
  xargs -0 grep -IlF "${topdir}/BUILD" | grep -q .; then
  echo "RPM payload contains an unremapped build-tree path" >&2
  exit 1
fi

expected_binary_rpath="\$ORIGIN/../lib/fcitx-vinput"
expected_native_rpath="\$ORIGIN"
test "$(patchelf --print-rpath "${package_root}/usr/bin/vinput")" = \
  "${expected_binary_rpath}"
test "$(patchelf --print-rpath "${package_root}/usr/bin/vinput-daemon")" = \
  "${expected_binary_rpath}"
test "$(patchelf --print-rpath "${package_root}/usr/lib/fcitx-vinput/libsherpa-onnx-c-api.so")" = \
  "${expected_native_rpath}"
for binary in "${package_root}/usr/bin/vinput" "${package_root}/usr/bin/vinput-daemon"; do
  linkage="$(ldd "${binary}")"
  if grep -q 'not found' <<<"${linkage}"; then
    echo "RPM binary has unresolved dynamic libraries: ${binary}" >&2
    exit 1
  fi
  grep -q "${package_root}/usr/bin/../lib/fcitx-vinput/libsherpa-onnx-c-api.so" \
    <<<"${linkage}"
done
gui_linkage="$(ldd "${package_root}/usr/bin/vinput-gui")"
if grep -q 'not found' <<<"${gui_linkage}"; then
  echo "RPM GUI has unresolved dynamic libraries" >&2
  exit 1
fi
ldd "${package_root}/usr/lib/fcitx-vinput/libsherpa-onnx-c-api.so" |
  grep -q "${package_root}/usr/lib/fcitx-vinput/libonnxruntime.so"

"${package_root}/usr/bin/vinput" --version | grep -q "${version}"
"${package_root}/usr/bin/vinput-daemon" --help >/dev/null
"${package_root}/usr/bin/vinput-gui" --version | grep -q "${version}"
isolated_config="${stage_root}/empty-config"
mkdir -p "${isolated_config}"
XDG_CONFIG_HOME="${isolated_config}" \
  "${package_root}/usr/bin/vinput-gui" --check --offline |
  jq -e '.ok and .application == "vinput-gui" and .daemon.skipped' >/dev/null
grep -qx 'Exec=vinput-gui' \
  "${package_root}/usr/share/applications/vinput-gui.desktop"
grep -qx 'SystemdService=vinput-daemon.service' \
  "${package_root}/usr/share/dbus-1/services/org.fcitx.Vinput.service"
grep -qx 'ExecStart=/usr/bin/vinput-daemon --dbus --configured-backends --audio-backend pipewire --exit-when-executable-replaced' \
  "${package_root}/usr/lib/systemd/user/vinput-daemon.service"

mkdir -p "${rpm_root}/var/lib/rpm" "${rpm_root}/home/test/.config/fcitx-vinput"
printf '%s\n' '{"schema_version":999}' >"${rpm_root}/home/test/.config/fcitx-vinput/config.json"
config_sha256="$(sha256sum "${rpm_root}/home/test/.config/fcitx-vinput/config.json" | awk '{print $1}')"
unshare -Ur rpm --root "${rpm_root}" --dbpath /var/lib/rpm --initdb
unshare -Ur rpm --root "${rpm_root}" --dbpath /var/lib/rpm \
  --nodeps --noscripts --nosignature -i "${initial_rpm}"
test "$(unshare -Ur rpm --root "${rpm_root}" --dbpath /var/lib/rpm -q \
  --qf '%{VERSION}-%{RELEASE}' fcitx-vinput-rs)" = "${version}-1"
unshare -Ur rpm --root "${rpm_root}" --dbpath /var/lib/rpm \
  -V --nodeps fcitx-vinput-rs
unshare -Ur rpm --root "${rpm_root}" --dbpath /var/lib/rpm \
  --nodeps --noscripts --nosignature -U "${upgrade_rpm}"
test "$(unshare -Ur rpm --root "${rpm_root}" --dbpath /var/lib/rpm -q \
  --qf '%{VERSION}-%{RELEASE}' fcitx-vinput-rs)" = "${version}-2"
unshare -Ur rpm --root "${rpm_root}" --dbpath /var/lib/rpm \
  -V --nodeps fcitx-vinput-rs
test "$(sha256sum "${rpm_root}/home/test/.config/fcitx-vinput/config.json" | awk '{print $1}')" = \
  "${config_sha256}"
unshare -Ur rpm --root "${rpm_root}" --dbpath /var/lib/rpm \
  --nodeps --noscripts -e fcitx-vinput-rs
if unshare -Ur rpm --root "${rpm_root}" --dbpath /var/lib/rpm \
  -q fcitx-vinput-rs >/dev/null 2>&1; then
  echo "RPM remained registered after removal" >&2
  exit 1
fi
test "$(sha256sum "${rpm_root}/home/test/.config/fcitx-vinput/config.json" | awk '{print $1}')" = \
  "${config_sha256}"
for relative in "${required_files[@]}"; do
  test ! -e "${rpm_root}/${relative}"
done

echo "RPM package build and transaction smoke passed: ${initial_rpm}"
