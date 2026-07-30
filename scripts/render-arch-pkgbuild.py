#!/usr/bin/env python3
"""Render the release Arch PKGBUILD from its checked template."""

from __future__ import annotations

import argparse
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--pkgrel", default="1")
    parser.add_argument("--source-url", required=True)
    parser.add_argument("--source-sha256", required=True)
    parser.add_argument("--source-dir", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--template",
        type=Path,
        default=Path("packaging/arch/PKGBUILD.in"),
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    replacements = {
        "@VINPUT_PKGVER@": args.version,
        "@VINPUT_PKGREL@": args.pkgrel,
        "@VINPUT_SOURCE_URL@": args.source_url,
        "@VINPUT_SOURCE_SHA256@": args.source_sha256,
        "@VINPUT_SOURCE_DIR@": args.source_dir,
    }
    rendered = args.template.read_text(encoding="utf-8")
    for placeholder, value in replacements.items():
        if placeholder not in rendered:
            raise SystemExit(f"missing template placeholder: {placeholder}")
        rendered = rendered.replace(placeholder, value)
    if "@VINPUT_" in rendered:
        raise SystemExit("unresolved Vinput PKGBUILD placeholder")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(rendered, encoding="utf-8")


if __name__ == "__main__":
    main()
