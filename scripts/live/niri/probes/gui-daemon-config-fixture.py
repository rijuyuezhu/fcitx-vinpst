#!/usr/bin/env python3
"""Private-session vinput daemon fixture for GUI config-mutation live gates."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path

import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib

SERVICE_NAME = "org.fcitx.Vinput"
OBJECT_PATH = "/org/fcitx/Vinput"
SERVICE_INTERFACE = "org.fcitx.Vinput.Service"


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
    payload = json.dumps(value, separators=(",", ":")) + "\n"
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
    try:
        os.write(descriptor, payload.encode("utf-8"))
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


class DaemonFixture(dbus.service.Object):
    def __init__(
        self,
        bus_name: dbus.service.BusName,
        method_log: Path,
        ready_file: Path,
    ) -> None:
        super().__init__(bus_name, OBJECT_PATH)
        self.method_log = method_log
        self.sequence = 0
        write_atomic(ready_file, "ready\n")

    def _record(self, method: str) -> None:
        self.sequence += 1
        append_json_line(
            self.method_log,
            {
                "sequence": self.sequence,
                "method": method,
            },
        )

    @dbus.service.method(SERVICE_INTERFACE, in_signature="", out_signature="s")
    def GetStatus(self) -> dbus.String:
        self._record("GetStatus")
        return dbus.String("idle")

    @dbus.service.method(SERVICE_INTERFACE, in_signature="", out_signature="s")
    def GetRuntimeStatus(self) -> dbus.String:
        self._record("GetRuntimeStatus")
        return dbus.String('{"active_session":false}')

    @dbus.service.method(SERVICE_INTERFACE, in_signature="", out_signature="s")
    def GetTextAdapterState(self) -> dbus.String:
        self._record("GetTextAdapterState")
        return dbus.String("{}")

    @dbus.service.method(SERVICE_INTERFACE, in_signature="", out_signature="")
    def ReloadAsrBackend(self) -> None:
        self._record("ReloadAsrBackend")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--method-log", required=True)
    parser.add_argument("--ready-file", required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus()
    bus_name = dbus.service.BusName(SERVICE_NAME, bus=bus, do_not_queue=True)
    DaemonFixture(bus_name, Path(args.method_log), Path(args.ready_file))
    GLib.MainLoop().run()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
