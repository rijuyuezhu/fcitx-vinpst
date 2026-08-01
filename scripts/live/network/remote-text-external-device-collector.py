#!/usr/bin/env python3
"""Collect fail-closed remote-text evidence from another network device."""

import argparse
import ipaddress
import json
import os
import subprocess
import time
from pathlib import Path
from typing import Any

from probes.websocket_cdp import WebSocketClient

RELEVANT_EVENT_TYPES = (
    "input_audio_buffer.committed",
    "conversation.item.input_audio_transcription.delta",
    "conversation.item.input_audio_transcription.completed",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--endpoint", required=True)
    parser.add_argument("--output-url", required=True)
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--challenge", required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--timeout-seconds", type=float, default=180.0)
    parser.add_argument("--physical-device-confirmed", action="store_true")
    parser.add_argument("--ip-command", type=Path, default=Path("/usr/bin/ip"))
    parser.add_argument("--ss-command", type=Path, default=Path("/usr/bin/ss"))
    return parser.parse_args()


def normalize_ip(value: str) -> ipaddress.IPv4Address | ipaddress.IPv6Address:
    address = ipaddress.ip_address(value.split("%", 1)[0])
    if isinstance(address, ipaddress.IPv6Address) and address.ipv4_mapped:
        return address.ipv4_mapped
    return address


def split_socket_address(value: str) -> tuple[str, int]:
    if value.startswith("["):
        end = value.rfind("]:")
        if end < 0:
            raise ValueError(f"invalid bracketed socket address: {value}")
        return value[1:end], int(value[end + 2 :])
    host, separator, port = value.rpartition(":")
    if not separator or not host or not port:
        raise ValueError(f"invalid socket address: {value}")
    return host, int(port)


def local_addresses(
    ip_command: Path,
) -> set[ipaddress.IPv4Address | ipaddress.IPv6Address]:
    completed = subprocess.run(
        [str(ip_command), "-j", "address", "show"],
        check=True,
        capture_output=True,
        text=True,
    )
    payload = json.loads(completed.stdout)
    addresses = {ipaddress.ip_address("127.0.0.1"), ipaddress.ip_address("::1")}
    for interface in payload:
        for info in interface.get("addr_info", []):
            value = info.get("local")
            if isinstance(value, str):
                addresses.add(normalize_ip(value))
    return addresses


def established_connections(ss_command: Path, port: int) -> list[dict[str, Any]]:
    completed = subprocess.run(
        [
            str(ss_command),
            "-Htn",
            "state",
            "established",
            f"( sport = :{port} )",
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    connections = []
    for line in completed.stdout.splitlines():
        fields = line.split()
        if len(fields) < 4:
            continue
        local_host, local_port = split_socket_address(fields[-2])
        peer_host, peer_port = split_socket_address(fields[-1])
        connections.append(
            {
                "local_address": str(normalize_ip(local_host)),
                "local_port": local_port,
                "peer_address": str(normalize_ip(peer_host)),
                "peer_port": peer_port,
            }
        )
    return connections


def receive_transcription(
    socket_client: WebSocketClient, challenge: str, timeout_seconds: float
) -> list[dict[str, Any]]:
    deadline = time.monotonic() + timeout_seconds
    events: list[dict[str, Any]] = []
    while len(events) < len(RELEVANT_EVENT_TYPES):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("timed out waiting for the external-device challenge")
        message = socket_client.recv_json(min(remaining, 10.0))
        if not isinstance(message, dict):
            continue
        if message.get("type") in RELEVANT_EVENT_TYPES:
            events.append(message)
    event_types = tuple(event.get("type") for event in events)
    if event_types != RELEVANT_EVENT_TYPES:
        raise RuntimeError(f"unexpected remote output event sequence: {event_types}")
    if events[1].get("delta") != challenge:
        raise RuntimeError(f"unexpected transcription delta: {events[1]}")
    if events[2].get("transcript") != challenge:
        raise RuntimeError(f"unexpected completed transcription: {events[2]}")
    if len({event.get("item_id") for event in events}) != 1:
        raise RuntimeError(f"remote output item ids do not match: {events}")
    return events


def main() -> int:
    args = parse_args()
    api_key = os.environ.get("VINPUT_REMOTE_TEXT_API_KEY", "")
    if not api_key:
        raise RuntimeError("VINPUT_REMOTE_TEXT_API_KEY is required")
    if args.timeout_seconds <= 0:
        raise ValueError("timeout must be positive")
    for command in (args.ip_command, args.ss_command):
        if not command.is_file() or not os.access(command, os.X_OK):
            raise RuntimeError(f"required network command is not executable: {command}")

    args.out_dir.mkdir(parents=True, exist_ok=True)
    output = WebSocketClient.connect(
        args.output_url,
        headers={"Authorization": f"Bearer {api_key}"},
        timeout=min(args.timeout_seconds, 10.0),
    )
    try:
        output.send_json(
            {"type": "session.update", "session": {"input_audio_format": "pcm16"}}
        )
        session_updated = output.recv_json(min(args.timeout_seconds, 10.0))
        if (
            not isinstance(session_updated, dict)
            or session_updated.get("type") != "session.updated"
        ):
            raise RuntimeError(f"unexpected session update response: {session_updated}")
        events = receive_transcription(output, args.challenge, args.timeout_seconds)
        local = local_addresses(args.ip_command)
        connections = established_connections(args.ss_command, args.port)
        external_peers = sorted(
            {
                connection["peer_address"]
                for connection in connections
                if normalize_ip(connection["peer_address"]) not in local
                and not normalize_ip(connection["peer_address"]).is_loopback
            }
        )
        loopback_outputs = [
            connection
            for connection in connections
            if normalize_ip(connection["peer_address"]).is_loopback
        ]
        if not external_peers:
            raise RuntimeError(
                "no established remote-text peer differs from every local address"
            )
        if not loopback_outputs:
            raise RuntimeError("no loopback Realtime output connection was observed")
        if not args.physical_device_confirmed:
            raise RuntimeError(
                "distinct network peer observed, but physical-device confirmation is missing"
            )
        summary = {
            "event": "summary",
            "endpoint": args.endpoint,
            "output_url": args.output_url,
            "challenge": args.challenge,
            "session_updated": session_updated,
            "events": events,
            "connections": connections,
            "external_peer_addresses": external_peers,
            "local_address_count": len(local),
            "loopback_output_connection": True,
            "same_host_lan_proof": False,
            "distinct_network_peer_proof": True,
            "operator_confirmed_physical_device": True,
            "cross_device_proof": True,
            "api_key_recorded": False,
            "ip_command": str(args.ip_command),
            "ss_command": str(args.ss_command),
        }
        (args.out_dir / "summary.json").write_text(
            json.dumps(summary, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        print(json.dumps(summary, ensure_ascii=False))
    finally:
        output.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
