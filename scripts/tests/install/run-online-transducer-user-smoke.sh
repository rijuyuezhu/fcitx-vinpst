#!/usr/bin/env bash
set -euo pipefail

scenario="${1:?usage: run-online-transducer-user-smoke.sh <activation|frontend|addon|command-addon>}"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"
cd "${repo_root}"

model=target/models/onnx-zf-en-20m-stream
wav="${model}/test_wavs/0.wav"
expected='THE YELLOW LAMPS WOULD LIGHT UP HERE AND THERE THE SQUALID QUARTER OF THE BRAFFLEL'
frontend=()
selected=()

case "${scenario}" in
  activation)
    ;;
  frontend)
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon \
      -DCMAKE_BUILD_TYPE=Debug \
      -DVINPST_FCITX_BRIDGE_ENABLE_FCITX_DEPS=OFF
    cmake --build target/cpp/fcitx5-addon \
      --target vinpst_fcitx_bridge_native_dbus_smoke --parallel
    frontend=(VINPST_NATIVE_ACTIVATION_FRONTEND_BIN=target/cpp/fcitx5-addon/vinpst_fcitx_bridge_native_dbus_smoke)
    ;;
  addon|command-addon)
    cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon \
      -DCMAKE_BUILD_TYPE=Debug \
      -DVINPST_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON
    cmake --build target/cpp/fcitx5-addon \
      --target vinpst_fcitx_native_addon_dbus_smoke --parallel
    frontend=(VINPST_NATIVE_ACTIVATION_FRONTEND_BIN=target/cpp/fcitx5-addon/vinpst_fcitx_native_addon_dbus_smoke)
    if [[ "${scenario}" == command-addon ]]; then
      selected=(VINPST_NATIVE_ADDON_SELECTED_TEXT='replace this text'
                VINPST_NATIVE_ADDON_EXPECT_CANDIDATE_MENU=1)
    fi
    ;;
  *)
    echo "unknown online-transducer user scenario: ${scenario}" >&2
    exit 2
    ;;
esac

env \
  "${frontend[@]}" \
  "${selected[@]}" \
  VINPST_SHERPA_MODEL="${model}" \
  VINPST_SHERPA_WAV="${wav}" \
  VINPST_SHERPA_EXPECT_TEXT="${expected}" \
  scripts/tests/install/run-user-ime-sherpa-native-activation-smoke.sh
