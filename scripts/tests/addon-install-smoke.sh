#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
cd "${repo_root}"

build_dir=target/cpp/fcitx5-addon-fcitx
stage=target/tmp/fcitx-addon-install-smoke
no_systemd_build=target/cpp/fcitx5-addon-no-systemd
no_systemd_stage=target/tmp/fcitx-addon-no-systemd

cmake -S cpp/fcitx5-addon -B "${build_dir}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DCMAKE_EXPORT_COMPILE_COMMANDS=ON \
  -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON
cmake --build "${build_dir}" --parallel

rm -rf "${stage}"
DESTDIR="${PWD}/${stage}" cmake --install "${build_dir}"
test -f "${stage}/usr/local/lib/fcitx5/fcitx5-vinput.so"
test -f "${stage}/usr/local/share/fcitx5/addon/vinput.conf"
test -f "${stage}/usr/local/share/locale/zh_CN/LC_MESSAGES/fcitx5-vinput.mo"
grep -qx 'Library=fcitx5-vinput' "${stage}/usr/local/share/fcitx5/addon/vinput.conf"
grep -qx 'Type=SharedLibrary' "${stage}/usr/local/share/fcitx5/addon/vinput.conf"
grep -qx 'OnDemand=False' "${stage}/usr/local/share/fcitx5/addon/vinput.conf"
grep -qx 'Configurable=True' "${stage}/usr/local/share/fcitx5/addon/vinput.conf"
grep -qx '0=dbus' "${stage}/usr/local/share/fcitx5/addon/vinput.conf"
grep -qx '1=clipboard' "${stage}/usr/local/share/fcitx5/addon/vinput.conf"
! grep -qE '^(Name|Comment)\[' "${stage}/usr/local/share/fcitx5/addon/vinput.conf"
test -f "${stage}/usr/local/share/dbus-1/services/org.fcitx.Vinput.service"
grep -qx 'Name=org.fcitx.Vinput' "${stage}/usr/local/share/dbus-1/services/org.fcitx.Vinput.service"
grep -qx 'Exec=/usr/local/bin/vinput-daemon --dbus --exit-when-executable-replaced' "${stage}/usr/local/share/dbus-1/services/org.fcitx.Vinput.service"
grep -qx 'SystemdService=vinput-daemon.service' "${stage}/usr/local/share/dbus-1/services/org.fcitx.Vinput.service"
test -f "${stage}/usr/lib/systemd/user/vinput-daemon.service"
grep -qx 'Type=dbus' "${stage}/usr/lib/systemd/user/vinput-daemon.service"
grep -qx 'BusName=org.fcitx.Vinput' "${stage}/usr/lib/systemd/user/vinput-daemon.service"
grep -qx 'ExecStart=/usr/local/bin/vinput-daemon --dbus --exit-when-executable-replaced' "${stage}/usr/lib/systemd/user/vinput-daemon.service"
grep -qx 'Restart=on-failure' "${stage}/usr/lib/systemd/user/vinput-daemon.service"

rm -rf "${no_systemd_build}" "${no_systemd_stage}"
cmake -S cpp/fcitx5-addon -B "${no_systemd_build}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON \
  -DVINPUT_FCITX_BRIDGE_INSTALL_SYSTEMD_SERVICE=OFF
cmake --build "${no_systemd_build}" --target fcitx5_vinput_addon --parallel
DESTDIR="${PWD}/${no_systemd_stage}" cmake --install "${no_systemd_build}"
! test -e "${no_systemd_stage}/usr/lib/systemd/user/vinput-daemon.service"
! grep -q '^SystemdService=' "${no_systemd_stage}/usr/local/share/dbus-1/services/org.fcitx.Vinput.service"
