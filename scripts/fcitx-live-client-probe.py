#!/usr/bin/env python3
"""Drive the real Fcitx5 input context through live vinput dictation paths."""

import argparse
import importlib
import json
import os
import re
import signal
import subprocess
import sys
import wave
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import gi

gi.require_version("FcitxG", "1.0")
gi.require_version("Gdk", "4.0")
FcitxG = importlib.import_module("gi.repository.FcitxG")
Gdk = importlib.import_module("gi.repository.Gdk")
GLib = importlib.import_module("gi.repository.GLib")


PLACEHOLDER_PREEDITS = {
    "... Recording ...",
    "... Commanding ...",
    "... Inferring ...",
    "... Postprocessing ...",
}


@dataclass
class ProbeState:
    connected: bool = False
    secondary_connected: bool = False
    key_events: list[dict[str, Any]] = field(default_factory=list)
    preedits: list[str] = field(default_factory=list)
    secondary_preedits: list[str] = field(default_factory=list)
    candidates: list[str] = field(default_factory=list)
    deletes: list[tuple[int, int]] = field(default_factory=list)
    commits: list[str] = field(default_factory=list)
    secondary_commits: list[str] = field(default_factory=list)
    buffer: str = ""
    playback: subprocess.Popen[bytes] | None = None
    candidate_selected: bool = False
    scheduled: bool = False
    focus_switched: bool = False
    owner_lost: bool = False
    owner_pid: int | None = None
    owner_loss_preedits: list[str] = field(default_factory=list)
    timed_out: bool = False


def emit(event: str, **fields: object) -> None:
    print(json.dumps({"event": event, **fields}, ensure_ascii=False), flush=True)


def wav_duration_ms(path: Path) -> int:
    with wave.open(str(path), "rb") as handle:
        rate = handle.getframerate()
        if rate <= 0:
            raise ValueError(f"invalid WAV sample rate: {rate}")
        return max(1, round(handle.getnframes() * 1000 / rate))


