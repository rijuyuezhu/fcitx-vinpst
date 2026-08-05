#!/usr/bin/env python3
"""Select one real Fcitx ASR-menu target and wait for its reload."""

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


def parse_gdbus_literal(value: str) -> object:
    normalized = re.sub(r"\btrue\b", "True", value)
    normalized = re.sub(r"\bfalse\b", "False", normalized)
    return ast.literal_eval(normalized)


def get_asr_state() -> dict[str, Any]:
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
    targets = []
    for row in rows:
        if not isinstance(row, tuple) or len(row) != 5:
            raise TypeError(f"invalid ASR target row: {row!r}")
        provider_id, kind, item_id, display_title, model_value = row
        targets.append(
            {
                "provider_id": provider_id,
                "kind": kind,
                "item_id": item_id,
                "display_title": display_title,
                "model_value": model_value,
            }
        )
    return {
        "target_provider": target_provider,
        "target_model": target_model,
        "effective_provider": effective_provider,
        "effective_model": effective_model,
        "reload_in_progress": reload_in_progress,
        "last_error": last_error,
        "targets": targets,
    }


@dataclass
class SelectionState:
    connected: bool = False
    menu_seen: bool = False
    menu_closed: bool = False
    filter_complete: bool = False
    selection_scheduled: bool = False
    selected: bool = False
    failure_preserved: bool = False
    timed_out: bool = False
    failure_error: str = ""
    final_state: dict[str, Any] = field(default_factory=dict)
    candidates: list[str] = field(default_factory=list)
    latest_candidates: list[str] = field(default_factory=list)
    commits: list[str] = field(default_factory=list)
    key_events: list[dict[str, Any]] = field(default_factory=list)


