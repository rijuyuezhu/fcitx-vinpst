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
nix build .#fcitx-vinpst \
  --no-update-lock-file \
  --print-build-logs \
  --out-link "${result_link}"

for executable in vinpst vinpst-daemon vinpst-gui; do
  test -x "${result_link}/bin/${executable}"
done
"${result_link}/bin/vinpst" --version
"${result_link}/bin/vinpst-daemon" --version
"${result_link}/bin/vinpst-gui" --check --offline \
  | jq -e '.ok and .application == "vinpst-gui" and .daemon.skipped' >/dev/null

required_files=(
  lib/fcitx5/fcitx5-vinpst.so
  share/applications/vinpst-gui.desktop
  share/dbus-1/services/org.fcitx.Vinpst.service
  share/fcitx5/addon/vinpst.conf
  share/fcitx-vinpst/default-config.json
  share/fcitx-vinpst/vad/silero_vad.onnx
  share/icons/hicolor/256x256/apps/vinpst-gui.png
  share/licenses/fcitx-vinpst/LICENSE
  share/locale/zh_CN/LC_MESSAGES/fcitx5-vinpst.mo
  share/systemd/user/vinpst-daemon.service
)
for path in "${required_files[@]}"; do
  test -f "${result_link}/${path}"
done

store_path="$(readlink -f "${result_link}")"
case "${store_path}" in
/nix/store/*-fcitx-vinpst-*) ;;
*)
  echo "unexpected Nix package result path: ${store_path}" >&2
  exit 1
  ;;
esac

echo "Nix package build and layout smoke passed: ${store_path}"
