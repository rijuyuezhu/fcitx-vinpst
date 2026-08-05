#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
cd "${repo_root}"

build_dir="${VINPST_ADDON_BUILD_DIR:-target/cpp/fcitx5-addon}"
cmake_args=(
  -S cpp/fcitx5-addon
  -B "${build_dir}"
  -DCMAKE_BUILD_TYPE=Debug
  -DCMAKE_EXPORT_COMPILE_COMMANDS=ON
)
if [[ "${VINPST_ADDON_REQUIRE_FCITX:-0}" == "1" ]]; then
  cmake_args+=(-DVINPST_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON)
else
  cmake_args+=(-DVINPST_FCITX_BRIDGE_ENABLE_FCITX_DEPS=OFF)
fi
cmake "${cmake_args[@]}"
ln -sfn "${build_dir}/compile_commands.json" compile_commands.json
cmake --build "${build_dir}" --parallel
