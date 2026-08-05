#!/usr/bin/env python3
"""Send one real Linux keyboard key through /dev/uinput."""

import argparse
import fcntl
import json
import os
import struct
import time
from pathlib import Path

EV_SYN = 0
EV_KEY = 1
SYN_REPORT = 0
BUS_USB = 0x03

KEY_CODES = {
    "ESCAPE": 1,
    "1": 2,
    "2": 3,
    "3": 4,
    "4": 5,
    "BACKSPACE": 14,
    "TAB": 15,
    "ENTER": 28,
    "CTRL": 29,
    "A": 30,
    "S": 31,
    "C": 46,
    "SHIFT": 42,
    "SPACE": 57,
    "F9": 67,
    "UP": 103,
    "LEFT": 105,
    "RIGHT": 106,
    "DOWN": 108,
    "F10": 68,
}
KEY_SEQUENCES = {
    "CTRL+A": (KEY_CODES["CTRL"], KEY_CODES["A"]),
    "CTRL+C": (KEY_CODES["CTRL"], KEY_CODES["C"]),
    "CTRL+S": (KEY_CODES["CTRL"], KEY_CODES["S"]),
    "CTRL+1": (KEY_CODES["CTRL"], KEY_CODES["1"]),
    "CTRL+2": (KEY_CODES["CTRL"], KEY_CODES["2"]),
    "CTRL+3": (KEY_CODES["CTRL"], KEY_CODES["3"]),
    "CTRL+4": (KEY_CODES["CTRL"], KEY_CODES["4"]),
    "SHIFT+TAB": (KEY_CODES["SHIFT"], KEY_CODES["TAB"]),
    "BACKSPACE": (KEY_CODES["BACKSPACE"],),
    "ENTER": (KEY_CODES["ENTER"],),
    "ESCAPE": (KEY_CODES["ESCAPE"],),
    "F9": (KEY_CODES["F9"],),
    "F10": (KEY_CODES["F10"],),
    "SPACE": (KEY_CODES["SPACE"],),
    "TAB": (KEY_CODES["TAB"],),
    "UP": (KEY_CODES["UP"],),
    "LEFT": (KEY_CODES["LEFT"],),
    "RIGHT": (KEY_CODES["RIGHT"],),
    "DOWN": (KEY_CODES["DOWN"],),
}


def _ioc(direction: int, kind: str, number: int, size: int) -> int:
    return (direction << 30) | (size << 16) | (ord(kind) << 8) | number


def _iow(kind: str, number: int, size: int) -> int:
    return _ioc(1, kind, number, size)


UI_SET_EVBIT = _iow("U", 100, struct.calcsize("i"))
UI_SET_KEYBIT = _iow("U", 101, struct.calcsize("i"))
UI_DEV_CREATE = _ioc(0, "U", 1, 0)
UI_DEV_DESTROY = _ioc(0, "U", 2, 0)


def emit_event(device: int, event_type: int, code: int, value: int) -> None:
    os.write(device, struct.pack("llHHi", 0, 0, event_type, code, value))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("key", choices=sorted(KEY_SEQUENCES))
    parser.add_argument("--device", default="/dev/uinput")
    parser.add_argument("--hold-ms", type=int, default=40)
    parser.add_argument("--settle-ms", type=int, default=500)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.hold_ms < 1 or args.settle_ms < 0:
        raise SystemExit("hold and settle durations must be non-negative")

    device_path = Path(args.device)
    if not device_path.exists():
        raise SystemExit(f"uinput device does not exist: {device_path}")
    if not os.access(device_path, os.W_OK):
        raise SystemExit(f"uinput device is not writable: {device_path}")

    key_codes = KEY_SEQUENCES[args.key]
    descriptor = os.open(device_path, os.O_WRONLY | os.O_NONBLOCK)
    created = False
    try:
        fcntl.ioctl(descriptor, UI_SET_EVBIT, EV_KEY)
        for key_code in key_codes:
            fcntl.ioctl(descriptor, UI_SET_KEYBIT, key_code)
        name = b"fcitx-vinput-live-keyboard"
        header = struct.pack(
            "80sHHHHI",
            name,
            BUS_USB,
            0x1209,
            0x0001,
            1,
            0,
        )
        absolute_axes = struct.pack("256i", *([0] * 256))
        os.write(descriptor, header + absolute_axes)
        fcntl.ioctl(descriptor, UI_DEV_CREATE)
        created = True
        time.sleep(args.settle_ms / 1000)
        for key_code in key_codes:
            emit_event(descriptor, EV_KEY, key_code, 1)
        emit_event(descriptor, EV_SYN, SYN_REPORT, 0)
        time.sleep(args.hold_ms / 1000)
        for key_code in reversed(key_codes):
            emit_event(descriptor, EV_KEY, key_code, 0)
        emit_event(descriptor, EV_SYN, SYN_REPORT, 0)
        time.sleep(0.05)
    finally:
        if created:
            fcntl.ioctl(descriptor, UI_DEV_DESTROY)
        os.close(descriptor)

    print(
        json.dumps(
            {
                "event": "uinput-key",
                "key": args.key,
                "code": key_codes[-1],
                "codes": key_codes,
                "device": str(device_path),
                "ok": True,
            }
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
