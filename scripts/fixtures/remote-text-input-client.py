#!/usr/bin/env python3
"""Authenticate one remote-text input client and finalize exact text."""

import argparse
import os
import sys
import time
from pathlib import Path

SCRIPTS_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS_ROOT / "live" / "network"))

from probes.websocket_cdp import WebSocketClient


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", required=True)
    parser.add_argument("--text", required=True)
    parser.add_argument("--hold-seconds", type=float, default=0.0)
    parser.add_argument("--require-output-connected", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    api_key = os.environ.get("VINPUT_REMOTE_TEXT_API_KEY", "")
    if not api_key:
        raise RuntimeError("VINPUT_REMOTE_TEXT_API_KEY is required")
    if args.hold_seconds < 0:
        raise ValueError("hold duration cannot be negative")

    client = WebSocketClient.connect(args.url)
    try:
        client.send_json({"type": "auth", "api_key": api_key})
        response = client.recv_json()
        if not isinstance(response, dict) or response.get("type") != "auth_ok":
            raise RuntimeError(f"remote input authentication failed: {response}")
        if args.require_output_connected:
            while True:
                response = client.recv_json()
                if not isinstance(response, dict):
                    continue
                if response.get("type") == "output_connected":
                    break
                if (
                    response.get("type") == "init"
                    and response.get("output_status") == "connected"
                ):
                    break
        client.send_json({"type": "text_update", "text": args.text})
        client.send_json({"type": "finalize"})
        if args.hold_seconds:
            time.sleep(args.hold_seconds)
    finally:
        client.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
