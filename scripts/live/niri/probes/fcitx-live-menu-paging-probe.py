#!/usr/bin/env python3
"""Exercise real Fcitx scene or ASR menu paging without selecting an item."""

import argparse
import ast
import importlib
import json
import os
import re
import subprocess
from dataclasses import dataclass, field
from typing import Any

import gi

gi.require_version("FcitxG", "1.0")
gi.require_version("Gdk", "4.0")
FcitxG = importlib.import_module("gi.repository.FcitxG")
Gdk = importlib.import_module("gi.repository.Gdk")
GLib = importlib.import_module("gi.repository.GLib")

DBUS_SERVICE = "org.fcitx.Vinpst"
DBUS_PATH = "/org/fcitx/Vinpst"
DBUS_INTERFACE = "org.fcitx.Vinpst.Service"
PAGE_SIZE = 10


def emit(event: str, **fields: object) -> None:
    print(json.dumps({"event": event, **fields}, ensure_ascii=False), flush=True)


def call_service(method: str) -> str:
    result = subprocess.run(
        [
            "gdbus",
            "call",
            "--session",
            "--dest",
            DBUS_SERVICE,
            "--object-path",
            DBUS_PATH,
            "--method",
            f"{DBUS_INTERFACE}.{method}",
        ],
        check=False,
        capture_output=True,
        text=True,
        timeout=3,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or f"{method} failed")
    return result.stdout.strip()


def parse_gdbus_literal(value: str) -> object:
    normalized = re.sub(r"\btrue\b", "True", value)
    normalized = re.sub(r"\bfalse\b", "False", normalized)
    return ast.literal_eval(normalized)


def get_menu_state(menu: str) -> dict[str, object]:
    if menu == "scene":
        parsed = parse_gdbus_literal(call_service("GetSceneState"))
        if not isinstance(parsed, tuple) or len(parsed) != 2:
            raise TypeError(f"unexpected GetSceneState reply: {parsed!r}")
        active_scene, rows = parsed
        return {
            "active_scene": active_scene,
            "rows": [(scene_id, label) for scene_id, label in rows],
        }

    parsed = parse_gdbus_literal(call_service("GetAsrDisplayMenuState"))
    if not isinstance(parsed, tuple) or len(parsed) != 7:
        raise TypeError(f"unexpected GetAsrDisplayMenuState reply: {parsed!r}")
    (
        target_provider,
        target_model,
        effective_provider,
        effective_model,
        reload_in_progress,
        last_error,
        rows,
    ) = parsed
    return {
        "target_provider": target_provider,
        "target_model": target_model,
        "effective_provider": effective_provider,
        "effective_model": effective_model,
        "reload_in_progress": reload_in_progress,
        "last_error": last_error,
        "rows": [tuple(row) for row in rows],
    }


def protected_menu_state(menu: str, state: dict[str, object]) -> dict[str, object]:
    if menu == "scene":
        return {"active_scene": state["active_scene"]}
    return {
        key: state[key]
        for key in (
            "target_provider",
            "target_model",
            "effective_provider",
            "effective_model",
            "reload_in_progress",
            "last_error",
        )
    }


@dataclass
class PagingState:
    connected: bool = False
    first_page_seen: bool = False
    second_page_seen: bool = False
    first_page_restored: bool = False
    menu_closed: bool = False
    timed_out: bool = False
    commits: list[str] = field(default_factory=list)
    key_events: list[dict[str, Any]] = field(default_factory=list)


