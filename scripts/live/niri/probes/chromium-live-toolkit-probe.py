#!/usr/bin/env python3
import argparse
import json
import os
import re
import secrets
import select
import shutil
import signal
import subprocess
import sys
import threading
import time
import urllib.parse
from dataclasses import dataclass, field
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

DBUS_DEST = "org.fcitx.Vinpst"
DBUS_PATH = "/org/fcitx/Vinpst"
DBUS_INTERFACE = "org.fcitx.Vinpst.Service"
PARTIAL_RE = re.compile(r"RecognitionPartial\s+\('((?:\\.|[^'])*)'")


@dataclass
class ProbeState:
    mode: str
    initial_text: str
    expected_commit_substring: str
    require_partial: bool
    log_path: Path
    lock: threading.Lock = field(default_factory=threading.Lock)
    done: threading.Event = field(default_factory=threading.Event)
    ready: bool = False
    selection_ready: bool = False
    partial_seen: bool = False
    recording_seen: bool = False
    commit_seen: bool = False
    replacement_seen: bool = False
    timed_out: bool = False
    text: str = ""

    def emit(self, event: str, **payload: Any) -> None:
        record = {"event": event, **payload}
        line = json.dumps(record, ensure_ascii=False, separators=(",", ":"))
        with self.lock:
            print(line, flush=True)
            with self.log_path.open("a", encoding="utf-8") as stream:
                stream.write(line + "\n")

    def browser_event(self, payload: dict[str, Any]) -> None:
        event = str(payload.get("event", ""))
        text = str(payload.get("text", ""))
        self.emit(
            event,
            toolkit="chromium",
            mode=self.mode,
            **{
                k: v
                for k, v in payload.items()
                if k not in {"event", "toolkit", "mode"}
            },
        )
        with self.lock:
            if event == "ready":
                self.ready = True
            elif event == "selection-ready":
                self.selection_ready = text == self.initial_text
            elif event == "changed":
                self.text = text

    def mark_partial(self, text: str) -> None:
        with self.lock:
            if text and self.ready:
                self.partial_seen = True
                self.recording_seen = True
        self.emit("daemon-partial", text=text)

    def evaluate(self, daemon_status: str) -> None:
        with self.lock:
            if daemon_status == "recording":
                self.recording_seen = True
                return
            if not self.ready or not self.recording_seen or daemon_status != "idle":
                return
            partial_ok = not self.require_partial or self.partial_seen
            selection_ok = self.mode == "normal" or self.selection_ready
            expected_ok = (
                not self.expected_commit_substring
                or self.expected_commit_substring in self.text
            )
            if self.mode == "normal":
                outcome_ok = bool(self.text)
            else:
                outcome_ok = bool(self.text) and self.text != self.initial_text
            if partial_ok and selection_ok and expected_ok and outcome_ok:
                self.commit_seen = True
                self.replacement_seen = self.mode == "command"
                self.done.set()

    def summary(self) -> dict[str, Any]:
        with self.lock:
            partial_ok = not self.require_partial or self.partial_seen
            selection_ok = self.mode == "normal" or self.selection_ready
            expected_ok = (
                not self.expected_commit_substring
                or self.expected_commit_substring in self.text
            )
            outcome_ok = (
                self.commit_seen if self.mode == "normal" else self.replacement_seen
            )
            ok = (
                partial_ok
                and selection_ok
                and expected_ok
                and outcome_ok
                and not self.timed_out
            )
            return {
                "event": "summary",
                "toolkit": "chromium",
                "mode": self.mode,
                "ready": self.ready,
                "partial": self.partial_seen,
                "commit": self.commit_seen,
                "replacement": self.replacement_seen,
                "selection_ready": self.selection_ready,
                "expected_commit": expected_ok,
                "timed_out": self.timed_out,
                "ok": ok,
                "text": self.text,
            }


class ProbeServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(
        self, address: tuple[str, int], state: ProbeState, token: str, html: bytes
    ):
        self.state = state
        self.token = token
        self.html = html
        super().__init__(address, ProbeHandler)


class ProbeHandler(BaseHTTPRequestHandler):
    server: ProbeServer

    def log_message(self, _format: str, *_args: Any) -> None:
        return

    def _authorized(self) -> bool:
        query = urllib.parse.parse_qs(urllib.parse.urlsplit(self.path).query)
        return query.get("token", [""])[0] == self.server.token

    def do_GET(self) -> None:
        parsed = urllib.parse.urlsplit(self.path)
        if parsed.path != "/probe" or not self._authorized():
            self.send_error(HTTPStatus.NOT_FOUND)
            return
        self.send_response(HTTPStatus.OK)
        self.send_header("content-type", "text/html; charset=utf-8")
        self.send_header("cache-control", "no-store")
        self.send_header("content-length", str(len(self.server.html)))
        self.end_headers()
        self.wfile.write(self.server.html)

    def do_POST(self) -> None:
        parsed = urllib.parse.urlsplit(self.path)
        if parsed.path != "/event" or not self._authorized():
            self.send_error(HTTPStatus.NOT_FOUND)
            return
        try:
            length = int(self.headers.get("content-length", "0"))
            payload = json.loads(self.rfile.read(length))
            if not isinstance(payload, dict):
                raise TypeError("event payload must be an object")
            self.server.state.browser_event(payload)
        except (TypeError, ValueError, json.JSONDecodeError) as exc:
            self.send_error(HTTPStatus.BAD_REQUEST, str(exc))
            return
        self.send_response(HTTPStatus.NO_CONTENT)
        self.end_headers()


