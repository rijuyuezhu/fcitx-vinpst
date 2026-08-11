#!/usr/bin/env python3
"""Serve one deterministic OpenAI-compatible text transformation request."""

import argparse
import json
import re
import ssl
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

TAG_PATTERN = re.compile(
    r"<vinput-(?P<tag>asr|selected)>\n(?P<text>.*?)\n</vinput-(?P=tag)>", re.DOTALL
)


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(
        json.dumps(value, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    temporary.replace(path)


def extract_tagged_text(content: str) -> dict[str, str]:
    return {
        match.group("tag"): match.group("text").strip()
        for match in TAG_PATTERN.finditer(content)
    }


class FixtureServer(ThreadingHTTPServer):
    allow_reuse_address = True
    daemon_threads = True

    def __init__(self, args: argparse.Namespace) -> None:
        super().__init__(("127.0.0.1", args.port), FixtureHandler)
        self.args = args
        self.request_count = 0


class FixtureHandler(BaseHTTPRequestHandler):
    server: FixtureServer

    def log_message(self, _format: str, *_args: object) -> None:
        return

    def send_json(
        self,
        status: int,
        value: object,
        body_delay_ms: int = 0,
        location: str | None = None,
    ) -> None:
        body = json.dumps(value, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        if location is not None:
            self.send_header("Location", location)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        if body_delay_ms:
            time.sleep(body_delay_ms / 1000)
        try:
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def fail(self, status: int, message: str) -> None:
        self.send_json(status, {"error": {"message": message}})
        self.server.args.error_file.write_text(message + "\n", encoding="utf-8")
        threading.Thread(target=self.server.shutdown, daemon=True).start()

    def do_POST(self) -> None:
        args = self.server.args
        if self.path != "/v1/chat/completions":
            self.fail(404, f"unexpected request path: {self.path}")
            return
        expected_authorization = f"Bearer {args.api_key}"
        if self.headers.get("Authorization") != expected_authorization:
            self.fail(401, "missing or invalid bearer token")
            return
        try:
            content_length = int(self.headers.get("Content-Length", ""))
        except ValueError:
            self.fail(400, "invalid Content-Length")
            return
        if content_length <= 0:
            self.fail(400, "request body is empty")
            return
        try:
            request = json.loads(self.rfile.read(content_length))
        except (json.JSONDecodeError, UnicodeDecodeError) as error:
            self.fail(400, f"invalid JSON request: {error}")
            return
        if request.get("model") != args.model:
            self.fail(400, f"unexpected model: {request.get('model')!r}")
            return
        if request.get("stream") is not False:
            self.fail(400, "stream must be false")
            return
        if request.get("response_format") != {"type": "json_object"}:
            self.fail(400, "response_format must request a JSON object")
            return
        messages = request.get("messages")
        if not isinstance(messages, list) or len(messages) != 1:
            self.fail(400, "exactly one chat message is required")
            return
        message = messages[0]
        if not isinstance(message, dict) or message.get("role") != "user":
            self.fail(400, "the chat message must have user role")
            return
        content = message.get("content")
        if not isinstance(content, str):
            self.fail(400, "the user message content must be text")
            return
        tagged = extract_tagged_text(content)
        selected = tagged.get("selected", "")
        raw_asr = tagged.get("asr", "")
        if not selected and not args.allow_empty_selected:
            self.fail(400, "the command request omitted selected text")
            return
        if not raw_asr:
            self.fail(400, "the command request omitted raw ASR text")
            return

        if selected:
            candidate = f"{args.response_prefix}{selected} | command: {raw_asr}"
        else:
            candidate = f"{args.response_prefix}{raw_asr}"
        self.server.request_count += 1
        trace: dict[str, Any] = {
            "event": "request",
            "request_count": self.server.request_count,
            "method": "POST",
            "path": self.path,
            "authorization_scheme": "Bearer",
            "authorization_value_recorded": False,
            "content_type": self.headers.get("Content-Type", ""),
            "model": request["model"],
            "stream": request["stream"],
            "response_format": request["response_format"],
            "selected_text": selected,
            "raw_asr_text": raw_asr,
            "candidate": candidate,
            "response_status": args.response_status,
            "response_delay_ms": args.response_delay_ms,
            "response_body_delay_ms": args.response_body_delay_ms,
            "response_padding_bytes": args.response_padding_bytes,
            "response_location_present": bool(args.response_location),
        }
        write_json(args.trace_file, trace)
        if args.response_delay_ms:
            time.sleep(args.response_delay_ms / 1000)
        if args.response_status >= 300:
            response: dict[str, object] = {"error": {"message": args.response_error}}
        else:
            response_content = json.dumps(
                {"candidates": [candidate]}, ensure_ascii=False
            )
            response = {
                "id": "fixture-response",
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
            }
        if args.response_padding_bytes:
            response["padding"] = "x" * args.response_padding_bytes
        self.send_json(
            args.response_status,
            response,
            args.response_body_delay_ms,
        )
        threading.Thread(target=self.server.shutdown, daemon=True).start()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ready-file", type=Path, required=True)
    parser.add_argument("--trace-file", type=Path, required=True)
    parser.add_argument("--error-file", type=Path, required=True)
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--api-key", required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--response-prefix", default="external-http: ")
    parser.add_argument("--response-status", type=int, default=200)
    parser.add_argument("--response-error", default="fixture request failed")
    parser.add_argument("--response-delay-ms", type=int, default=0)
    parser.add_argument("--response-body-delay-ms", type=int, default=0)
    parser.add_argument("--response-padding-bytes", type=int, default=0)
    parser.add_argument("--response-location", default="")
    parser.add_argument("--tls-cert", type=Path)
    parser.add_argument("--tls-key", type=Path)
    parser.add_argument("--allow-empty-selected", action="store_true")
    parser.add_argument("--expect-error", action="store_true")
    args = parser.parse_args()
    if not 0 <= args.port <= 65_535:
        parser.error("--port must be from 0 to 65535")
    if not args.api_key:
        parser.error("--api-key must be non-empty")
    if not args.model:
        parser.error("--model must be non-empty")
    if not args.response_prefix:
        parser.error("--response-prefix must be non-empty")
    if not 200 <= args.response_status <= 599:
        parser.error("--response-status must be from 200 to 599")
    if args.response_delay_ms < 0:
        parser.error("--response-delay-ms must be non-negative")
    if args.response_body_delay_ms < 0:
        parser.error("--response-body-delay-ms must be non-negative")
    if not 0 <= args.response_padding_bytes <= 16 * 1024 * 1024:
        parser.error("--response-padding-bytes must be from 0 to 16777216")
    if args.response_location and not 300 <= args.response_status < 400:
        parser.error("--response-location requires a 3xx response status")
    if bool(args.tls_cert) != bool(args.tls_key):
        parser.error("--tls-cert and --tls-key must be provided together")
    if args.response_status >= 300 and not args.response_error:
        parser.error("--response-error must be non-empty for failure responses")
    return args


def main() -> int:
    args = parse_args()
    for path in (args.ready_file, args.trace_file, args.error_file):
        path.unlink(missing_ok=True)
    server = FixtureServer(args)
    scheme = "http"
    if args.tls_cert is not None and args.tls_key is not None:
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(args.tls_cert, args.tls_key)
        server.socket = context.wrap_socket(server.socket, server_side=True)
        scheme = "https"
    host, port = server.server_address
    write_json(
        args.ready_file,
        {
            "event": "ready",
            "host": host,
            "port": port,
            "base_url": f"{scheme}://{host}:{port}/v1",
            "path": "/v1/chat/completions",
            "tls": scheme == "https",
            "api_key_recorded": False,
        },
    )
    try:
        server.serve_forever()
    finally:
        server.server_close()
    if args.expect_error:
        return int(server.request_count != 0 or not args.error_file.is_file())
    return int(server.request_count != 1 or args.error_file.exists())


if __name__ == "__main__":
    raise SystemExit(main())
