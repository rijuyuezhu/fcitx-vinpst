#!/usr/bin/env python3
"""Exercise one persisted Fcitx Tap/Hold/Both trigger mode against the daemon."""

import argparse
import ast
import importlib
import json
import os
import subprocess
import time
from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Any

import gi

gi.require_version("FcitxG", "1.0")
gi.require_version("Gdk", "4.0")
FcitxG = importlib.import_module("gi.repository.FcitxG")
Gdk = importlib.import_module("gi.repository.Gdk")
GLib = importlib.import_module("gi.repository.GLib")

DBUS_CALL = [
    "gdbus",
    "call",
    "--session",
    "--dest",
    "org.fcitx.Vinpst",
    "--object-path",
    "/org/fcitx/Vinpst",
    "--method",
    "org.fcitx.Vinpst.Service.GetStatus",
]


def emit(event: str, **fields: object) -> None:
    print(json.dumps({"event": event, **fields}, ensure_ascii=False), flush=True)


def daemon_status() -> str:
    result = subprocess.run(
        DBUS_CALL,
        check=False,
        capture_output=True,
        text=True,
        timeout=3,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or "GetStatus failed")
    parsed = ast.literal_eval(result.stdout.strip())
    if (
        not isinstance(parsed, tuple)
        or len(parsed) != 1
        or not isinstance(parsed[0], str)
    ):
        raise TypeError(f"unexpected GetStatus reply: {parsed!r}")
    return parsed[0]


@dataclass
class TriggerState:
    connected: bool = False
    timed_out: bool = False
    failed: str = ""
    phase: str = "initial"
    status_samples: list[dict[str, object]] = field(default_factory=list)
    key_events: list[dict[str, object]] = field(default_factory=list)
    commits: list[str] = field(default_factory=list)
    tap_started: bool = False
    tap_release_kept_recording: bool = False
    tap_stopped: bool = False
    short_hold_cancelled: bool = False
    hold_started: bool = False
    hold_stopped: bool = False


