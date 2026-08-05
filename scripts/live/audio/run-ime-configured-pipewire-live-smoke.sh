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

build_dir="target/cpp/fcitx5-ime-configured-pipewire-live"
stage_dir="target/tmp/fcitx-ime-configured-pipewire-live-smoke"
stage_abs="${repo_root}/${stage_dir}"
daemon_path="${stage_abs}/usr/local/bin/vinpst-daemon"
config_path="${stage_abs}/usr/local/share/fcitx-vinpst/e2e-configured-pipewire-live.json"
smoke_bin="${repo_root}/${build_dir}/vinpst_fcitx_bridge_dbus_smoke"
addon_smoke_bin="${repo_root}/${build_dir}/vinpst_fcitx_addon_dbus_smoke"
service_file="${stage_abs}/usr/local/share/dbus-1/services/org.fcitx.Vinpst.service"
record_ms="${VINPST_DBUS_SMOKE_RECORD_MS:-100}"
expected_text="live final: live pipewire command result"

rm -rf "${build_dir}" "${stage_dir}"
cargo build -q -p vinpst-daemon --features pipewire-backend
install -Dm755 target/debug/vinpst-daemon "${daemon_path}"
install -Dm644 data/e2e-configured-pipewire-live.json "${config_path}"

cmake -S cpp/fcitx5-addon -B "${build_dir}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DVINPST_FCITX_BRIDGE_INSTALL_SYSTEMD_SERVICE=OFF \
  -DVINPST_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON \
  -DVINPST_DAEMON_EXECUTABLE="${daemon_path}" \
  -DVINPST_DAEMON_ARGS="--dbus --configured-backends --config ${config_path} --audio-backend pipewire"
cmake --build "${build_dir}" --target fcitx5_vinpst_addon --parallel
cmake --build "${build_dir}" --target vinpst_fcitx_bridge_dbus_smoke --parallel
cmake --build "${build_dir}" --target vinpst_fcitx_addon_dbus_smoke --parallel
DESTDIR="${stage_abs}" cmake --install "${build_dir}"

test -x "${daemon_path}"
test -f "${config_path}"
test -f "${stage_abs}/usr/local/lib/fcitx5/fcitx5-vinpst.so"
test -f "${stage_abs}/usr/local/share/fcitx5/addon/vinpst.conf"
grep -qx "Name=org.fcitx.Vinpst" "${service_file}"
! grep -q '^SystemdService=' "${service_file}"
grep -qx "Exec=${daemon_path} --dbus --configured-backends --config ${config_path} --audio-backend pipewire --exit-when-executable-replaced" "${service_file}"

echo "PipeWire audio diagnostics from staged configured daemon:"
"${daemon_path}" --config "${config_path}" audio-devices

XDG_DATA_DIRS="${stage_abs}/usr/local/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
VINPST_DBUS_SMOKE_RECORD_MS="${record_ms}" \
VINPST_DBUS_SMOKE_EXPECTED_NORMAL="${expected_text}" \
VINPST_DBUS_SMOKE_EXPECTED_COMMAND="${expected_text}" \
VINPST_DBUS_SMOKE_EXPECTED_ACTIVE_SCENE="live-postprocess" \
VINPST_DBUS_SMOKE_EXPECT_SCENE_PERSISTED="1" \
  timeout 20s dbus-run-session -- bash -euo pipefail -c '"$1"; "$2"' \
    bash "${smoke_bin}" "${addon_smoke_bin}"
