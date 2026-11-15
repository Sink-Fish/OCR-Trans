"""Exercise signing search-list restoration without accessing any real keychain."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class SigningSearchList(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="ocrtrans signing test ")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.app = self.root / "OCR Trans.app"
        for directory in ("scripts", "bin", ".signing", "OCR Trans.app/Contents/Frameworks"):
            (self.root / directory).mkdir(parents=True, exist_ok=True)
        (self.app / "Contents/lib runtime.dylib").write_text("fixture")
        (self.root / ".signing/keychain.pass").write_text("fixture-only-password")
        (self.root / "scripts/setup_signing.sh").write_text("#!/bin/bash\nexit 0\n")
        (self.root / "scripts/setup_signing.sh").chmod(0o755)
        shutil.copy2(ROOT / "scripts/sign_mac.sh", self.root / "scripts/sign_mac.sh")
        self.log = self.root / "commands.jsonl"
        self.stub("security", '''
args = sys.argv[1:]
with open(os.environ["MOCK_LOG"], "a") as log:
    log.write(json.dumps(["security", *args]) + "\\n")
if args[0] == "find-identity":
    print('1) ABCDEF0123456789 "OCR Trans Local Signing"')
elif args[0] == "list-keychains" and "-s" not in args:
    for path in json.loads(os.environ["MOCK_KEYCHAINS"]):
        print('    "' + path + '"')
    if os.environ.get("FAIL_LIST"):
        sys.exit(23)
''')
        self.stub("codesign", '''
args = sys.argv[1:]
with open(os.environ["MOCK_LOG"], "a") as log:
    log.write(json.dumps(["codesign", *args]) + "\\n")
if os.environ.get("FAIL_SIGN") and "--force" in args:
    sys.exit(24)
if "-dr" in args:
    print('designated => identifier "com.sinkfish.ocrtrans" and certificate leaf = H"MOCK"')
''')

    def stub(self, name, body):
        path = self.root / "bin" / name
        path.write_text(f"#!{sys.executable}\nimport json, os, sys\n" + body)
        path.chmod(0o755)

    def run_sign(self, keychains, **environment):
        env = dict(os.environ, PATH=f"{self.root / 'bin'}:{os.environ['PATH']}",
                   MOCK_LOG=str(self.log), MOCK_KEYCHAINS=json.dumps(keychains), **environment)
        # /bin/bash is the system Bash 3.2 on macOS, not the optional Homebrew Bash.
        result = subprocess.run(["/bin/bash", str(self.root / "scripts/sign_mac.sh"), str(self.app)],
                                env=env, capture_output=True, text=True, timeout=15)
        commands = [json.loads(line) for line in self.log.read_text().splitlines()]
        changes = [command[5:] for command in commands
                   if command[:5] == ["security", "list-keychains", "-d", "user", "-s"]]
        return result, commands, changes

    def test_paths_with_spaces_are_restored_as_separate_exact_arguments(self):
        original = ["/Users/mock/Library/Keychains/Login Session.keychain-db",
                    "/Users/mock/共享钥匙串/Team Identity.keychain-db"]
        result, _, changes = self.run_sign(original)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(changes, [original + [str(self.root / ".signing/ocrtrans-signing.keychain-db")], original])

    def test_empty_search_list_works_with_system_bash_and_is_restored_empty(self):
        result, _, changes = self.run_sign([])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(changes, [[str(self.root / ".signing/ocrtrans-signing.keychain-db")], []])

    def test_failed_list_query_never_changes_the_search_list(self):
        result, commands, changes = self.run_sign(["/partial/result with spaces.keychain-db"], FAIL_LIST="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(changes, [])
        self.assertFalse(any(command[0] == "codesign" for command in commands))

    def test_nested_signing_failure_still_restores_the_original_list(self):
        original = ["/Users/mock/Library/Keychains/Login Session.keychain-db"]
        result, _, changes = self.run_sign(original, FAIL_SIGN="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(changes[-1], original)
        self.assertEqual(len(changes), 2)


if __name__ == "__main__":
    unittest.main()
