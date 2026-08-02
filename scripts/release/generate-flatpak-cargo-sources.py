#!/usr/bin/env python3
"""Render Flatpak Builder Cargo vendor sources from a locked registry graph."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any

import tomllib

SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
CRATE_TOKEN_PATTERN = re.compile(r"^[A-Za-z0-9_+.-]+$")
CRATES_IO_SOURCE = "registry+https://github.com/rust-lang/crates.io-index"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("cargo_lock", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    return parser.parse_args()


def load_packages(path: Path) -> list[dict[str, Any]]:
    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise ValueError(f"cannot read Cargo lock file {path}: {error}") from error
    packages = data.get("package")
    if not isinstance(packages, list):
        raise TypeError("Cargo.lock must contain a package array")
    return packages


def registry_packages(packages: list[dict[str, Any]]) -> list[tuple[str, str, str]]:
    entries: list[tuple[str, str, str]] = []
    destinations: set[str] = set()
    for package in packages:
        source = package.get("source")
        if source is None:
            continue
        name = package.get("name")
        version = package.get("version")
        checksum = package.get("checksum")
        if source != CRATES_IO_SOURCE:
            raise ValueError(
                f"Flatpak Cargo vendoring does not permit unlocked source {source!r} "
                f"for {name!r}"
            )
        if not isinstance(name, str) or not CRATE_TOKEN_PATTERN.fullmatch(name):
            raise ValueError(f"unsafe crate name: {name!r}")
        if not isinstance(version, str) or not CRATE_TOKEN_PATTERN.fullmatch(version):
            raise ValueError(f"unsafe crate version for {name}: {version!r}")
        if not isinstance(checksum, str) or not SHA256_PATTERN.fullmatch(checksum):
            raise ValueError(f"invalid crates.io checksum for {name} {version}")
        destination = f"cargo/vendor/{name}-{version}"
        if destination in destinations:
            raise ValueError(f"duplicate Flatpak Cargo destination: {destination}")
        destinations.add(destination)
        entries.append((name, version, checksum))
    return sorted(entries)


def render_sources(packages: list[tuple[str, str, str]]) -> list[dict[str, Any]]:
    sources: list[dict[str, Any]] = []
    for name, version, checksum in packages:
        destination = f"cargo/vendor/{name}-{version}"
        sources.extend(
            [
                {
                    "type": "archive",
                    "archive-type": "tar-gzip",
                    "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
                    "sha256": checksum,
                    "dest": destination,
                },
                {
                    "type": "inline",
                    "contents": json.dumps(
                        {"package": checksum, "files": {}},
                        sort_keys=True,
                        separators=(",", ":"),
                    ),
                    "dest": destination,
                    "dest-filename": ".cargo-checksum.json",
                },
            ]
        )
    sources.append(
        {
            "type": "inline",
            "contents": (
                "[source.vendored-sources]\n"
                'directory = "cargo/vendor"\n\n'
                "[source.crates-io]\n"
                'replace-with = "vendored-sources"\n'
            ),
            "dest": "cargo",
            "dest-filename": "config.toml",
        }
    )
    return sources


def main() -> None:
    args = parse_args()
    packages = registry_packages(load_packages(args.cargo_lock))
    if not packages:
        raise ValueError("Cargo.lock contains no crates.io packages")
    output = json.dumps(render_sources(packages), indent=2, sort_keys=True) + "\n"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    temporary = args.output.with_name(f".{args.output.name}.tmp")
    temporary.write_text(output, encoding="utf-8")
    temporary.replace(args.output)


if __name__ == "__main__":
    main()
