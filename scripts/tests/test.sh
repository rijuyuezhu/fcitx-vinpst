#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
cd "${repo_root}"

cargo test --workspace --all-targets
dbus-run-session -- cargo test -p vinpst-daemon --features dbus-integration --test dbus_integration

cmake -S cpp/fcitx5-addon -B target/cpp/fcitx5-addon \
  -DCMAKE_BUILD_TYPE=Debug \
  -DCMAKE_EXPORT_COMPILE_COMMANDS=ON \
  -DVINPST_FCITX_BRIDGE_ENABLE_FCITX_DEPS=OFF
ln -sfn target/cpp/fcitx5-addon/compile_commands.json compile_commands.json
cmake --build target/cpp/fcitx5-addon --parallel
ctest --test-dir target/cpp/fcitx5-addon --output-on-failure