class MenuPagingProbe:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.state = PagingState()
        self.before = get_menu_state(args.menu)
        rows = self.before["rows"]
        if not isinstance(rows, list) or len(rows) <= PAGE_SIZE:
            count = len(rows) if isinstance(rows, list) else 0
            raise RuntimeError(
                f"{args.menu} paging needs more than {PAGE_SIZE} state rows, found {count}"
            )
        self.protected_before = protected_menu_state(args.menu, self.before)
        self.first_page: list[str] = []
        self.second_page: list[str] = []
        self.stage = 0
        self.loop = GLib.MainLoop()
        self.client = FcitxG.Client.new()
        self.client.set_program(f"fcitx-vinpst-{args.menu}-menu-paging-live-probe")
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
        emit(
            "input-panel",
            menu=self.args.menu,
            preedit=self.formatted_text(preedit),
            title=self.formatted_text(aux_up),
            status=self.formatted_text(aux_down),
            candidates=candidates,
            candidate_cursor=candidate_cursor,
            has_prev=has_prev,
            has_next=has_next,
            stage=self.stage,
        )
        if self.stage == 0 and candidates:
            self.first_page = list(candidates)
            if len(candidates) == PAGE_SIZE and not has_prev and has_next:
                self.state.first_page_seen = True
                self.stage = 1
                GLib.timeout_add(100, self.tap, "page-next", self.args.page_next_keyval)
            return
        if self.stage == 1 and candidates and candidates != self.first_page:
            self.second_page = list(candidates)
            if has_prev and not has_next:
                self.state.second_page_seen = True
                self.stage = 2
                GLib.timeout_add(100, self.tap, "page-prev", self.args.page_prev_keyval)
            return
        if self.stage == 2 and candidates == self.first_page:
            if not has_prev and has_next:
                self.state.first_page_restored = True
                self.stage = 3
                GLib.timeout_add(100, self.tap, "escape", Gdk.KEY_Escape)
            return
        if self.stage == 3 and not candidates:
            self.state.menu_closed = True
            self.stage = 4
            GLib.timeout_add(100, self.finish)

    def on_connected(self, _client: Any) -> None:
        self.state.connected = True
        capabilities = (1 << 1) | (1 << 4) | (1 << 6) | (1 << 39)
        self.client.set_capability(capabilities)
        self.client.focus_in()
        emit(
            "connected",
            valid=bool(self.client.is_valid()),
            menu=self.args.menu,
            protected_state=self.protected_before,
        )
        GLib.timeout_add(200, self.tap, "trigger", self.args.trigger_keyval)

    def finish(self) -> bool:
        self.loop.quit()
        return GLib.SOURCE_REMOVE

    def timeout(self) -> bool:
        self.state.timed_out = True
        emit("timeout", menu=self.args.menu, stage=self.stage)
        self.loop.quit()
        return GLib.SOURCE_REMOVE

    def run(self) -> None:
        GLib.timeout_add(self.args.timeout_ms, self.timeout)
        try:
            self.loop.run()
        finally:
            if self.state.connected:
                self.client.focus_out()
        self.validate()

    def validate(self) -> None:
        failures: list[str] = []
        if self.state.timed_out:
            failures.append(f"{self.args.menu} menu paging timed out")
        if not self.state.connected:
            failures.append("Fcitx input context did not connect")
        if not self.state.first_page_seen:
            failures.append(f"first {self.args.menu} page did not expose has_next")
        if not self.state.second_page_seen:
            failures.append(
                "configured next-page key did not expose the second "
                f"{self.args.menu} page"
            )
        if not self.state.first_page_restored:
            failures.append(
                "configured previous-page key did not restore the first "
                f"{self.args.menu} page"
            )
        if not self.state.menu_closed:
            failures.append(f"Escape did not close the paged {self.args.menu} menu")
        after = get_menu_state(self.args.menu)
        protected_after = protected_menu_state(self.args.menu, after)
        if protected_after != self.protected_before:
            failures.append(f"{self.args.menu} paging changed protected menu state")
        if self.state.commits:
            failures.append(f"{self.args.menu} paging unexpectedly committed text")
        key_events = {event["label"]: event for event in self.state.key_events}
        for label in ("trigger", "page-next", "page-prev", "escape"):
            event = key_events.get(label)
            if event is None or not event["pressed"]:
                failures.append(f"addon did not consume {label} key press")

        emit(
            "summary",
            menu=self.args.menu,
            protected_state=self.protected_before,
            page_next_key=self.args.page_next_key,
            page_prev_key=self.args.page_prev_key,
            first_page=self.first_page,
            second_page=self.second_page,
            first_page_count=len(self.first_page),
            second_page_count=len(self.second_page),
            first_page_seen=self.state.first_page_seen,
            second_page_seen=self.state.second_page_seen,
            first_page_restored=self.state.first_page_restored,
            menu_closed=self.state.menu_closed,
            commit_count=len(self.state.commits),
            ok=not failures,
            failures=failures,
        )
        if failures:
            raise RuntimeError("; ".join(failures))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--menu", choices=("scene", "asr"), default="scene")
    parser.add_argument("--trigger-key", default="F7")
    parser.add_argument("--page-next-key", default="Page_Down")
    parser.add_argument("--page-prev-key", default="Page_Up")
    parser.add_argument("--timeout-ms", type=int, default=8000)
    args = parser.parse_args()
    for name in ("trigger", "page_next", "page_prev"):
        value = getattr(args, f"{name}_key")
        keyval = Gdk.keyval_from_name(value)
        if keyval == Gdk.KEY_VoidSymbol:
            parser.error(f"unknown GDK {name.replace('_', '-')} key: {value}")
        setattr(args, f"{name}_keyval", keyval)
    return args


def main() -> int:
    try:
        MenuPagingProbe(parse_args()).run()
    except (GLib.Error, OSError, RuntimeError, TypeError, ValueError) as error:
        emit("fatal", error=str(error))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
