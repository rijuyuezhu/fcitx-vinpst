#!/usr/bin/env python3
"""Validate exhaustive, non-overlapping upstream/current parity ownership."""

from __future__ import annotations

import json
import sys
from collections import Counter
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]
LEDGER_PATH = REPO_ROOT / "docs/migration/module-parity-ledger.json"
SOURCE_SUFFIXES = {".c", ".cc", ".cpp", ".cxx", ".h", ".hh", ".hpp", ".hxx"}
ALLOWED_STATES = {
    "audited-aligned",
    "audited-fixed",
    "audited-intentional-divergence",
    "audited-with-gap",
    "active",
}


def fail(message: str) -> None:
    raise SystemExit(f"module parity ledger check failed: {message}")


def expect_list(value: Any, description: str) -> list[Any]:
    if not isinstance(value, list):
        fail(f"{description} must be a list")
    return value


def safe_relative_path(value: Any, description: str) -> str:
    if not isinstance(value, str) or not value:
        fail(f"{description} must be a non-empty string")
    path = Path(value)
    if path.is_absolute() or ".." in path.parts:
        fail(f"{description} must be a safe relative path: {value!r}")
    return value


def current_sources() -> list[str]:
    paths: list[str] = []
    for path in REPO_ROOT.glob("crates/*/src/**/*.rs"):
        rel = path.relative_to(REPO_ROOT)
        if "tests" in rel.parts:
            continue
        if (
            path.name == "tests.rs"
            or path.name.endswith("_tests.rs")
            or path.name == "test_support.rs"
        ):
            continue
        paths.append(rel.as_posix())
    cpp_root = REPO_ROOT / "cpp/fcitx5-addon/src"
    for path in cpp_root.glob("*"):
        if path.is_file() and path.suffix in SOURCE_SUFFIXES:
            paths.append(path.relative_to(REPO_ROOT).as_posix())
    return sorted(paths)


def assert_exact_ownership(
    expected: list[str],
    audited: list[str],
    pending: list[str],
    side: str,
) -> None:
    claims = Counter(audited + pending)
    duplicates = sorted(path for path, count in claims.items() if count != 1)
    expected_set = set(expected)
    claimed_set = set(claims)
    missing = sorted(expected_set - claimed_set)
    stale = sorted(claimed_set - expected_set)
    if duplicates or missing or stale:
        fail(
            f"{side} ownership mismatch: duplicates={duplicates}, missing={missing}, stale={stale}"
        )


def main() -> int:
    ledger = json.loads(LEDGER_PATH.read_text(encoding="utf-8"))
    if not isinstance(ledger, dict) or ledger.get("format_version") != 1:
        fail("unsupported ledger format")

    inventory_path = REPO_ROOT / safe_relative_path(
        ledger.get("upstream_inventory"), "upstream_inventory"
    )
    inventory = json.loads(inventory_path.read_text(encoding="utf-8"))
    if ledger.get("upstream_commit") != inventory.get("upstream_commit"):
        fail("ledger upstream_commit differs from frozen inventory")
    upstream_expected = [entry["path"] for entry in inventory["files"]]
    current_expected = current_sources()

    audits = expect_list(ledger.get("audits"), "audits")
    ids: list[str] = []
    audited_upstream: list[str] = []
    audited_current: list[str] = []
    for index, unit in enumerate(audits):
        if not isinstance(unit, dict):
            fail(f"audits[{index}] must be an object")
        unit_id = unit.get("id")
        if not isinstance(unit_id, str) or not unit_id:
            fail(f"audits[{index}].id must be non-empty")
        ids.append(unit_id)
        state = unit.get("state")
        if state not in ALLOWED_STATES:
            fail(f"audit {unit_id} has unsupported state {state!r}")
        for key, destination in (
            ("upstream_files", audited_upstream),
            ("current_files", audited_current),
        ):
            values = [
                safe_relative_path(value, f"audit {unit_id} {key}")
                for value in expect_list(unit.get(key), f"audit {unit_id} {key}")
            ]
            if values != sorted(values):
                fail(f"audit {unit_id} {key} must be sorted")
            destination.extend(values)
        supporting = [
            safe_relative_path(value, f"audit {unit_id} supporting_current_files")
            for value in expect_list(
                unit.get("supporting_current_files", []),
                f"audit {unit_id} supporting_current_files",
            )
        ]
        if supporting != sorted(supporting):
            fail(f"audit {unit_id} supporting_current_files must be sorted")
        missing_support = sorted(
            value for value in supporting if not (REPO_ROOT / value).exists()
        )
        if missing_support:
            fail(f"audit {unit_id} has missing supporting paths: {missing_support}")
        notes = expect_list(unit.get("notes"), f"audit {unit_id} notes")
        if not all(isinstance(note, str) and note for note in notes):
            fail(f"audit {unit_id} notes must contain non-empty strings")

    if len(ids) != len(set(ids)):
        fail("audit ids must be unique")

    upstream_pending = [
        safe_relative_path(value, "upstream_pending")
        for value in expect_list(ledger.get("upstream_pending"), "upstream_pending")
    ]
    current_pending = [
        safe_relative_path(value, "current_pending")
        for value in expect_list(ledger.get("current_pending"), "current_pending")
    ]
    if upstream_pending != sorted(upstream_pending):
        fail("upstream_pending must be sorted")
    if current_pending != sorted(current_pending):
        fail("current_pending must be sorted")

    assert_exact_ownership(
        upstream_expected, audited_upstream, upstream_pending, "upstream"
    )
    assert_exact_ownership(
        current_expected, audited_current, current_pending, "current"
    )

    print(
        "Module parity ledger check passed: "
        f"upstream {len(audited_upstream)} audited/{len(upstream_pending)} pending, "
        f"current {len(audited_current)} audited/{len(current_pending)} pending, "
        f"{len(audits)} audit units"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
