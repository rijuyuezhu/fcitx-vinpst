#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
cd "${repo_root}"

cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon \
  -DCMAKE_BUILD_TYPE=Debug \
  -DCMAKE_EXPORT_COMPILE_COMMANDS=ON \
  -DVINPST_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON
ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
mapfile -t addon_sources < <(find cpp/fcitx5-addon -type f -name '*.cpp' -print | sort)
clang-tidy -p target/cpp/fcitx5-addon "${addon_sources[@]}"
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p vinpst-daemon --all-targets --features dbus-integration -- -D warnings
