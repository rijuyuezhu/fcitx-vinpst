#!/usr/bin/env python3
"""Render checked Debian binary-package metadata."""

import argparse
import re
from pathlib import Path

SAFE_VERSION_RE = re.compile(r"[0-9][0-9A-Za-z.+~:-]*")
SAFE_RELEASE_RE = re.compile(r"[0-9][0-9A-Za-z.+~]*")
SAFE_ARCH_RE = re.compile(r"[a-z0-9][a-z0-9-]*")
PLACEHOLDER_RE = re.compile(r"@[A-Z0-9_]+@")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--template", type=Path, default=Path("packaging/debian/control.in")
    )
    parser.add_argument("--version", required=True)
    parser.add_argument("--release", default="1")
    parser.add_argument("--architecture", required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def checked(value: str, pattern: re.Pattern[str], name: str) -> str:
    if pattern.fullmatch(value) is None:
        raise SystemExit(f"invalid Debian renderer {name}: {value!r}")
    return value


def main() -> None:
    args = parse_args()
    version = checked(args.version, SAFE_VERSION_RE, "version")
    release = checked(args.release, SAFE_RELEASE_RE, "release")
    architecture = checked(args.architecture, SAFE_ARCH_RE, "architecture")

    rendered = args.template.read_text(encoding="utf-8")
    replacements = {
        "@VINPUT_VERSION@": version,
        "@VINPUT_RELEASE@": release,
        "@VINPUT_ARCHITECTURE@": architecture,
    }
    for placeholder, value in replacements.items():
        rendered = rendered.replace(placeholder, value)

    unresolved = sorted(set(PLACEHOLDER_RE.findall(rendered)))
    if unresolved:
        raise SystemExit(
            f"unresolved Debian control placeholders: {', '.join(unresolved)}"
        )
    if not rendered.endswith("\n"):
        rendered += "\n"

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(rendered, encoding="utf-8")


if __name__ == "__main__":
    main()
