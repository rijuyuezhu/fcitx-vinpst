#!/usr/bin/env python3
"""Tunnel one authenticated HTTPS CONNECT request without retaining payloads."""

import argparse
import base64
import json
import socket
import socketserver
import ssl
import threading
from pathlib import Path
from typing import Any

MAX_HEADER_BYTES = 16 * 1024
TUNNEL_TIMEOUT_SECONDS = 10


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(
        json.dumps(value, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    temporary.replace(path)


def parse_headers(payload: bytes) -> tuple[str, dict[str, str]]:
    text = payload.decode("iso-8859-1")
    lines = text.split("\r\n")
    if not lines or not lines[0]:
        raise ValueError("missing proxy request line")
    headers: dict[str, str] = {}
    for line in lines[1:]:
        if not line:
            break
        name, separator, value = line.partition(":")
        if not separator or not name.strip():
            raise ValueError("invalid proxy request header")
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


class ConnectProxyServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True

    def __init__(self, args: argparse.Namespace) -> None:
        super().__init__(("127.0.0.1", 0), ConnectProxyHandler)
        self.args = args
        self.request_count = 0


class ConnectProxyHandler(socketserver.BaseRequestHandler):
    server: ConnectProxyServer

    def fail(self, status: str, message: str) -> None:
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
        self.server.args.error_file.write_text(message + "\n", encoding="utf-8")
        threading.Thread(target=self.server.shutdown, daemon=True).start()

    def handle(self) -> None:
        request = self.request
        request.settimeout(TUNNEL_TIMEOUT_SECONDS)
        received = bytearray()
        try:
            while b"\r\n\r\n" not in received:
                chunk = request.recv(4096)
                if not chunk:
                    self.fail(
                        "400 Bad Request", "proxy client closed before request headers"
                    )
                    return
                received.extend(chunk)
                if len(received) > MAX_HEADER_BYTES:
                    self.fail(
                        "431 Request Header Fields Too Large",
                        "proxy headers are too large",
                    )
                    return
            header_end = received.index(b"\r\n\r\n") + 4
            request_line, headers = parse_headers(bytes(received[:header_end]))
            method, target, version = request_line.split(" ", 2)
            if method != "CONNECT" or version != "HTTP/1.1":
                self.fail(
                    "405 Method Not Allowed", "expected one HTTP/1.1 CONNECT request"
                )
                return
            target_host, target_port = parse_connect_target(target)
        except (OSError, UnicodeDecodeError, ValueError) as error:
            self.fail("400 Bad Request", f"invalid CONNECT request: {error}")
            return

        args = self.server.args
        if target_host != args.expected_host or target_port != args.expected_port:
            self.fail("502 Bad Gateway", "unexpected CONNECT target")
            return
        credentials = f"{args.proxy_username}:{args.proxy_password}".encode()
        expected_authorization = "Basic " + base64.b64encode(credentials).decode()
        if headers.get("proxy-authorization", "") != expected_authorization:
            self.fail(
                "407 Proxy Authentication Required", "invalid proxy authorization"
            )
            return

        try:
            upstream = socket.create_connection(
                (args.upstream_host, args.upstream_port),
                timeout=TUNNEL_TIMEOUT_SECONDS,
            )
        except OSError:
            self.fail("502 Bad Gateway", "failed to connect to proxy upstream")
            return

        self.server.request_count += 1
        try:
            request.sendall(
                b"HTTP/1.1 200 Connection Established\r\n"
                b"Proxy-Agent: vinput-fixture\r\n\r\n"
            )
            buffered_tls = bytes(received[header_end:])
            if buffered_tls:
                upstream.sendall(buffered_tls)
            counts = [len(buffered_tls), 0]
            tunnel_errors: list[str] = []

            def pump(
                source: socket.socket, destination: socket.socket, index: int
            ) -> None:
                source.settimeout(TUNNEL_TIMEOUT_SECONDS)
                try:
                    while True:
                        data = source.recv(64 * 1024)
                        if not data:
                            break
                        destination.sendall(data)
                        counts[index] += len(data)
                except TimeoutError:
                    tunnel_errors.append("timeout")
                except OSError:
                    pass
                finally:
                    try:
                        destination.shutdown(socket.SHUT_WR)
                    except OSError:
                        pass

            client_to_upstream = threading.Thread(
                target=pump,
                args=(request, upstream, 0),
                daemon=True,
            )
            upstream_to_client = threading.Thread(
                target=pump,
                args=(upstream, request, 1),
                daemon=True,
            )
            client_to_upstream.start()
            upstream_to_client.start()
            client_to_upstream.join()
            upstream_to_client.join()
        finally:
            upstream.close()

        trace: dict[str, Any] = {
            "event": "connect-tunnel",
            "request_count": self.server.request_count,
            "method": "CONNECT",
            "target_host": target_host,
            "target_port": target_port,
            "proxy_authorization_scheme": "Basic",
            "proxy_authorization_value_recorded": False,
            "proxy_authenticated": True,
            "proxy_tls": args.tls_cert is not None,
            "client_to_upstream_bytes": counts[0],
            "upstream_to_client_bytes": counts[1],
            "tunnel_timeout": bool(tunnel_errors),
            "payload_recorded": False,
        }
        write_json(args.trace_file, trace)
        threading.Thread(target=self.server.shutdown, daemon=True).start()


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
    parser.add_argument("--tls-cert", type=Path)
    parser.add_argument("--tls-key", type=Path)
    args = parser.parse_args()
    for name in ("expected_host", "upstream_host", "proxy_username", "proxy_password"):
        if not getattr(args, name):
            parser.error(f"--{name.replace('_', '-')} must be non-empty")
    for name in ("expected_port", "upstream_port"):
        if not 1 <= getattr(args, name) <= 65_535:
            parser.error(f"--{name.replace('_', '-')} must be from 1 to 65535")
    if bool(args.tls_cert) != bool(args.tls_key):
        parser.error("--tls-cert and --tls-key must be provided together")
    return args


def main() -> int:
    args = parse_args()
    for path in (args.ready_file, args.trace_file, args.error_file):
        path.unlink(missing_ok=True)
    server = ConnectProxyServer(args)
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
            "proxy_url": f"{scheme}://{host}:{port}",
            "proxy_tls": scheme == "https",
            "proxy_auth_required": True,
            "proxy_credentials_recorded": False,
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
