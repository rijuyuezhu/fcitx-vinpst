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
source scripts/tests/dbus-session-common.sh

build_dir="target/cpp/fcitx5-addon-dbus-configured-activation"
stage_dir="target/tmp/vinpst-cpp-dbus-configured-activation-smoke"
stage_abs="${repo_root}/${stage_dir}"
xdg_config_home="${stage_abs}/xdg-config-home"
xdg_data_home="${stage_abs}/xdg-data-home"
bus_config="${stage_abs}/session.conf"
daemon_path="${repo_root}/target/debug/vinpst-daemon"
source_config_path="${repo_root}/data/e2e-command-demo-config.json"
config_path="${repo_root}/target/tmp/vinpst-cpp-dbus-configured-activation-config.json"
wav_path="${repo_root}/target/tmp/vinpst-cpp-dbus-configured-activation-demo.wav"
smoke_bin="${repo_root}/${build_dir}/vinpst_fcitx_bridge_dbus_smoke"
addon_smoke_bin="${repo_root}/${build_dir}/vinpst_fcitx_addon_dbus_smoke"
service_file="${stage_abs}/usr/local/share/dbus-1/services/org.fcitx.Vinpst.service"

cargo build -q -p vinpst-daemon
install -Dm644 "${source_config_path}" "${config_path}"
python3 scripts/fixtures/write-demo-wav.py "${wav_path}"
cmake -S cpp/fcitx5-addon -B "${build_dir}" \
  -DCMAKE_BUILD_TYPE=Debug \
  -DVINPST_FCITX_BRIDGE_INSTALL_SYSTEMD_SERVICE=OFF \
  -DVINPST_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON \
  -DVINPST_DAEMON_EXECUTABLE="${daemon_path}" \
  -DVINPST_DAEMON_ARGS="--dbus --configured-backends --config ${config_path} --wav ${wav_path}"
cmake --build "${build_dir}" --target fcitx5_vinpst_addon --parallel
cmake --build "${build_dir}" --target vinpst_fcitx_bridge_dbus_smoke --parallel
cmake --build "${build_dir}" --target vinpst_fcitx_addon_dbus_smoke --parallel
rm -rf "${stage_dir}"
DESTDIR="${stage_abs}" cmake --install "${build_dir}"
mkdir -p "${xdg_config_home}" "${xdg_data_home}"
write_isolated_dbus_session_config "${bus_config}" "$(dirname "${service_file}")"

grep -qx "Name=org.fcitx.Vinpst" "${service_file}"
! grep -q '^SystemdService=' "${service_file}"
grep -qx "Exec=${daemon_path} --dbus --configured-backends --config ${config_path} --wav ${wav_path} --exit-when-executable-replaced" "${service_file}"

XDG_CONFIG_HOME="${xdg_config_home}" \
XDG_DATA_HOME="${xdg_data_home}" \
XDG_DATA_DIRS="${stage_abs}/usr/local/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
VINPST_DBUS_SMOKE_EXPECTED_NORMAL="demo final: demo heard 16000 bytes" \
VINPST_DBUS_SMOKE_EXPECTED_COMMAND="demo final: demo heard 16000 bytes" \
VINPST_DBUS_SMOKE_EXPECTED_TAKEOVER="demo final: demo heard 16000 bytes" \
VINPST_DBUS_SMOKE_EXPECTED_ACTIVE_SCENE="demo-postprocess" \
VINPST_DBUS_SMOKE_EXPECT_SCENE_PERSISTED="1" \
  timeout 20s dbus-run-session --config-file="${bus_config}" -- bash -euo pipefail -c '
    stop_activation_owner() {
      local owner_pid
      owner_pid=$(busctl --user call org.freedesktop.DBus /org/freedesktop/DBus \
        org.freedesktop.DBus GetConnectionUnixProcessID s org.fcitx.Vinpst \
        2>/dev/null | awk "{print \$2}") || return 0
      kill "${owner_pid}" 2>/dev/null || true
    }
    trap stop_activation_owner EXIT

    busctl --user call org.fcitx.Vinpst /org/fcitx/Vinpst \
      org.fcitx.Vinpst.Service GetStatus >/dev/null
    owner_pid=$(busctl --user call org.freedesktop.DBus /org/freedesktop/DBus \
      org.freedesktop.DBus GetConnectionUnixProcessID s org.fcitx.Vinpst | \
      awk "{print \$2}")
    owner_executable=$(readlink -f "/proc/${owner_pid}/exe")
    expected_executable=$(readlink -f "${3}")
    if [[ "${owner_executable}" != "${expected_executable}" ]]; then
      echo "activation selected unexpected daemon: ${owner_executable}" >&2
      exit 1
    fi

    "${1}"
    "${2}"
  ' bash "${smoke_bin}" "${addon_smoke_bin}" "${daemon_path}"
