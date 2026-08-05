#!/usr/bin/env python3
"""Exercise real Fcitx5 scene and ASR menus without mutating selection."""

import argparse
import importlib
import json
import os
import sys
from dataclasses import dataclass, field
from typing import Any

import gi

gi.require_version("FcitxG", "1.0")
gi.require_version("Gdk", "4.0")
FcitxG = importlib.import_module("gi.repository.FcitxG")
Gdk = importlib.import_module("gi.repository.Gdk")
GLib = importlib.import_module("gi.repository.GLib")


def emit(event: str, **fields: object) -> None:
    print(json.dumps({"event": event, **fields}, ensure_ascii=False), flush=True)


@dataclass
class MenuState:
    connected: bool = False
    menu_seen: bool = False
    filter_seen: bool = False
    filter_cleared: bool = False
    menu_closed: bool = False
    candidates: list[str] = field(default_factory=list)
    key_events: list[dict[str, Any]] = field(default_factory=list)
    commits: list[str] = field(default_factory=list)
    stage: int = 0
    timed_out: bool = False


class MenuProbe:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.state = MenuState()
        self.loop = GLib.MainLoop()
        self.client = FcitxG.Client.new()
        self.client.set_program(f"fcitx-vinpst-{args.menu}-menu-live-probe")
        self.client.set_display(
            os.environ.get("WAYLAND_DISPLAY") or os.environ.get("DISPLAY") or ""
        )
        self.client.connect("connected", self.on_connected)
        self.client.connect("commit-string", self.on_commit)
        self.client.connect("update-client-side-ui", self.on_client_ui)

    def tap(self, label: str, keyval: int) -> bool:
        pressed = bool(self.client.process_key_sync(keyval, 0, 0, False, 0))
        released = bool(self.client.process_key_sync(keyval, 0, 0, True, 0))
        event = {
            "label": label,
            "keyval": keyval,
            "pressed": pressed,
            "released": released,
        }
        self.state.key_events.append(event)
        emit("key", **event)
        return GLib.SOURCE_REMOVE

    def on_commit(self, _client: Any, text: str) -> None:
        self.state.commits.append(text)
        emit("commit", text=text)

    @staticmethod
    def formatted_text(items: object) -> str:
        return "".join((item.string or "") for item in (items or []))

    def on_client_ui(
        self,
        _client: Any,
        preedit: object,
        _preedit_cursor: int,
        aux_up: object,
        aux_down: object,
        candidate_list: object,
        candidate_cursor: int,
        _candidate_layout_hint: int,
        has_prev: bool,
        has_next: bool,
    ) -> None:
        candidates = [item.candidate or "" for item in (candidate_list or [])]
        title = self.formatted_text(aux_up)
        status = self.formatted_text(aux_down)
        emit(
            "input-panel",
            preedit=self.formatted_text(preedit),
            title=title,
            status=status,
            candidates=candidates,
            candidate_cursor=candidate_cursor,
            has_prev=has_prev,
            has_next=has_next,
            stage=self.state.stage,
        )

        if candidates and not self.state.menu_seen:
            self.state.menu_seen = True
            self.state.candidates = candidates
            self.state.stage = 1
            GLib.timeout_add(100, self.tap, "slash", Gdk.KEY_slash)
            return
        if not self.state.menu_seen:
            return
        if self.state.stage == 1 and candidates:
            self.state.filter_seen = "/" in title
            self.state.stage = 2
            GLib.timeout_add(100, self.tap, "escape-filter", Gdk.KEY_Escape)
            return
        if self.state.stage == 2 and candidates:
            self.state.filter_cleared = True
            self.state.stage = 3
            GLib.timeout_add(100, self.tap, "escape-menu", Gdk.KEY_Escape)
            return
        if self.state.stage == 3 and not candidates:
            self.state.menu_closed = True
            self.state.stage = 4
            GLib.timeout_add(100, self.finish)

    def on_connected(self, _client: Any) -> None:
        self.state.connected = True
        capabilities = (1 << 1) | (1 << 4) | (1 << 6) | (1 << 39)
        self.client.set_capability(capabilities)
        self.client.focus_in()
        emit("connected", valid=bool(self.client.is_valid()), menu=self.args.menu)
        GLib.timeout_add(200, self.tap, "trigger", self.args.trigger_keyval)

    def finish(self) -> bool:
        self.loop.quit()
        return GLib.SOURCE_REMOVE

    def timeout(self) -> bool:
        self.state.timed_out = True
        emit("timeout", menu=self.args.menu, stage=self.state.stage)
        self.loop.quit()
        return GLib.SOURCE_REMOVE

    def run(self) -> None:
        GLib.timeout_add(self.args.timeout_ms, self.timeout)
        try:
            self.loop.run()
            self.validate()
        finally:
            if self.state.connected:
                self.client.focus_out()

    def validate(self) -> None:
        failures: list[str] = []
        if self.state.timed_out:
            failures.append("menu probe timed out")
        if not self.state.connected:
            failures.append("Fcitx input context did not connect")
        if not self.state.menu_seen or not self.state.candidates:
            failures.append("menu produced no candidates")
        if not self.state.filter_seen:
            failures.append("slash did not activate menu filter mode")
        if not self.state.filter_cleared:
            failures.append("first Escape did not clear menu filter mode")
        if not self.state.menu_closed:
            failures.append("second Escape did not close the menu")
        if self.state.commits:
            failures.append("menu navigation unexpectedly committed text")
        key_events = {event["label"]: event for event in self.state.key_events}
        required_labels = {"trigger", "slash", "escape-filter", "escape-menu"}
        if not required_labels.issubset(key_events):
            failures.append("menu probe did not send every expected key tap")
        elif not all(key_events[label]["pressed"] for label in required_labels):
            failures.append("addon did not consume every menu key press")
        elif not all(
            key_events[label]["released"]
            for label in ("trigger", "slash", "escape-filter")
        ):
            failures.append("addon did not consume active-menu key releases")

        emit(
            "summary",
            menu=self.args.menu,
            candidate_count=len(self.state.candidates),
            filter_seen=self.state.filter_seen,
            filter_cleared=self.state.filter_cleared,
            menu_closed=self.state.menu_closed,
            escape_menu_release_consumed=next(
                (
                    event["released"]
                    for event in self.state.key_events
                    if event["label"] == "escape-menu"
                ),
                None,
            ),
            commit_count=len(self.state.commits),
            ok=not failures,
            failures=failures,
        )
        if failures:
            raise RuntimeError("; ".join(failures))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--menu", choices=("scene", "asr"), required=True)
    parser.add_argument("--trigger-key", required=True)
    parser.add_argument("--timeout-ms", type=int, default=8000)
    args = parser.parse_args()
    args.trigger_keyval = Gdk.keyval_from_name(args.trigger_key)
    if args.trigger_keyval == Gdk.KEY_VoidSymbol:
        parser.error(f"unknown GDK trigger key: {args.trigger_key}")
    return args


def main() -> int:
    try:
        MenuProbe(parse_args()).run()
    except (GLib.Error, OSError, RuntimeError, ValueError) as error:
        emit("fatal", error=str(error))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
