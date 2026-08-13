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

install_script="packaging/arch/fcitx-vinpst.install"
test -f "${install_script}"
bash -n "${install_script}"

check_root="$(mktemp -d)"
trap 'rm -rf "${check_root}"' EXIT

run_hook() {
  local hook="$1"
  PATH=/definitely/missing /bin/bash -euo pipefail -c '
    source "$1"
    declare -F "$2" >/dev/null
    "$2" 0.1.0-1 0.1.0-0
  ' arch-install-hook "${install_script}" "${hook}"
}

for hook in post_install post_upgrade post_remove; do
  stderr_path="${check_root}/${hook}.stderr"
  output="$(run_hook "${hook}" 2>"${stderr_path}")"
  test ! -s "${stderr_path}"
  [[ -n "${output//[[:space:]]/}" ]]
  mapfile -t lines <<<"${output}"
  if ((${#lines[@]} > 3)); then
    echo "${hook} package guidance is unexpectedly verbose" >&2
    exit 1
  fi
done

if PATH=/definitely/missing /bin/bash -euo pipefail -c '
  source "$1"
  declare -F pre_remove >/dev/null
' arch-install-hook "${install_script}"; then
  echo "Arch package must not mutate runtime state during pre-remove" >&2
  exit 1
fi

echo "Arch install script check passed"
