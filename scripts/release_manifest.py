#!/usr/bin/env python3
"""Assemble and verify a checksum-pinned release artifact bundle."""

import argparse
import hashlib
import json
import os
import re
import shutil
import sys
import tempfile
from pathlib import Path
from typing import Any

MANIFEST_NAME = "manifest.json"
CHECKSUMS_NAME = "SHA256SUMS"
SCHEMA_VERSION = 1
ALLOWED_METADATA_FILES = {MANIFEST_NAME, CHECKSUMS_NAME}
ROLE_PATTERN = re.compile(r"^[a-z0-9][a-z0-9._-]*$")
FILE_NAME_PATTERN = re.compile(r"^[A-Za-z0-9._+@-]+$")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def regular_bundle_files(bundle: Path) -> list[Path]:
    files: list[Path] = []
    for path in bundle.iterdir():
        if path.is_symlink():
            raise ValueError(f"release bundle must not contain symlinks: {path.name}")
        if path.is_dir():
            raise ValueError(f"release bundle must be flat: {path.name}")
        if path.is_file():
            files.append(path)
            continue
        raise ValueError(f"release bundle contains a non-regular entry: {path.name}")
    return sorted(files, key=lambda path: path.name)


def parse_artifact(value: str) -> tuple[str, Path]:
    role, separator, raw_path = value.partition("=")
    if not separator or not role or not raw_path:
        raise argparse.ArgumentTypeError("artifact must use ROLE=PATH")
    if not ROLE_PATTERN.fullmatch(role):
        raise argparse.ArgumentTypeError(
            "artifact role must use lowercase letters, digits, dots, underscores, or hyphens"
        )
    return role, Path(raw_path)


def artifact_record(role: str, path: Path) -> dict[str, Any]:
    return {
        "name": path.name,
        "role": role,
        "size": path.stat().st_size,
        "sha256": sha256_file(path),
    }


def write_checksums(bundle: Path, artifacts: list[dict[str, Any]]) -> dict[str, Any]:
    checksums_path = bundle / CHECKSUMS_NAME
    lines = [f"{artifact['sha256']}  {artifact['name']}\n" for artifact in artifacts]
    checksums_path.write_text("".join(lines), encoding="utf-8")
    return {
        "name": CHECKSUMS_NAME,
        "size": checksums_path.stat().st_size,
        "sha256": sha256_file(checksums_path),
    }


