#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

install_script="packaging/arch/fcitx-vinput-rs.install"
test -f "${install_script}"
bash -n "${install_script}"

run_hook() {
  local hook="$1"
  PATH=/definitely/missing /bin/bash -euo pipefail -c \
    'source "$1"; "$2" 0.1.0-1 0.1.0-0' \
    arch-install-hook "${install_script}" "${hook}"
}

post_install_output="$(run_hook post_install)"
grep -qx ':: fcitx-vinput-rs installed.' <<<"${post_install_output}"
grep -qx '   systemctl --user enable --now vinput-daemon.service' \
  <<<"${post_install_output}"
grep -qx '   fcitx5 -r' <<<"${post_install_output}"

post_upgrade_output="$(run_hook post_upgrade)"
grep -qx ':: fcitx-vinput-rs upgraded.' <<<"${post_upgrade_output}"
grep -qx ':: Daemons started by the current systemd user unit restart automatically.' \
  <<<"${post_upgrade_output}"
grep -qx ':: Older or direct-activation owners may still require:' \
  <<<"${post_upgrade_output}"
grep -qx '   vinput daemon handoff' <<<"${post_upgrade_output}"
grep -qx '   fcitx5 -r' <<<"${post_upgrade_output}"

post_remove_output="$(run_hook post_remove)"
grep -qx ':: fcitx-vinput-rs removed.' <<<"${post_remove_output}"
grep -qx ':: User config, models, and cache were intentionally preserved.' \
  <<<"${post_remove_output}"
grep -qx '   systemctl --user stop vinput-daemon.service' \
  <<<"${post_remove_output}"
grep -qx '   fcitx5 -r' <<<"${post_remove_output}"

! grep -Eq '^[[:space:]]*(systemctl|fcitx5|vinput)[[:space:]]' "${install_script}"

echo "Arch install script check passed"
