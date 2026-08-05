#!/usr/bin/env python3
"""Private-session XDG FileChooser portal fixture for GUI live validation."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
from typing import Any
from urllib.parse import quote

import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

PORTAL_NAME = "org.freedesktop.portal.Desktop"
PORTAL_PATH = "/org/freedesktop/portal/desktop"
FILE_CHOOSER_INTERFACE = "org.freedesktop.portal.FileChooser"
REQUEST_INTERFACE = "org.freedesktop.portal.Request"


def write_atomic(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f"{path.name}.{os.getpid()}.tmp")
    with temporary.open("w", encoding="utf-8") as stream:
        stream.write(content)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)


def append_json_line(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(value, ensure_ascii=False, separators=(",", ":")) + "\n"
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
    try:
        os.write(descriptor, payload.encode("utf-8"))
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def decode_byte_path(value: Any) -> str | None:
    if value is None:
        return None
    raw = bytes(int(item) for item in value)
    return os.fsdecode(raw.rstrip(b"\0"))


def normalize_filters(value: Any) -> list[dict[str, object]]:
    result: list[dict[str, object]] = []
    for label, entries in value or []:
        result.append(
            {
                "label": str(label),
                "patterns": [
                    {"kind": int(kind), "pattern": str(pattern)}
                    for kind, pattern in entries
                ],
            }
        )
    return result


class Request(dbus.service.Object):
    def __init__(self, bus_name: dbus.service.BusName, path: str) -> None:
        super().__init__(bus_name, path)
        self.path = path

    @dbus.service.signal(REQUEST_INTERFACE, signature="ua{sv}")
    def Response(self, response: dbus.UInt32, results: dbus.Dictionary) -> None:
        """Emit the standard portal request response."""


class FileChooserPortal(dbus.service.Object):
    def __init__(
        self,
        bus_name: dbus.service.BusName,
        selected_path: Path,
        response_modes: list[str],
        request_log: Path,
        ready_file: Path,
    ) -> None:
        super().__init__(bus_name, PORTAL_PATH)
        self.bus_name = bus_name
        self.selected_path = selected_path.resolve()
        self.response_modes = response_modes
        self.request_log = request_log
        self.requests: dict[str, Request] = {}
        write_atomic(ready_file, "ready\n")

    @dbus.service.method(
        FILE_CHOOSER_INTERFACE,
        in_signature="ssa{sv}",
        out_signature="o",
        sender_keyword="sender",
    )
    def OpenFile(
        self,
        parent_window: dbus.String,
        title: dbus.String,
        options: dbus.Dictionary,
        sender: str,
    ) -> dbus.ObjectPath:
        request_index = len(self.requests)
        mode = self.response_modes[min(request_index, len(self.response_modes) - 1)]
        token = str(options["handle_token"])
        sender_component = sender.removeprefix(":").replace(".", "_")
        request_path = (
            f"/org/freedesktop/portal/desktop/request/{sender_component}/{token}"
        )
        request = Request(self.bus_name, request_path)
        self.requests[request_path] = request
        append_json_line(
            self.request_log,
            {
                "request_index": request_index + 1,
                "parent_window": str(parent_window),
                "title": str(title),
                "handle_token_prefix": token.split("_", maxsplit=1)[0],
                "multiple": bool(options.get("multiple", False)),
                "directory": bool(options.get("directory", False)),
                "current_folder": decode_byte_path(options.get("current_folder")),
                "filters": normalize_filters(options.get("filters")),
                "response_mode": mode,
            },
        )
        GLib.idle_add(self._respond, request_path, mode)
        return dbus.ObjectPath(request_path)

    def _respond(self, request_path: str, mode: str) -> bool:
        request = self.requests[request_path]
        if mode == "select":
            uri = "file://" + quote(os.fsencode(self.selected_path), safe=b"/")
            results = dbus.Dictionary(
                {"uris": dbus.Array([dbus.String(uri)], signature="s")},
                signature="sv",
            )
            response = dbus.UInt32(0)
        elif mode == "cancel":
            results = dbus.Dictionary({}, signature="sv")
            response = dbus.UInt32(1)
        else:
            raise RuntimeError(f"unsupported response mode: {mode}")
        request.Response(response, results)
        return GLib.SOURCE_REMOVE


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--selected-path", required=True)
    parser.add_argument("--responses", default="select,cancel")
    parser.add_argument("--request-log", required=True)
    parser.add_argument("--ready-file", required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    response_modes = [
        mode.strip() for mode in args.responses.split(",") if mode.strip()
    ]
    if not response_modes or any(
        mode not in {"select", "cancel"} for mode in response_modes
    ):
        raise SystemExit(
            "responses must be a comma-separated sequence of select/cancel"
        )
    DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus()
    bus_name = dbus.service.BusName(PORTAL_NAME, bus=bus, do_not_queue=True)
    FileChooserPortal(
        bus_name,
        Path(args.selected_path),
        response_modes,
        Path(args.request_log),
        Path(args.ready_file),
    )
    GLib.MainLoop().run()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