def assemble(args: argparse.Namespace) -> None:
    bundle = args.output_dir.resolve()
    if bundle == Path(bundle.anchor):
        raise ValueError("output directory must not be a filesystem root")

    copied_sources: list[tuple[str, Path]] = []
    names: set[str] = set()
    roles: set[str] = set()
    for role, raw_source in args.artifact:
        source = raw_source.resolve()
        if source == bundle or bundle in source.parents:
            raise ValueError(f"artifact must not be inside the output directory: {source}")
        if not source.is_file() or source.is_symlink():
            raise ValueError(f"artifact must be a regular file: {source}")
        if not FILE_NAME_PATTERN.fullmatch(source.name) or source.name in {".", ".."}:
            raise ValueError(f"artifact basename is unsafe: {source.name!r}")
        if source.name in ALLOWED_METADATA_FILES:
            raise ValueError(f"artifact name is reserved: {source.name}")
        if source.name in names:
            raise ValueError(f"duplicate artifact basename: {source.name}")
        if role in roles:
            raise ValueError(f"duplicate artifact role: {role}")
        names.add(source.name)
        roles.add(role)
        copied_sources.append((role, source))

    if bundle.exists():
        if not args.force:
            raise ValueError(f"output directory already exists: {bundle}")
        if bundle.is_symlink() or not bundle.is_dir():
            raise ValueError(f"existing output must be a regular directory: {bundle}")
        verify_bundle(bundle)

    bundle.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(
        tempfile.mkdtemp(prefix=f".{bundle.name}.tmp-", dir=bundle.parent)
    )
    try:
        copied: list[tuple[str, Path]] = []
        for role, source in copied_sources:
            destination = staging / source.name
            shutil.copyfile(source, destination)
            copied.append((role, destination))

        artifacts = sorted(
            (artifact_record(role, path) for role, path in copied),
            key=lambda record: str(record["name"]),
        )
        checksum_file = write_checksums(staging, artifacts)
        manifest = {
            "schema_version": SCHEMA_VERSION,
            "package": {
                "name": args.package_name,
                "version": args.version,
                "architecture": args.architecture,
            },
            "checksum_file": checksum_file,
            "artifacts": artifacts,
        }
        (staging / MANIFEST_NAME).write_text(
            json.dumps(manifest, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        verify_bundle(staging)
        if bundle.exists():
            shutil.rmtree(bundle)
        os.replace(staging, bundle)
    finally:
        if staging.exists():
            shutil.rmtree(staging)


def require_object(value: Any, description: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError(f"{description} must be an object")
    return value


def require_exact_keys(
    value: dict[str, Any], expected: set[str], description: str
) -> None:
    actual = set(value)
    if actual != expected:
        raise ValueError(
            f"{description} fields mismatch: extras={sorted(actual - expected)}, "
            f"missing={sorted(expected - actual)}"
        )


def require_string(value: Any, description: str) -> str:
    if not isinstance(value, str) or not value:
        raise ValueError(f"{description} must be a non-empty string")
    return value


def require_nonnegative_integer(value: Any, description: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or value < 0:
        raise ValueError(f"{description} must be a non-negative integer")
    return value


def require_sha256(value: Any, description: str) -> str:
    digest = require_string(value, description)
    if len(digest) != 64 or any(character not in "0123456789abcdef" for character in digest):
        raise ValueError(f"{description} must be a lowercase SHA-256 digest")
    return digest


def parse_checksum_file(path: Path) -> list[tuple[str, str]]:
    entries: list[tuple[str, str]] = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        digest, separator, name = line.partition("  ")
        if not separator or not name or "/" in name or "\\" in name:
            raise ValueError(f"invalid checksum line {line_number}")
        entries.append((require_sha256(digest, f"checksum line {line_number}"), name))
    if entries != sorted(entries, key=lambda entry: entry[1]):
        raise ValueError("checksum entries must be sorted by artifact name")
    if len({name for _, name in entries}) != len(entries):
        raise ValueError("checksum entries contain duplicate artifact names")
    return entries


def verify_bundle(bundle: Path) -> None:
    if bundle.is_symlink() or not bundle.is_dir():
        raise ValueError(f"release bundle directory is missing or is a symlink: {bundle}")
    manifest_path = bundle / MANIFEST_NAME
    checksums_path = bundle / CHECKSUMS_NAME
    manifest = require_object(
        json.loads(manifest_path.read_text(encoding="utf-8")),
        "manifest",
    )
    require_exact_keys(
        manifest,
        {"schema_version", "package", "checksum_file", "artifacts"},
        "manifest",
    )
    if manifest.get("schema_version") != SCHEMA_VERSION:
        raise ValueError(f"unsupported manifest schema: {manifest.get('schema_version')}")

    package = require_object(manifest.get("package"), "package")
    require_exact_keys(package, {"name", "version", "architecture"}, "package")
    for field in ("name", "version", "architecture"):
        require_string(package.get(field), f"package.{field}")

    checksum_file = require_object(manifest.get("checksum_file"), "checksum_file")
    require_exact_keys(checksum_file, {"name", "size", "sha256"}, "checksum_file")
    if checksum_file.get("name") != CHECKSUMS_NAME:
        raise ValueError(f"checksum_file.name must be {CHECKSUMS_NAME}")
    checksum_size = require_nonnegative_integer(checksum_file.get("size"), "checksum_file.size")
    checksum_digest = require_sha256(checksum_file.get("sha256"), "checksum_file.sha256")
    if checksums_path.stat().st_size != checksum_size:
        raise ValueError("checksum file size mismatch")
    if sha256_file(checksums_path) != checksum_digest:
        raise ValueError("checksum file digest mismatch")

    raw_artifacts = manifest.get("artifacts")
    if not isinstance(raw_artifacts, list) or not raw_artifacts:
        raise ValueError("artifacts must be a non-empty array")
    artifacts: list[dict[str, Any]] = []
    names: set[str] = set()
    roles: set[str] = set()
    for index, raw_artifact in enumerate(raw_artifacts):
        artifact = require_object(raw_artifact, f"artifacts[{index}]")
        require_exact_keys(
            artifact,
            {"name", "role", "size", "sha256"},
            f"artifacts[{index}]",
        )
        name = require_string(artifact.get("name"), f"artifacts[{index}].name")
        if (
            Path(name).name != name
            or not FILE_NAME_PATTERN.fullmatch(name)
            or name in {".", ".."}
            or name in ALLOWED_METADATA_FILES
        ):
            raise ValueError(f"invalid artifact name: {name}")
        if name in names:
            raise ValueError(f"duplicate manifest artifact: {name}")
        names.add(name)
        role = require_string(artifact.get("role"), f"artifacts[{index}].role")
        if not ROLE_PATTERN.fullmatch(role):
            raise ValueError(f"invalid artifact role: {role}")
        if role in roles:
            raise ValueError(f"duplicate manifest artifact role: {role}")
        roles.add(role)
        require_nonnegative_integer(artifact.get("size"), f"artifacts[{index}].size")
        require_sha256(artifact.get("sha256"), f"artifacts[{index}].sha256")
        artifacts.append(artifact)
    if artifacts != sorted(artifacts, key=lambda artifact: str(artifact["name"])):
        raise ValueError("manifest artifacts must be sorted by name")

    checksum_entries = parse_checksum_file(checksums_path)
    expected_checksum_entries = [
        (str(artifact["sha256"]), str(artifact["name"])) for artifact in artifacts
    ]
    if checksum_entries != expected_checksum_entries:
        raise ValueError("SHA256SUMS does not exactly match manifest artifacts")

    for artifact in artifacts:
        artifact_path = bundle / str(artifact["name"])
        if not artifact_path.is_file() or artifact_path.is_symlink():
            raise ValueError(f"artifact is missing or not a regular file: {artifact_path.name}")
        if artifact_path.stat().st_size != artifact["size"]:
            raise ValueError(f"artifact size mismatch: {artifact_path.name}")
        if sha256_file(artifact_path) != artifact["sha256"]:
            raise ValueError(f"artifact digest mismatch: {artifact_path.name}")

    actual_names = {path.name for path in regular_bundle_files(bundle)}
    expected_names = names | ALLOWED_METADATA_FILES
    if actual_names != expected_names:
        extras = sorted(actual_names - expected_names)
        missing = sorted(expected_names - actual_names)
        raise ValueError(f"release bundle inventory mismatch: extras={extras}, missing={missing}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    assemble_parser = subparsers.add_parser("assemble")
    assemble_parser.add_argument("--package-name", required=True)
    assemble_parser.add_argument("--version", required=True)
    assemble_parser.add_argument("--architecture", required=True)
    assemble_parser.add_argument("--output-dir", type=Path, required=True)
    assemble_parser.add_argument(
        "--artifact",
        action="append",
        type=parse_artifact,
        required=True,
        metavar="ROLE=PATH",
    )
    assemble_parser.add_argument("--force", action="store_true")

    verify_parser = subparsers.add_parser("verify")
    verify_parser.add_argument("bundle", type=Path)
    return parser


def main() -> None:
    args = build_parser().parse_args()
    try:
        if args.command == "assemble":
            assemble(args)
        else:
            verify_bundle(args.bundle)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"release manifest error: {error}", file=sys.stderr)
        raise SystemExit(1) from error


if __name__ == "__main__":
    main()
