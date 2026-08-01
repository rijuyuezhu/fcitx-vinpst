#!/usr/bin/env python3
"""Drive the remote-text browser page through a real Chromium LAN connection."""

import argparse
import json
import os
import shutil
import signal
import subprocess
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

from probes.websocket_cdp import CdpClient, WebSocketClient


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--browser", type=Path, required=True)
    parser.add_argument("--endpoint", required=True)
    parser.add_argument("--output-url", required=True)
    parser.add_argument("--text", required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--debug-port", type=int, required=True)
    return parser.parse_args()


def fetch_json(url: str, timeout: float = 1.0) -> Any:
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with opener.open(url, timeout=timeout) as response:
        return json.load(response)


def wait_for_debug_page(
    debug_port: int, browser: subprocess.Popen[str]
) -> dict[str, Any]:
    deadline = time.monotonic() + 20.0
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        if browser.poll() is not None:
            raise RuntimeError(
                f"browser exited before DevTools became ready: {browser.returncode}"
            )
        try:
            pages = fetch_json(f"http://127.0.0.1:{debug_port}/json/list")
            for page in pages:
                if page.get("type") == "page":
                    return page
        except (OSError, TimeoutError, ValueError, urllib.error.URLError) as error:
            last_error = error
        time.sleep(0.1)
    raise RuntimeError(f"timed out waiting for browser page: {last_error}")


def wait_for_page_ready(cdp: CdpClient) -> dict[str, Any]:
    expression = """(() => ({
      title: document.title,
      input: document.getElementById('input')?.textContent || '',
      output: document.getElementById('output')?.textContent || '',
      disabled: document.getElementById('editor')?.disabled ?? true,
      url: location.href
    }))()"""
    deadline = time.monotonic() + 15.0
    state: dict[str, Any] = {}
    while time.monotonic() < deadline:
        value = cdp.evaluate(expression)
        if isinstance(value, dict):
            state = value
        if (
            state.get("title") == "VInput Remote"
            and state.get("input") == "input connected"
            and state.get("output") == "output connected"
            and state.get("disabled") is False
        ):
            return state
        time.sleep(0.1)
    raise RuntimeError(f"remote browser page did not become ready: {state}")


def browser_process_descendants(root_pid: int) -> list[int]:
    parents: dict[int, int] = {}
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            fields = (entry / "stat").read_text().split()
            parents[int(entry.name)] = int(fields[3])
        except (OSError, ValueError, IndexError):
            continue
    descendants: list[int] = []
    pending = [root_pid]
    while pending:
        parent = pending.pop()
        children = [pid for pid, ppid in parents.items() if ppid == parent]
        descendants.extend(children)
        pending.extend(children)
    return descendants


def process_cmdline(pid: int) -> str:
    try:
        return (
            Path(f"/proc/{pid}/cmdline")
            .read_bytes()
            .replace(b"\0", b" ")
            .decode()
            .strip()
        )
    except OSError:
        return ""


def renderer_sandbox(browser_pid: int) -> dict[str, Any]:
    deadline = time.monotonic() + 10.0
    renderer_pid = 0
    while time.monotonic() < deadline:
        for pid in browser_process_descendants(browser_pid):
            if "--type=renderer" in process_cmdline(pid):
                renderer_pid = pid
                break
        if renderer_pid:
            break
        time.sleep(0.1)
    if not renderer_pid:
        raise RuntimeError("no Chromium renderer process was found")
    status: dict[str, str] = {}
    for line in Path(f"/proc/{renderer_pid}/status").read_text().splitlines():
        key, separator, value = line.partition(":")
        if separator:
            status[key] = value.strip()
    return {
        "pid": renderer_pid,
        "no_new_privs": int(status.get("NoNewPrivs", "-1")),
        "seccomp": int(status.get("Seccomp", "-1")),
        "cap_eff": status.get("CapEff", ""),
        "nspid": status.get("NSpid", ""),
        "cmdline": process_cmdline(renderer_pid),
    }


def established_connections(port: int) -> list[str]:
    completed = subprocess.run(
        ["ss", "-Htn", "state", "established", f"( sport = :{port} )"],
        check=True,
        capture_output=True,
        text=True,
    )
    return [line.strip() for line in completed.stdout.splitlines() if line.strip()]


def terminate_group(process: subprocess.Popen[str]) -> None:
    if process.poll() is not None:
        return
    try:
        os.killpg(process.pid, signal.SIGTERM)
        process.wait(timeout=5)
    except (ProcessLookupError, subprocess.TimeoutExpired):
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait(timeout=5)


def main() -> int:
    args = parse_args()
    args.out_dir.mkdir(parents=True, exist_ok=True)
    user_data = args.out_dir / "chrome-profile"
    user_data.mkdir()
    browser_log_path = args.out_dir / "browser.log"
    browser_log = browser_log_path.open("w", encoding="utf-8")
    api_key = os.environ.get("VINPUT_REMOTE_TEXT_API_KEY", "")
    if not api_key:
        raise RuntimeError("VINPUT_REMOTE_TEXT_API_KEY is required")
    page_url = f"{args.endpoint}/#key={urllib.parse.quote(api_key)}"
    allow_origin = f"http://127.0.0.1:{args.debug_port}"
    command = [
        str(args.browser),
        "--headless=new",
        f"--user-data-dir={user_data}",
        "--remote-debugging-address=127.0.0.1",
        f"--remote-debugging-port={args.debug_port}",
        f"--remote-allow-origins={allow_origin}",
        "--no-first-run",
        "--no-default-browser-check",
        "--no-proxy-server",
        "--disable-background-networking",
        "--disable-component-update",
        "--disable-sync",
        "--window-size=1000,800",
        "about:blank",
    ]
    if any(flag in command for flag in ("--no-sandbox", "--disable-setuid-sandbox")):
        raise RuntimeError("browser sandbox disable flags are forbidden")

    output = WebSocketClient.connect(
        args.output_url,
        headers={"Authorization": f"Bearer {api_key}"},
    )
    output.send_json(
        {"type": "session.update", "session": {"input_audio_format": "pcm16"}}
    )
    session_updated = output.recv_json()
    browser = subprocess.Popen(
        command,
        stdout=browser_log,
        stderr=subprocess.STDOUT,
        text=True,
        start_new_session=True,
        env={
            key: value
            for key, value in os.environ.items()
            if not key.lower().endswith("_proxy")
        },
    )
    cdp: CdpClient | None = None
    try:
        page = wait_for_debug_page(args.debug_port, browser)
        cdp_socket = WebSocketClient.connect(
            page["webSocketDebuggerUrl"], headers={"Origin": allow_origin}
        )
        cdp = CdpClient(cdp_socket)
        cdp.call("Runtime.enable")
        cdp.call("Page.enable")
        cdp.call("Page.navigate", {"url": page_url})
        ready = wait_for_page_ready(cdp)
        connections = established_connections(
            urllib.parse.urlsplit(args.endpoint).port or 80
        )
        expression = f"""(() => {{
          const editor = document.getElementById('editor');
          editor.value = {json.dumps(args.text)};
          editor.dispatchEvent(new Event('input', {{bubbles: true}}));
          document.getElementById('send').click();
          return {{value: editor.value, count: document.getElementById('count').textContent}};
        }})()"""
        browser_action = cdp.evaluate(expression)
        events = [output.recv_json() for _ in range(3)]
        event_types = [event.get("type") for event in events]
        expected_types = [
            "input_audio_buffer.committed",
            "conversation.item.input_audio_transcription.delta",
            "conversation.item.input_audio_transcription.completed",
        ]
        if event_types != expected_types:
            raise RuntimeError(
                f"unexpected remote output event sequence: {event_types}"
            )
        if (
            events[1].get("delta") != args.text
            or events[2].get("transcript") != args.text
        ):
            raise RuntimeError(f"unexpected remote output text: {events}")
        if len({event.get("item_id") for event in events}) != 1:
            raise RuntimeError(f"remote output item ids do not match: {events}")
        sandbox = renderer_sandbox(browser.pid)
        browser_cmdline = process_cmdline(browser.pid)
        if (
            "--no-sandbox" in browser_cmdline
            or "--disable-setuid-sandbox" in browser_cmdline
        ):
            raise RuntimeError("running browser disabled its sandbox")
        if sandbox["no_new_privs"] != 1 or sandbox["seccomp"] != 2:
            raise RuntimeError(f"renderer sandbox is incomplete: {sandbox}")
        if sandbox["cap_eff"] != "0000000000000000":
            raise RuntimeError(f"renderer has effective capabilities: {sandbox}")
        if len(sandbox["nspid"].split()) < 2:
            raise RuntimeError(
                f"renderer lacks nested PID namespace evidence: {sandbox}"
            )
        lan_host = urllib.parse.urlsplit(args.endpoint).hostname or ""
        lan_connection = any(
            lan_host in line and "127.0.0.1" not in line for line in connections
        )
        loopback_connection = any("127.0.0.1" in line for line in connections)
        if not lan_connection or not loopback_connection:
            raise RuntimeError(
                f"expected LAN browser and loopback output sockets: {connections}"
            )
        summary = {
            "event": "summary",
            "browser_executable": str(args.browser.resolve()),
            "browser_version": subprocess.check_output(
                [str(args.browser), "--version"], text=True
            ).strip(),
            "browser_pid": browser.pid,
            "browser_cmdline": browser_cmdline,
            "page_target_url": ready.get("url", ""),
            "page_ready": ready,
            "browser_action": browser_action,
            "endpoint": args.endpoint,
            "output_url": args.output_url,
            "session_updated": session_updated,
            "connections": connections,
            "lan_browser_connection": True,
            "loopback_output_connection": True,
            "events": events,
            "renderer": sandbox,
            "api_key_recorded": False,
            "same_host_lan_proof": True,
            "cross_device_proof": False,
        }
        (args.out_dir / "summary.json").write_text(
            json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )
        print(json.dumps(summary, ensure_ascii=False))
    finally:
        if cdp is not None:
            cdp.close()
        output.close()
        terminate_group(browser)
        browser_log.close()
        shutil.rmtree(user_data, ignore_errors=True)
        if browser_log_path.exists():
            log_text = browser_log_path.read_text(encoding="utf-8", errors="replace")
            browser_log_path.write_text(
                log_text.replace(api_key, "[REDACTED]"), encoding="utf-8"
            )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
