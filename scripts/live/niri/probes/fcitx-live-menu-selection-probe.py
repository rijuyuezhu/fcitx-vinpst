#!/usr/bin/env python3
"""Select one real Fcitx scene-menu candidate and restore the original scene."""

import argparse
import ast
import importlib
import json
import os
import subprocess
from dataclasses import dataclass, field
from typing import Any

import gi

gi.require_version("FcitxG", "1.0")
gi.require_version("Gdk", "4.0")
FcitxG = importlib.import_module("gi.repository.FcitxG")
Gdk = importlib.import_module("gi.repository.Gdk")
GLib = importlib.import_module("gi.repository.GLib")

DBUS_DEST = "org.fcitx.Vinpst"
DBUS_PATH = "/org/fcitx/Vinpst"
DBUS_INTERFACE = "org.fcitx.Vinpst.Service"


def emit(event: str, **fields: object) -> None:
    print(json.dumps({"event": event, **fields}, ensure_ascii=False), flush=True)


def call_service(method: str, *args: str) -> str:
    result = subprocess.run(
        [
            "gdbus",
            "call",
            "--session",
            "--dest",
            DBUS_DEST,
            "--object-path",
            DBUS_PATH,
            "--method",
            f"{DBUS_INTERFACE}.{method}",
            *args,
        ],
        check=False,
        capture_output=True,
        text=True,
        timeout=3,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(f"{method} failed: {detail}")
    return result.stdout.strip()


def get_scene_state() -> tuple[str, list[tuple[str, str]]]:
    parsed = ast.literal_eval(call_service("GetSceneState"))
    if not isinstance(parsed, tuple) or len(parsed) != 2:
        raise RuntimeError(f"unexpected GetSceneState reply: {parsed!r}")
    active, rows = parsed
    if not isinstance(active, str) or not isinstance(rows, list):
        raise TypeError(f"invalid GetSceneState reply: {parsed!r}")
    scenes: list[tuple[str, str]] = []
    for row in rows:
        if (
            not isinstance(row, tuple)
            or len(row) != 2
            or not all(isinstance(value, str) for value in row)
        ):
            raise RuntimeError(f"invalid scene row: {row!r}")
        scenes.append((row[0], row[1]))
    return active, scenes


def set_active_scene(scene_id: str) -> None:
    call_service("SetActiveScene", scene_id)


@dataclass
class SelectionState:
    connected: bool = False
    menu_seen: bool = False
    menu_closed: bool = False
    switched: bool = False
    restored: bool = False
    timed_out: bool = False
    candidates: list[str] = field(default_factory=list)
    key_events: list[dict[str, Any]] = field(default_factory=list)
    commits: list[str] = field(default_factory=list)


class SceneMenuSelectionProbe:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.state = SelectionState()
        self.original_scene, self.scenes = get_scene_state()
        alternatives = [row for row in self.scenes if row[0] != self.original_scene]
        if not alternatives:
            raise RuntimeError("scene menu needs at least one non-active scene")
        self.target_scene, self.target_label = alternatives[0]
        self.expected_candidates = [label for scene_id, label in alternatives]
        self.loop = GLib.MainLoop()
        self.client = FcitxG.Client.new()
        self.client.set_program("fcitx-vinpst-scene-menu-selection-live-probe")
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
            preedit=self.formatted_text(preedit),
            title=self.formatted_text(aux_up),
            status=self.formatted_text(aux_down),
            candidates=candidates,
            candidate_cursor=candidate_cursor,
            has_prev=has_prev,
            has_next=has_next,
        )
        if candidates and not self.state.menu_seen:
            self.state.menu_seen = True
            self.state.candidates = candidates
            emit(
                "selection-target",
                original_scene=self.original_scene,
                target_scene=self.target_scene,
                target_label=self.target_label,
            )
            GLib.timeout_add(100, self.tap, "enter", Gdk.KEY_Return)
            return
        if self.state.menu_seen and not candidates:
            self.state.menu_closed = True

    def on_connected(self, _client: Any) -> None:
        self.state.connected = True
        capabilities = (1 << 1) | (1 << 4) | (1 << 6) | (1 << 39)
        self.client.set_capability(capabilities)
        self.client.focus_in()
        emit(
            "connected",
            valid=bool(self.client.is_valid()),
            original_scene=self.original_scene,
            target_scene=self.target_scene,
        )
        GLib.timeout_add(200, self.tap, "trigger", self.args.trigger_keyval)
        GLib.timeout_add(100, self.poll_switch)

    def poll_switch(self) -> bool:
        if not self.state.menu_seen:
            return GLib.SOURCE_CONTINUE
        active, _scenes = get_scene_state()
        emit("scene-state", active_scene=active)
        if active != self.target_scene:
            return GLib.SOURCE_CONTINUE
        self.state.switched = True
        GLib.timeout_add(100, self.finish)
        return GLib.SOURCE_REMOVE

    def finish(self) -> bool:
        self.loop.quit()
        return GLib.SOURCE_REMOVE

    def timeout(self) -> bool:
        self.state.timed_out = True
        emit("timeout", target_scene=self.target_scene)
        self.loop.quit()
        return GLib.SOURCE_REMOVE

    def restore(self) -> None:
        current, _scenes = get_scene_state()
        if current != self.original_scene:
            set_active_scene(self.original_scene)
        for _ in range(50):
            active, _scenes = get_scene_state()
            if active == self.original_scene:
                self.state.restored = True
                emit("scene-restored", active_scene=active)
                return
            GLib.usleep(50_000)
        raise RuntimeError(
            f"failed to restore scene {self.original_scene!r}; current={active!r}"
        )

    def run(self) -> None:
        GLib.timeout_add(self.args.timeout_ms, self.timeout)
        try:
            self.loop.run()
        finally:
            if self.state.connected:
                self.client.focus_out()
            self.restore()
        self.validate()

    def validate(self) -> None:
        failures: list[str] = []
        if self.state.timed_out:
            failures.append("scene menu selection timed out")
        if not self.state.connected:
            failures.append("Fcitx input context did not connect")
        if not self.state.menu_seen:
            failures.append("scene menu produced no candidates")
        if self.state.candidates != self.expected_candidates:
            failures.append("scene menu candidates did not match daemon scene state")
        if not self.state.menu_closed:
            failures.append("scene menu did not close after selection")
        if not self.state.switched:
            failures.append("Enter did not switch to the first scene candidate")
        if not self.state.restored:
            failures.append("original scene was not restored")
        if self.state.commits:
            failures.append("scene menu selection unexpectedly committed text")
        key_events = {event["label"]: event for event in self.state.key_events}
        for label in ("trigger", "enter"):
            event = key_events.get(label)
            if event is None or not event["pressed"]:
                failures.append(f"addon did not consume {label} key press")

        emit(
            "summary",
            original_scene=self.original_scene,
            target_scene=self.target_scene,
            candidate_count=len(self.state.candidates),
            menu_closed=self.state.menu_closed,
            switched=self.state.switched,
            restored=self.state.restored,
            commit_count=len(self.state.commits),
            ok=not failures,
            failures=failures,
        )
        if failures:
            raise RuntimeError("; ".join(failures))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--trigger-key", default="F7")
    parser.add_argument("--timeout-ms", type=int, default=8000)
    args = parser.parse_args()
    args.trigger_keyval = Gdk.keyval_from_name(args.trigger_key)
    if args.trigger_keyval == Gdk.KEY_VoidSymbol:
        parser.error(f"unknown GDK trigger key: {args.trigger_key}")
    return args


def main() -> int:
    try:
        SceneMenuSelectionProbe(parse_args()).run()
    except (GLib.Error, OSError, RuntimeError, ValueError) as error:
        emit("fatal", error=str(error))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
