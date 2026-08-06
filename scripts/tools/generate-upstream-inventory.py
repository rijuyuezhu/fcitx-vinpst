#!/usr/bin/env python3
"""Generate the checked C++ source and callable inventory used by parity review."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

SOURCE_SUFFIXES = {".c", ".cc", ".cpp", ".cxx", ".h", ".hh", ".hpp", ".hxx"}
CALLABLE_KINDS = {"function", "prototype", "signal", "slot"}
FORMAT_VERSION = 1


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--upstream-root",
        type=Path,
        required=True,
        help="Path to a clean fcitx5-vinput checkout.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("docs/legacy/upstream-source-inventory.json"),
        help="Tracked JSON inventory path.",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Fail when the tracked output differs instead of rewriting it.",
    )
    return parser.parse_args()


def run(command: list[str], *, cwd: Path | None = None) -> str:
    completed = subprocess.run(
        command,
        cwd=cwd,
        check=True,
        text=True,
        capture_output=True,
    )
    return completed.stdout.strip()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_files(source_root: Path) -> list[Path]:
    return sorted(
        path
        for path in source_root.rglob("*")
        if path.is_file() and path.suffix.lower() in SOURCE_SUFFIXES
    )


def extract_callables(
    source_root: Path, files: list[Path]
) -> dict[str, list[dict[str, Any]]]:
    ctags = shutil.which("ctags")
    if ctags is None:
        raise SystemExit(
            "Universal Ctags is required to generate the upstream inventory"
        )

    command = [
        ctags,
        "--output-format=json",
        "--fields=+neKSt",
        "--extras=-F",
        "--kinds-C++=+p",
        "--sort=no",
        "-o",
        "-",
        *(str(path) for path in files),
    ]
    completed = subprocess.run(
        command,
        check=True,
        text=True,
        capture_output=True,
    )

    result: dict[str, list[dict[str, Any]]] = {
        str(path.relative_to(source_root)): [] for path in files
    }
    for raw_line in completed.stdout.splitlines():
        tag = json.loads(raw_line)
        if tag.get("_type") != "tag" or tag.get("kind") not in CALLABLE_KINDS:
            continue
        path = Path(tag["path"])
        try:
            relative = str(path.resolve().relative_to(source_root.resolve()))
        except ValueError as error:
            raise SystemExit(
                f"ctags returned a path outside the upstream source root: {path}"
            ) from error
        entry = {
            "name": tag["name"],
            "kind": tag["kind"],
            "line": int(tag["line"]),
        }
        for field in ("end", "scope", "scopeKind", "signature", "typeref"):
            if field in tag:
                entry[field] = tag[field]
        result[relative].append(entry)

    for entries in result.values():
        entries.sort(
            key=lambda entry: (
                entry["line"],
                entry["kind"],
                entry.get("scope", ""),
                entry["name"],
                entry.get("signature", ""),
            )
        )
    return result


def build_inventory(upstream_root: Path) -> dict[str, Any]:
    upstream_root = upstream_root.resolve()
    source_root = upstream_root / "src"
    if not source_root.is_dir():
        raise SystemExit(f"upstream source directory does not exist: {source_root}")
    if run(["git", "status", "--porcelain"], cwd=upstream_root):
        raise SystemExit(
            "upstream checkout must be clean before generating the inventory"
        )

    files = source_files(source_root)
    callables = extract_callables(source_root, files)
    file_entries: list[dict[str, Any]] = []
    total_lines = 0
    callable_count = 0
    for path in files:
        relative = str(path.relative_to(source_root))
        line_count = len(
            path.read_text(encoding="utf-8", errors="replace").splitlines()
        )
        entries = callables[relative]
        total_lines += line_count
        callable_count += len(entries)
        file_entries.append(
            {
                "path": relative,
                "lines": line_count,
                "sha256": sha256(path),
                "callables": entries,
            }
        )

    return {
        "format_version": FORMAT_VERSION,
        "upstream_repository": "xifan2333/fcitx5-vinput",
        "upstream_commit": run(["git", "rev-parse", "HEAD"], cwd=upstream_root),
        "upstream_describe": run(
            ["git", "describe", "--tags", "--always"], cwd=upstream_root
        ),
        "source_root": "src",
        "file_count": len(file_entries),
        "total_lines": total_lines,
        "callable_count": callable_count,
        "callable_kinds": sorted(CALLABLE_KINDS),
        "files": file_entries,
    }


def rendered(inventory: dict[str, Any]) -> str:
    return json.dumps(inventory, ensure_ascii=False, indent=2, sort_keys=False) + "\n"


def main() -> int:
    args = parse_args()
    output = args.output.resolve()
    expected = rendered(build_inventory(args.upstream_root))
    if args.check:
        if not output.is_file():
            print(f"upstream inventory is missing: {output}", file=sys.stderr)
            return 1
        actual = output.read_text(encoding="utf-8")
        if actual != expected:
            print(
                "upstream source inventory is stale; regenerate with "
                "scripts/tools/generate-upstream-inventory.py",
                file=sys.stderr,
            )
            return 1
        print(f"Upstream source inventory is current: {output}")
        return 0

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(expected, encoding="utf-8")
    print(f"Wrote upstream source inventory: {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
