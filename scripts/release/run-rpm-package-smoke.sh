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
    jq -r '.packages[] | select(.name == "vinpst-cli") | .version'
)"
test -n "${version}"
source_dir="fcitx-vinpst-${version}"
stage_root="${repo_root}/target/tmp/rpm-package-smoke"
build_cache_root="${repo_root}/target/tmp/rpm-package-cache"
package_source_cache="$(scripts/release/resolve-package-source-cache.sh \
  "${VINPST_PACKAGE_SOURCE_CACHE:-${repo_root}/target/package-source-cache}")"
package_cargo_home="${package_source_cache}/cargo-home"
topdir="${stage_root}/rpmbuild"
source_cache="${topdir}/SOURCES"
spec_dir="${topdir}/SPECS"
package_root="${stage_root}/package-root"
rpm_root="${stage_root}/rpm-root"
source_archive="${source_cache}/${source_dir}.tar.gz"
asset_cache="${package_source_cache}/runtime-assets"
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
sherpa_license="${asset_cache}/sherpa-onnx-LICENSE-${sherpa_version}"
onnxruntime_license="${asset_cache}/onnxruntime-LICENSE-${onnxruntime_version}"

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
  "${package_cargo_home}" \
  "${build_cache_root}/cargo-target"

scripts/release/fetch-checked-asset.sh \
  "https://github.com/k2-fsa/sherpa-onnx/releases/download/v${sherpa_version}/${sherpa_archive_name}" \
  "${sherpa_archive}" \
  "${sherpa_sha256}"
scripts/release/fetch-checked-asset.sh \
  "https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/v${sherpa_version}/LICENSE" \
  "${sherpa_license}" \
  "${sherpa_license_sha256}"
scripts/release/fetch-checked-asset.sh \
  "https://raw.githubusercontent.com/microsoft/onnxruntime/v${onnxruntime_version}/LICENSE" \
  "${onnxruntime_license}" \
  "${onnxruntime_license_sha256}"
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

scripts/release/create-source-archive.sh "${source_archive}" "${version}" >/dev/null
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
    --define "_vinpst_cargo_home ${package_cargo_home}" \
    --define "_vinpst_cargo_target ${build_cache_root}/cargo-target" \
    --define "_vinpst_cmake_build ${build_cache_root}/cmake-build" \
    "${spec}"
}

initial_spec="${spec_dir}/fcitx-vinpst.spec"
upgrade_spec="${spec_dir}/fcitx-vinpst-upgrade.spec"
render_spec 1 "${initial_spec}"
rpmspec -P "${initial_spec}" >"${stage_root}/expanded.spec"
build_rpm "${initial_spec}"
initial_rpm="$(find "${topdir}/RPMS" -type f \
  -name "fcitx-vinpst-${version}-1.*.rpm" -print -quit)"
test -n "${initial_rpm}"

render_spec 2 "${upgrade_spec}"
build_rpm "${upgrade_spec}"
upgrade_rpm="$(find "${topdir}/RPMS" -type f \
  -name "fcitx-vinpst-${version}-2.*.rpm" -print -quit)"
test -n "${upgrade_rpm}"

verify_db="${stage_root}/verify-rpmdb"
mkdir -p "${verify_db}"
rpm --dbpath "${verify_db}" --initdb
rpm --dbpath "${verify_db}" -K --nosignature "${initial_rpm}"
test "$(rpm --dbpath "${verify_db}" -qp --qf '%{NAME}' "${initial_rpm}")" = fcitx-vinpst
test "$(rpm --dbpath "${verify_db}" -qp --qf '%{VERSION}' "${initial_rpm}")" = "${version}"
test "$(rpm --dbpath "${verify_db}" -qp --qf '%{RELEASE}' "${initial_rpm}")" = 1
test "$(rpm --dbpath "${verify_db}" -qp --qf '%{ARCH}' "${initial_rpm}")" = x86_64
test "$(rpm --dbpath "${verify_db}" -qp --qf '%{LICENSE}' "${initial_rpm}")" = \
  'GPL-3.0-or-later AND Apache-2.0 AND MIT'
! rpm --dbpath "${verify_db}" -qp --provides "${initial_rpm}" | grep -Eq '^fcitx5-vinpst = '
! rpm --dbpath "${verify_db}" -qp --conflicts "${initial_rpm}" | grep -q .
! rpm --dbpath "${verify_db}" -qp --obsoletes "${initial_rpm}" | grep -q .
rpm --dbpath "${verify_db}" -qp --requires "${initial_rpm}" | grep -qx 'fcitx5'
rpm --dbpath "${verify_db}" -qp --requires "${initial_rpm}" | grep -qx 'pipewire-libs'
rpm --dbpath "${verify_db}" -qp --scripts "${initial_rpm}" >"${stage_root}/scriptlets.txt"
grep -Fq '/usr/lib/fcitx-vinpst/package-upgrade-handoff' \
  "${stage_root}/scriptlets.txt"
grep -Fq '/usr/lib/fcitx-vinpst/package-remove-handoff' \
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
  usr/bin/vinpst
  usr/bin/vinpst-daemon
  usr/bin/vinpst-gui
  usr/lib/fcitx-vinpst/package-session-common.sh
  usr/lib/fcitx-vinpst/package-upgrade-handoff
  usr/lib/fcitx-vinpst/package-remove-handoff
  usr/lib/fcitx-vinpst/libsherpa-onnx-c-api.so
  usr/lib/fcitx-vinpst/libonnxruntime.so
  usr/lib64/fcitx5/fcitx5-vinpst.so
  usr/lib/systemd/user/vinpst-daemon.service
  usr/share/applications/vinpst-gui.desktop
  usr/share/dbus-1/services/org.fcitx.Vinpst.service
  usr/share/fcitx5/addon/vinpst.conf
  usr/share/icons/hicolor/16x16/apps/vinpst-gui.png
  usr/share/icons/hicolor/128x128/apps/vinpst-gui.png
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
if test -e "${package_root}/usr/lib/fcitx-vinpst/libsherpa-onnx-cxx-api.so"; then
  echo "RPM unexpectedly includes the sherpa C++ API library" >&2
  exit 1
