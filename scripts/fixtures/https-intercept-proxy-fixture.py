#!/usr/bin/env python3
"""Terminate one authenticated CONNECT tunnel and relay one HTTPS exchange."""

import argparse
import base64
import json
import socket
import socketserver
import ssl
import threading
from pathlib import Path
from typing import Any

MAX_HEADER_BYTES = 64 * 1024
MAX_BODY_BYTES = 16 * 1024 * 1024
SOCKET_TIMEOUT_SECONDS = 10


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(
        json.dumps(value, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    temporary.replace(path)


def read_headers(stream: socket.socket) -> tuple[bytes, bytes]:
    received = bytearray()
    while b"\r\n\r\n" not in received:
        chunk = stream.recv(4096)
        if not chunk:
            raise ValueError("peer closed before HTTP headers")
        received.extend(chunk)
        if len(received) > MAX_HEADER_BYTES:
            raise ValueError("HTTP headers are too large")
    header_end = received.index(b"\r\n\r\n") + 4
    return bytes(received[:header_end]), bytes(received[header_end:])


def parse_headers(payload: bytes) -> tuple[str, dict[str, str]]:
    text = payload.decode("iso-8859-1")
    lines = text.split("\r\n")
    if not lines or not lines[0]:
        raise ValueError("missing HTTP request or status line")
    headers: dict[str, str] = {}
    for line in lines[1:]:
        if not line:
            break
        name, separator, value = line.partition(":")
        if not separator or not name.strip():
            raise ValueError("invalid HTTP header")
        headers[name.strip().lower()] = value.strip()
    return lines[0], headers


def parse_connect_target(target: str) -> tuple[str, int]:
    if target.startswith("["):
        host, separator, remainder = target[1:].partition("]")
        if separator != "]" or not remainder.startswith(":"):
            raise ValueError("invalid bracketed CONNECT target")
        port_text = remainder[1:]
    else:
        host, separator, port_text = target.rpartition(":")
        if not separator:
            raise ValueError("CONNECT target omitted its port")
    if not host:
        raise ValueError("CONNECT target omitted its host")
    port = int(port_text)
    if not 1 <= port <= 65_535:
        raise ValueError("CONNECT target port is out of range")
    return host, port


def content_length(headers: dict[str, str]) -> int:
    if "transfer-encoding" in headers:
        raise ValueError("chunked HTTP messages are unsupported by this fixture")
    value = headers.get("content-length")
    if value is None:
        raise ValueError("HTTP message omitted Content-Length")
    length = int(value)
    if not 0 <= length <= MAX_BODY_BYTES:
        raise ValueError("HTTP message body is out of range")
    return length


def read_http_message(stream: socket.socket) -> tuple[bytes, int, int]:
    headers_payload, buffered_body = read_headers(stream)
    _, headers = parse_headers(headers_payload)
    body_length = content_length(headers)
    body = bytearray(buffered_body)
    if len(body) > body_length:
        raise ValueError("HTTP message contains pipelined bytes")
    while len(body) < body_length:
        chunk = stream.recv(min(64 * 1024, body_length - len(body)))
        if not chunk:
            raise ValueError("peer closed before the HTTP body completed")
        body.extend(chunk)
    return headers_payload + bytes(body), len(headers_payload), body_length


class InterceptProxyServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True

    def __init__(self, args: argparse.Namespace) -> None:
        super().__init__(("127.0.0.1", 0), InterceptProxyHandler)
        self.args = args
        self.request_count = 0


class InterceptProxyHandler(socketserver.BaseRequestHandler):
    server: InterceptProxyServer

    def stop_server(self) -> None:
        threading.Thread(target=self.server.shutdown, daemon=True).start()

    def record_error(self, message: str) -> None:
        self.server.args.error_file.write_text(message + "\n", encoding="utf-8")
        self.stop_server()

    def fail_connect(self, status: str, message: str) -> None:
        body = json.dumps({"error": message}).encode("utf-8")
        response = (
            f"HTTP/1.1 {status}\r\n"
            "Content-Type: application/json\r\n"
            f"Content-Length: {len(body)}\r\n"
            "Connection: close\r\n\r\n"
        ).encode("ascii") + body
        try:
            self.request.sendall(response)
        except OSError:
            pass
        self.record_error(message)

    def handle(self) -> None:
        request = self.request
        request.settimeout(SOCKET_TIMEOUT_SECONDS)
        try:
            connect_payload, buffered = read_headers(request)
            if buffered:
                self.fail_connect(
                    "400 Bad Request", "CONNECT request included unexpected payload"
                )
                return
            request_line, headers = parse_headers(connect_payload)
            method, target, version = request_line.split(" ", 2)
            if method != "CONNECT" or version != "HTTP/1.1":
                self.fail_connect(
                    "405 Method Not Allowed", "expected one HTTP/1.1 CONNECT request"
                )
                return
            target_host, target_port = parse_connect_target(target)
        except (OSError, UnicodeDecodeError, ValueError):
            self.fail_connect("400 Bad Request", "invalid CONNECT request")
            return

        args = self.server.args
        if target_host != args.expected_host or target_port != args.expected_port:
            self.fail_connect("502 Bad Gateway", "unexpected CONNECT target")
            return
        credentials = f"{args.proxy_username}:{args.proxy_password}".encode()
        expected_authorization = "Basic " + base64.b64encode(credentials).decode()
        if headers.get("proxy-authorization", "") != expected_authorization:
            self.fail_connect(
                "407 Proxy Authentication Required", "invalid proxy authorization"
            )
            return

        upstream_socket: socket.socket | None = None
        client_tls: ssl.SSLSocket | None = None
        upstream_tls: ssl.SSLSocket | None = None
        stage = "upstream TCP connection"
        try:
            upstream_socket = socket.create_connection(
                (args.upstream_host, args.upstream_port),
                timeout=SOCKET_TIMEOUT_SECONDS,
            )
            stage = "upstream TLS handshake"
            upstream_context = ssl.create_default_context(cafile=args.upstream_ca_cert)
            upstream_tls = upstream_context.wrap_socket(
                upstream_socket,
                server_hostname=target_host,
            )
            upstream_tls.settimeout(SOCKET_TIMEOUT_SECONDS)
            upstream_tls_version = upstream_tls.version()

            stage = "CONNECT response"
            request.sendall(
                b"HTTP/1.1 200 Connection Established\r\n"
                b"Proxy-Agent: vinpst-fixture\r\n\r\n"
            )

            stage = "client TLS handshake"
            client_context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            client_context.load_cert_chain(args.intercept_cert, args.intercept_key)
            client_tls = client_context.wrap_socket(request, server_side=True)
            client_tls.settimeout(SOCKET_TIMEOUT_SECONDS)
            client_tls_version = client_tls.version()

            stage = "intercepted request read"
            request_payload, request_header_bytes, request_body_bytes = (
                read_http_message(client_tls)
            )
            stage = "intercepted request relay"
            upstream_tls.sendall(request_payload)

            stage = "upstream response read"
            response_payload, response_header_bytes, response_body_bytes = (
                read_http_message(upstream_tls)
            )
            stage = "intercepted response relay"
            client_tls.sendall(response_payload)
        except (OSError, ssl.SSLError, UnicodeDecodeError, ValueError):
            self.record_error(f"TLS interception relay failed during {stage}")
            return
        finally:
            if upstream_tls is not None:
                try:
                    upstream_tls.close()
                except OSError:
                    pass
            elif upstream_socket is not None:
                upstream_socket.close()
            if client_tls is not None:
                try:
                    client_tls.close()
                except OSError:
                    pass

        self.server.request_count += 1
        trace: dict[str, Any] = {
            "event": "tls-intercept",
            "request_count": self.server.request_count,
            "method": "CONNECT",
            "target_host": target_host,
            "target_port": target_port,
            "proxy_authorization_scheme": "Basic",
            "proxy_authorization_value_recorded": False,
            "proxy_authenticated": True,
            "client_tls_version": client_tls_version,
            "upstream_tls_version": upstream_tls_version,
            "request_header_bytes": request_header_bytes,
            "request_body_bytes": request_body_bytes,
            "response_header_bytes": response_header_bytes,
            "response_body_bytes": response_body_bytes,
            "payload_recorded": False,
        }
        write_json(args.trace_file, trace)
        self.stop_server()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ready-file", type=Path, required=True)
    parser.add_argument("--trace-file", type=Path, required=True)
    parser.add_argument("--error-file", type=Path, required=True)
    parser.add_argument("--expected-host", required=True)
    parser.add_argument("--expected-port", type=int, required=True)
    parser.add_argument("--upstream-host", required=True)
    parser.add_argument("--upstream-port", type=int, required=True)
    parser.add_argument("--proxy-username", required=True)
    parser.add_argument("--proxy-password", required=True)
    parser.add_argument("--intercept-cert", type=Path, required=True)
    parser.add_argument("--intercept-key", type=Path, required=True)
    parser.add_argument("--upstream-ca-cert", type=Path, required=True)
    args = parser.parse_args()
    for name in ("expected_host", "upstream_host", "proxy_username", "proxy_password"):
        if not getattr(args, name):
            parser.error(f"--{name.replace('_', '-')} must be non-empty")
    for name in ("expected_port", "upstream_port"):
        if not 1 <= getattr(args, name) <= 65_535:
            parser.error(f"--{name.replace('_', '-')} must be from 1 to 65535")
    return args


def main() -> int:
    args = parse_args()
    for path in (args.ready_file, args.trace_file, args.error_file):
        path.unlink(missing_ok=True)
    server = InterceptProxyServer(args)
    host, port = server.server_address
    write_json(
        args.ready_file,
        {
            "event": "ready",
            "host": host,
            "port": port,
            "proxy_url": f"http://{host}:{port}",
            "proxy_auth_required": True,
            "proxy_credentials_recorded": False,
            "intercept_tls": True,
            "payload_recorded": False,
        },
    )
    try:
        server.serve_forever()
    finally:
        server.server_close()
    return int(
        server.request_count != 1
        or not args.trace_file.is_file()
        or args.error_file.exists()
    )


if __name__ == "__main__":
    raise SystemExit(main())
