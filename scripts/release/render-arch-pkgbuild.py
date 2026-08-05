#!/usr/bin/env python3
"""Render the release Arch PKGBUILD from its checked template."""

import argparse
import shutil
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


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--pkgrel", default="1")
    parser.add_argument("--source-url", required=True)
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
        default=REPOSITORY_ROOT / "packaging/arch/PKGBUILD.in",
    )
    parser.add_argument(
        "--install-script",
        type=Path,
        default=REPOSITORY_ROOT / "packaging/arch/fcitx-vinpst.install",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    runtime = load_runtime_bundle(args.runtime_bundles, args.runtime_bundle)
    replacements = {
        "@VINPST_PKGVER@": args.version,
        "@VINPST_PKGREL@": args.pkgrel,
        "@VINPST_SOURCE_URL@": args.source_url,
        "@VINPST_SOURCE_SHA256@": args.source_sha256,
        "@VINPST_SOURCE_DIR@": args.source_dir,
        "@VINPST_PACKAGE_ARCH@": runtime["package_arch"],
        "@VINPST_RUST_TARGET@": runtime["rust_target"],
        "@VINPST_SHERPA_ONNX_VERSION@": runtime["sherpa_onnx_version"],
        "@VINPST_SHERPA_ONNX_ARCHIVE@": runtime["sherpa_onnx_archive"],
        "@VINPST_SHERPA_ONNX_ARCHIVE_ROOT@": runtime["sherpa_onnx_archive_root"],
        "@VINPST_SHERPA_ONNX_SHA256@": runtime["sherpa_onnx_sha256"],
        "@VINPST_SHERPA_ONNX_LICENSE_SHA256@": runtime["sherpa_onnx_license_sha256"],
        "@VINPST_ONNXRUNTIME_VERSION@": runtime["onnxruntime_version"],
        "@VINPST_ONNXRUNTIME_LICENSE_SHA256@": runtime["onnxruntime_license_sha256"],
    }
    rendered = args.template.read_text(encoding="utf-8")
    for placeholder, value in replacements.items():
        if placeholder not in rendered:
            raise SystemExit(f"missing template placeholder: {placeholder}")
        rendered = rendered.replace(placeholder, value)
    if "@VINPST_" in rendered:
        raise SystemExit("unresolved Vinpst PKGBUILD placeholder")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(rendered, encoding="utf-8")
    install_output = args.output.parent / args.install_script.name
    if args.install_script.resolve() != install_output.resolve():
        shutil.copyfile(args.install_script, install_output)


if __name__ == "__main__":
    main()
