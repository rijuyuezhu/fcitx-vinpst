#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
cd "${repo_root}"

production_limit=1200
test_limit=3000
failed=0

while IFS= read -r -d '' source; do
  lines="$(wc -l <"${source}")"
  limit="${production_limit}"
  case "${source}" in
    */tests/*|*/tests.rs)
      limit="${test_limit}"
      ;;
  esac
  if (( lines > limit )); then
    printf 'source layout limit exceeded: %s has %d lines (limit %d)\n' \
      "${source}" "${lines}" "${limit}" >&2
    failed=1
  fi
done < <(
  find crates cpp -type f \
    \( -name '*.rs' -o -name '*.cpp' -o -name '*.h' \) \
    -print0
)

exit "${failed}"
