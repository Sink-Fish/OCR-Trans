"""Exercise release failure paths with external commands replaced by local stubs."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class MacPackaging(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="ocrtrans-release-test-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        for directory in ("scripts", "bin", "runtime", "models/ocr", "models/opus-mt-en-zh",
                          "src-tauri", "release", "target/release/bundle/macos/OCR Trans.app"):
            (self.root / directory).mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / "scripts/package_mac.sh", self.root / "scripts/package_mac.sh")
        runtime_stub = self.root / "scripts/fetch_runtime.sh"
        runtime_stub.write_text("#!/bin/sh\nexit 0\n")
        runtime_stub.chmod(0o755)
        for file in ("runtime/libonnxruntime.dylib", "models/ocr/rec.onnx", "models/opus-mt-en-zh/model.bin"):
            (self.root / file).write_text("fixture")
        (self.root / "src-tauri/tauri.conf.json").write_text('{"version":"9.9.9"}')
        self.stub("pnpm", "exit 0")
        self.stub("uname", 'echo arm64')
        self.stub("node", 'echo 9.9.9')
        self.stub("ditto", 'cp -R "$1" "$2"')
        self.stub("hdiutil", '''
if [ "$1" = create ]; then
  for argument in "$@"; do output="$argument"; done
  printf 'new image' > "$output"
  [ "${FAIL_CREATE:-0}" != 1 ]
elif [ "$1" = verify ]; then
  [ "${FAIL_VERIFY:-0}" != 1 ]
fi
''')
        self.stub("../scripts/sign_mac.sh", "exit 0")
        self.stub("../scripts/install_mac.sh", 'touch installed')
        self.release = self.root / "release/OCR-Trans_9.9.9_arm64.dmg"
        self.release.write_text("last good release")

    def stub(self, name, body):
        path = self.root / "bin" / name
        path.write_text("#!/bin/bash\nset -eu\n" + body + "\n")
        path.chmod(0o755)

    def run_package(self, *args, **environment):
        env = dict(os.environ, PATH=f"{self.root / 'bin'}:{os.environ['PATH']}", **environment)
        return subprocess.run(["bash", str(self.root / "scripts/package_mac.sh"), *args],
                              env=env, capture_output=True, text=True)

    def test_creation_failure_preserves_previous_release(self):
        result = self.run_package("--no-install", FAIL_CREATE="1")
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.release.read_text(), "last good release")
        self.assertEqual(list((self.root / "release").iterdir()), [self.release])

    def test_verification_failure_preserves_previous_release(self):
        result = self.run_package("--no-install", FAIL_VERIFY="1")
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.release.read_text(), "last good release")
        self.assertEqual(list((self.root / "release").iterdir()), [self.release])

    def test_success_publishes_verified_image_without_installing(self):
        result = self.run_package("--no-install")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.release.read_text(), "new image")
        self.assertFalse((self.root / "installed").exists())

    def test_misspelled_option_does_not_install(self):
        result = self.run_package("--no-instal")
        self.assertEqual(result.returncode, 2)
        self.assertEqual(self.release.read_text(), "last good release")
        self.assertFalse((self.root / "installed").exists())


if __name__ == "__main__":
    unittest.main()