class LiveProbe:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        initial_buffer = (
            args.selected_text
            if args.mode == "command" and not args.primary_selection_fallback
            else ""
        )
        self.state = ProbeState(buffer=initial_buffer)
        self.loop = GLib.MainLoop()
        self.client = FcitxG.Client.new()
        self.client.set_program(f"fcitx-vinput-{args.mode}-live-probe")
        self.client.set_display(
            os.environ.get("WAYLAND_DISPLAY") or os.environ.get("DISPLAY") or ""
        )
        self.secondary_client = FcitxG.Client.new() if args.focus_switch else None
        self.stop_after_ms = (
            args.start_delay_ms + args.manual_recording_ms
            if args.manual_recording_ms > 0
            else args.play_delay_ms + wav_duration_ms(args.wav) + args.playback_tail_ms
        )

        self.client.connect("connected", self.on_connected)
        self.client.connect("commit-string", self.on_commit)
        self.client.connect("delete-surrounding-text", self.on_delete)
        self.client.connect("update-formatted-preedit", self.on_formatted_preedit)
        self.client.connect("update-client-side-ui", self.on_client_ui)
        if self.secondary_client is not None:
            self.secondary_client.set_program("fcitx-vinput-focus-target-live-probe")
            self.secondary_client.set_display(
                os.environ.get("WAYLAND_DISPLAY") or os.environ.get("DISPLAY") or ""
            )
            self.secondary_client.connect("connected", self.on_secondary_connected)
            self.secondary_client.connect("commit-string", self.on_secondary_commit)
            self.secondary_client.connect(
                "update-formatted-preedit", self.on_secondary_formatted_preedit
            )
            self.secondary_client.connect(
                "update-client-side-ui", self.on_secondary_client_ui
            )

    def tap(self, client: Any, context: str, keyval: int) -> None:
        pressed = bool(client.process_key_sync(keyval, 0, 0, False, 0))
        released = bool(client.process_key_sync(keyval, 0, 0, True, 0))
        event = {
            "context": context,
            "keyval": keyval,
            "pressed": pressed,
            "released": released,
        }
        self.state.key_events.append(event)
        emit("key", **event)

    def record_preedit(self, event: str, items: object, cursor: int) -> str:
        text = "".join((item.string or "") for item in (items or []))
        if text and text not in PLACEHOLDER_PREEDITS:
            self.state.preedits.append(text)
        if self.state.owner_lost:
            self.state.owner_loss_preedits.append(text)
        emit(event, text=text, cursor=cursor)
        return text

    def on_formatted_preedit(
        self, _client: FcitxG.Client, items: object, cursor: int
    ) -> None:
        self.record_preedit("formatted-preedit", items, cursor)

    def record_secondary_preedit(self, event: str, items: object, cursor: int) -> None:
        text = "".join((item.string or "") for item in (items or []))
        if text and text not in PLACEHOLDER_PREEDITS:
            self.state.secondary_preedits.append(text)
        emit(event, text=text, cursor=cursor)

    def on_secondary_formatted_preedit(
        self, _client: FcitxG.Client, items: object, cursor: int
    ) -> None:
        self.record_secondary_preedit("secondary-formatted-preedit", items, cursor)

    def on_client_ui(
        self,
        _client: FcitxG.Client,
        preedit: object,
        preedit_cursor: int,
        _aux_up: object,
        _aux_down: object,
        candidate_list: object,
        candidate_cursor: int,
        _candidate_layout_hint: int,
        _has_prev: bool,
        _has_next: bool,
    ) -> None:
        text = self.record_preedit("client-ui", preedit, preedit_cursor)
        candidates = [item.candidate or "" for item in (candidate_list or [])]
        if candidates:
            self.state.candidates = candidates
        emit(
            "input-panel",
            text=text,
            candidates=candidates,
            candidate_cursor=candidate_cursor,
        )
        if (
            self.args.mode == "command"
            and candidates
            and not self.state.candidate_selected
        ):
            GLib.timeout_add(
                self.args.candidate_delay_ms, self.select_command_candidate
            )

    def on_secondary_client_ui(
        self,
        _client: FcitxG.Client,
        preedit: object,
        preedit_cursor: int,
        _aux_up: object,
        _aux_down: object,
        candidate_list: object,
        candidate_cursor: int,
        _candidate_layout_hint: int,
        _has_prev: bool,
        _has_next: bool,
    ) -> None:
        self.record_secondary_preedit("secondary-client-ui", preedit, preedit_cursor)
        candidates = [item.candidate or "" for item in (candidate_list or [])]
        emit(
            "secondary-input-panel",
            candidates=candidates,
            candidate_cursor=candidate_cursor,
        )

    def on_delete(self, _client: FcitxG.Client, cursor: int, length: int) -> None:
        self.state.deletes.append((cursor, length))
        encoded = self.state.buffer.encode("utf-8")
        start = len(encoded) + cursor
        if start < 0 or start + length > len(encoded):
            emit(
                "delete-invalid", cursor=cursor, length=length, buffer=self.state.buffer
            )
            return
        self.state.buffer = (encoded[:start] + encoded[start + length :]).decode(
            "utf-8"
        )
        emit("delete", cursor=cursor, length=length, buffer=self.state.buffer)

    def on_commit(self, _client: FcitxG.Client, text: str) -> None:
        self.state.commits.append(text)
        self.state.buffer += text
        emit("commit", text=text, buffer=self.state.buffer)
        GLib.timeout_add(300, self.finish)

    def on_secondary_commit(self, _client: FcitxG.Client, text: str) -> None:
        self.state.secondary_commits.append(text)
        emit("secondary-commit", text=text)

    def select_command_candidate(self) -> bool:
        if self.state.candidate_selected or not self.state.candidates:
            return GLib.SOURCE_REMOVE
        self.state.candidate_selected = True
        index = len(self.state.candidates) - 1
        emit("select-candidate", index=index, candidate=self.state.candidates[index])
        self.client.select_candidate(index)
        return GLib.SOURCE_REMOVE

    def start_recording(self) -> bool:
        self.tap(
            self.client,
            "primary",
            Gdk.KEY_F10 if self.args.mode == "command" else Gdk.KEY_F9,
        )
        return GLib.SOURCE_REMOVE

    def start_playback(self) -> bool:
        command = [self.args.playback_command]
        if self.args.playback_target:
            command.extend(("--target", self.args.playback_target))
        command.append(str(self.args.wav))
        self.state.playback = subprocess.Popen(command)
        emit(
            "playback-start",
            pid=self.state.playback.pid,
            command=command,
            sample=str(self.args.wav),
            target=self.args.playback_target,
        )
        return GLib.SOURCE_REMOVE

    def stop_recording(self) -> bool:
        if self.state.playback is not None:
            try:
                returncode = self.state.playback.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.state.playback.terminate()
                returncode = self.state.playback.wait(timeout=2)
            emit("playback-exit", returncode=returncode)
        stop_client = self.secondary_client if self.args.focus_switch else self.client
        assert stop_client is not None
        self.tap(
            stop_client,
            "secondary" if self.args.focus_switch else "primary",
            Gdk.KEY_F10 if self.args.mode == "command" else Gdk.KEY_F9,
        )
        return GLib.SOURCE_REMOVE

    def switch_focus(self) -> bool:
        if self.secondary_client is None:
            return GLib.SOURCE_REMOVE
        self.client.focus_out()
        self.secondary_client.focus_in()
        self.state.focus_switched = True
        emit("focus-switch", source="primary", target="secondary")
        return GLib.SOURCE_REMOVE

    @staticmethod
    def dbus_call(method: str, *arguments: str) -> str:
        completed = subprocess.run(
            [
                "gdbus",
                "call",
                "--session",
                "--dest",
                "org.freedesktop.DBus",
                "--object-path",
                "/org/freedesktop/DBus",
                "--method",
                f"org.freedesktop.DBus.{method}",
                *arguments,
            ],
            check=True,
            capture_output=True,
            text=True,
            timeout=5,
        )
        return completed.stdout.strip()

    def kill_daemon_owner(self) -> bool:
        try:
            owner_result = self.dbus_call("GetNameOwner", "org.fcitx.Vinput")
            owner_match = re.search(r"'([^']+)'", owner_result)
            if owner_match is None:
                raise RuntimeError(f"could not parse daemon owner: {owner_result}")
            pid_result = self.dbus_call(
                "GetConnectionUnixProcessID", owner_match.group(1)
            )
            pid_match = re.search(r"uint32\s+(\d+)", pid_result)
            if pid_match is None:
                raise RuntimeError(f"could not parse daemon owner PID: {pid_result}")
            pid = int(pid_match.group(1))
            executable = os.readlink(f"/proc/{pid}/exe")
            command_line = (
                Path(f"/proc/{pid}/cmdline").read_bytes().replace(b"\0", b" ")
            )
            if (
                "vinput-daemon" not in Path(executable).name
                and b"vinput-daemon" not in command_line
            ):
                raise RuntimeError(
                    "refusing to stop unexpected org.fcitx.Vinput owner: "
                    f"pid={pid} exe={executable}"
                )
            self.state.owner_pid = pid
            self.state.owner_lost = True
            try:
                os.kill(pid, signal.SIGTERM)
            except OSError:
                self.state.owner_lost = False
                raise
            emit("owner-loss", pid=pid, executable=executable)
            GLib.timeout_add(self.args.owner_loss_settle_ms, self.finish)
        except (OSError, RuntimeError, subprocess.SubprocessError) as error:
            emit("owner-loss-error", error=str(error))
            self.state.timed_out = True
            self.loop.quit()
        return GLib.SOURCE_REMOVE

    def cleanup(self) -> None:
        playback = self.state.playback
        if playback is not None and playback.poll() is None:
            playback.terminate()
            try:
                playback.wait(timeout=2)
            except subprocess.TimeoutExpired:
                playback.kill()
                playback.wait(timeout=2)
        if self.state.connected:
            self.client.focus_out()
        if self.state.secondary_connected and self.secondary_client is not None:
            self.secondary_client.focus_out()

    def maybe_schedule(self) -> None:
        if self.state.scheduled or not self.state.connected:
            return
        if self.args.focus_switch and not self.state.secondary_connected:
            return
        self.state.scheduled = True
        self.client.focus_in()
        emit(
            "connected",
            valid=bool(self.client.is_valid()),
            mode=self.args.mode,
            buffer=self.state.buffer,
            selection_source=(
                "primary"
                if self.args.primary_selection_fallback
                else "surrounding"
                if self.args.mode == "command"
                else "none"
            ),
        )
        if self.args.manual_recording_ms > 0:
            emit(
                "manual-speech-window",
                starts_after_ms=self.args.start_delay_ms,
                recording_ms=self.args.manual_recording_ms,
            )
        GLib.timeout_add(self.args.start_delay_ms, self.start_recording)
        if self.args.manual_recording_ms == 0:
            GLib.timeout_add(self.args.play_delay_ms, self.start_playback)
        if self.args.focus_switch:
            GLib.timeout_add(self.args.focus_switch_delay_ms, self.switch_focus)
        if self.args.owner_loss:
            GLib.timeout_add(self.args.owner_loss_delay_ms, self.kill_daemon_owner)
        else:
            GLib.timeout_add(self.stop_after_ms, self.stop_recording)

    def on_connected(self, _client: FcitxG.Client) -> None:
        self.state.connected = True
        capabilities = (1 << 1) | (1 << 4) | (1 << 6) | (1 << 39)
        self.client.set_capability(capabilities)
        if self.args.mode == "command" and not self.args.primary_selection_fallback:
            selected_bytes = len(self.args.selected_text.encode("utf-8"))
            self.client.set_surrounding_text(self.args.selected_text, selected_bytes, 0)
        self.maybe_schedule()

    def on_secondary_connected(self, _client: FcitxG.Client) -> None:
        assert self.secondary_client is not None
        self.state.secondary_connected = True
        capabilities = (1 << 1) | (1 << 4) | (1 << 6) | (1 << 39)
        self.secondary_client.set_capability(capabilities)
        emit("secondary-connected", valid=bool(self.secondary_client.is_valid()))
        self.maybe_schedule()

    def finish(self) -> bool:
        self.loop.quit()
        return GLib.SOURCE_REMOVE

    def timeout(self) -> bool:
        self.state.timed_out = True
        emit("timeout", mode=self.args.mode, buffer=self.state.buffer)
        self.loop.quit()
        return GLib.SOURCE_REMOVE

    def run(self) -> None:
        GLib.timeout_add(self.stop_after_ms + self.args.result_timeout_ms, self.timeout)
        try:
            self.loop.run()
            self.validate()
        finally:
            self.cleanup()

    def validate(self) -> None:
        failures: list[str] = []
        if self.state.timed_out:
            failures.append("probe timed out before commit")
        if not self.state.connected:
            failures.append("Fcitx input context did not connect")
        if self.args.focus_switch and not self.state.secondary_connected:
            failures.append("secondary Fcitx input context did not connect")
        expected_key_events = 1 if self.args.owner_loss else 2
        if len(self.state.key_events) < expected_key_events or not all(
            event["pressed"] and event["released"] for event in self.state.key_events
        ):
            failures.append("addon did not consume the expected trigger taps")
        if self.args.require_partial and not self.state.preedits:
            failures.append("client received no non-placeholder partial preedit")
        if self.args.owner_loss:
            if not self.state.owner_lost:
                failures.append("daemon owner was not stopped")
            if self.state.commits:
                failures.append("owner loss committed a partial result")
            if not any(
                "unavailable" in preedit.lower()
                for preedit in self.state.owner_loss_preedits
            ):
                failures.append("owner loss did not surface an unavailable preedit")
        elif not self.state.commits or not self.state.commits[-1]:
            failures.append("client received no final commit")
        if self.args.expected_commit_prefix and (
            not self.state.commits
            or not self.state.commits[-1].startswith(self.args.expected_commit_prefix)
        ):
            failures.append("final commit did not match expected prefix")
        if self.args.mode == "command":
            if not self.state.candidates and not self.args.allow_direct_command_commit:
                failures.append("command mode produced no candidate menu")
            final_commit = self.state.commits[-1] if self.state.commits else ""
            if self.args.primary_selection_fallback:
                if self.state.deletes:
                    failures.append(
                        "primary-selection fallback unexpectedly deleted surrounding text"
                    )
                if self.args.selected_text not in final_commit:
                    failures.append(
                        "primary-selection fallback commit did not contain selected text"
                    )
            else:
                if not self.state.deletes:
                    failures.append("command mode did not delete selected text")
                if self.state.buffer == self.args.selected_text:
                    failures.append("command mode did not replace selected text")
        if self.args.focus_switch:
            if not self.state.focus_switched:
                failures.append("focus did not switch to the secondary context")
            if self.state.secondary_preedits:
                failures.append("partial preedit leaked to the secondary context")
            if self.state.secondary_commits:
                failures.append("final commit leaked to the secondary context")

        summary = {
            "mode": self.args.mode,
            "manual_recording_ms": self.args.manual_recording_ms,
            "manual_speech": self.args.manual_recording_ms > 0,
            "require_partial": self.args.require_partial,
            "partial_count": len(self.state.preedits),
            "commit": self.state.commits[-1] if self.state.commits else "",
            "expected_commit_prefix": self.args.expected_commit_prefix,
            "allow_direct_command_commit": self.args.allow_direct_command_commit,
            "primary_selection_fallback": self.args.primary_selection_fallback,
            "selection_source": (
                "primary"
                if self.args.primary_selection_fallback
                else "surrounding"
                if self.args.mode == "command"
                else "none"
            ),
            "selected_text": self.args.selected_text,
            "surrounding_text_provided": (
                self.args.mode == "command" and not self.args.primary_selection_fallback
            ),
            "candidate_count": len(self.state.candidates),
            "delete_count": len(self.state.deletes),
            "focus_switch": self.args.focus_switch,
            "focus_switched": self.state.focus_switched,
            "secondary_partial_count": len(self.state.secondary_preedits),
            "secondary_commit_count": len(self.state.secondary_commits),
            "owner_loss": self.args.owner_loss,
            "owner_pid": self.state.owner_pid,
            "owner_loss_preedit_count": len(self.state.owner_loss_preedits),
            "owner_loss_preedit": (
                self.state.owner_loss_preedits[-1]
                if self.state.owner_loss_preedits
                else ""
            ),
            "final_buffer": self.state.buffer,
            "ok": not failures,
            "failures": failures,
        }
        emit("summary", **summary)
        if failures:
            raise RuntimeError("; ".join(failures))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=("normal", "command"), required=True)
    parser.add_argument("--wav", type=Path)
    parser.add_argument("--manual-recording-ms", type=int, default=0)
    parser.add_argument("--selected-text", default="selected text")
    parser.add_argument("--expected-commit-prefix", default="")
    parser.add_argument("--allow-direct-command-commit", action="store_true")
    parser.add_argument("--primary-selection-fallback", action="store_true")
    parser.add_argument(
        "--require-partial",
        action=argparse.BooleanOptionalAction,
        default=True,
    )
    parser.add_argument("--playback-command", default="pw-play")
    parser.add_argument("--playback-target", default="")
    parser.add_argument("--start-delay-ms", type=int, default=300)
    parser.add_argument("--play-delay-ms", type=int, default=1200)
    parser.add_argument("--playback-tail-ms", type=int, default=1000)
    parser.add_argument("--candidate-delay-ms", type=int, default=200)
    parser.add_argument("--focus-switch", action="store_true")
    parser.add_argument("--focus-switch-delay-ms", type=int, default=2000)
    parser.add_argument("--owner-loss", action="store_true")
    parser.add_argument("--owner-loss-delay-ms", type=int, default=2500)
    parser.add_argument("--owner-loss-settle-ms", type=int, default=1500)
    parser.add_argument("--result-timeout-ms", type=int, default=8000)
    args = parser.parse_args()
    if args.manual_recording_ms < 0:
        parser.error("--manual-recording-ms must be non-negative")
    if args.manual_recording_ms > 0:
        if args.wav is not None:
            parser.error("--wav and --manual-recording-ms are mutually exclusive")
        if args.mode != "normal":
            parser.error("--manual-recording-ms currently supports normal mode only")
        if args.focus_switch or args.owner_loss:
            parser.error(
                "--manual-recording-ms is separate from focus-switch and owner-loss cases"
            )
    else:
        if args.wav is None:
            parser.error("--wav is required unless --manual-recording-ms is used")
        args.wav = args.wav.resolve()
        if not args.wav.is_file():
            parser.error(f"WAV does not exist: {args.wav}")
    if args.focus_switch and args.mode != "normal":
        parser.error("--focus-switch currently supports normal mode only")
    if args.owner_loss and args.mode != "normal":
        parser.error("--owner-loss currently supports normal mode only")
    if args.owner_loss and args.focus_switch:
        parser.error("--owner-loss and --focus-switch are separate live cases")
    if args.expected_commit_prefix and args.mode != "command":
        parser.error("--expected-commit-prefix currently supports command mode only")
    if args.allow_direct_command_commit and args.mode != "command":
        parser.error("--allow-direct-command-commit supports command mode only")
    if args.primary_selection_fallback and args.mode != "command":
        parser.error("--primary-selection-fallback supports command mode only")
    if args.focus_switch and args.focus_switch_delay_ms >= (
        args.play_delay_ms + wav_duration_ms(args.wav) + args.playback_tail_ms
    ):
        parser.error("--focus-switch-delay-ms must occur before the stop trigger")
    if args.owner_loss and args.owner_loss_delay_ms <= args.play_delay_ms:
        parser.error("--owner-loss-delay-ms must occur after playback starts")
    return args


def main() -> int:
    try:
        LiveProbe(parse_args()).run()
    except (
        GLib.Error,
        OSError,
        RuntimeError,
        ValueError,
        subprocess.SubprocessError,
    ) as error:
        emit("fatal", error=str(error))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
