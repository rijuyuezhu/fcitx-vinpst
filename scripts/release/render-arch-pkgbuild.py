#!/usr/bin/env python3
"""Render the release Arch PKGBUILD from its checked template."""

import argparse
import json
import re
import shutil
from pathlib import Path


def find_repository_root(start: Path) -> Path:
    for candidate in (start, *start.parents):
        if (candidate / "Cargo.toml").is_file() and (candidate / "scripts").is_dir():
            return candidate
    raise RuntimeError(f"cannot locate repository root from {start}")


REPOSITORY_ROOT = find_repository_root(Path(__file__).resolve().parent)
RUNTIME_BUNDLES_PATH = REPOSITORY_ROOT / "packaging/arch/runtime-bundles.json"
SHA256_RE = re.compile(r"[0-9a-f]{64}")
SAFE_TOKEN_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._+-]*")
RUNTIME_FIELDS = {
    "id",
    "package_arch",
    "rust_target",
    "sherpa_onnx_version",
    "sherpa_onnx_archive",
    "sherpa_onnx_archive_root",
    "sherpa_onnx_sha256",
    "sherpa_onnx_license_sha256",
    "onnxruntime_version",
    "onnxruntime_license_sha256",
}


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
        default=REPOSITORY_ROOT / "packaging/arch/fcitx-vinput-rs.install",
    )
    return parser.parse_args()


def require_string(entry: dict[str, object], field: str) -> str:
    value = entry.get(field)
    if not isinstance(value, str) or not value:
        raise SystemExit(f"runtime bundle field must be a non-empty string: {field}")
    return value


def load_runtime_bundle(path: Path, requested_id: str | None) -> dict[str, str]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(
            f"cannot read runtime bundle manifest {path}: {error}"
        ) from error

    if not isinstance(document, dict) or set(document) != {
        "schema_version",
        "default_bundle",
        "bundles",
    }:
        raise SystemExit("runtime bundle manifest fields mismatch")
    if document["schema_version"] != 1:
        raise SystemExit(
            f"unsupported runtime bundle manifest schema: {document['schema_version']}"
        )
    default_bundle = document["default_bundle"]
    bundles = document["bundles"]
    if not isinstance(default_bundle, str) or not default_bundle:
        raise SystemExit("runtime bundle manifest default_bundle must be non-empty")
    if not isinstance(bundles, list) or not bundles:
        raise SystemExit("runtime bundle manifest bundles must be a non-empty list")

    selected_id = requested_id or default_bundle
    selected: dict[str, str] | None = None
    seen_ids: set[str] = set()
    for raw_entry in bundles:
        if not isinstance(raw_entry, dict) or set(raw_entry) != RUNTIME_FIELDS:
            raise SystemExit("runtime bundle entry fields mismatch")
        entry = {field: require_string(raw_entry, field) for field in RUNTIME_FIELDS}
        bundle_id = entry["id"]
        if bundle_id in seen_ids:
            raise SystemExit(f"duplicate runtime bundle id: {bundle_id}")
        seen_ids.add(bundle_id)
        for field in (
            "sherpa_onnx_sha256",
            "sherpa_onnx_license_sha256",
            "onnxruntime_license_sha256",
        ):
            if SHA256_RE.fullmatch(entry[field]) is None:
                raise SystemExit(
                    f"runtime bundle field must be lowercase SHA-256: {field}"
                )
        for field in (
            "id",
            "package_arch",
            "rust_target",
            "sherpa_onnx_version",
            "sherpa_onnx_archive",
            "sherpa_onnx_archive_root",
            "onnxruntime_version",
        ):
            if SAFE_TOKEN_RE.fullmatch(entry[field]) is None:
                raise SystemExit(f"runtime bundle field must be a safe token: {field}")
        if not entry["sherpa_onnx_archive"].endswith(".tar.bz2"):
            raise SystemExit("runtime sherpa archive must end in .tar.bz2")
        if bundle_id == selected_id:
            selected = entry

    if default_bundle not in seen_ids:
        raise SystemExit(f"default runtime bundle is not defined: {default_bundle}")
    if selected is None:
        raise SystemExit(f"unknown runtime bundle: {selected_id}")
    return selected


def main() -> None:
    args = parse_args()
    runtime = load_runtime_bundle(args.runtime_bundles, args.runtime_bundle)
    replacements = {
        "@VINPUT_PKGVER@": args.version,
        "@VINPUT_PKGREL@": args.pkgrel,
        "@VINPUT_SOURCE_URL@": args.source_url,
        "@VINPUT_SOURCE_SHA256@": args.source_sha256,
        "@VINPUT_SOURCE_DIR@": args.source_dir,
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
            raise SystemExit(f"missing template placeholder: {placeholder}")
        rendered = rendered.replace(placeholder, value)
    if "@VINPUT_" in rendered:
        raise SystemExit("unresolved Vinput PKGBUILD placeholder")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(rendered, encoding="utf-8")
    install_output = args.output.parent / args.install_script.name
    if args.install_script.resolve() != install_output.resolve():
        shutil.copyfile(args.install_script, install_output)


if __name__ == "__main__":
    main()