def daemon_status() -> str:
    try:
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
                f"{DBUS_INTERFACE}.GetStatus",
            ],
            check=False,
            capture_output=True,
            text=True,
            timeout=2,
        )
    except (OSError, subprocess.TimeoutExpired):
        return "unavailable"
    if result.returncode != 0:
        return "unavailable"
    if "recording" in result.stdout:
        return "recording"
    if "idle" in result.stdout:
        return "idle"
    return "unknown"


def monitor_partials(state: ProbeState, stop: threading.Event) -> None:
    process = subprocess.Popen(
        [
            "gdbus",
            "monitor",
            "--session",
            "--dest",
            DBUS_DEST,
            "--object-path",
            DBUS_PATH,
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        bufsize=1,
    )
    try:
        assert process.stdout is not None
        while not stop.is_set():
            readable, _, _ = select.select([process.stdout], [], [], 0.2)
            if not readable:
                if process.poll() is not None:
                    return
                continue
            line = process.stdout.readline()
            if not line:
                return
            if "RecognitionPartial" not in line:
                continue
            match = PARTIAL_RE.search(line)
            state.mark_partial(match.group(1) if match else line.strip())
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=1)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=1)


def status_loop(state: ProbeState, stop: threading.Event) -> None:
    while not stop.wait(0.2):
        state.evaluate(daemon_status())
        if state.done.is_set():
            return


def terminate_process_group(process: subprocess.Popen[Any]) -> None:
    if process.poll() is not None:
        return
    try:
        os.killpg(process.pid, signal.SIGTERM)
        process.wait(timeout=3)
    except (ProcessLookupError, subprocess.TimeoutExpired):
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait(timeout=3)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=("normal", "command"), default="normal")
    parser.add_argument("--browser", required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    timeout = int(os.environ.get("VINPST_TOOLKIT_TIMEOUT_SECONDS", "120"))
    initial_text = os.environ.get("VINPST_TOOLKIT_INITIAL_TEXT", "selected text")
    expected = os.environ.get(
        "VINPST_TOOLKIT_EXPECTED_COMMIT_SUBSTRING",
        initial_text if args.mode == "command" else "",
    )
    require_partial = os.environ.get(
        "VINPST_TOOLKIT_REQUIRE_PARTIAL", "1"
    ).lower() not in {"0", "false", "no"}

    args.out_dir.mkdir(parents=True, exist_ok=True)
    log_path = args.out_dir / f"{args.mode}.jsonl"
    log_path.write_text("", encoding="utf-8")
    profile = args.out_dir / f"profile-{args.mode}"
    shutil.rmtree(profile, ignore_errors=True)
    html = Path(__file__).with_name("chromium-live-toolkit-probe.html").read_bytes()
    state = ProbeState(args.mode, initial_text, expected, require_partial, log_path)
    token = secrets.token_urlsafe(24)
    server = ProbeServer(("127.0.0.1", 0), state, token, html)
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()

    query = urllib.parse.urlencode(
        {"mode": args.mode, "initial_text": initial_text, "token": token}
    )
    url = f"http://127.0.0.1:{server.server_port}/probe?{query}"
    browser_log = (args.out_dir / f"{args.mode}-browser.log").open(
        "w", encoding="utf-8"
    )
    env = os.environ.copy()
    env.update(
        {"GTK_IM_MODULE": "fcitx", "QT_IM_MODULE": "fcitx", "XMODIFIERS": "@im=fcitx"}
    )
    browser = subprocess.Popen(
        [
            args.browser,
            f"--user-data-dir={profile}",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-background-networking",
            "--disable-component-update",
            "--disable-sync",
            "--metrics-recording-only",
            "--ozone-platform=wayland",
            "--enable-wayland-ime",
            f"--app={url}",
        ],
        stdout=browser_log,
        stderr=subprocess.STDOUT,
        env=env,
        start_new_session=True,
    )

    stop = threading.Event()
    partial_thread = threading.Thread(
        target=monitor_partials, args=(state, stop), daemon=True
    )
    poll_thread = threading.Thread(target=status_loop, args=(state, stop), daemon=True)
    partial_thread.start()
    poll_thread.start()
    try:
        deadline = time.monotonic() + timeout
        while not state.done.wait(0.2):
            if browser.poll() is not None:
                break
            if time.monotonic() >= deadline:
                with state.lock:
                    state.timed_out = True
                break
    finally:
        stop.set()
        terminate_process_group(browser)
        server.shutdown()
        server.server_close()
        browser_log.close()
        partial_thread.join(timeout=2)
        poll_thread.join(timeout=2)
        shutil.rmtree(profile, ignore_errors=True)

    summary = state.summary()
    state.emit(
        "summary", **{key: value for key, value in summary.items() if key != "event"}
    )
    return 0 if summary["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
