#!/usr/bin/env python3
"""Verify that the published Fcitx C header matches the static library ABI."""

import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

SYMBOL_PREFIX = "vinput_fcitx_"
HEADER_FUNCTION_PATTERN = re.compile(
    r"\b(?P<name>vinput_fcitx_[A-Za-z0-9_]+)\s*\(",
    re.MULTILINE,
)


def fail(message: str) -> None:
    raise RuntimeError(message)


def names(pattern: re.Pattern[str], text: str) -> set[str]:
    return {match.group("name") for match in pattern.finditer(text)}


def target_root(repo_root: Path) -> Path:
    configured = os.environ.get("CARGO_TARGET_DIR")
    if configured is None:
        return repo_root / "target"
    path = Path(configured)
    return path if path.is_absolute() else repo_root / path


def build_static_library(repo_root: Path) -> Path:
    subprocess.run(
        ["cargo", "build", "--locked", "-p", "vinput-fcitx-ffi"],
        cwd=repo_root,
        check=True,
    )
    library = target_root(repo_root) / "debug" / "libvinput_fcitx_ffi.a"
    if not library.is_file():
        fail(f"Rust FFI static library was not produced: {library}")
    return library


def binary_symbols(library: Path) -> set[str]:
    nm = shutil.which("nm")
    if nm is None:
        fail("nm is required for the Fcitx FFI ABI check")
    result = subprocess.run(
        [nm, "-g", "--defined-only", str(library)],
        check=True,
        capture_output=True,
        text=True,
    )
    found: set[str] = set()
    for line in result.stdout.splitlines():
        fields = line.split()
        if len(fields) >= 2 and fields[-1].startswith(SYMBOL_PREFIX):
            found.add(fields[-1])
    return found


def require_equal(label: str, expected: set[str], actual: set[str]) -> None:
    missing = sorted(expected - actual)
    extra = sorted(actual - expected)
    if missing or extra:
        fail(f"{label} mismatch: missing={missing}, extra={extra}")


def main() -> int:
    repo_root = Path(__file__).resolve().parents[2]
    header = (
        repo_root / "crates" / "vinput-fcitx-ffi" / "include" / "vinput_fcitx_ffi.h"
    )

    header_text = header.read_text(encoding="utf-8")
    header_exports = names(HEADER_FUNCTION_PATTERN, header_text)

    if not header_exports:
        fail("no Fcitx ABI functions were declared in the public C header")

    library = build_static_library(repo_root)
    require_equal("static-library symbols", header_exports, binary_symbols(library))

    print(
        "Fcitx FFI ABI check passed: "
        f"{len(header_exports)} public header declarations match the archive symbols"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Fcitx FFI ABI check failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
