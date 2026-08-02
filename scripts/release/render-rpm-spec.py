#!/usr/bin/env python3
"""Render the checked RPM spec from the shared native-runtime bundle manifest."""

import argparse
import re
import sys
from pathlib import Path

sys.dont_write_bytecode = True

from runtime_bundles import load_runtime_bundle


def find_repository_root(start: Path) -> Path:
    for candidate in (start, *start.parents):
        if (candidate / "Cargo.toml").is_file() and (candidate / "scripts").is_dir():
            return candidate
    raise RuntimeError(f"cannot locate repository root from {start}")


REPOSITORY_ROOT = find_repository_root(Path(__file__).resolve().parent)
RUNTIME_BUNDLES_PATH = REPOSITORY_ROOT / "packaging/arch/runtime-bundles.json"
SAFE_VERSION_RE = re.compile(r"[0-9][A-Za-z0-9._+]*")
SAFE_RELEASE_RE = re.compile(r"[1-9][0-9]*")
SAFE_SOURCE_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._+-]*\.tar\.gz")
SAFE_SOURCE_DIR_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._+-]*")
SHA256_RE = re.compile(r"[0-9a-f]{64}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--release", default="1")
    parser.add_argument("--source-name", required=True)
    parser.add_argument("--source-sha256", required=True)
    parser.add_argument("--source-dir", required=True)
    parser.add_argument("--runtime-bundle")
    parser.add_argument(
        "--runtime-bundles",
        type=Path,
        default=RUNTIME_BUNDLES_PATH,
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--template",
        type=Path,
        default=REPOSITORY_ROOT / "packaging/rpm/fcitx-vinput-rs.spec.in",
    )
    return parser.parse_args()


def checked(value: str, pattern: re.Pattern[str], field: str) -> str:
    if pattern.fullmatch(value) is None:
        raise SystemExit(f"invalid RPM renderer {field}: {value}")
    return value


def main() -> None:
    args = parse_args()
    runtime = load_runtime_bundle(args.runtime_bundles, args.runtime_bundle)
    version = checked(args.version, SAFE_VERSION_RE, "version")
    release = checked(args.release, SAFE_RELEASE_RE, "release")
    source_name = checked(args.source_name, SAFE_SOURCE_RE, "source name")
    source_sha256 = checked(args.source_sha256, SHA256_RE, "source SHA-256")
    source_dir = checked(args.source_dir, SAFE_SOURCE_DIR_RE, "source directory")
    replacements = {
        "@VINPUT_VERSION@": version,
        "@VINPUT_RELEASE@": release,
        "@VINPUT_SOURCE_NAME@": source_name,
        "@VINPUT_SOURCE_SHA256@": source_sha256,
        "@VINPUT_SOURCE_DIR@": source_dir,
        "@VINPUT_PACKAGE_ARCH@": runtime["package_arch"],
        "@VINPUT_RUST_TARGET@": runtime["rust_target"],
        "@VINPUT_SHERPA_ONNX_VERSION@": runtime["sherpa_onnx_version"],
        "@VINPUT_SHERPA_ONNX_ARCHIVE@": runtime["sherpa_onnx_archive"],
        "@VINPUT_SHERPA_ONNX_ARCHIVE_ROOT@": runtime["sherpa_onnx_archive_root"],
        "@VINPUT_SHERPA_ONNX_SHA256@": runtime["sherpa_onnx_sha256"],
        "@VINPUT_SHERPA_ONNX_LICENSE_SHA256@": runtime["sherpa_onnx_license_sha256"],
        "@VINPUT_ONNXRUNTIME_VERSION@": runtime["onnxruntime_version"],
        "@VINPUT_ONNXRUNTIME_LICENSE_SHA256@": runtime["onnxruntime_license_sha256"],
    }
    rendered = args.template.read_text(encoding="utf-8")
    for placeholder, value in replacements.items():
        if placeholder not in rendered:
            raise SystemExit(f"missing RPM template placeholder: {placeholder}")
        rendered = rendered.replace(placeholder, value)
    if "@VINPUT_" in rendered:
        raise SystemExit("unresolved Vinput RPM placeholder")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(rendered, encoding="utf-8")


if __name__ == "__main__":
    main()
