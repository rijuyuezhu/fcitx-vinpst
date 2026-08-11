#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
cd "${repo_root}"

mapfile -t shell_scripts < <(find scripts -type f -name '*.sh' -print | sort)
for script in "${shell_scripts[@]}"; do
  bash -n "${script}"
done

command -v shellcheck >/dev/null 2>&1 || {
  echo "missing required script lint tool: shellcheck" >&2
  exit 1
}
shellcheck -S warning "${shell_scripts[@]}"

mapfile -t python_scripts < <(find scripts -type f -name '*.py' -print | sort)
pycache_root="${repo_root}/target/tmp/scripts-lint-pycache"
rm -rf "${pycache_root}"
PYTHONPYCACHEPREFIX="${pycache_root}" python3 -m py_compile "${python_scripts[@]}"
ruff check "${python_scripts[@]}"
ruff format --check "${python_scripts[@]}"
scripts/tests/source-layout-check.sh
