#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

mode="${1:-${VINPUT_LIVE_TOOLKIT_MODE:-normal}}"
case "${mode}" in
normal | command) ;;
*)
  echo "mode must be normal or command" >&2
  exit 2
  ;;
esac

out_dir="${VINPUT_LIVE_TOOLKIT_OUT_DIR:-target/tmp/ime-gtk4-native-live}"
binary="${out_dir}/gtk4-live-toolkit-probe"
log="${out_dir}/${mode}.jsonl"
wav="${VINPUT_LIVE_TOOLKIT_WAV:-}"

command -v cc >/dev/null 2>&1 || {
  echo "cc is required to build the GTK4 live probe" >&2
  exit 1
}
pkg-config --exists gtk4 || {
  echo "GTK4 development files are required (pkg-config gtk4)" >&2
  exit 1
}
command -v fcitx5-remote >/dev/null 2>&1 || {
  echo "fcitx5-remote is required" >&2
  exit 1
}
if ! fcitx5-remote --check >/dev/null 2>&1; then
  echo "Fcitx5 is not running in this session" >&2
  exit 1
fi
if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  echo "DBUS_SESSION_BUS_ADDRESS is not set" >&2
  exit 1
fi
if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
  runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
  mapfile -t wayland_sockets < <(
    find "${runtime_dir}" -maxdepth 1 -type s -name 'wayland-*' -printf '%f\n' 2>/dev/null
  )
  if [[ "${#wayland_sockets[@]}" == 1 ]]; then
    export WAYLAND_DISPLAY="${wayland_sockets[0]}"
  fi
fi
if [[ -n "${wav}" ]]; then
  test -f "${wav}" || {
    echo "speech WAV does not exist: ${wav}" >&2
    exit 1
  }
  command -v pw-play >/dev/null 2>&1 || {
    echo "pw-play is required when VINPUT_LIVE_TOOLKIT_WAV is set" >&2
    exit 1
  }
  command -v gdbus >/dev/null 2>&1 || {
    echo "gdbus is required when VINPUT_LIVE_TOOLKIT_WAV is set" >&2
    exit 1
  }
fi

mkdir -p "${out_dir}"
read -r -a gtk_cflags <<<"$(pkg-config --cflags gtk4)"
read -r -a gtk_libs <<<"$(pkg-config --libs gtk4)"
cc -std=c11 -Wall -Wextra -Werror scripts/gtk4-live-toolkit-probe.c \
  -o "${binary}" "${gtk_cflags[@]}" "${gtk_libs[@]}"

playback_pid=""
probe_pid=""
# shellcheck disable=SC2329
cleanup() {
  if [[ -n "${playback_pid}" ]]; then
    kill "${playback_pid}" 2>/dev/null || true
    wait "${playback_pid}" 2>/dev/null || true
  fi
  if [[ -n "${probe_pid}" ]]; then
    kill "${probe_pid}" 2>/dev/null || true
    wait "${probe_pid}" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

if [[ -n "${wav}" ]]; then
  (
    for _ in $(seq 1 300); do
      status="$(gdbus call --session --dest org.fcitx.Vinput \
        --object-path /org/fcitx/Vinput \
        --method org.fcitx.Vinput.Service.GetStatus 2>/dev/null || true)"
      if [[ "${status}" == *"recording"* ]]; then
        pw-play "${wav}"
        exit 0
      fi
      sleep 0.1
    done
    echo "daemon did not enter recording before the WAV playback deadline" >&2
    exit 1
  ) &
  playback_pid=$!
fi

echo "GTK4 live probe (${mode})" >&2
echo "Use the real Fcitx shortcut in the focused field; no GDK key events are synthesized." >&2
if [[ -n "${wav}" ]]; then
  echo "The WAV starts automatically after the daemon enters recording; press the shortcut again after playback." >&2
fi

set +e
GTK_IM_MODULE=fcitx "${binary}" "${mode}" | tee "${log}" &
probe_pid=$!
wait "${probe_pid}"
status=$?
probe_pid=""
set -e

if [[ -n "${playback_pid}" ]]; then
  wait "${playback_pid}" || status=1
  playback_pid=""
fi
exit "${status}"
