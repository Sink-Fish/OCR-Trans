"""Windows DLL CRT release gates; no Windows execution is claimed by these host tests."""
import contextlib
import importlib.util
import io
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("windows_crt", ROOT / "scripts/dev/windows_crt.py")
crt = importlib.util.module_from_spec(spec)
spec.loader.exec_module(crt)


class WindowsCrt(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="ocrtrans windows crt ")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.out = self.root / "llama-cpp-sys-2-fixture/out"
        self.cache = self.out / "build/CMakeCache.txt"
        self.cache.parent.mkdir(parents=True)
        self.write_cache()
        (self.out / "bin").mkdir()
        (self.out / "backends").mkdir()
        for name in ("llama.dll", "ggml.dll", "ggml-base.dll", "ggml-vulkan.dll", "ggml-cpu-baseline.dll"):
            (self.out / "bin" / name).write_text("Name: KERNEL32.dll\nName: VCRUNTIME140.dll\nName: api-ms-win-crt-stdio-l1-1-0.dll\n")
        self.readobj = self.root / "fake-readobj"
        self.readobj.write_text('#!/bin/bash\n[ "$1" = --coff-imports ] || exit 2\ncat "$2"\n')
        self.readobj.chmod(0o755)

    def write_cache(self, c="-MD", cxx="/MD", release=""):
        self.cache.write_text(f"BUILD_SHARED_LIBS:BOOL=ON\nCMAKE_C_FLAGS:STRING= /O2 {c}\nCMAKE_CXX_FLAGS:STRING={cxx}\nCMAKE_CXX_FLAGS_RELEASE:STRING={release}\nCMAKE_MSVC_RUNTIME_LIBRARY:STRING=\n")

    def verify(self):
        with contextlib.redirect_stdout(io.StringIO()):
            crt.verify(self.out, str(self.readobj))

    def test_prepare_removes_static_cache_and_preserves_dynamic_cache(self):
        crt.prepare(self.root)
        self.assertTrue(self.cache.exists())
        self.write_cache(c="-MT")
        with contextlib.redirect_stdout(io.StringIO()):
            crt.prepare(self.root)
        self.assertFalse(self.out.parent.exists())

    def test_prepare_removes_unknown_or_mixed_release_cache(self):
        for c, release in (("", ""), ("-MD", "-MT"), ("/MDd", "")):
            with self.subTest(c=c, release=release):
                self.write_cache(c=c, release=release)
                self.assertFalse(crt.shared_release_cache(self.out))

    def test_all_shared_crt_dlls_pass_in_paths_with_spaces(self):
        self.verify()

    def test_bundle_only_rejects_static_cache_without_deleting_it(self):
        self.write_cache(c="-MT", cxx="-MT")
        with self.assertRaisesRegex(ValueError, "without --bundle-only"):
            self.verify()
        self.assertTrue(self.cache.exists())

    def test_every_backend_must_import_shared_release_crt(self):
        backend = self.out / "bin/ggml-cpu-baseline.dll"
        for imports in ("Name: KERNEL32.dll\n", "Name: VCRUNTIME140.dll\nName: ucrtbased.dll\n"):
            with self.subTest(imports=imports):
                backend.write_text(imports)
                with self.assertRaisesRegex(ValueError, "ggml-cpu-baseline.dll"):
                    self.verify()

    def test_missing_and_duplicate_backend_files_rejected(self):
        backend = self.out / "bin/ggml-cpu-baseline.dll"
        backend.rename(self.out / "backends/ggml-cpu-baseline.dll")
        self.verify()
        backend.write_text("Name: VCRUNTIME140.dll\n")
        with self.assertRaisesRegex(ValueError, "duplicate"):
            self.verify()
        backend.unlink()
        (self.out / "bin/llama.dll").unlink()
        with self.assertRaisesRegex(ValueError, "incomplete"):
            self.verify()

    def test_import_inspection_failure_fails_closed(self):
        self.readobj.write_text("#!/bin/bash\nexit 7\n")
        with self.assertRaises(subprocess.CalledProcessError):
            self.verify()


class InstallerHookSyntax(unittest.TestCase):
    @unittest.skipUnless(shutil.which("makensis"), "makensis is needed for the NSIS compile check")
    def test_prerequisite_hook_compiles_with_error_code_and_retry_paths(self):
        # Compile the actual hook with a tiny payload, including System.dll calls and every failure branch.
        # Administrator prompts, registry changes and child exit codes still require a Windows runtime test.
        with tempfile.TemporaryDirectory(prefix="ocrtrans-nsis-hook-") as tmp:
            root = Path(tmp)
            hook_dir = root / "src-tauri/windows"
            hook_dir.mkdir(parents=True)
            hook = hook_dir / "installer-hooks.nsh"
            shutil.copy2(ROOT / "src-tauri/windows/installer-hooks.nsh", hook)
            script = root / "check.nsi"
            script.write_text(f'Unicode true\n!include "LogicLib.nsh"\nName "CRT check"\nOutFile "{root / "check.exe"}"\n!include "{hook}"\nSection\n!insertmacro NSIS_HOOK_PREINSTALL\nSectionEnd\n')
            result = subprocess.run(["makensis", "-WX", "-V2", str(script)], capture_output=True, text=True,
                                    env=dict(os.environ, LANG="en_US.UTF-8", LC_ALL="en_US.UTF-8"))
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertTrue((root / "check.exe").is_file())


if __name__ == "__main__":
    unittest.main()
