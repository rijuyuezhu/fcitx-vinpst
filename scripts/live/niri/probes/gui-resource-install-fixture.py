#!/usr/bin/env python3
"""Loopback static registry fixture for GUI resource-install live validation."""

from __future__ import annotations

import argparse
import json
import os
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


def write_atomic(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f"{path.name}.{os.getpid()}.tmp")
    with temporary.open("w", encoding="utf-8") as stream:
        stream.write(content)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)


def append_json_line(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(value, separators=(",", ":")) + "\n"
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
    try:
        os.write(descriptor, payload.encode("utf-8"))
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", required=True)
    parser.add_argument("--port-file", required=True)
    parser.add_argument("--request-log", required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = Path(args.root).resolve(strict=True)
    if not root.is_dir():
        raise SystemExit(f"fixture root is not a directory: {root}")
    port_file = Path(args.port_file)
    request_log = Path(args.request_log)

    class Handler(SimpleHTTPRequestHandler):
        def __init__(self, *handler_args: object, **handler_kwargs: object) -> None:
            super().__init__(
                *handler_args,
                directory=str(root),
                **handler_kwargs,
            )

        def log_message(self, format: str, *values: object) -> None:
            del format, values

        def do_GET(self) -> None:
            append_json_line(
                request_log,
                {
                    "method": "GET",
                    "path": self.path,
                    "client": self.client_address[0],
                },
            )
            super().do_GET()

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    write_atomic(port_file, f"{server.server_port}\n")
    try:
        server.serve_forever(poll_interval=0.05)
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
