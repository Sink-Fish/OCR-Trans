"""Verify OCR, translation and canvas rendering through the packaged desktop application."""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile


def run_desktop(executable, profile, test_env):
    bundle = executable.parent.parent.parent
    if sys.platform != "darwin" or bundle.suffix != ".app" or executable.parent != bundle / "Contents/MacOS":
        subprocess.run([str(executable)], cwd=profile, env={**os.environ, **test_env},
                       check=True, timeout=300, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return

    # Launch Services starts the bundle as its own app. Executing Contents/MacOS directly
    # lets Tahoe attribute the installed user's menu item to the test runner.
    output = profile / "stdout"
    output.touch()
    args = ["/usr/bin/open", "-n", "-W", str(bundle), "--stdout", str(output)]
    for key, value in test_env.items():
        args.extend(["--env", f"{key}={value}"])
    try:
        subprocess.run(args, cwd=profile, check=True, timeout=300)
    except (subprocess.TimeoutExpired, KeyboardInterrupt):
        # open -W is a waiter, not the app's parent. Only stop this run's processes
        # holding its unique stdout file, never the installed user's app.
        holders = subprocess.run(["/usr/sbin/lsof", "-t", str(output)], capture_output=True, text=True)
        for pid in holders.stdout.split():
            command = subprocess.run(["ps", "-p", pid, "-o", "comm="], capture_output=True, text=True)
            if command.stdout.strip() == str(executable):
                try:
                    os.kill(int(pid), signal.SIGKILL)
                except ProcessLookupError:
                    pass
        raise


def verify_desktop(executable, screenshot, models, output):
    output.mkdir(parents=True, exist_ok=True)
    for name, engine, device, expected in [
        ("fast", "fast", "auto", "nmt"), ("quality-cpu", "quality", "cpu", "llm"),
        ("quality-auto", "quality", "auto", "llm")
    ]:
        with tempfile.TemporaryDirectory(prefix="ocr-trans-release-") as tmp:
            profile = Path(tmp)
            settings = dict(version=2, onboarded=True, hotkey_capture="", keep_history=False,
                            engine=engine, llm_device=device, models_dir=str(models))
            (profile / "settings.json").write_text(json.dumps(settings), encoding="utf-8")
            env = dict(OCRTRANS_TEST_PROFILE=tmp, OCRTRANS_DATA_DIR=tmp,
                       OCRTRANS_TEST_PIN=str(screenshot) + ":translate", OCRTRANS_ENGINE=engine)
            try:
                run_desktop(executable, profile, env)
            finally:
                path = profile / "ocr-trans.log"
                log = path.read_text(encoding="utf-8") if path.exists() else ""
                (output / f"{name}.log").write_text(log, encoding="utf-8")
            if f"release-check displayed engine={expected} warnings=0" not in log:
                raise AssertionError(f"{name} failed rendering; see {output}/{name}.log")
            if "exit: models released" not in log:
                raise AssertionError(f"{name} did not exit cleanly; see {output}/{name}.log")
            if device == "cpu" and "llm: model on CPU" not in log:
                raise AssertionError("CPU selection was not honored")
            print(f"{name}: render and exit PASS", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for arg in ("executable", "screenshot", "models", "output"):
        parser.add_argument(arg, type=Path)
    args = parser.parse_args()
    verify_desktop(*(v.resolve() for v in vars(args).values()))
