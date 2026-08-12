#!/usr/bin/env python3
"""Render the AUR binary PKGBUILD from its checked template."""

import argparse
import re
import sys
from pathlib import Path

sys.dont_write_bytecode = True


def find_repository_root(start: Path) -> Path:
    for candidate in (start, *start.parents):
        if (candidate / "Cargo.toml").is_file() and (candidate / "scripts").is_dir():
            return candidate
    raise RuntimeError(f"cannot locate repository root from {start}")


REPOSITORY_ROOT = find_repository_root(Path(__file__).resolve().parent)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--package-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--template",
        type=Path,
        default=REPOSITORY_ROOT / "packaging/aur/PKGBUILD.template",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if not re.fullmatch(r"[0-9][0-9A-Za-z._+]*", args.version):
        raise SystemExit(f"invalid AUR package version: {args.version!r}")
    package_sha256 = args.package_sha256.lower()
    if not re.fullmatch(r"[0-9a-f]{64}", package_sha256):
        raise SystemExit(
            "AUR package SHA-256 must be 64 lowercase hexadecimal characters"
        )

    replacements = {
        "@VINPST_PKGVER@": args.version,
        "@VINPST_PACKAGE_SHA256@": package_sha256,
    }
    rendered = args.template.read_text(encoding="utf-8")
    for placeholder, value in replacements.items():
        if placeholder not in rendered:
            raise SystemExit(f"missing template placeholder: {placeholder}")
        rendered = rendered.replace(placeholder, value)
    if "@VINPST_" in rendered:
        raise SystemExit("unresolved Vinpst AUR PKGBUILD placeholder")

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(rendered, encoding="utf-8")


if __name__ == "__main__":
    main()
