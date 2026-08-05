#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="${script_dir}"
while [[ ! -f "${repo_root}/Cargo.toml" || ! -d "${repo_root}/packaging" ]]; do
  parent="$(dirname "${repo_root}")"
  if [[ "${parent}" == "${repo_root}" ]]; then
    echo "cannot locate repository root from ${script_dir}" >&2
    exit 1
  fi
  repo_root="${parent}"
done
cd "${repo_root}"

python3 - <<'PY'
import json
from pathlib import Path

lock = json.loads(Path("flake.lock").read_text(encoding="utf-8"))
if lock.get("version") != 7:
    raise SystemExit("unsupported flake.lock schema")
nodes = lock.get("nodes")
if not isinstance(nodes, dict) or set(nodes) != {"nixpkgs", "root", "sherpa-onnx"}:
    raise SystemExit("flake.lock node inventory mismatch")
root_inputs = nodes["root"].get("inputs")
if root_inputs != {"nixpkgs": "nixpkgs", "sherpa-onnx": "sherpa-onnx"}:
    raise SystemExit("flake.lock root inputs mismatch")
for name in ("nixpkgs", "sherpa-onnx"):
    locked = nodes[name].get("locked")
    if not isinstance(locked, dict):
        raise SystemExit(f"flake.lock missing locked input: {name}")
    for field in ("owner", "repo", "rev", "narHash", "type"):
        value = locked.get(field)
        if not isinstance(value, str) or not value:
            raise SystemExit(f"flake.lock input {name} missing {field}")
PY

test -s LICENSE
grep -q '"x86_64-linux"' flake.nix
grep -q '"aarch64-linux"' flake.nix
grep -q 'cargoLock.lockFile = ./Cargo.lock;' flake.nix
grep -q 'SHERPA_ONNX_LIB_DIR = "${sherpaRuntime}/lib";' flake.nix
grep -q 'sherpa-onnx-backend' flake.nix
grep -q 'VINPST_FCITX_MODULE_INSTALL_DIR=lib/fcitx5' flake.nix
grep -q 'VINPST_FCITX_ADDON_INSTALL_DIR=share/fcitx5/addon' flake.nix
grep -q 'VINPST_SYSTEMD_USER_UNIT_DIR=lib/systemd/user' flake.nix
grep -q 'share/licenses/fcitx-vinpst/LICENSE' flake.nix
grep -q 'license = lib.licenses.gpl3Plus;' flake.nix
grep -q 'nix flake check' packaging/nix/README.md

if grep -Eq 'github:[^";]+/(main|master)([";?]|$)' flake.lock; then
  echo "flake.lock contains an unpinned branch reference" >&2
  exit 1
fi

echo "Nix flake metadata check passed"
