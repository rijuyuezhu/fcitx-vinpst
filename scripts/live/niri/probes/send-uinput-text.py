#!/usr/bin/env python3
"""Type bounded printable ASCII text through one real Linux uinput keyboard."""

import argparse
import fcntl
import hashlib
import json
import os
import struct
import time
from pathlib import Path

EV_SYN = 0
EV_KEY = 1
SYN_REPORT = 0
BUS_USB = 0x03
KEY_LEFTSHIFT = 42

LETTER_CODES = {
    **dict(zip("qwertyuiop", range(16, 26), strict=True)),
    **dict(zip("asdfghjkl", range(30, 39), strict=True)),
    **dict(zip("zxcvbnm", range(44, 51), strict=True)),
}
DIGIT_CODES = dict(zip("1234567890", range(2, 12), strict=True))
CHARACTER_CODES = {
    **{letter: (code, False) for letter, code in LETTER_CODES.items()},
    **{letter.upper(): (code, True) for letter, code in LETTER_CODES.items()},
    **{digit: (code, False) for digit, code in DIGIT_CODES.items()},
    " ": (57, False),
    "-": (12, False),
    "_": (12, True),
    ".": (52, False),
    "/": (53, False),
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


def sync(device: int) -> None:
    emit_event(device, EV_SYN, SYN_REPORT, 0)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("text")
    parser.add_argument("--device", default="/dev/uinput")
    parser.add_argument("--key-delay-ms", type=int, default=25)
    parser.add_argument("--settle-ms", type=int, default=500)
    parser.add_argument("--max-bytes", type=int, default=4096)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    encoded = args.text.encode("ascii", errors="strict")
    if not encoded or len(encoded) > args.max_bytes:
        raise SystemExit("text must be non-empty and within the configured byte limit")
    if args.key_delay_ms < 0 or args.settle_ms < 0 or args.max_bytes < 1:
        raise SystemExit("delay and size arguments must be non-negative")
    unsupported = sorted(
        {character for character in args.text if character not in CHARACTER_CODES}
    )
    if unsupported:
        raise SystemExit(f"unsupported ASCII characters: {unsupported!r}")

    device_path = Path(args.device)
    if not device_path.exists():
        raise SystemExit(f"uinput device does not exist: {device_path}")
    if not os.access(device_path, os.W_OK):
        raise SystemExit(f"uinput device is not writable: {device_path}")

    required_codes = {KEY_LEFTSHIFT}
    required_codes.update(CHARACTER_CODES[character][0] for character in args.text)
    descriptor = os.open(device_path, os.O_WRONLY | os.O_NONBLOCK)
    created = False
    try:
        fcntl.ioctl(descriptor, UI_SET_EVBIT, EV_KEY)
        for key_code in sorted(required_codes):
            fcntl.ioctl(descriptor, UI_SET_KEYBIT, key_code)
        name = b"fcitx-vinpst-live-text-keyboard"
        header = struct.pack("80sHHHHI", name, BUS_USB, 0x1209, 0x0002, 1, 0)
        absolute_axes = struct.pack("256i", *([0] * 256))
        os.write(descriptor, header + absolute_axes)
        fcntl.ioctl(descriptor, UI_DEV_CREATE)
        created = True
        time.sleep(args.settle_ms / 1000)
        for character in args.text:
            key_code, shifted = CHARACTER_CODES[character]
            if shifted:
                emit_event(descriptor, EV_KEY, KEY_LEFTSHIFT, 1)
                sync(descriptor)
            emit_event(descriptor, EV_KEY, key_code, 1)
            sync(descriptor)
            emit_event(descriptor, EV_KEY, key_code, 0)
            sync(descriptor)
            if shifted:
                emit_event(descriptor, EV_KEY, KEY_LEFTSHIFT, 0)
                sync(descriptor)
            time.sleep(args.key_delay_ms / 1000)
        time.sleep(0.05)
    finally:
        if created:
            fcntl.ioctl(descriptor, UI_DEV_DESTROY)
        os.close(descriptor)

    print(
        json.dumps(
            {
                "event": "uinput-text",
                "bytes": len(encoded),
                "sha256": hashlib.sha256(encoded).hexdigest(),
                "device": str(device_path),
                "ok": True,
            }
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
