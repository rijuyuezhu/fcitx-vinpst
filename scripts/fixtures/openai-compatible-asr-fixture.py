#!/usr/bin/env python3
"""Serve one deterministic OpenAI-compatible audio transcription request."""

import argparse
import array
import hashlib
import io
import json
import ssl
import threading
import time
import wave
from email import policy
from email.parser import BytesParser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(
        json.dumps(value, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    temporary.replace(path)


def parse_multipart(
    content_type: str, body: bytes
) -> tuple[dict[str, str], dict[str, Any]]:
    message = BytesParser(policy=policy.default).parsebytes(
        (f"MIME-Version: 1.0\r\nContent-Type: {content_type}\r\n\r\n").encode() + body
    )
    if not message.is_multipart():
        raise ValueError("request body is not multipart")
    fields: dict[str, str] = {}
    file_part: dict[str, Any] | None = None
    for part in message.iter_parts():
        name = part.get_param("name", header="content-disposition")
        if not isinstance(name, str) or not name:
            raise ValueError("multipart part omitted its field name")
        payload = part.get_payload(decode=True)
        if payload is None:
            raise ValueError(f"multipart field {name!r} has no payload")
        filename = part.get_filename()
        if filename is not None:
            if file_part is not None:
                raise ValueError("request included more than one file")
            file_part = {
                "name": name,
                "filename": filename,
                "content_type": part.get_content_type(),
                "payload": payload,
            }
        else:
            fields[name] = payload.decode("utf-8")
    if file_part is None:
        raise ValueError("request omitted its audio file")
    return fields, file_part


def inspect_wav(payload: bytes) -> dict[str, int | str]:
    with wave.open(io.BytesIO(payload), "rb") as wav:
        channels = wav.getnchannels()
        sample_rate = wav.getframerate()
        sample_width = wav.getsampwidth()
        frames = wav.getnframes()
        pcm = wav.readframes(frames)
    if sample_width != 2:
        raise ValueError(f"expected 16-bit PCM WAV, got {sample_width * 8}-bit")
    samples = array.array("h")
    samples.frombytes(pcm)
    if samples.itemsize != 2:
        raise ValueError("host signed-short width is not 16-bit")
    if __import__("sys").byteorder != "little":
        samples.byteswap()
    peak = max((abs(sample) for sample in samples), default=0)
    return {
        "sha256": hashlib.sha256(payload).hexdigest(),
        "bytes": len(payload),
        "channels": channels,
        "sample_rate": sample_rate,
        "sample_width_bits": sample_width * 8,
        "frames": frames,
        "peak": peak,
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

    def send_json(self, status: int, value: object, body_delay_ms: int = 0) -> None:
        body = json.dumps(value, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
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
        self.send_json(status, {"error": message})
        self.server.args.error_file.write_text(message + "\n", encoding="utf-8")
        threading.Thread(target=self.server.shutdown, daemon=True).start()

    def do_POST(self) -> None:
        args = self.server.args
        if self.path != "/v1/audio/transcriptions":
            self.fail(404, f"unexpected request path: {self.path}")
            return
        if self.headers.get("Authorization") != f"Bearer {args.api_key}":
            self.fail(401, "missing or invalid bearer token")
            return
        content_type = self.headers.get("Content-Type", "")
        if not content_type.lower().startswith("multipart/form-data;"):
            self.fail(400, f"unexpected Content-Type: {content_type!r}")
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
            fields, file_part = parse_multipart(
                content_type,
                self.rfile.read(content_length),
            )
            wav = inspect_wav(file_part["payload"])
        except (UnicodeDecodeError, ValueError, wave.Error) as error:
            self.fail(400, f"invalid multipart transcription request: {error}")
            return
        if file_part["name"] != "file":
            self.fail(400, f"unexpected audio field: {file_part['name']!r}")
            return
        if file_part["content_type"] != "audio/wav":
            self.fail(
                400, f"unexpected audio content type: {file_part['content_type']!r}"
            )
            return
        if fields.get("model") != args.model:
            self.fail(400, f"unexpected model: {fields.get('model')!r}")
            return
        if fields.get("language") != args.language:
            self.fail(400, f"unexpected language: {fields.get('language')!r}")
            return
        if fields.get("prompt") != args.prompt:
            self.fail(400, "unexpected prompt")
            return
        if wav["sample_rate"] != args.sample_rate:
            self.fail(400, f"unexpected sample rate: {wav['sample_rate']}")
            return
        if wav["channels"] != args.channels:
            self.fail(400, f"unexpected channel count: {wav['channels']}")
            return
        if wav["frames"] <= 0 or wav["peak"] <= 0:
            self.fail(400, "WAV contained no audible PCM signal")
            return

        self.server.request_count += 1
        trace: dict[str, Any] = {
            "event": "request",
            "request_count": self.server.request_count,
            "method": "POST",
            "path": self.path,
            "authorization_scheme": "Bearer",
            "authorization_value_recorded": False,
            "content_type": "multipart/form-data",
            "file_field": file_part["name"],
            "file_name": file_part["filename"],
            "file_content_type": file_part["content_type"],
            "model": fields["model"],
            "language": fields["language"],
            "prompt_matched": True,
            "prompt_value_recorded": False,
            "wav": wav,
            "response_text": args.response_text,
            "response_status": args.response_status,
            "response_delay_ms": args.response_delay_ms,
            "response_body_delay_ms": args.response_body_delay_ms,
            "response_padding_bytes": args.response_padding_bytes,
        }
        write_json(args.trace_file, trace)
        if args.response_delay_ms:
            time.sleep(args.response_delay_ms / 1000)
        if 200 <= args.response_status < 300:
            response: dict[str, object] = {"text": args.response_text}
        else:
            response = {"error": args.response_error}
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
    parser.add_argument("--language", required=True)
    parser.add_argument("--prompt", required=True)
    parser.add_argument("--response-text", required=True)
    parser.add_argument("--response-status", type=int, default=200)
    parser.add_argument("--response-error", default="fixture request failed")
    parser.add_argument("--response-delay-ms", type=int, default=0)
    parser.add_argument("--response-body-delay-ms", type=int, default=0)
    parser.add_argument("--response-padding-bytes", type=int, default=0)
    parser.add_argument("--tls-cert", type=Path)
    parser.add_argument("--tls-key", type=Path)
    parser.add_argument("--sample-rate", type=int, default=16_000)
    parser.add_argument("--channels", type=int, default=1)
    args = parser.parse_args()
    if not 0 <= args.port <= 65_535:
        parser.error("--port must be from 0 to 65535")
    for name in ("api_key", "model", "language", "prompt", "response_text"):
        if not getattr(args, name):
            parser.error(f"--{name.replace('_', '-')} must be non-empty")
    if args.sample_rate <= 0:
        parser.error("--sample-rate must be positive")
    if args.channels <= 0:
        parser.error("--channels must be positive")
    if not 200 <= args.response_status <= 599:
        parser.error("--response-status must be from 200 to 599")
    if args.response_delay_ms < 0:
        parser.error("--response-delay-ms must be non-negative")
    if args.response_body_delay_ms < 0:
        parser.error("--response-body-delay-ms must be non-negative")
    if not 0 <= args.response_padding_bytes <= 16 * 1024 * 1024:
        parser.error("--response-padding-bytes must be from 0 to 16777216")
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
            "path": "/v1/audio/transcriptions",
            "tls": scheme == "https",
            "api_key_recorded": False,
            "prompt_value_recorded": False,
        },
    )
    try:
        server.serve_forever()
    finally:
        server.server_close()
    return 0 if server.request_count == 1 else 1


if __name__ == "__main__":
    raise SystemExit(main())
