"""Minimal RFC 6455 and Chrome DevTools Protocol clients for live probes."""

import base64
import hashlib
import json
import os
import socket
import struct
import urllib.parse
from typing import Any


class WebSocketError(RuntimeError):
    """Raised for a failed WebSocket handshake or frame exchange."""


class WebSocketClient:
    """Small RFC 6455 client sufficient for JSON text and CDP traffic."""

    def __init__(self, sock: socket.socket, buffered: bytes = b"") -> None:
        self._sock = sock
        self._buffer = bytearray(buffered)

    @classmethod
    def connect(
        cls,
        url: str,
        *,
        headers: dict[str, str] | None = None,
        timeout: float = 10.0,
    ) -> "WebSocketClient":
        parsed = urllib.parse.urlsplit(url)
        if parsed.scheme != "ws" or not parsed.hostname:
            raise WebSocketError(f"unsupported WebSocket URL: {url}")
        port = parsed.port or 80
        path = urllib.parse.urlunsplit(("", "", parsed.path or "/", parsed.query, ""))
        sock = socket.create_connection((parsed.hostname, port), timeout=timeout)
        sock.settimeout(timeout)
        key = base64.b64encode(os.urandom(16)).decode("ascii")
        request_headers = {
            "Host": parsed.netloc,
            "Upgrade": "websocket",
            "Connection": "Upgrade",
            "Sec-WebSocket-Key": key,
            "Sec-WebSocket-Version": "13",
        }
        request_headers.update(headers or {})
        request = "GET " + path + " HTTP/1.1\r\n"
        request += "".join(
            f"{name}: {value}\r\n" for name, value in request_headers.items()
        )
        request += "\r\n"
        sock.sendall(request.encode("ascii"))

        response = bytearray()
        while b"\r\n\r\n" not in response:
            chunk = sock.recv(4096)
            if not chunk:
                raise WebSocketError("WebSocket server closed during handshake")
            response.extend(chunk)
            if len(response) > 64 * 1024:
                raise WebSocketError("WebSocket handshake headers are too large")
        header_bytes, buffered = bytes(response).split(b"\r\n\r\n", 1)
        lines = header_bytes.decode("iso-8859-1").split("\r\n")
        if not lines or " 101 " not in f" {lines[0]} ":
            raise WebSocketError(
                f"WebSocket upgrade failed: {lines[0] if lines else ''}"
            )
        response_headers: dict[str, str] = {}
        for line in lines[1:]:
            name, separator, value = line.partition(":")
            if separator:
                response_headers[name.strip().lower()] = value.strip()
        expected_accept = base64.b64encode(
            hashlib.sha1(
                (key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()
            ).digest()
        ).decode("ascii")
        if response_headers.get("sec-websocket-accept") != expected_accept:
            raise WebSocketError("WebSocket server returned an invalid accept key")
        return cls(sock, buffered)

    def close(self) -> None:
        try:
            self._send_frame(0x8, b"")
        except OSError:
            pass
        self._sock.close()

    def send_json(self, value: Any) -> None:
        self.send_text(json.dumps(value, separators=(",", ":"), ensure_ascii=False))

    def send_text(self, text: str) -> None:
        self._send_frame(0x1, text.encode("utf-8"))

    def recv_json(self, timeout: float = 10.0) -> Any:
        return json.loads(self.recv_text(timeout))

    def recv_text(self, timeout: float = 10.0) -> str:
        self._sock.settimeout(timeout)
        fragments = bytearray()
        text_started = False
        while True:
            fin, opcode, payload = self._recv_frame()
            if opcode == 0x8:
                raise WebSocketError("WebSocket peer closed the connection")
            if opcode == 0x9:
                self._send_frame(0xA, payload)
                continue
            if opcode == 0xA:
                continue
            if opcode == 0x1:
                fragments = bytearray(payload)
                text_started = True
            elif opcode == 0x0 and text_started:
                fragments.extend(payload)
            else:
                continue
            if fin:
                return fragments.decode("utf-8")

    def _send_frame(self, opcode: int, payload: bytes) -> None:
        first = 0x80 | opcode
        length = len(payload)
        if length < 126:
            header = struct.pack("!BB", first, 0x80 | length)
        elif length <= 0xFFFF:
            header = struct.pack("!BBH", first, 0x80 | 126, length)
        else:
            header = struct.pack("!BBQ", first, 0x80 | 127, length)
        mask = os.urandom(4)
        masked = bytes(value ^ mask[index % 4] for index, value in enumerate(payload))
        self._sock.sendall(header + mask + masked)

    def _recv_frame(self) -> tuple[bool, int, bytes]:
        first, second = self._read_exact(2)
        fin = bool(first & 0x80)
        opcode = first & 0x0F
        masked = bool(second & 0x80)
        length = second & 0x7F
        if length == 126:
            length = struct.unpack("!H", self._read_exact(2))[0]
        elif length == 127:
            length = struct.unpack("!Q", self._read_exact(8))[0]
        mask = self._read_exact(4) if masked else b""
        payload = self._read_exact(length)
        if masked:
            payload = bytes(
                value ^ mask[index % 4] for index, value in enumerate(payload)
            )
        return fin, opcode, payload

    def _read_exact(self, length: int) -> bytes:
        while len(self._buffer) < length:
            chunk = self._sock.recv(max(4096, length - len(self._buffer)))
            if not chunk:
                raise WebSocketError("WebSocket peer closed unexpectedly")
            self._buffer.extend(chunk)
        result = bytes(self._buffer[:length])
        del self._buffer[:length]
        return result


class CdpClient:
    """Minimal Chrome DevTools Protocol request client."""

    def __init__(self, socket_client: WebSocketClient) -> None:
        self._socket = socket_client
        self._next_id = 1

    def close(self) -> None:
        self._socket.close()

    def call(self, method: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        request_id = self._next_id
        self._next_id += 1
        self._socket.send_json(
            {"id": request_id, "method": method, "params": params or {}}
        )
        while True:
            response = self._socket.recv_json(10.0)
            if response.get("id") != request_id:
                continue
            if "error" in response:
                raise RuntimeError(f"CDP {method} failed: {response['error']}")
            return response.get("result", {})

    def evaluate(self, expression: str) -> Any:
        result = self.call(
            "Runtime.evaluate",
            {
                "expression": expression,
                "awaitPromise": True,
                "returnByValue": True,
            },
        )
        remote_object = result.get("result", {})
        if remote_object.get("subtype") == "error":
            raise RuntimeError(f"browser evaluation failed: {remote_object}")
        return remote_object.get("value")
