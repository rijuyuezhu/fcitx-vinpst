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
mkdir -p target/tmp

expect_failure() {
  local expected="$1"
  shift
  local stderr_file
  stderr_file="$(mktemp target/tmp/niri-live-argument.XXXXXX)"
  if "$@" >/dev/null 2>"${stderr_file}"; then
    printf 'command unexpectedly succeeded: %q ' "$@" >&2
    printf '\n' >&2
    rm -f "${stderr_file}"
    return 1
  fi
  if ! grep -Fq -- "${expected}" "${stderr_file}"; then
    printf 'command failed without expected diagnostic: %s\n' "${expected}" >&2
    cat "${stderr_file}" >&2
    rm -f "${stderr_file}"
    return 1
  fi
  rm -f "${stderr_file}"
}

expect_failure \
  "bounded GTK4 soak cycles must be an integer from 10 to 20" \
  scripts/live/niri/run-ime-gtk4-soak-virtual-live.sh normal 9
expect_failure \
  "bounded GTK4 soak cycles must be an integer from 10 to 20" \
  scripts/live/niri/run-ime-gtk4-soak-virtual-live.sh command 21
expect_failure \
  "usage: scripts/live/niri/run-ime-gtk4-soak-virtual-live.sh [normal|command] [cycles: 10-20]" \
  scripts/live/niri/run-ime-gtk4-soak-virtual-live.sh invalid 10
expect_failure \
  "VINPST_TOOLKIT_TIMEOUT_SECONDS must be an integer from 1 to 3600" \
  env VINPST_TOOLKIT_TIMEOUT_SECONDS=0 \
  scripts/live/niri/run-ime-gtk4-native-live.sh normal
expect_failure \
  "VINPST_TOOLKIT_TIMEOUT_SECONDS must be an integer from 1 to 3600" \
  env VINPST_TOOLKIT_TIMEOUT_SECONDS=3601 \
  scripts/live/niri/run-ime-gtk4-native-live.sh command

printf 'niri live argument smoke passed\n'