class AsrSelectionProbe:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.state = SelectionState()
        self.before = get_asr_state()
        expected_rows = [
            target
            for target in self.before["targets"]
            if target["provider_id"] == args.expected_provider
            and target["model_value"] == args.expected_model
        ]
        if len(expected_rows) != 1:
            raise RuntimeError(
                "expected exactly one ASR target row for "
                f"{args.expected_provider}/{args.expected_model}, found {len(expected_rows)}"
            )
        if (
            self.before["effective_provider"] == args.expected_provider
            and self.before["effective_model"] == args.expected_model
        ):
            raise RuntimeError("expected ASR target is already effective")
        self.target = expected_rows[0]
        self.loop = GLib.MainLoop()
        self.client = FcitxG.Client.new()
        self.client.set_program("fcitx-vinpst-asr-selection-live-probe")
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

    def send_filter_step(self, index: int) -> bool:
        if index == 0:
            self.tap("filter-slash", Gdk.KEY_slash)
        else:
            character = self.args.filter_text[index - 1]
            self.tap(f"filter-{index - 1}", Gdk.unicode_to_keyval(ord(character)))
        if index < len(self.args.filter_text):
            GLib.timeout_add(40, self.send_filter_step, index + 1)
        else:
            self.state.filter_complete = True
            GLib.timeout_add(100, self.select_filtered_candidate)
        return GLib.SOURCE_REMOVE

    def select_filtered_candidate(self) -> bool:
        if self.state.selection_scheduled:
            return GLib.SOURCE_REMOVE
        if len(self.state.latest_candidates) != 1:
            return GLib.SOURCE_CONTINUE
        self.state.selection_scheduled = True
        self.state.candidates = list(self.state.latest_candidates)
        emit(
            "selection-target",
            provider_id=self.target["provider_id"],
            item_id=self.target["item_id"],
            display_title=self.target["display_title"],
            model_value=self.target["model_value"],
            filter_text=self.args.filter_text,
        )
        GLib.timeout_add(100, self.tap, "enter", Gdk.KEY_Return)
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
        self.state.latest_candidates = candidates
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
            if self.args.filter_text:
                GLib.timeout_add(100, self.send_filter_step, 0)
            else:
                self.state.filter_complete = True
                GLib.timeout_add(100, self.select_filtered_candidate)
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
            before_provider=self.before["effective_provider"],
            before_model=self.before["effective_model"],
            target_provider=self.args.expected_provider,
            target_model=self.args.expected_model,
        )
        GLib.timeout_add(200, self.tap, "trigger", self.args.trigger_keyval)
        GLib.timeout_add(100, self.poll_switch)

    def poll_switch(self) -> bool:
        if not self.state.menu_seen:
            return GLib.SOURCE_CONTINUE
        state = get_asr_state()
        emit(
            "asr-state",
            target_provider=state["target_provider"],
            target_model=state["target_model"],
            effective_provider=state["effective_provider"],
            effective_model=state["effective_model"],
            reload_in_progress=state["reload_in_progress"],
            last_error=state["last_error"],
        )
        if state["last_error"]:
            self.state.failure_error = state["last_error"]
            self.state.final_state = state
            emit("reload-failed", error=self.state.failure_error)
            if self.args.expect_reload_failure:
                self.state.failure_preserved = (
                    not state["reload_in_progress"]
                    and state["target_provider"] == self.args.expected_provider
                    and state["target_model"] == self.args.expected_model
                    and state["effective_provider"] == self.before["effective_provider"]
                    and state["effective_model"] == self.before["effective_model"]
                )
                GLib.timeout_add(100, self.finish)
            else:
                self.loop.quit()
            return GLib.SOURCE_REMOVE
        if (
            not state["reload_in_progress"]
            and state["target_provider"] == self.args.expected_provider
            and state["target_model"] == self.args.expected_model
            and state["effective_provider"] == self.args.expected_provider
            and state["effective_model"] == self.args.expected_model
        ):
            self.state.selected = True
            GLib.timeout_add(100, self.finish)
            return GLib.SOURCE_REMOVE
        return GLib.SOURCE_CONTINUE

    def finish(self) -> bool:
        self.loop.quit()
        return GLib.SOURCE_REMOVE

    def timeout(self) -> bool:
        self.state.timed_out = True
        emit(
            "timeout",
            provider=self.args.expected_provider,
            model=self.args.expected_model,
        )
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
            failures.append("ASR menu selection timed out")
        if not self.state.connected:
            failures.append("Fcitx input context did not connect")
        if not self.state.menu_seen:
            failures.append("ASR menu produced no candidate")
        if not self.state.filter_complete:
            failures.append("ASR menu filter did not complete")
        if len(self.state.candidates) != 1:
            failures.append(
                "ASR selection fixture did not expose exactly one candidate"
            )
        if self.args.expect_reload_failure:
            if not self.state.failure_error:
                failures.append("ASR target reload unexpectedly succeeded")
            if not self.state.failure_preserved:
                failures.append(
                    "failed ASR reload did not preserve the previous backend"
                )
            if self.state.selected:
                failures.append("failed ASR target was reported as selected")
        else:
            if self.state.failure_error:
                failures.append(f"ASR target reload failed: {self.state.failure_error}")
            if not self.state.selected:
                failures.append("Enter did not complete the expected ASR target reload")
        if not self.state.menu_closed:
            failures.append("ASR menu did not close after selection")
        if self.state.commits:
            failures.append("ASR menu selection unexpectedly committed text")
        key_events = {event["label"]: event for event in self.state.key_events}
        for label in ("trigger", "enter"):
            event = key_events.get(label)
            if event is None or not event["pressed"]:
                failures.append(f"addon did not consume {label} key press")

        emit(
            "summary",
            before_provider=self.before["effective_provider"],
            before_model=self.before["effective_model"],
            target_provider=self.args.expected_provider,
            target_model=self.args.expected_model,
            filter_text=self.args.filter_text,
            filter_complete=self.state.filter_complete,
            candidate_count=len(self.state.candidates),
            menu_closed=self.state.menu_closed,
            selected=self.state.selected,
            expect_reload_failure=self.args.expect_reload_failure,
            failure_error=self.state.failure_error,
            failure_preserved=self.state.failure_preserved,
            final_target_provider=self.state.final_state.get("target_provider", ""),
            final_target_model=self.state.final_state.get("target_model", ""),
            final_effective_provider=self.state.final_state.get(
                "effective_provider", ""
            ),
            final_effective_model=self.state.final_state.get("effective_model", ""),
            commit_count=len(self.state.commits),
            ok=not failures,
            failures=failures,
        )
        if failures:
            raise RuntimeError("; ".join(failures))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--trigger-key", default="F8")
    parser.add_argument("--expected-provider", required=True)
    parser.add_argument("--expected-model", required=True)
    parser.add_argument("--filter-text", default="")
    parser.add_argument("--expect-reload-failure", action="store_true")
    parser.add_argument("--timeout-ms", type=int, default=30_000)
    args = parser.parse_args()
    args.trigger_keyval = Gdk.keyval_from_name(args.trigger_key)
    if args.trigger_keyval == Gdk.KEY_VoidSymbol:
        parser.error(f"unknown GDK trigger key: {args.trigger_key}")
    return args


def main() -> int:
    try:
        AsrSelectionProbe(parse_args()).run()
    except (GLib.Error, OSError, RuntimeError, TypeError, ValueError) as error:
        emit("fatal", error=str(error))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
