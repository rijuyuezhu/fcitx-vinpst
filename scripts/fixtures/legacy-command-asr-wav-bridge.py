#!/usr/bin/env python3
"""Bridge legacy raw-PCM command ASR providers to WAV-file based commands.

The legacy command provider writes signed 16-bit little-endian PCM bytes to
stdin and treats trimmed stdout as the final recognized text. This bridge wraps
those bytes in a temporary WAV file, exposes its path through VINPST_ASR_WAV,
and forwards one downstream command's non-empty stdout.
"""

import argparse
import os
import subprocess
import sys
import tempfile
import wave
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sample-rate", type=int, default=16_000)
    parser.add_argument("--channels", type=int, default=1)
    parser.add_argument("--timeout-ms", type=int)
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
    if args.sample_rate <= 0:
        parser.error("--sample-rate must be positive")
    if args.channels <= 0:
        parser.error("--channels must be positive")
    if args.timeout_ms is not None and args.timeout_ms <= 0:
        parser.error("--timeout-ms must be positive")
    return args


def read_pcm() -> bytes:
    pcm = sys.stdin.buffer.read()
    if not pcm:
        raise ValueError("legacy command ASR PCM input is empty")
    if len(pcm) % 2 != 0:
        raise ValueError("legacy command ASR PCM byte length must be even")
    return pcm


def write_wav(path: Path, sample_rate: int, channels: int, pcm: bytes) -> int:
    frame_width = channels * 2
    if len(pcm) % frame_width != 0:
        raise ValueError(
            "legacy command ASR PCM byte length is not aligned to the channel count"
        )
    with wave.open(str(path), "wb") as handle:
        handle.setnchannels(channels)
        handle.setsampwidth(2)
        handle.setframerate(sample_rate)
        handle.writeframes(pcm)
    return len(pcm) // frame_width


def command_env(
    wav_path: Path, sample_rate: int, channels: int, frames: int
) -> dict[str, str]:
    env = os.environ.copy()
    env.update(
        {
            "VINPST_ASR_WAV": str(wav_path),
            "VINPST_ASR_SAMPLE_RATE_HZ": str(sample_rate),
            "VINPST_ASR_CHANNELS": str(channels),
            "VINPST_ASR_FRAMES": str(frames),
        }
    )
    return env


def main() -> int:
    args = parse_args()
    try:
        pcm = read_pcm()
        timeout_s = None if args.timeout_ms is None else args.timeout_ms / 1000.0
        with tempfile.TemporaryDirectory(
            prefix="vinpst-legacy-command-asr-"
        ) as temp_dir:
            wav_path = Path(temp_dir) / "request.wav"
            frames = write_wav(wav_path, args.sample_rate, args.channels, pcm)
            completed = subprocess.run(
                args.command,
                check=False,
                capture_output=True,
                text=True,
                env=command_env(wav_path, args.sample_rate, args.channels, frames),
                timeout=timeout_s,
            )
        stdout = completed.stdout.strip()
        stderr = completed.stderr.strip()
        if completed.returncode != 0:
            detail = (
                stderr
                or stdout
                or f"external ASR command exited with {completed.returncode}"
            )
            raise RuntimeError(detail)
        if not stdout:
            raise RuntimeError("external ASR command produced no text")
        print(stdout)
        return 0
    except subprocess.TimeoutExpired as error:
        print(
            f"external ASR command timed out after {error.timeout:.3g}s",
            file=sys.stderr,
        )
    except (OSError, RuntimeError, ValueError) as error:
        print(str(error), file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
