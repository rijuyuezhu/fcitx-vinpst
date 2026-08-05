#!/usr/bin/env python3
"""Loopback notification-feed and direct-argv opener fixture for GUI live gates."""

from __future__ import annotations

import argparse
import json
import os
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import NoReturn


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
    payload = json.dumps(value, ensure_ascii=False, separators=(",", ":")) + "\n"
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
    try:
        os.write(descriptor, payload.encode("utf-8"))
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def run_opener(target: str) -> int:
    log_value = os.environ.get("VINPUT_GUI_DESKTOP_LIVE_OPEN_LOG", "")
    if not log_value:
        print("VINPUT_GUI_DESKTOP_LIVE_OPEN_LOG is required", file=sys.stderr)
        return 2
    append_json_line(
        Path(log_value),
        {
            "argument_count": 1,
            "target": target,
        },
    )
    return 0


def serve(args: argparse.Namespace) -> NoReturn:
    notification = {
        "id": args.notification_id,
        "title": {
            "en_US": "Desktop integration live fixture",
            "zh_CN": "桌面集成实时测试",
        },
        "text": {
            "en_US": "Open the validated Details target and persist this notification as read.",
            "zh_CN": "打开已验证的详情目标，并将此通知持久化为已读。",
        },
        "url": args.details_url,
    }
    body = json.dumps(notification, ensure_ascii=False).encode("utf-8")
    request_log = Path(args.request_log)

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:
            append_json_line(request_log, {"path": self.path})
            if self.path != "/notification.json":
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header("Content-Type", "application/json; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, _format: str, *_args: object) -> None:
            return

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    port = server.server_address[1]
    write_atomic(Path(args.port_file), f"{port}\n")
    try:
        server.serve_forever(poll_interval=0.1)
    finally:
        server.server_close()
    raise SystemExit(0)


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    subcommands = result.add_subparsers(dest="command", required=True)

    opener = subcommands.add_parser("open")
    opener.add_argument("target")

    server = subcommands.add_parser("serve")
    server.add_argument("--port-file", required=True)
    server.add_argument("--request-log", required=True)
    server.add_argument("--notification-id", type=int, required=True)
    server.add_argument("--details-url", required=True)
    return result


def main() -> int:
    if len(sys.argv) == 2 and sys.argv[1] not in {"open", "serve", "-h", "--help"}:
        return run_opener(sys.argv[1])
    args = parser().parse_args()
    if args.command == "open":
        return run_opener(args.target)
    serve(args)


if __name__ == "__main__":
    raise SystemExit(main())
