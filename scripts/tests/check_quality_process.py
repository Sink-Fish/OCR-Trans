"""Run the shipped worker with the release model, including a second request in the same process."""
import argparse
import json
from pathlib import Path
import queue
import subprocess
import threading
import time


def smoke(executable, model, cpu):
    process = subprocess.Popen([str(executable), "--quality-worker"], stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, encoding="utf-8", errors="strict")
    output = queue.Queue()
    def reader():
        try:
            for line in process.stdout:
                output.put(json.loads(line))
        finally:
            output.put(None)
    threading.Thread(target=reader, daemon=True).start()
    try:
        for source in ("Open the file", "Save the document"):
            deadline = time.monotonic() + 240
            request = dict(model=str(model), cpu=cpu, paragraphs=[["en", source]], glossary=[])
            process.stdin.write(json.dumps(request) + "\n")
            process.stdin.flush()
            while True:
                message = output.get(timeout=max(0, deadline - time.monotonic()))
                if message is None:
                    raise RuntimeError(f"Worker exited during translation: {process.poll()}")
                if "Finished" in message:
                    reply = message["Finished"]
                    if "Err" in reply:
                        raise RuntimeError(reply["Err"])
                    result = reply["Ok"]
                    translations = result["translations"]
                    if len(translations) != 1 or not translations[0] or not any("\u4e00" <= c <= "\u9fff" for c in translations[0]):
                        raise AssertionError(result)
                    if cpu and not result["device"].startswith("CPU"):
                        raise AssertionError(result)
                    print(result["device"], translations[0], flush=True)
                    break
        process.stdin.close()
        if process.wait(timeout=10) != 0:
            raise RuntimeError("worker failed on shutdown")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("executable", type=Path)
    parser.add_argument("model", type=Path)
    parser.add_argument("--auto", action="store_true")
    args = parser.parse_args()
    smoke(args.executable.resolve(), args.model.resolve(), not args.auto)
