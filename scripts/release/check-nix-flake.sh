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
import base64
import binascii
import json
import re
from pathlib import Path

lock = json.loads(Path("flake.lock").read_text(encoding="utf-8"))
if lock.get("version") != 7:
    raise SystemExit("unsupported flake.lock schema")

nodes = lock.get("nodes")
root_name = lock.get("root")
if not isinstance(nodes, dict) or not nodes:
    raise SystemExit("flake.lock nodes must be a non-empty object")
if not isinstance(root_name, str) or root_name not in nodes:
    raise SystemExit("flake.lock root must reference an existing node")

root_node = nodes[root_name]
if not isinstance(root_node, dict):
    raise SystemExit("flake.lock root node must be an object")
root_inputs = root_node.get("inputs")
if not isinstance(root_inputs, dict) or not root_inputs:
    raise SystemExit("flake.lock root must declare inputs")


def resolve_reference(reference, resolving):
    if isinstance(reference, str):
        if reference not in nodes:
            raise SystemExit(f"flake.lock references missing node {reference}")
        return reference
    if not (
        isinstance(reference, list)
        and reference
        and all(isinstance(part, str) and part for part in reference)
    ):
        raise SystemExit("flake.lock contains an invalid input reference")

    path = tuple(reference)
    if path in resolving:
        raise SystemExit(f"flake.lock contains a cyclic follows path: {'/'.join(path)}")
    resolving.add(path)
    current = root_name
    for part in path:
        inputs = nodes[current].get("inputs", {})
        if not isinstance(inputs, dict) or part not in inputs:
            raise SystemExit(f"flake.lock follows path does not exist: {'/'.join(path)}")
        current = resolve_reference(inputs[part], resolving)
    resolving.remove(path)
    return current


def validate_nar_hash(name, value):
    prefix = "sha256-"
    if not isinstance(value, str) or not value.startswith(prefix):
        raise SystemExit(f"flake.lock input {name} has an invalid narHash")
    try:
        digest = base64.b64decode(value[len(prefix):], validate=True)
    except (binascii.Error, ValueError):
        raise SystemExit(f"flake.lock input {name} has an invalid narHash") from None
    if len(digest) != 32:
        raise SystemExit(f"flake.lock input {name} has an invalid narHash")

for name, node in nodes.items():
    if not isinstance(node, dict):
        raise SystemExit(f"flake.lock node {name} must be an object")
    inputs = node.get("inputs", {})
    if not isinstance(inputs, dict):
        raise SystemExit(f"flake.lock node {name} inputs must be an object")
    for input_name, reference in inputs.items():
        if not isinstance(input_name, str) or not input_name:
            raise SystemExit(f"flake.lock node {name} contains an invalid input name")
        try:
            resolve_reference(reference, set())
        except SystemExit as error:
            raise SystemExit(f"flake.lock node {name} input {input_name}: {error}") from None

    if name == root_name:
        continue
    locked = node.get("locked")
    if not isinstance(locked, dict):
        raise SystemExit(f"flake.lock input {name} is not pinned")
    input_type = locked.get("type")
    if not isinstance(input_type, str) or not input_type:
        raise SystemExit(f"flake.lock input {name} missing type")
    validate_nar_hash(name, locked.get("narHash"))

    if input_type == "github":
        for field in ("owner", "repo"):
            value = locked.get(field)
            if not isinstance(value, str) or not value:
                raise SystemExit(f"flake.lock GitHub input {name} missing {field}")
        revision = locked.get("rev")
        if not isinstance(revision, str) or re.fullmatch(r"[0-9a-f]{40}", revision) is None:
            raise SystemExit(f"flake.lock GitHub input {name} has an invalid rev")
        original = node.get("original")
        original_ref = original.get("ref") if isinstance(original, dict) else None
        if isinstance(original_ref, str) and original_ref.lower() in {"main", "master"}:
            raise SystemExit(f"flake.lock GitHub input {name} tracks a default branch")
PY
echo "Nix flake metadata check passed"