class TriggerModeProbe:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.state = TriggerState()
        self.started_at = time.monotonic()
        self.loop = GLib.MainLoop()
        self.client = FcitxG.Client.new()
        self.client.set_program(f"fcitx-vinpst-trigger-{args.mode.lower()}-live-probe")
        self.client.set_display(
            os.environ.get("WAYLAND_DISPLAY") or os.environ.get("DISPLAY") or ""
        )
        self.client.connect("connected", self.on_connected)
        self.client.connect("commit-string", self.on_commit)

    def elapsed_ms(self) -> int:
        return round((time.monotonic() - self.started_at) * 1000)

    def sample_status(self, label: str) -> str:
        status = daemon_status()
        sample = {"label": label, "status": status, "elapsed_ms": self.elapsed_ms()}
        self.state.status_samples.append(sample)
        emit("status", mode=self.args.mode, **sample)
        return status

    def key(self, label: str, release: bool) -> bool:
        accepted = bool(
            self.client.process_key_sync(
                self.args.trigger_keyval,
                0,
                0,
                release,
                0,
            )
        )
        event = {
            "label": label,
            "release": release,
            "accepted": accepted,
            "elapsed_ms": self.elapsed_ms(),
        }
        self.state.key_events.append(event)
        emit("key", mode=self.args.mode, **event)
        if not accepted:
            self.fail(f"addon did not consume {label}")
        return GLib.SOURCE_REMOVE

    def press(self, label: str) -> bool:
        return self.key(label, False)

    def release(self, label: str) -> bool:
        return self.key(label, True)

    def on_commit(self, _client: Any, text: str) -> None:
        self.state.commits.append(text)
        emit("commit", mode=self.args.mode, text=text)

    def fail(self, message: str) -> None:
        if not self.state.failed:
            self.state.failed = message
            emit("failure", mode=self.args.mode, phase=self.state.phase, error=message)
            self.loop.quit()

    def wait_status(
        self,
        expected: str,
        label: str,
        timeout_ms: int,
        callback: Callable[[], None],
    ) -> None:
        deadline = time.monotonic() + timeout_ms / 1000

        def poll() -> bool:
            try:
                status = self.sample_status(label)
            except (
                OSError,
                RuntimeError,
                subprocess.SubprocessError,
                TypeError,
                ValueError,
            ) as error:
                self.fail(str(error))
                return GLib.SOURCE_REMOVE
            if status == expected:
                callback()
                return GLib.SOURCE_REMOVE
            if time.monotonic() >= deadline:
                self.fail(f"{label} did not reach {expected}; last status was {status}")
                return GLib.SOURCE_REMOVE
            return GLib.SOURCE_CONTINUE

        GLib.timeout_add(50, poll)

    def require_status_after(
        self,
        delay_ms: int,
        expected: str,
        label: str,
        callback: Callable[[], None],
    ) -> None:
        def check() -> bool:
            try:
                status = self.sample_status(label)
            except (
                OSError,
                RuntimeError,
                subprocess.SubprocessError,
                TypeError,
                ValueError,
            ) as error:
                self.fail(str(error))
                return GLib.SOURCE_REMOVE
            if status != expected:
                self.fail(f"{label} expected {expected}, found {status}")
                return GLib.SOURCE_REMOVE
            callback()
            return GLib.SOURCE_REMOVE

        GLib.timeout_add(delay_ms, check)

    def start_tap_cycle(self, callback: Callable[[], None]) -> None:
        self.state.phase = "tap-start"
        self.press("tap-start-press")

        def recording_started() -> None:
            self.state.tap_started = True
            self.release("tap-start-release")

            def release_preserved() -> None:
                self.state.tap_release_kept_recording = True
                self.state.phase = "tap-stop"
                self.press("tap-stop-press")
                GLib.timeout_add(50, self.release, "tap-stop-release")

                def stopped() -> None:
                    self.state.tap_stopped = True
                    callback()

                self.wait_status("idle", "tap-stop", 5000, stopped)

            self.require_status_after(
                350,
                "recording",
                "tap-release-preserved-recording",
                release_preserved,
            )

        self.wait_status("recording", "tap-start", 2000, recording_started)

    def start_hold_cycle(self, callback: Callable[[], None]) -> None:
        self.state.phase = "hold-long-start"
        self.press("hold-long-press")

        def recording_started() -> None:
            self.state.hold_started = True

            def release_long() -> bool:
                self.release("hold-long-release")

                def stopped() -> None:
                    self.state.hold_stopped = True
                    callback()

                self.wait_status("idle", "hold-release-stop", 6000, stopped)
                return GLib.SOURCE_REMOVE

            release_delay_ms = 400 if self.args.mode == "Both" else 150
            GLib.timeout_add(release_delay_ms, release_long)

        self.wait_status("recording", "hold-long-start", 2500, recording_started)

    def start_hold_mode(self) -> None:
        self.state.phase = "hold-short-cancel"
        self.press("hold-short-press")
        GLib.timeout_add(100, self.release, "hold-short-release")

        def short_cancelled() -> None:
            self.state.short_hold_cancelled = True
            self.start_hold_cycle(self.finish)

        self.require_status_after(
            550,
            "idle",
            "hold-short-remained-idle",
            short_cancelled,
        )

    def start_both_mode(self) -> None:
        def tap_done() -> None:
            self.require_status_after(
                200,
                "idle",
                "both-between-cycles",
                lambda: self.start_hold_cycle(self.finish),
            )

        self.start_tap_cycle(tap_done)

    def on_connected(self, _client: Any) -> None:
        self.state.connected = True
        capabilities = (1 << 1) | (1 << 4) | (1 << 6) | (1 << 39)
        self.client.set_capability(capabilities)
        self.client.focus_in()
        emit(
            "connected",
            mode=self.args.mode,
            valid=bool(self.client.is_valid()),
            initial_status=self.sample_status("initial"),
        )
        if self.args.mode == "Tap":
            GLib.timeout_add(200, self.start_tap_mode)
        elif self.args.mode == "Hold":
            GLib.timeout_add(200, self.start_hold_mode)
        else:
            GLib.timeout_add(200, self.start_both_mode)

    def start_tap_mode(self) -> bool:
        self.start_tap_cycle(self.finish)
        return GLib.SOURCE_REMOVE

    def finish(self) -> None:
        self.state.phase = "complete"
        self.loop.quit()

    def timeout(self) -> bool:
        self.state.timed_out = True
        self.fail("trigger mode probe timed out")
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
        if self.state.failed:
            failures.append(self.state.failed)
        if self.state.timed_out:
            failures.append("trigger mode probe timed out")
        if not self.state.connected:
            failures.append("Fcitx input context did not connect")
        if self.state.commits:
            failures.append("mock-audio trigger probe unexpectedly committed text")
        if self.args.mode in ("Tap", "Both"):
            if not self.state.tap_started:
                failures.append("tap press did not start recording")
            if not self.state.tap_release_kept_recording:
                failures.append("tap release unexpectedly stopped recording")
            if not self.state.tap_stopped:
                failures.append("second tap did not stop recording")
        if self.args.mode == "Hold" and not self.state.short_hold_cancelled:
            failures.append("short Hold press did not remain idle")
        if self.args.mode in ("Hold", "Both"):
            if not self.state.hold_started:
                failures.append("long Hold press did not start recording")
            if not self.state.hold_stopped:
                failures.append("long Hold release did not stop recording")
        if daemon_status() != "idle":
            failures.append("daemon was not idle after trigger mode probe")
        if not self.state.key_events or not all(
            bool(event["accepted"]) for event in self.state.key_events
        ):
            failures.append("not every trigger key event was consumed")

        emit(
            "summary",
            mode=self.args.mode,
            tap_started=self.state.tap_started,
            tap_release_kept_recording=self.state.tap_release_kept_recording,
            tap_stopped=self.state.tap_stopped,
            short_hold_cancelled=self.state.short_hold_cancelled,
            hold_started=self.state.hold_started,
            hold_stopped=self.state.hold_stopped,
            status_samples=self.state.status_samples,
            key_event_count=len(self.state.key_events),
            commit_count=len(self.state.commits),
            final_status=daemon_status(),
            ok=not failures,
            failures=failures,
        )
        if failures:
            raise RuntimeError("; ".join(failures))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=("Tap", "Hold", "Both"), required=True)
    parser.add_argument("--trigger-key", default="F9")
    parser.add_argument("--timeout-ms", type=int, default=15_000)
    args = parser.parse_args()
    args.trigger_keyval = Gdk.keyval_from_name(args.trigger_key)
    if args.trigger_keyval == Gdk.KEY_VoidSymbol:
        parser.error(f"unknown GDK trigger key: {args.trigger_key}")
    return args


def main() -> int:
    try:
        TriggerModeProbe(parse_args()).run()
    except (
        GLib.Error,
        OSError,
        RuntimeError,
        subprocess.SubprocessError,
        TypeError,
        ValueError,
    ) as error:
        emit("fatal", error=str(error))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
