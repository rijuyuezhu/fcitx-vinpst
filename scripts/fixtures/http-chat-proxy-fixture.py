#!/usr/bin/env python3
"""Act as one deterministic HTTP proxy for an OpenAI-compatible chat request."""

import argparse
import hashlib
import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)


class ProxyServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, args: argparse.Namespace) -> None:
        super().__init__(("127.0.0.1", 0), ProxyHandler)
        self.args = args
        self.request_count = 0


class ProxyHandler(BaseHTTPRequestHandler):
    server: ProxyServer

    def log_message(self, _format: str, *_args: object) -> None:
        return

    def send_json(self, status: int, value: object) -> None:
        body = json.dumps(value).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self) -> None:
        args = self.server.args
        target = urlsplit(self.path)
        authorization = self.headers.get("Authorization", "")
        content_type = self.headers.get("Content-Type", "")
        try:
            content_length = int(self.headers.get("Content-Length", ""))
        except ValueError:
            content_length = -1
        if (
            target.scheme != "http"
            or target.hostname != args.expected_host
            or target.path != "/v1/chat/completions"
        ):
            self.send_json(502, {"error": {"message": "unexpected proxy target"}})
            threading.Thread(target=self.server.shutdown, daemon=True).start()
            return
        if authorization != f"Bearer {args.api_key}":
            self.send_json(407, {"error": {"message": "invalid bearer token"}})
            threading.Thread(target=self.server.shutdown, daemon=True).start()
            return
        if not content_type.lower().startswith("application/json"):
            self.send_json(400, {"error": {"message": "unexpected content type"}})
            threading.Thread(target=self.server.shutdown, daemon=True).start()
            return
        if content_length <= 0:
            self.send_json(400, {"error": {"message": "invalid content length"}})
            threading.Thread(target=self.server.shutdown, daemon=True).start()
            return
        body = self.rfile.read(content_length)
        try:
            request = json.loads(body)
        except (json.JSONDecodeError, UnicodeDecodeError):
            self.send_json(400, {"error": {"message": "invalid JSON body"}})
            threading.Thread(target=self.server.shutdown, daemon=True).start()
            return
        messages = request.get("messages")
        content = ""
        if isinstance(messages, list) and len(messages) == 1:
            message = messages[0]
            if isinstance(message, dict) and isinstance(message.get("content"), str):
                content = message["content"]
        if (
            request.get("model") != args.model
            or request.get("stream") is not False
            or request.get("response_format") != {"type": "json_object"}
            or args.input_text not in content
        ):
            self.send_json(400, {"error": {"message": "unexpected chat payload"}})
            threading.Thread(target=self.server.shutdown, daemon=True).start()
            return

        self.server.request_count += 1
        write_json(
            args.trace_file,
            {
                "event": "proxy-request",
                "request_count": self.server.request_count,
                "method": "POST",
                "absolute_target": self.path,
                "target_scheme": target.scheme,
                "target_host": target.hostname,
                "target_path": target.path,
                "authorization_scheme": "Bearer",
                "authorization_value_recorded": False,
                "content_type": "application/json",
                "model": request["model"],
                "input_text_present": True,
                "input_text_recorded": False,
                "body_sha256": hashlib.sha256(body).hexdigest(),
                "body_bytes": len(body),
                "response_text": args.response_text,
            },
        )
        response_content = json.dumps({"candidates": [args.response_text]})
        self.send_json(
            200,
            {
                "id": "proxy-response",
                "object": "chat.completion",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": response_content,
                        },
                        "finish_reason": "stop",
                    }
                ],
            },
        )
        threading.Thread(target=self.server.shutdown, daemon=True).start()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ready-file", type=Path, required=True)
    parser.add_argument("--trace-file", type=Path, required=True)
    parser.add_argument("--api-key", required=True)
    parser.add_argument("--expected-host", required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--input-text", required=True)
    parser.add_argument("--response-text", required=True)
    args = parser.parse_args()
    for name in ("api_key", "expected_host", "model", "input_text", "response_text"):
        if not getattr(args, name):
            parser.error(f"--{name.replace('_', '-')} must be non-empty")
    return args


def main() -> int:
    args = parse_args()
    args.ready_file.unlink(missing_ok=True)
    args.trace_file.unlink(missing_ok=True)
    server = ProxyServer(args)
    host, port = server.server_address
    write_json(
        args.ready_file,
        {
            "event": "ready",
            "proxy_url": f"http://{host}:{port}",
            "api_key_recorded": False,
            "input_text_recorded": False,
        },
    )
    try:
        server.serve_forever()
    finally:
        server.server_close()
    return 0 if server.request_count == 1 else 1


if __name__ == "__main__":
    raise SystemExit(main())
