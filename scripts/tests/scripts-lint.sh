#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
cd "${repo_root}"

mapfile -t shell_scripts < <(find scripts -type f -name '*.sh' -print | sort)
for script in "${shell_scripts[@]}"; do
  bash -n "${script}"
done

shellcheck_bin="$(command -v shellcheck || true)"
if [[ -z "${shellcheck_bin}" && -x "${HOME}/.local/share/nvim/mason/bin/shellcheck" ]]; then
  shellcheck_bin="${HOME}/.local/share/nvim/mason/bin/shellcheck"
fi
if [[ -n "${shellcheck_bin}" ]]; then
  "${shellcheck_bin}" -S warning "${shell_scripts[@]}"
fi

mapfile -t python_scripts < <(find scripts -type f -name '*.py' -print | sort)
pycache_root="${repo_root}/target/tmp/scripts-lint-pycache"
rm -rf "${pycache_root}"
PYTHONPYCACHEPREFIX="${pycache_root}" python3 -m py_compile "${python_scripts[@]}"
ruff check "${python_scripts[@]}"
ruff format --check "${python_scripts[@]}"
scripts/tests/source-layout-check.sh
