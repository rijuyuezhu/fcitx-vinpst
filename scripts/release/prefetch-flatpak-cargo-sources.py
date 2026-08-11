#!/usr/bin/env python3
"""Materialize pinned crates.io archives for a Flatpak build."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

sys.dont_write_bytecode = True

SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
SAFE_FILENAME = re.compile(r"^[A-Za-z0-9._+-]+\.crate$")
SAFE_DESTINATION = re.compile(r"^cargo/vendor/[A-Za-z0-9._+-]+$")
CRATES_HOST = "static.crates.io"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sources", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--cache-dir", action="append", default=[], type=Path)
    parser.add_argument("--write-cache-dir", type=Path)
    parser.add_argument("--jobs", default=8, type=int)
    parser.add_argument("--attempts", default=3, type=int)
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    if args.jobs < 1 or args.jobs > 64:
        parser.error("--jobs must be between 1 and 64")
    if args.attempts < 1 or args.attempts > 10:
        parser.error("--attempts must be between 1 and 10")
    return args


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_archives(path: Path) -> list[dict[str, str]]:
    sources: Any = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(sources, list) or not sources:
        raise ValueError("Flatpak Cargo source list must be a non-empty array")

    archives: list[dict[str, str]] = []
    filenames: set[str] = set()
    destinations: set[str] = set()
    for entry in sources:
        if not isinstance(entry, dict) or entry.get("type") != "archive":
            continue
        url = entry.get("url")
        expected = entry.get("sha256")
        destination = entry.get("dest")
        if not isinstance(url, str) or not isinstance(expected, str):
            raise TypeError("Flatpak Cargo archive entries require url and sha256")
        if not isinstance(destination, str) or not SAFE_DESTINATION.fullmatch(
            destination
        ):
            raise ValueError(
                f"invalid Flatpak Cargo archive destination: {destination!r}"
            )
        parsed = urlparse(url)
        filename = Path(parsed.path).name
        if (
            parsed.scheme != "https"
            or parsed.hostname != CRATES_HOST
            or not SAFE_FILENAME.fullmatch(filename)
        ):
            raise ValueError(f"unsupported Flatpak Cargo archive URL: {url}")
        if not SHA256_PATTERN.fullmatch(expected):
            raise ValueError(f"invalid Flatpak Cargo archive SHA-256: {filename}")
        if entry.get("archive-type") != "tar-gzip":
            raise ValueError(f"unexpected Flatpak Cargo archive type: {filename}")
        if filename in filenames:
            raise ValueError(f"duplicate Flatpak Cargo archive filename: {filename}")
        if destination in destinations:
            raise ValueError(
                f"duplicate Flatpak Cargo archive destination: {destination}"
            )
        filenames.add(filename)
        destinations.add(destination)
        archives.append({"filename": filename, "sha256": expected, "url": url})

    if not archives:
        raise ValueError("Flatpak Cargo source list contains no archives")
    return archives


def checked_directory(path: Path, label: str, *, create: bool = False) -> Path:
    if path.is_symlink():
        raise ValueError(f"{label} must not be a symbolic link: {path}")
    if create:
        path.mkdir(parents=True, exist_ok=True)
    resolved = path.resolve()
    if not resolved.is_dir():
        raise ValueError(f"{label} must be a directory: {resolved}")
    return resolved


def index_cache(cache_dirs: list[Path]) -> dict[str, list[Path]]:
    index: dict[str, list[Path]] = {}
    for cache_dir in cache_dirs:
        if not cache_dir.exists():
            continue
        root = checked_directory(cache_dir, "Cargo archive cache")
        for candidate in root.rglob("*.crate"):
            if candidate.is_file() and not candidate.is_symlink():
                index.setdefault(candidate.name, []).append(candidate)
    return index


def atomic_copy(source: Path, destination: Path) -> None:
    temporary = destination.with_name(
        f".{destination.name}.{os.getpid()}.{threading.get_ident()}.partial"
    )
    try:
        shutil.copyfile(source, temporary)
        temporary.replace(destination)
    finally:
        temporary.unlink(missing_ok=True)


def populate_write_cache(
    source: Path, write_cache_dir: Path, filename: str, expected: str
) -> None:
    destination = write_cache_dir / filename
    if destination == source:
        return
    if destination.is_symlink():
        raise ValueError(
            f"Cargo write-cache entry must not be a symlink: {destination}"
        )
    if destination.is_file():
        if sha256_file(destination) == expected:
            return
        destination.unlink()
    atomic_copy(source, destination)
    if sha256_file(destination) != expected:
        raise RuntimeError(f"written Cargo cache digest mismatch: {filename}")


def download_archive(url: str, destination: Path, expected: str, attempts: int) -> None:
    temporary = destination.with_name(
        f".{destination.name}.{os.getpid()}.{threading.get_ident()}.partial"
    )
    try:
        for attempt in range(1, attempts + 1):
            temporary.unlink(missing_ok=True)
            result = subprocess.run(
                [
                    "curl",
                    "--retry",
                    "3",
                    "--retry-all-errors",
                    "--retry-delay",
                    "2",
                    "--connect-timeout",
                    "30",
                    "--max-time",
                    "300",
                    "--speed-limit",
                    "1024",
                    "--speed-time",
                    "30",
                    "--proto",
                    "=https",
                    "--tlsv1.2",
                    "-fsSL",
                    url,
                    "-o",
                    str(temporary),
                ],
                check=False,
            )
            if result.returncode == 0 and sha256_file(temporary) == expected:
                temporary.replace(destination)
                return
            if attempt < attempts:
                time.sleep(attempt * 2)
        raise RuntimeError(f"failed to download checked Cargo archive: {url}")
    finally:
        temporary.unlink(missing_ok=True)


def materialize_archive(
    archive: dict[str, str],
    output_dir: Path,
    cache_index: dict[str, list[Path]],
    write_cache_dir: Path | None,
    attempts: int,
    offline: bool,
) -> str:
    filename = archive["filename"]
    expected = archive["sha256"]
    destination = output_dir / filename
    if destination.is_symlink():
        raise ValueError(
            f"Cargo archive destination must not be a symlink: {destination}"
        )

    origin: str | None = None
    if destination.is_file():
        if sha256_file(destination) == expected:
            origin = "output"
        else:
            destination.unlink()

    if origin is None:
        for candidate in cache_index.get(filename, []):
            if sha256_file(candidate) == expected:
                atomic_copy(candidate, destination)
                if sha256_file(destination) != expected:
                    raise RuntimeError(
                        f"copied Cargo archive digest mismatch: {filename}"
                    )
                origin = "cache"
                break

    if origin is None:
        if offline:
            raise RuntimeError(
                f"missing checked Cargo archive in offline mode: {filename}"
            )
        download_archive(archive["url"], destination, expected, attempts)
        origin = "download"

    if write_cache_dir is not None:
        populate_write_cache(destination, write_cache_dir, filename, expected)
    return origin


def main() -> None:
    args = parse_args()
    if shutil.which("curl") is None and not args.offline:
        raise RuntimeError("curl is required to prefetch Flatpak Cargo archives")
    archives = load_archives(args.sources.resolve())
    output_dir = checked_directory(
        args.output_dir, "Flatpak Cargo source directory", create=True
    )
    write_cache_dir = (
        checked_directory(args.write_cache_dir, "Cargo write cache", create=True)
        if args.write_cache_dir is not None
        else None
    )
    cache_dirs = args.cache_dir
    if not cache_dirs:
        cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
        cache_dirs = [cargo_home / "registry/cache"]
    if write_cache_dir is not None:
        cache_dirs.append(write_cache_dir)
    cache_index = index_cache(cache_dirs)

    results: list[str] = []
    errors: list[str] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as executor:
        future_to_name = {
            executor.submit(
                materialize_archive,
                archive,
                output_dir,
                cache_index,
                write_cache_dir,
                args.attempts,
                args.offline,
            ): archive["filename"]
            for archive in archives
        }
        for future in concurrent.futures.as_completed(future_to_name):
            name = future_to_name[future]
            try:
                results.append(future.result())
            except Exception as error:  # noqa: BLE001 - report all worker failures together
                errors.append(f"{name}: {error}")

    if errors:
        for error in sorted(errors):
            print(error, file=sys.stderr)
        raise RuntimeError(
            f"failed to materialize {len(errors)} of {len(archives)} Cargo archives"
        )

    counts = {kind: results.count(kind) for kind in ("output", "cache", "download")}
    print(
        "Flatpak Cargo sources ready: "
        f"{len(archives)} total, {counts['output']} existing, "
        f"{counts['cache']} cached, {counts['download']} downloaded"
    )


if __name__ == "__main__":
    main()