fi
if find "${package_root}" -type f -print0 |
  xargs -0 grep -IlF "${topdir}/BUILD" | grep -q .; then
  echo "RPM payload contains an unremapped build-tree path" >&2
  exit 1
fi

expected_binary_rpath="\$ORIGIN/../lib/fcitx-vinpst"
expected_native_rpath="\$ORIGIN"
test "$(patchelf --print-rpath "${package_root}/usr/bin/vinpst")" = \
  "${expected_binary_rpath}"
test "$(patchelf --print-rpath "${package_root}/usr/bin/vinpst-daemon")" = \
  "${expected_binary_rpath}"
test "$(patchelf --print-rpath "${package_root}/usr/lib/fcitx-vinpst/libsherpa-onnx-c-api.so")" = \
  "${expected_native_rpath}"
for binary in "${package_root}/usr/bin/vinpst" "${package_root}/usr/bin/vinpst-daemon"; do
  linkage="$(ldd "${binary}")"
  if grep -q 'not found' <<<"${linkage}"; then
    echo "RPM binary has unresolved dynamic libraries: ${binary}" >&2
    exit 1
  fi
  grep -q "${package_root}/usr/bin/../lib/fcitx-vinpst/libsherpa-onnx-c-api.so" \
    <<<"${linkage}"
done
gui_linkage="$(ldd "${package_root}/usr/bin/vinpst-gui")"
if grep -q 'not found' <<<"${gui_linkage}"; then
  echo "RPM GUI has unresolved dynamic libraries" >&2
  exit 1
fi
ldd "${package_root}/usr/lib/fcitx-vinpst/libsherpa-onnx-c-api.so" |
  grep -q "${package_root}/usr/lib/fcitx-vinpst/libonnxruntime.so"

"${package_root}/usr/bin/vinpst" --version | grep -q "${version}"
"${package_root}/usr/bin/vinpst-daemon" --help >/dev/null
"${package_root}/usr/bin/vinpst-gui" --version | grep -q "${version}"
isolated_config="${stage_root}/empty-config"
mkdir -p "${isolated_config}"
XDG_CONFIG_HOME="${isolated_config}" \
  "${package_root}/usr/bin/vinpst-gui" --check --offline |
  jq -e '.ok and .application == "vinpst-gui" and .daemon.skipped' >/dev/null
grep -qx 'Exec=vinpst-gui' \
  "${package_root}/usr/share/applications/vinpst-gui.desktop"
grep -qx 'SystemdService=vinpst-daemon.service' \
  "${package_root}/usr/share/dbus-1/services/org.fcitx.Vinpst.service"
grep -qx 'ExecStart=/usr/bin/vinpst-daemon --dbus --configured-backends --audio-backend pipewire --exit-when-executable-replaced' \
  "${package_root}/usr/lib/systemd/user/vinpst-daemon.service"

mkdir -p "${rpm_root}/var/lib/rpm" "${rpm_root}/home/test/.config/fcitx-vinpst"
printf '%s\n' '{"schema_version":999}' >"${rpm_root}/home/test/.config/fcitx-vinpst/config.json"
config_sha256="$(sha256sum "${rpm_root}/home/test/.config/fcitx-vinpst/config.json" | awk '{print $1}')"
unshare -Ur rpm --root "${rpm_root}" --dbpath /var/lib/rpm --initdb
unshare -Ur rpm --root "${rpm_root}" --dbpath /var/lib/rpm \
  --nodeps --noscripts --nosignature -i "${initial_rpm}"
test "$(unshare -Ur rpm --root "${rpm_root}" --dbpath /var/lib/rpm -q \
  --qf '%{VERSION}-%{RELEASE}' fcitx-vinpst)" = "${version}-1"
unshare -Ur rpm --root "${rpm_root}" --dbpath /var/lib/rpm \
  -V --nodeps fcitx-vinpst
unshare -Ur rpm --root "${rpm_root}" --dbpath /var/lib/rpm \
  --nodeps --noscripts --nosignature -U "${upgrade_rpm}"
test "$(unshare -Ur rpm --root "${rpm_root}" --dbpath /var/lib/rpm -q \
  --qf '%{VERSION}-%{RELEASE}' fcitx-vinpst)" = "${version}-2"
unshare -Ur rpm --root "${rpm_root}" --dbpath /var/lib/rpm \
  -V --nodeps fcitx-vinpst
test "$(sha256sum "${rpm_root}/home/test/.config/fcitx-vinpst/config.json" | awk '{print $1}')" = \
  "${config_sha256}"
unshare -Ur rpm --root "${rpm_root}" --dbpath /var/lib/rpm \
  --nodeps --noscripts -e fcitx-vinpst
if unshare -Ur rpm --root "${rpm_root}" --dbpath /var/lib/rpm \
  -q fcitx-vinpst >/dev/null 2>&1; then
  echo "RPM remained registered after removal" >&2
  exit 1
fi
test "$(sha256sum "${rpm_root}/home/test/.config/fcitx-vinpst/config.json" | awk '{print $1}')" = \
  "${config_sha256}"
for relative in "${required_files[@]}"; do
  test ! -e "${rpm_root}/${relative}"
done

echo "RPM package build and transaction smoke passed: ${initial_rpm}"
