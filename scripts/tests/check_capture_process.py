"""Verify a real desktop capture and discard the pixels, keeping only dimensions and elapsed time."""
import argparse
import json
import struct
import subprocess
import time
from pathlib import Path


def check(executable):
    started = time.monotonic()
    process = subprocess.run([str(executable.resolve()), "--capture-worker", "NaN", "NaN"],
                             stdout=subprocess.PIPE, check=True, timeout=10)
    data = process.stdout
    if len(data) < 4:
        raise ValueError("Missing capture response")
    size, = struct.unpack_from("<I", data)
    if size > 8192 or size + 4 > len(data):
        raise ValueError("Invalid capture header")
    header = json.loads(data[4:size + 4])
    if "error" in header:
        raise RuntimeError(header["error"])
    width, height = header["width"], header["height"]
    if min(width, height) < 1 or len(data) != 4 + size + width * height * 4:
        raise ValueError("Incomplete capture")
    return {"width": width, "height": height, "elapsed_ms": round((time.monotonic() - started) * 1000)}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("executable", type=Path)
    print(json.dumps(check(parser.parse_args().executable)))
