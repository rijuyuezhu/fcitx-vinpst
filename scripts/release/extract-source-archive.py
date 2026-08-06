#!/usr/bin/env python3
"""Safely materialize one checked Vinpst release source archive."""

from __future__ import annotations

import argparse
import os
import re
import shutil
import stat
import tarfile
import tempfile
from pathlib import Path, PurePosixPath

VERSION_RE = re.compile(r"^[0-9][0-9A-Za-z.+~-]*$")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Safely extract a Vinpst release source archive"
    )
    parser.add_argument("--archive", required=True, type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--output-root", required=True, type=Path)
    return parser.parse_args()


def repository_root() -> Path:
    return Path(__file__).resolve().parents[2]


def require_regular_file(path: Path, description: str) -> Path:
    absolute = path.absolute()
    try:
        file_stat = absolute.lstat()
    except FileNotFoundError as error:
        raise SystemExit(f"missing {description}: {absolute}") from error
    if stat.S_ISLNK(file_stat.st_mode) or not stat.S_ISREG(file_stat.st_mode):
        raise SystemExit(f"{description} must be a regular file: {absolute}")
    return absolute


def checked_output_root(repo_root: Path, requested: Path) -> Path:
    output_root = requested.resolve(strict=False)
    allowed_roots = ((repo_root / "target").resolve(), (repo_root / "dist").resolve())
    if not any(
        output_root == root or output_root.is_relative_to(root)
        for root in allowed_roots
    ):
        raise SystemExit(
            f"source extraction output must be under target/ or dist/: {output_root}"
        )
    output_root.mkdir(parents=True, exist_ok=True)
    return output_root.resolve()


def normalized_member_path(member_name: str, source_dir: str) -> PurePosixPath | None:
    path = PurePosixPath(member_name)
    if path.is_absolute() or ".." in path.parts:
        raise SystemExit(f"unsafe source archive member path: {member_name!r}")

    parts = tuple(part for part in path.parts if part not in ("", "."))
    if not parts or parts[0] != source_dir:
        raise SystemExit(
            f"source archive member is outside {source_dir}/: {member_name!r}"
        )
    relative_parts = parts[1:]
    if not relative_parts:
        return None
    return PurePosixPath(*relative_parts)


def extract_archive(archive: Path, version: str, output_root: Path) -> Path:
    source_dir = f"fcitx-vinpst-{version}"
    destination = output_root / source_dir
    if destination.exists() or destination.is_symlink():
        raise SystemExit(f"source extraction destination already exists: {destination}")

    temporary = Path(tempfile.mkdtemp(prefix=f".{source_dir}.tmp.", dir=output_root))
    seen: set[PurePosixPath] = set()
    required = {PurePosixPath("Cargo.toml"), PurePosixPath("Cargo.lock")}

    try:
        with tarfile.open(archive, mode="r:gz") as source:
            for member in source:
                relative = normalized_member_path(member.name, source_dir)
                if relative is None:
                    if not member.isdir():
                        raise SystemExit(
                            f"source archive root must be a directory: {member.name!r}"
                        )
                    continue
                if relative in seen:
                    raise SystemExit(
                        f"duplicate source archive member path: {member.name!r}"
                    )
                seen.add(relative)

                target = temporary.joinpath(*relative.parts)
                if member.isdir():
                    target.mkdir(parents=True, exist_ok=False)
                    target.chmod(member.mode & 0o777)
                    continue
                if not member.isfile():
                    raise SystemExit(
                        f"unsupported source archive member type: {member.name!r}"
                    )

                target.parent.mkdir(parents=True, exist_ok=True)
                source_file = source.extractfile(member)
                if source_file is None:
                    raise SystemExit(
                        f"failed to read source archive member: {member.name!r}"
                    )
                with source_file, target.open("xb") as destination_file:
                    shutil.copyfileobj(source_file, destination_file)
                target.chmod(member.mode & 0o777)

        missing = sorted(str(path) for path in required - seen)
        if missing:
            raise SystemExit(
                "source archive is missing required files: " + ", ".join(missing)
            )
        os.replace(temporary, destination)
    except BaseException:
        shutil.rmtree(temporary, ignore_errors=True)
        raise

    return destination


def main() -> None:
    args = parse_args()
    if not VERSION_RE.fullmatch(args.version):
        raise SystemExit(f"invalid source archive version: {args.version!r}")

    repo_root = repository_root()
    archive = require_regular_file(args.archive, "source archive")
    output_root = checked_output_root(repo_root, args.output_root)
    print(extract_archive(archive, args.version, output_root))


if __name__ == "__main__":
    main()
