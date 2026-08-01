#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="${script_dir}"
while [[ ! -f "${repo_root}/Cargo.toml" || ! -d "${repo_root}/scripts" ]]; do
  parent="$(dirname "${repo_root}")"
  if [[ "${parent}" == "${repo_root}" ]]; then
    echo "cannot locate repository root from ${script_dir}" >&2
    exit 1
  fi
  repo_root="${parent}"
done
cd "${repo_root}"

install_script="packaging/arch/fcitx-vinput-rs.install"
test -f "${install_script}"
bash -n "${install_script}"

run_hook() {
  local hook="$1"
  VINPUT_PACKAGE_UPGRADE_HANDOFF="${VINPUT_PACKAGE_UPGRADE_HANDOFF:-}" \
    VINPUT_PACKAGE_REMOVE_HANDOFF="${VINPUT_PACKAGE_REMOVE_HANDOFF:-}" \
    PATH=/definitely/missing /bin/bash -euo pipefail -c \
    'source "$1"; "$2" 0.1.0-1 0.1.0-0' \
    arch-install-hook "${install_script}" "${hook}"
}

post_install_output="$(run_hook post_install)"
grep -qx ':: fcitx-vinput-rs installed.' <<<"${post_install_output}"
grep -qx '   systemctl --user enable --now vinput-daemon.service' \
  <<<"${post_install_output}"
grep -qx '   fcitx5 -r' <<<"${post_install_output}"

helper_dir="$(mktemp -d)"
trap 'rm -rf "${helper_dir}"' EXIT
cat >"${helper_dir}/upgrade-helper" <<'SH'
#!/bin/bash
printf '%s\n' 'guarded upgrade helper invoked'
SH
cat >"${helper_dir}/upgrade-helper-failure" <<'SH'
#!/bin/bash
printf '%s\n' 'guarded upgrade helper failed' >&2
exit 23
SH
cat >"${helper_dir}/remove-helper" <<'SH'
#!/bin/bash
printf '%s\n' 'guarded removal helper invoked'
SH
chmod +x \
  "${helper_dir}/upgrade-helper" \
  "${helper_dir}/upgrade-helper-failure" \
  "${helper_dir}/remove-helper"

VINPUT_PACKAGE_UPGRADE_HANDOFF="${helper_dir}/upgrade-helper"
post_upgrade_output="$(run_hook post_upgrade)"
grep -qx 'guarded upgrade helper invoked' <<<"${post_upgrade_output}"
grep -qx ':: fcitx-vinput-rs upgraded.' <<<"${post_upgrade_output}"
grep -qx \
  ':: Live user sessions with an existing daemon owner were checked automatically.' \
  <<<"${post_upgrade_output}"
grep -qx \
  ':: Current owners are unchanged; stale owners use the guarded daemon handoff.' \
  <<<"${post_upgrade_output}"
grep -qx \
  ':: If a session was unavailable, that desktop user can retry:' \
  <<<"${post_upgrade_output}"
grep -qx '   vinput daemon handoff' <<<"${post_upgrade_output}"
grep -qx '   fcitx5 -r' <<<"${post_upgrade_output}"

VINPUT_PACKAGE_UPGRADE_HANDOFF="${helper_dir}/upgrade-helper-failure"
if run_hook post_upgrade \
  >"${helper_dir}/upgrade-failure.stdout" \
  2>"${helper_dir}/upgrade-failure.stderr"; then
  echo "failing upgrade helper unexpectedly succeeded" >&2
  exit 1
fi
grep -Fq 'guarded upgrade helper failed' \
  "${helper_dir}/upgrade-failure.stderr"
grep -Fq \
  'Automatic vinput daemon handoff failed for at least one live session.' \
  "${helper_dir}/upgrade-failure.stderr"

VINPUT_PACKAGE_UPGRADE_HANDOFF="${helper_dir}/upgrade-helper"
VINPUT_PACKAGE_REMOVE_HANDOFF="${helper_dir}/remove-helper"
pre_remove_output="$(run_hook pre_remove)"
grep -qx 'guarded removal helper invoked' <<<"${pre_remove_output}"

post_remove_output="$(run_hook post_remove)"
grep -qx ':: fcitx-vinput-rs removed.' <<<"${post_remove_output}"
grep -qx ':: Active daemon owners were stopped by the guarded pre-remove handoff.' \
  <<<"${post_remove_output}"
grep -qx ':: User config, models, and cache were intentionally preserved.' \
  <<<"${post_remove_output}"
grep -qx '   fcitx5 -r' <<<"${post_remove_output}"

if grep -Eq '^[[:space:]]*(systemctl|fcitx5|vinput)[[:space:]]' "${install_script}"; then
  echo "Arch install hooks must not invoke unqualified runtime commands" >&2
  exit 1
fi

echo "Arch install script check passed"
