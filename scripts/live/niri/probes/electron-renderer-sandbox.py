#!/usr/bin/env python3
"""Attest one isolated Electron window renderer and its sandbox state."""

import argparse
import json
import os
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--application", required=True)
    parser.add_argument("--user-data-dir", type=Path, required=True)
    parser.add_argument("--window-pid", type=int, required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def process_cmdline(pid: int) -> str:
    return (
        Path(f"/proc/{pid}/cmdline")
        .read_bytes()
        .replace(b"\0", b" ")
        .decode(errors="replace")
        .strip()
    )


def matching_processes(user_data_dir: Path) -> dict[int, str]:
    needle = str(user_data_dir.resolve())
    matches: dict[int, str] = {}
    for cmdline_path in Path("/proc").glob("[0-9]*/cmdline"):
        try:
            cmdline = cmdline_path.read_bytes().replace(b"\0", b" ").decode()
        except (OSError, UnicodeDecodeError):
            continue
        if needle not in cmdline:
            continue
        matches[int(cmdline_path.parent.name)] = cmdline.strip()
    return matches


def status_fields(pid: int) -> dict[str, str]:
    fields: dict[str, str] = {}
    for line in Path(f"/proc/{pid}/status").read_text().splitlines():
        name, separator, value = line.partition(":")
        if separator:
            fields[name] = value.strip()
    return fields


def choose_renderer(processes: dict[int, str]) -> tuple[int, str]:
    renderers = [
        (pid, cmdline)
        for pid, cmdline in processes.items()
        if "--type=renderer" in cmdline and "--extension-process" not in cmdline
    ]
    window_renderers = [
        candidate
        for candidate in renderers
        if "--vscode-window-config=" in candidate[1]
    ]
    candidates = window_renderers or renderers
    if len(candidates) != 1:
        raise RuntimeError(
            f"expected exactly one isolated Electron window renderer, found {candidates}"
        )
    return candidates[0]


def main() -> int:
    args = parse_args()
    processes = matching_processes(args.user_data_dir)
    if args.window_pid not in processes:
        raise RuntimeError(
            "niri window PID is not part of the isolated Electron instance"
        )
    if not processes:
        raise RuntimeError("isolated Electron instance has no matching processes")
    for pid, cmdline in processes.items():
        if any(
            flag in cmdline
            for flag in (
                "--no-sandbox",
                "--disable-setuid-sandbox",
                "--disable-chromium-sandbox",
            )
        ):
            raise RuntimeError(f"Electron process {pid} disabled its sandbox")

    renderer_pid, renderer_cmdline = choose_renderer(processes)
    status = status_fields(renderer_pid)
    no_new_privs = int(status.get("NoNewPrivs", "-1"))
    seccomp = int(status.get("Seccomp", "-1"))
    cap_eff = status.get("CapEff", "")
    nspid = status.get("NSpid", "")
    nspid_depth = len(nspid.split())
    if (
        no_new_privs != 1
        or seccomp != 2
        or cap_eff != "0000000000000000"
        or nspid_depth < 2
    ):
        raise RuntimeError(
            "Electron renderer sandbox status is incomplete: "
            f"NoNewPrivs={no_new_privs} Seccomp={seccomp} "
            f"CapEff={cap_eff} NSpid={nspid}"
        )

    window_executable = os.path.realpath(f"/proc/{args.window_pid}/exe")
    renderer_executable = os.path.realpath(f"/proc/{renderer_pid}/exe")
    if window_executable != renderer_executable:
        raise RuntimeError(
            "Electron window and renderer executables differ: "
            f"{window_executable} != {renderer_executable}"
        )

    summary: dict[str, Any] = {
        "event": "renderer-sandbox",
        "application": args.application,
        "window_pid": args.window_pid,
        "renderer_pid": renderer_pid,
        "executable": window_executable,
        "process_count": len(processes),
        "process_ids": sorted(processes),
        "renderer_cmdline": renderer_cmdline,
        "no_sandbox_flag": False,
        "no_new_privs": no_new_privs,
        "seccomp": seccomp,
        "cap_eff": cap_eff,
        "nspid": nspid,
        "nspid_depth": nspid_depth,
        "ok": True,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
