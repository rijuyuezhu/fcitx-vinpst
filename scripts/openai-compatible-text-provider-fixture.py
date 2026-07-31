#!/usr/bin/env python3
"""Serve one deterministic OpenAI-compatible text transformation request."""

import argparse
import json
import re
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

TAG_PATTERN = re.compile(
    r"<(?P<tag>asr|selected)>\n(?P<text>.*?)\n</(?P=tag)>", re.DOTALL
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
    daemon_threads = True

    def __init__(self, args: argparse.Namespace) -> None:
        super().__init__(("127.0.0.1", 0), FixtureHandler)
        self.args = args
        self.request_count = 0


class FixtureHandler(BaseHTTPRequestHandler):
    server: FixtureServer

    def log_message(self, _format: str, *_args: object) -> None:
        return

    def send_json(self, status: int, value: object) -> None:
        body = json.dumps(value, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)

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
        if not selected:
            self.fail(400, "the command request omitted selected text")
            return
        if not raw_asr:
            self.fail(400, "the command request omitted raw ASR text")
            return

        candidate = f"{args.response_prefix}{selected} | command: {raw_asr}"
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
        }
        write_json(args.trace_file, trace)
        response_content = json.dumps({"candidates": [candidate]}, ensure_ascii=False)
        self.send_json(
            200,
            {
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
            },
        )
        threading.Thread(target=self.server.shutdown, daemon=True).start()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ready-file", type=Path, required=True)
    parser.add_argument("--trace-file", type=Path, required=True)
    parser.add_argument("--error-file", type=Path, required=True)
    parser.add_argument("--api-key", required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--response-prefix", default="external-http: ")
    args = parser.parse_args()
    if not args.api_key:
        parser.error("--api-key must be non-empty")
    if not args.model:
        parser.error("--model must be non-empty")
    if not args.response_prefix:
        parser.error("--response-prefix must be non-empty")
    return args


def main() -> int:
    args = parse_args()
    for path in (args.ready_file, args.trace_file, args.error_file):
        path.unlink(missing_ok=True)
    server = FixtureServer(args)
    host, port = server.server_address
    write_json(
        args.ready_file,
        {
            "event": "ready",
            "host": host,
            "port": port,
            "base_url": f"http://{host}:{port}/v1",
            "path": "/v1/chat/completions",
            "api_key_recorded": False,
        },
    )
    try:
        server.serve_forever()
    finally:
        server.server_close()
    if server.request_count != 1:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
