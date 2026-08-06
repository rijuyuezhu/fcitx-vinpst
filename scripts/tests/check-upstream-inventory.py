#!/usr/bin/env python3
"""Validate the checked upstream inventory and file-level annotation coverage."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]
INVENTORY_PATH = REPO_ROOT / "docs/legacy/upstream-source-inventory.json"
ANNOTATIONS_PATH = REPO_ROOT / "docs/legacy/source-annotations.md"
SOURCE_ROW = re.compile(r"\| `([^`]+\.(?:c|cc|cpp|cxx|h|hh|hpp|hxx))` \|")
CALLABLE_KINDS = {"function", "prototype", "signal", "slot"}
SHA256 = re.compile(r"[0-9a-f]{64}")


def fail(message: str) -> None:
    raise SystemExit(f"upstream inventory check failed: {message}")


def expect_type(value: Any, expected: type, description: str) -> Any:
    if not isinstance(value, expected):
        fail(f"{description} must be {expected.__name__}")
    return value


def main() -> int:
    inventory = json.loads(INVENTORY_PATH.read_text(encoding="utf-8"))
    expect_type(inventory, dict, "inventory")
    if inventory.get("format_version") != 1:
        fail("unsupported format_version")
    commit = inventory.get("upstream_commit")
    if not isinstance(commit, str) or not re.fullmatch(r"[0-9a-f]{40}", commit):
        fail("upstream_commit must be a full Git commit")

    files = expect_type(inventory.get("files"), list, "files")
    paths: list[str] = []
    total_lines = 0
    callable_count = 0
    for index, file_entry in enumerate(files):
        file_entry = expect_type(file_entry, dict, f"files[{index}]")
        path = file_entry.get("path")
        if (
            not isinstance(path, str)
            or path.startswith("/")
            or ".." in Path(path).parts
        ):
            fail(f"files[{index}].path must be a safe relative path")
        if path in paths:
            fail(f"duplicate source path: {path}")
        paths.append(path)
        lines = file_entry.get("lines")
        if not isinstance(lines, int) or lines <= 0:
            fail(f"invalid line count for {path}")
        total_lines += lines
        digest = file_entry.get("sha256")
        if not isinstance(digest, str) or not SHA256.fullmatch(digest):
            fail(f"invalid SHA-256 for {path}")
        callables = expect_type(
            file_entry.get("callables"), list, f"callables for {path}"
        )
        last_key: tuple[Any, ...] | None = None
        for callable_entry in callables:
            callable_entry = expect_type(callable_entry, dict, f"callable in {path}")
            kind = callable_entry.get("kind")
            name = callable_entry.get("name")
            line = callable_entry.get("line")
            if kind not in CALLABLE_KINDS:
                fail(f"unexpected callable kind in {path}: {kind}")
            if not isinstance(name, str) or not name:
                fail(f"callable name is missing in {path}")
            if not isinstance(line, int) or not 1 <= line <= lines:
                fail(f"callable line is out of range in {path}: {line}")
            key = (
                line,
                kind,
                callable_entry.get("scope", ""),
                name,
                callable_entry.get("signature", ""),
            )
            if last_key is not None and key < last_key:
                fail(f"callables are not sorted in {path}")
            last_key = key
        callable_count += len(callables)

    if paths != sorted(paths):
        fail("source paths are not sorted")
    if inventory.get("file_count") != len(files):
        fail("file_count does not match files")
    if inventory.get("total_lines") != total_lines:
        fail("total_lines does not match files")
    if inventory.get("callable_count") != callable_count:
        fail("callable_count does not match files")

    annotation_text = ANNOTATIONS_PATH.read_text(encoding="utf-8")
    annotated_paths = SOURCE_ROW.findall(annotation_text)
    if len(annotated_paths) != len(set(annotated_paths)):
        fail("source annotations contain duplicate paths")
    inventory_paths = set(paths)
    annotation_paths = set(annotated_paths)
    missing = sorted(inventory_paths - annotation_paths)
    stale = sorted(annotation_paths - inventory_paths)
    if missing or stale:
        fail(f"source annotation coverage mismatch: missing={missing}, stale={stale}")

    print(
        "Upstream inventory check passed: "
        f"{len(files)} files, {total_lines} lines, {callable_count} callable entries"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
