#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
cd "${repo_root}"

probe_dir=scripts/live/niri/probes
out_dir=target/tmp/toolkit-probe-check
mkdir -p "${out_dir}"
read -r -a gtk3_flags <<<"$(pkg-config --cflags --libs gtk+-3.0)"
read -r -a gtk4_flags <<<"$(pkg-config --cflags --libs gtk4)"
read -r -a qt6_flags <<<"$(pkg-config --cflags --libs Qt6Widgets)"
cc -std=c11 -Wall -Wextra -Werror "${probe_dir}/gtk3-live-toolkit-probe.c" \
  -o "${out_dir}/gtk3-live-toolkit-probe" "${gtk3_flags[@]}"
cc -std=c11 -Wall -Wextra -Werror "${probe_dir}/gtk4-live-toolkit-probe.c" \
  -o "${out_dir}/gtk4-live-toolkit-probe" "${gtk4_flags[@]}"
c++ -std=c++20 -fPIC -Wall -Wextra -Werror "${probe_dir}/qt6-live-toolkit-probe.cpp" \
  -o "${out_dir}/qt6-live-toolkit-probe" "${qt6_flags[@]}"
python3 -m py_compile \
  "${probe_dir}/chromium-live-toolkit-probe.py" \
  "${probe_dir}/send-uinput-key.py"
bash -n \
  scripts/live/niri/run-ime-chromium-native-live.sh \
  scripts/live/niri/run-ime-chromium-virtual-live.sh \
  scripts/live/niri/run-ime-gtk4-native-live.sh \
  scripts/live/niri/run-ime-gnome-text-editor-live.sh \
  scripts/live/niri/run-ime-kitty-live.sh \
  scripts/live/niri/run-ime-fcitx-virtual-source-live.sh
