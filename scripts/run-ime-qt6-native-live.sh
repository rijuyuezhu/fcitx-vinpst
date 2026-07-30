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

out_dir="${VINPUT_LIVE_TOOLKIT_OUT_DIR:-target/tmp/ime-qt6-native-live}"
binary="${out_dir}/qt6-live-toolkit-probe"
log="${out_dir}/${mode}.jsonl"
wav="${VINPUT_LIVE_TOOLKIT_WAV:-}"

command -v c++ >/dev/null 2>&1 || {
  echo "c++ is required to build the Qt6 live probe" >&2
  exit 1
}
pkg-config --exists Qt6Widgets || {
  echo "Qt6 Widgets development files are required (pkg-config Qt6Widgets)" >&2
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
read -r -a qt_cflags <<<"$(pkg-config --cflags Qt6Widgets)"
read -r -a qt_libs <<<"$(pkg-config --libs Qt6Widgets)"
c++ -std=c++20 -fPIC -Wall -Wextra -Werror scripts/qt6-live-toolkit-probe.cpp \
  -o "${binary}" "${qt_cflags[@]}" "${qt_libs[@]}"

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

echo "Qt6 live probe (${mode})" >&2
echo "Use the real Fcitx shortcut in the focused field; no Qt key events are synthesized." >&2
if [[ -n "${wav}" ]]; then
  echo "The WAV starts automatically after the daemon enters recording; press the shortcut again after playback." >&2
fi

set +e
QT_IM_MODULE=fcitx "${binary}" "${mode}" | tee "${log}" &
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
