import argparse
import struct
import wave
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument("--sample-rate", type=int, default=16000)
    args = parser.parse_args()
    sample_pattern = [1000, -1000, 2000, -2000, 3000, -3000, 4000, -4000]
    samples = sample_pattern * 1000
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(args.output), "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(args.sample_rate)
        for sample in samples:
            wav.writeframesraw(struct.pack("<h", sample))


if __name__ == "__main__":
    main()
