#!/usr/bin/env python3
"""Bridge vinpst command-ASR JSON requests to WAV-file based ASR CLIs.

The helper reads a vinpst CommandAsrRequest JSON document from stdin, writes its
PCM samples to a temporary WAV file, runs a user-provided command, and emits the
trimmed command stdout as a vinpst CommandAsrResponse JSON document.

Example:
  command-asr-wav-helper.py -- whisper-cli -m model.bin -f "$VINPST_ASR_WAV"
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
import wave
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--timeout-ms",
        type=int,
        default=None,
        help="override command timeout in milliseconds; defaults to request.timeout_ms or no timeout",
    )
    parser.add_argument(
        "command",
        nargs=argparse.REMAINDER,
        help="external ASR command to run; prefix with -- before the command",
    )
    args = parser.parse_args()
    if args.command and args.command[0] == "--":
        args.command = args.command[1:]
    if not args.command:
        parser.error("missing external ASR command after --")
    return args


def load_request() -> dict[str, Any]:
    try:
        request = json.load(sys.stdin)
    except json.JSONDecodeError as error:
        raise ValueError(f"invalid command ASR request JSON: {error}") from error
    if not isinstance(request, dict):
        raise TypeError("command ASR request must be a JSON object")
    return request


def pcm_spec(request: dict[str, Any]) -> tuple[int, int]:
    pcm = request.get("pcm") if isinstance(request.get("pcm"), dict) else {}
    sample_rate = int(pcm.get("sample_rate_hz") or 16_000)
    channels = int(pcm.get("channels") or 1)
    if sample_rate <= 0:
        raise ValueError("pcm.sample_rate_hz must be positive")
    if channels <= 0:
        raise ValueError("pcm.channels must be positive")
    return sample_rate, channels


def pcm_samples(request: dict[str, Any]) -> list[int]:
    samples = request.get("samples")
    if not isinstance(samples, list):
        raise TypeError("samples must be an array of signed 16-bit integers")
    result: list[int] = []
    for index, sample in enumerate(samples):
        value = int(sample)
        if value < -32768 or value > 32767:
            raise ValueError(f"samples[{index}] is outside the signed 16-bit range")
        result.append(value)
    return result


def write_wav(path: Path, sample_rate: int, channels: int, samples: list[int]) -> None:
    with wave.open(str(path), "wb") as handle:
        handle.setnchannels(channels)
        handle.setsampwidth(2)
        handle.setframerate(sample_rate)
        frames = bytearray()
        for sample in samples:
            frames.extend(int(sample).to_bytes(2, byteorder="little", signed=True))
        handle.writeframes(bytes(frames))


def command_env(
    request: dict[str, Any], wav_path: Path, sample_rate: int, channels: int
) -> dict[str, str]:
    env = os.environ.copy()
    env.update(
        {
            "VINPST_ASR_WAV": str(wav_path),
            "VINPST_ASR_PROVIDER_ID": str(request.get("provider_id") or ""),
            "VINPST_ASR_MODEL_ID": str(request.get("model_id") or ""),
            "VINPST_ASR_HOTWORDS_FILE": str(request.get("hotwords_file") or ""),
            "VINPST_ASR_SAMPLE_RATE_HZ": str(sample_rate),
            "VINPST_ASR_CHANNELS": str(channels),
        }
    )
    return env


def response(payload: dict[str, str]) -> int:
    print(json.dumps(payload, ensure_ascii=False))
    return 0


def main() -> int:
    args = parse_args()
    try:
        request = load_request()
        sample_rate, channels = pcm_spec(request)
        samples = pcm_samples(request)
        timeout_ms = (
            args.timeout_ms
            if args.timeout_ms is not None
            else request.get("timeout_ms")
        )
        timeout_s = (
            None if timeout_ms in (None, "") else max(int(timeout_ms), 1) / 1000.0
        )
        with tempfile.TemporaryDirectory(prefix="vinpst-command-asr-") as temp_dir:
            wav_path = Path(temp_dir) / "request.wav"
            write_wav(wav_path, sample_rate, channels, samples)
            completed = subprocess.run(
                args.command,
                check=False,
                capture_output=True,
                text=True,
                env=command_env(request, wav_path, sample_rate, channels),
                timeout=timeout_s,
            )
        stdout = completed.stdout.strip()
        stderr = completed.stderr.strip()
        if completed.returncode != 0:
            message = (
                stderr
                or stdout
                or f"external ASR command exited with {completed.returncode}"
            )
            return response({"error": message})
        if not stdout:
            return response({"error": "external ASR command produced no text"})
        return response({"text": stdout})
    except subprocess.TimeoutExpired as error:
        return response(
            {"error": f"external ASR command timed out after {error.timeout:.3g}s"}
        )
    except (OSError, TypeError, ValueError, subprocess.SubprocessError) as error:
        # Keep expected helper and child-process failures inside the command-ASR protocol.
        return response({"error": str(error)})


if __name__ == "__main__":
    raise SystemExit(main())
