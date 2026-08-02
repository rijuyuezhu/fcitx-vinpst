#!/usr/bin/env python3
"""Render the pinned Fcitx Flatpak extension manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
from pathlib import Path
from typing import Any

sys.dont_write_bytecode = True

from runtime_bundles import load_runtime_bundle

SAFE_TOKEN = re.compile(r"^[A-Za-z0-9._+-]+$")
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
CRATE_URL_PATTERN = re.compile(
    r"^https://static\.crates\.io/crates/[A-Za-z0-9_+.-]+/"
    r"(?P<filename>[A-Za-z0-9_+.-]+\.crate)$"
)
APP_ID = "org.fcitx.Fcitx5.Addon.Vinput"
PREFIX = "/app/addons/Vinput"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--source-dir", type=Path)
    parser.add_argument("--source-archive", type=Path)
    parser.add_argument("--source-sha256")
    parser.add_argument("--runtime-source-dir", type=Path)
    parser.add_argument("--runtime-manifest", type=Path)
    parser.add_argument("--cargo-source-dir", type=Path)
    parser.add_argument("--cargo-sources-manifest", type=Path)
    parser.add_argument("--runtime-bundle")
    parser.add_argument("--runtime-version", default="stable")
    parser.add_argument("--branch", default="stable")
    parser.add_argument("--revision", default="1")
    args = parser.parse_args()
    if (args.source_dir is None) == (args.source_archive is None):
        parser.error("exactly one of --source-dir or --source-archive is required")
    if args.source_archive is not None and args.source_sha256 is None:
        parser.error("--source-sha256 is required with --source-archive")
    if args.source_dir is not None and args.source_sha256 is not None:
        parser.error("--source-sha256 is valid only with --source-archive")
    return args


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def relative_source_path(source: Path, output: Path) -> str:
    return os.path.relpath(source.resolve(), output.resolve().parent)


def source_entry(args: argparse.Namespace) -> dict[str, str]:
    if args.source_dir is not None:
        if args.source_dir.is_symlink():
            raise ValueError(
                f"Flatpak source directory must not be a symbolic link: {args.source_dir}"
            )
        source = args.source_dir.resolve()
        if not source.is_dir():
            raise ValueError(
                f"Flatpak source directory must be a regular directory: {source}"
            )
        return {"type": "dir", "path": relative_source_path(source, args.output)}

    if args.source_archive.is_symlink():
        raise ValueError(
            f"Flatpak source archive must not be a symbolic link: {args.source_archive}"
        )
    source = args.source_archive.resolve()
    if not source.is_file():
        raise ValueError(f"Flatpak source archive must be a regular file: {source}")
    if not SHA256_PATTERN.fullmatch(args.source_sha256):
        raise ValueError("Flatpak source SHA-256 must be lowercase hexadecimal")
    actual = sha256_file(source)
    if actual != args.source_sha256:
        raise ValueError(
            f"Flatpak source archive digest mismatch: expected {args.source_sha256}, got {actual}"
        )
    return {
        "type": "archive",
        "path": relative_source_path(source, args.output),
        "sha256": actual,
    }


def local_runtime_source(
    source: Path,
    output: Path,
    expected_sha256: str,
    source_type: str,
    *,
    dest_filename: str | None = None,
) -> dict[str, str]:
    if not source.is_file() or source.is_symlink():
        raise ValueError(f"Flatpak runtime source must be a regular file: {source}")
    actual = sha256_file(source)
    if actual != expected_sha256:
        raise ValueError(
            f"Flatpak runtime source digest mismatch for {source.name}: "
            f"expected {expected_sha256}, got {actual}"
        )
    entry = {
        "type": source_type,
        "path": relative_source_path(source, output),
        "sha256": actual,
    }
    if dest_filename is not None:
        entry["dest-filename"] = dest_filename
    return entry


def cargo_archive_filename(source: dict[str, Any]) -> str:
    url = source.get("url")
    if not isinstance(url, str):
        raise TypeError("Flatpak Cargo archive source must contain a URL")
    match = CRATE_URL_PATTERN.fullmatch(url)
    if match is None:
        raise ValueError(f"unsupported Flatpak Cargo archive URL: {url!r}")
    return match.group("filename")


def localize_cargo_sources(
    sources: list[dict[str, Any]], output: Path, cargo_source_dir: Path | None
) -> list[dict[str, Any]]:
    if cargo_source_dir is None:
        return sources
    if cargo_source_dir.is_symlink():
        raise ValueError(
            "Flatpak Cargo source directory must not be a symbolic link: "
            f"{cargo_source_dir}"
        )
    source_dir = cargo_source_dir.resolve()
    if not source_dir.is_dir():
        raise ValueError(
            f"Flatpak Cargo source directory must be a regular directory: {source_dir}"
        )

    localized: list[dict[str, Any]] = []
    for source in sources:
        if source.get("type") != "archive":
            localized.append(source)
            continue
        expected_sha256 = source.get("sha256")
        if not isinstance(expected_sha256, str) or not SHA256_PATTERN.fullmatch(
            expected_sha256
        ):
            raise ValueError(
                "Flatpak Cargo archive SHA-256 must be lowercase hexadecimal"
            )
        filename = cargo_archive_filename(source)
        local_source = source_dir / filename
        if not local_source.is_file() or local_source.is_symlink():
            raise ValueError(
                f"Flatpak Cargo source must be a regular file: {local_source}"
            )
        actual_sha256 = sha256_file(local_source)
        if actual_sha256 != expected_sha256:
            raise ValueError(
                f"Flatpak Cargo source digest mismatch for {filename}: "
                f"expected {expected_sha256}, got {actual_sha256}"
            )
        entry = dict(source)
        del entry["url"]
        entry["path"] = relative_source_path(local_source, output)
        localized.append(entry)
    return localized


def checked_token(name: str, value: str) -> str:
    if not SAFE_TOKEN.fullmatch(value):
        raise ValueError(f"unsafe Flatpak {name}: {value!r}")
    return value


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"cannot read JSON {path}: {error}") from error


def runtime_module(
    bundle: dict[str, Any], output: Path, runtime_source_dir: Path | None
) -> dict[str, Any]:
    sherpa_version = bundle["sherpa_onnx_version"]
    archive = bundle["sherpa_onnx_archive"]
    onnxruntime_version = bundle["onnxruntime_version"]
    if runtime_source_dir is None:
        sources = [
            {
                "type": "archive",
                "url": (
                    "https://github.com/k2-fsa/sherpa-onnx/releases/download/"
                    f"v{sherpa_version}/{archive}"
                ),
                "sha256": bundle["sherpa_onnx_sha256"],
            },
            {
                "type": "file",
                "url": (
                    "https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/"
                    f"v{sherpa_version}/LICENSE"
                ),
                "sha256": bundle["sherpa_onnx_license_sha256"],
                "dest-filename": "sherpa-onnx-LICENSE",
            },
            {
                "type": "file",
                "url": (
                    "https://raw.githubusercontent.com/microsoft/onnxruntime/"
                    f"v{onnxruntime_version}/LICENSE"
                ),
                "sha256": bundle["onnxruntime_license_sha256"],
                "dest-filename": "onnxruntime-LICENSE",
            },
        ]
    else:
        if runtime_source_dir.is_symlink():
            raise ValueError(
                "Flatpak runtime source directory must not be a symbolic link: "
                f"{runtime_source_dir}"
            )
        source_dir = runtime_source_dir.resolve()
        if not source_dir.is_dir():
            raise ValueError(
                f"Flatpak runtime source directory must be a regular directory: {source_dir}"
            )
        sources = [
            local_runtime_source(
                source_dir / archive,
                output,
                bundle["sherpa_onnx_sha256"],
                "archive",
            ),
            local_runtime_source(
                source_dir / "sherpa-onnx-LICENSE",
                output,
                bundle["sherpa_onnx_license_sha256"],
                "file",
                dest_filename="sherpa-onnx-LICENSE",
            ),
            local_runtime_source(
                source_dir / "onnxruntime-LICENSE",
                output,
                bundle["onnxruntime_license_sha256"],
                "file",
                dest_filename="onnxruntime-LICENSE",
            ),
        ]
    return {
        "name": "sherpa-onnx-runtime",
        "buildsystem": "simple",
        "build-commands": [
            f"install -Dm755 lib/libsherpa-onnx-c-api.so {PREFIX}/lib/libsherpa-onnx-c-api.so",
            f"install -Dm755 lib/libonnxruntime.so {PREFIX}/lib/libonnxruntime.so",
            f"install -Dm644 sherpa-onnx-LICENSE {PREFIX}/share/licenses/fcitx-vinput-rs/sherpa-onnx-LICENSE",
            f"install -Dm644 onnxruntime-LICENSE {PREFIX}/share/licenses/fcitx-vinput-rs/onnxruntime-LICENSE",
        ],
        "sources": sources,
    }


def product_build_commands(revision: str) -> list[str]:
    return [
        (
            "cargo build --frozen --release "
            "-p vinput-cli --features pipewire-backend,sherpa-onnx-backend "
            "-p vinput-daemon --features pipewire-backend,sherpa-onnx-backend "
            "-p vinput-gui"
        ),
        (
            "cmake -S cpp/fcitx5-addon -B build/fcitx-addon -G Ninja "
            "-DBUILD_TESTING=OFF -DCMAKE_BUILD_TYPE=Release "
            f"-DCMAKE_INSTALL_PREFIX={PREFIX} -DCMAKE_INSTALL_LIBDIR=lib "
            f"-DVINPUT_DAEMON_EXECUTABLE={PREFIX}/bin/vinput-daemon "
            "-DVINPUT_DAEMON_ARGS='--dbus --configured-backends --audio-backend pipewire' "
            "-DVINPUT_FCITX_BRIDGE_ENABLE_TESTS=OFF "
            "-DVINPUT_FCITX_BRIDGE_REQUIRE_FCITX_CORE=ON "
            "-DVINPUT_FCITX_MODULE_INSTALL_DIR=lib/fcitx5 "
            "-DVINPUT_FCITX_RUNTIME_BUILD_LOCALEDIR='' "
            "-DVINPUT_SYSTEMD_USER_UNIT_DIR=share/systemd/user"
        ),
        "cmake --build build/fcitx-addon --target fcitx5_vinput_addon --parallel",
        f"install -Dm755 target/release/vinput {PREFIX}/bin/vinput",
        f"install -Dm755 target/release/vinput-daemon {PREFIX}/bin/vinput-daemon",
        f"install -Dm755 target/release/vinput-gui {PREFIX}/bin/vinput-gui",
        "cmake --install build/fcitx-addon",
        f"install -Dm644 data/vinput-gui.desktop {PREFIX}/share/applications/vinput-gui.desktop",
        (
            "for size in 16 22 24 32 48 64 128 256 512; do "
            f"install -Dm644 data/icons/hicolor/${{size}}x${{size}}/apps/vinput-gui.png "
            f"{PREFIX}/share/icons/hicolor/${{size}}x${{size}}/apps/vinput-gui.png; done"
        ),
        f"install -Dm644 data/default-config.json {PREFIX}/share/fcitx-vinput/default-config.json",
        f"install -Dm644 data/vad/silero_vad.onnx {PREFIX}/share/fcitx-vinput/vad/silero_vad.onnx",
        f"install -Dm644 LICENSE {PREFIX}/share/licenses/fcitx-vinput-rs/LICENSE",
        f"install -Dm644 data/vad/LICENSE {PREFIX}/share/licenses/fcitx-vinput-rs/silero-vad-LICENSE",
        (
            f"printf '%s\\n' '{revision}' >package-revision && "
            f"install -Dm644 package-revision {PREFIX}/share/fcitx-vinput/package-revision"
        ),
    ]


def product_module(
    product_source: dict[str, str], cargo_sources: list[dict[str, Any]], revision: str
) -> dict[str, Any]:
    return {
        "name": "fcitx-vinput-rs",
        "buildsystem": "simple",
        "build-options": {
            "env": {
                "CARGO_HOME": "/run/build/fcitx-vinput-rs/cargo",
                "CARGO_NET_OFFLINE": "true",
                "SHERPA_ONNX_LIB_DIR": f"{PREFIX}/lib",
            }
        },
        "build-commands": product_build_commands(revision),
        "sources": [product_source, *cargo_sources],
    }


def main() -> None:
    args = parse_args()
    repo_root = Path(__file__).resolve().parents[2]
    base = load_json(repo_root / "packaging/flatpak/manifest-base.json")
    cargo_sources_manifest = (
        args.cargo_sources_manifest.resolve()
        if args.cargo_sources_manifest is not None
        else repo_root / "packaging/flatpak/cargo-sources.json"
    )
    cargo_sources = load_json(cargo_sources_manifest)
    if not isinstance(base, dict) or base.get("app-id") != APP_ID:
        raise ValueError("Flatpak base manifest has an unexpected app ID")
    if not isinstance(cargo_sources, list) or not cargo_sources:
        raise ValueError("Flatpak Cargo source list must be non-empty")
    if not all(isinstance(source, dict) for source in cargo_sources):
        raise ValueError("Flatpak Cargo source entries must be objects")
    cargo_sources = localize_cargo_sources(
        cargo_sources, args.output, args.cargo_source_dir
    )
    runtime_manifest = (
        args.runtime_manifest.resolve()
        if args.runtime_manifest is not None
        else repo_root / "packaging/arch/runtime-bundles.json"
    )
    bundle = load_runtime_bundle(runtime_manifest, args.runtime_bundle)
    if (
        bundle["package_arch"] != "x86_64"
        or bundle["rust_target"] != "x86_64-unknown-linux-gnu"
    ):
        raise ValueError("the checked Flatpak baseline currently supports x86_64 only")
    revision = checked_token("revision", args.revision)
    base["runtime-version"] = checked_token("runtime version", args.runtime_version)
    base["branch"] = checked_token("branch", args.branch)
    base["modules"] = [
        runtime_module(bundle, args.output, args.runtime_source_dir),
        product_module(source_entry(args), cargo_sources, revision),
    ]
    output = json.dumps(base, indent=2, sort_keys=True) + "\n"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    temporary = args.output.with_name(f".{args.output.name}.tmp")
    temporary.write_text(output, encoding="utf-8")
    temporary.replace(args.output)


if __name__ == "__main__":
    main()
