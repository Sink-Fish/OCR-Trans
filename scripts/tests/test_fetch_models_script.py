"""Exercise interrupted model replacement with offline fixtures and local command stubs."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ModelConversionRecovery(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="ocrtrans model recovery ")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        for directory in ("scripts", "bin", "models/ocr", "models/opus-mt-en-zh", "runtime"):
            (self.root / directory).mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / "scripts/fetch_models.sh", self.root / "scripts/fetch_models.sh")
        shutil.copy2(ROOT / "scripts/fetch_runtime.sh", self.root / "scripts/fetch_runtime.sh")
        for name in ("det.onnx", "rec.onnx", "dict.txt"):
            (self.root / "models/ocr" / name).write_text("fixture")
        (self.root / "runtime/libonnxruntime.dylib").write_text("fixture")
        self.old_model = self.root / "models/opus-mt-en-zh/model.bin"
        self.old_model.write_text("previous model")
        self.stub("uname", 'print("Darwin" if sys.argv[1] == "-s" else "arm64")')
        self.stub("curl", 'raise RuntimeError("unexpected network request")')
        self.stub("shasum", '''
hashes = {
    "libonnxruntime.dylib": "83973572676032b4ba9418afed0b51f45599d35bb4e4628ca8ae1954c8362e31",
    "det.onnx": "4d97c44a20d30a81aad087d6a396b08f786c4635742afc391f6621f5c6ae78ae",
    "rec.onnx": "5825fc7ebf84ae7a412be049820b4d86d77620f204a041697b0494669b1742c5",
    "dict.txt": "d1979e9f794c464c0d2e0b70a7fe14dd978e9dc644c0e71f14158cdf8342af1b",
}
print(hashes[Path(sys.argv[-1]).name] + "  " + sys.argv[-1])
''')
        self.stub("uv", '''
if "ct2-transformers-converter" not in sys.argv:
    print("/offline/snapshot")
else:
    output = Path(sys.argv[sys.argv.index("--output_dir") + 1])
    output.mkdir()
    (output / "model.bin").write_text("new model")
    if os.environ.get("FAIL_CONVERT"):
        sys.exit(29)
''')
        self.stub("mv", '''
source, dest = map(Path, sys.argv[-2:])
os.rename(source, dest)
if os.environ.get("TERM_AFTER_BACKUP") and dest.name == "previous":
    import signal
    os.kill(os.getppid(), signal.SIGTERM)
''')

    def stub(self, name, body):
        path = self.root / "bin" / name
        path.write_text(f"#!{sys.executable}\nimport os, sys\nfrom pathlib import Path\n" + body)
        path.chmod(0o755)

    def run_fetch(self, **environment):
        env = dict(os.environ, PATH=f"{self.root / 'bin'}:{os.environ['PATH']}", **environment)
        env.pop("MODELS_DIR", None)
        return subprocess.run(["/bin/bash", str(self.root / "scripts/fetch_models.sh")],
                              env=env, capture_output=True, text=True, timeout=15)

    def assert_previous_model_and_no_staging(self):
        self.assertEqual(self.old_model.read_text(), "previous model")
        self.assertEqual(list((self.root / "models").glob(".convert.*")), [])

    def test_termination_after_moving_old_model_restores_it(self):
        result = self.run_fetch(TERM_AFTER_BACKUP="1")
        self.assertEqual(result.returncode, 143, result.stderr)
        self.assert_previous_model_and_no_staging()

    def test_converter_failure_keeps_old_model_and_removes_staging(self):
        result = self.run_fetch(FAIL_CONVERT="1")
        self.assertNotEqual(result.returncode, 0)
        self.assert_previous_model_and_no_staging()

    def test_success_replaces_old_model_and_removes_staging(self):
        result = self.run_fetch()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.old_model.read_text(), "new model")
        self.assertEqual(list((self.root / "models").glob(".convert.*")), [])


if __name__ == "__main__":
    unittest.main()
