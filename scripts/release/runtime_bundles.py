#!/usr/bin/env python3
"""Checked native-runtime bundle manifest loading shared by package renderers."""

import json
import re
from pathlib import Path

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


def require_string(entry: dict[str, object], field: str) -> str:
    """Return one required non-empty string field or terminate rendering."""
    value = entry.get(field)
    if not isinstance(value, str) or not value:
        raise SystemExit(f"runtime bundle field must be a non-empty string: {field}")
    return value


def load_runtime_bundle(path: Path, requested_id: str | None) -> dict[str, str]:
    """Load and strictly validate one selected runtime bundle."""
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"cannot read runtime bundle manifest {path}: {error}") from error

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
