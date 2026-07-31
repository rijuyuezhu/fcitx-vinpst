#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

build_dir="target/cpp/fcitx5-ime-pipewire-live"
stage_dir="target/tmp/fcitx-ime-pipewire-live-smoke"
stage_abs="${repo_root}/${stage_dir}"
daemon_path="${stage_abs}/usr/local/bin/vinput-daemon"
smoke_bin="${repo_root}/${build_dir}/vinput_fcitx_bridge_dbus_smoke"
addon_smoke_bin="${repo_root}/${build_dir}/vinput_fcitx_addon_dbus_smoke"
service_file="${stage_abs}/usr/local/share/dbus-1/services/org.fcitx.Vinput.service"
record_ms="${VINPUT_DBUS_SMOKE_RECORD_MS:-100}"

rm -rf "${build_dir}" "${stage_dir}"
cargo build -q -p vinput-daemon --features pipewire-backend
install -Dm755 target/debug/vinput-daemon "${daemon_path}"

cmake -S cpp/fcitx5-addon -B "${build_dir}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DVINPUT_FCITX_BRIDGE_INSTALL_SYSTEMD_SERVICE=OFF \
  -DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON \
  -DVINPUT_DAEMON_EXECUTABLE="${daemon_path}" \
  -DVINPUT_DAEMON_ARGS="--dbus --audio-backend pipewire"
cmake --build "${build_dir}" --target fcitx5_vinput_addon --parallel
cmake --build "${build_dir}" --target vinput_fcitx_bridge_dbus_smoke --parallel
cmake --build "${build_dir}" --target vinput_fcitx_addon_dbus_smoke --parallel
DESTDIR="${stage_abs}" cmake --install "${build_dir}"

test -x "${daemon_path}"
test -f "${stage_abs}/usr/local/lib/fcitx5/fcitx5-vinput.so"
test -f "${stage_abs}/usr/local/share/fcitx5/addon/vinput.conf"
grep -qx "Name=org.fcitx.Vinput" "${service_file}"
! grep -q '^SystemdService=' "${service_file}"
grep -qx "Exec=${daemon_path} --dbus --audio-backend pipewire --exit-when-executable-replaced" "${service_file}"

echo "PipeWire audio diagnostics from staged daemon:"
"${daemon_path}" audio-devices

XDG_DATA_DIRS="${stage_abs}/usr/local/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
VINPUT_DBUS_SMOKE_RECORD_MS="${record_ms}" \
  timeout 20s dbus-run-session -- bash -euo pipefail -c '"$1"; "$2"' \
    bash "${smoke_bin}" "${addon_smoke_bin}"
