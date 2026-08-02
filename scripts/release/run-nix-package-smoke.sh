#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="${script_dir}"
while [[ ! -f "${repo_root}/Cargo.toml" || ! -f "${repo_root}/flake.nix" ]]; do
  parent="$(dirname "${repo_root}")"
  if [[ "${parent}" == "${repo_root}" ]]; then
    echo "cannot locate repository root from ${script_dir}" >&2
    exit 1
  fi
  repo_root="${parent}"
done
cd "${repo_root}"

for command in jq nix; do
  command -v "${command}" >/dev/null || {
    echo "missing Nix package smoke tool: ${command}" >&2
    exit 1
  }
done

smoke_root="${repo_root}/target/tmp/nix-package-smoke"
result_link="${smoke_root}/result"
rm -rf "${smoke_root}"
mkdir -p "${smoke_root}"

nix flake metadata --no-update-lock-file >/dev/null
nix flake check --no-update-lock-file --print-build-logs
nix build .#fcitx-vinput-rs \
  --no-update-lock-file \
  --print-build-logs \
  --out-link "${result_link}"

for executable in vinput vinput-daemon vinput-gui; do
  test -x "${result_link}/bin/${executable}"
done
"${result_link}/bin/vinput" --version
"${result_link}/bin/vinput-daemon" --version
"${result_link}/bin/vinput-gui" --check --offline \
  | jq -e '.ok and .application == "vinput-gui" and .daemon.skipped' >/dev/null

required_files=(
  lib/fcitx5/fcitx5-vinput.so
  share/applications/vinput-gui.desktop
  share/dbus-1/services/org.fcitx.Vinput.service
  share/fcitx5/addon/vinput.conf
  share/fcitx-vinput/default-config.json
  share/fcitx-vinput/vad/silero_vad.onnx
  share/icons/hicolor/256x256/apps/vinput-gui.png
  share/licenses/fcitx-vinput-rs/LICENSE
  share/locale/zh_CN/LC_MESSAGES/fcitx5-vinput.mo
  share/systemd/user/vinput-daemon.service
)
for path in "${required_files[@]}"; do
  test -f "${result_link}/${path}"
done

store_path="$(readlink -f "${result_link}")"
case "${store_path}" in
/nix/store/*-fcitx-vinput-rs-*) ;;
*)
  echo "unexpected Nix package result path: ${store_path}" >&2
  exit 1
  ;;
esac

echo "Nix package build and layout smoke passed: ${store_path}"
