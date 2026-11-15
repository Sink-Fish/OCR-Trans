"""Verify OCR, translation and canvas rendering through the packaged desktop application."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


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
            env = dict(os.environ, OCRTRANS_TEST_PROFILE=tmp, OCRTRANS_DATA_DIR=tmp,
                       OCRTRANS_TEST_PIN=str(screenshot) + ":translate", OCRTRANS_ENGINE=engine)
            try:
                subprocess.run([str(executable)], cwd=tmp, env=env, check=True, timeout=300,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            finally:
                path = profile / "ocr-trans.log"
                log = path.read_text(encoding="utf-8") if path.exists() else ""
                (output / f"{name}.log").write_text(log, encoding="utf-8")
            if f"release-check displayed engine={expected} warnings=0" not in log:
                raise AssertionError(f"{name} failed rendering; see {output}/{name}.log")
            if device == "cpu" and "llm: model on CPU" not in log:
                raise AssertionError("CPU selection was not honored")
            print(f"{name}: render and exit PASS", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for arg in ("executable", "screenshot", "models", "output"):
        parser.add_argument(arg, type=Path)
    args = parser.parse_args()
    verify_desktop(*(v.resolve() for v in vars(args).values()))
