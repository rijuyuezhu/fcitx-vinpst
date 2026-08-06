#!/usr/bin/env python3
"""Send one real Linux keyboard key or bounded ASCII text through /dev/uinput."""

import argparse
import fcntl
import json
import os
import struct
import subprocess
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
    "V": 47,
    "SHIFT": 42,
    "SPACE": 57,
    "F6": 64,
    "F9": 67,
    "F10": 68,
    "UP": 103,
    "LEFT": 105,
    "RIGHT": 106,
    "DOWN": 108,
}
KEY_SEQUENCES = {
    "CTRL+A": (KEY_CODES["CTRL"], KEY_CODES["A"]),
    "CTRL+C": (KEY_CODES["CTRL"], KEY_CODES["C"]),
    "CTRL+V": (KEY_CODES["CTRL"], KEY_CODES["V"]),
    "CTRL+S": (KEY_CODES["CTRL"], KEY_CODES["S"]),
    "F6": (KEY_CODES["F6"],),
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
TEXT_KEY_CODES = {
    "a": 30,
    "b": 48,
    "c": 46,
    "d": 32,
    "e": 18,
    "f": 33,
    "g": 34,
    "h": 35,
    "i": 23,
    "j": 36,
    "k": 37,
    "l": 38,
    "m": 50,
    "n": 49,
    "o": 24,
    "p": 25,
    "q": 16,
    "r": 19,
    "s": 31,
    "t": 20,
    "u": 22,
    "v": 47,
    "w": 17,
    "x": 45,
    "y": 21,
    "z": 44,
    "0": 11,
    "1": 2,
    "2": 3,
    "3": 4,
    "4": 5,
    "5": 6,
    "6": 7,
    "7": 8,
    "8": 9,
    "9": 10,
    "-": 12,
    ".": 52,
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


def emit_key_sequence(device: int, key_codes: tuple[int, ...], hold_s: float) -> None:
    for key_code in key_codes:
        emit_event(device, EV_KEY, key_code, 1)
    emit_event(device, EV_SYN, SYN_REPORT, 0)
    time.sleep(hold_s)
    for key_code in reversed(key_codes):
        emit_event(device, EV_KEY, key_code, 0)
    emit_event(device, EV_SYN, SYN_REPORT, 0)


def matching_keyboard_events() -> set[str]:
    events: set[str] = set()
    for name_path in Path("/sys/class/input").glob("event*/device/name"):
        try:
            if (
                name_path.read_text(encoding="utf-8").strip()
                != "fcitx-vinpst-live-keyboard"
            ):
                continue
        except OSError:
            continue
        event = name_path.parents[1].name
        info = subprocess.run(
            [
                "udevadm",
                "info",
                "--query=property",
                f"--name=/dev/input/{event}",
            ],
            check=False,
            capture_output=True,
            text=True,
        )
        if info.returncode == 0 and "ID_INPUT_KEYBOARD=1\n" in info.stdout:
            events.add(event)
    return events


def wait_for_keyboard_event(existing: set[str], timeout_s: float = 5.0) -> str:
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        candidates = matching_keyboard_events() - existing
        if len(candidates) == 1:
            return candidates.pop()
        if len(candidates) > 1:
            raise RuntimeError("multiple new live keyboard devices appeared")
        time.sleep(0.02)
    raise RuntimeError("uinput device was not classified as a keyboard")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("key", nargs="?", choices=sorted(KEY_SEQUENCES))
    parser.add_argument("--text")
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

    if (args.key is None) == (args.text is None):
        raise SystemExit("provide exactly one key or --text value")
    if args.text is not None:
        unsupported = sorted(set(args.text) - set(TEXT_KEY_CODES))
        if unsupported:
            raise SystemExit(f"unsupported text characters: {unsupported!r}")
        if not args.text or len(args.text) > 256:
            raise SystemExit("text must contain between 1 and 256 characters")

    key_codes = KEY_SEQUENCES[args.key] if args.key is not None else ()
    existing_events = matching_keyboard_events()
    descriptor = os.open(device_path, os.O_WRONLY | os.O_NONBLOCK)
    created = False
    event = ""
    try:
        fcntl.ioctl(descriptor, UI_SET_EVBIT, EV_KEY)
        # Advertise a standard keyboard key range so udev/libinput classify the
        # temporary device as ID_INPUT_KEYBOARD. Emitted events remain limited
        # to the explicitly requested sequence below.
        for key_code in range(1, 128):
            fcntl.ioctl(descriptor, UI_SET_KEYBIT, key_code)
        name = b"fcitx-vinpst-live-keyboard"
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
        event = wait_for_keyboard_event(existing_events)
        time.sleep(args.settle_ms / 1000)
        hold_s = args.hold_ms / 1000
        if args.key is not None:
            emit_key_sequence(descriptor, key_codes, hold_s)
        else:
            for character in args.text:
                emit_key_sequence(descriptor, (TEXT_KEY_CODES[character],), hold_s)
                time.sleep(0.01)
        time.sleep(0.05)
    finally:
        if created:
            fcntl.ioctl(descriptor, UI_DEV_DESTROY)
        os.close(descriptor)

    evidence = {
        "event": "uinput-key" if args.key is not None else "uinput-text",
        "device": str(device_path),
        "input_event": event,
        "ok": True,
    }
    if args.key is not None:
        evidence.update({"key": args.key, "code": key_codes[-1], "codes": key_codes})
    else:
        evidence["text_length"] = len(args.text)
    print(json.dumps(evidence))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
