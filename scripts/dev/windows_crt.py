#!/usr/bin/env python3
"""Reject separate static CRTs in the shared Windows llama/ggml DLLs."""
import argparse
from pathlib import Path
import re
import shutil
import subprocess


def shared_release_cache(out):
    """Require the actual CMake C and C++ release flags to select /MD, not /MT."""
    cache = out / "build/CMakeCache.txt"
    if not cache.is_file():
        return False
    values = dict(re.findall(r"^([^#/:\n][^:\n]*):[^=\n]*=(.*)$", cache.read_text(), re.M))
    if values.get("BUILD_SHARED_LIBS", "").upper() not in ("ON", "TRUE", "1"):
        return False
    for language in ("C", "CXX"):
        flags = values.get(f"CMAKE_{language}_FLAGS", "") + " " + values.get(f"CMAKE_{language}_FLAGS_RELEASE", "")
        modes = re.findall(r"(?:^|\s)[/-](M[DT]d?)(?=\s|$)", flags)
        if not modes or any(mode != "MD" for mode in modes):
            return False
    runtime = values.get("CMAKE_MSVC_RUNTIME_LIBRARY", "")
    # cmake-rs normally leaves this empty and selects -MD through the compiler flags.
    return not runtime or runtime == "MultiThreadedDLL"


def prepare(build_root):
    for cache in sorted(build_root.glob("llama-cpp-sys-2-*/out/build/CMakeCache.txt")):
        out = cache.parent.parent
        if not shared_release_cache(out):
            print(f"removing incompatible llama.cpp CMake build: {out.parent}")
            shutil.rmtree(out.parent)


def verify(out, readobj="llvm-readobj"):
    if not shared_release_cache(out):
        raise ValueError(f"llama.cpp cache does not use shared release CRT (/MD): {out}; run a full Windows build (without --bundle-only)")
    dlls = sorted(p for folder in (out / "bin", out / "backends") for p in folder.glob("*.dll"))
    native = [p for p in dlls if p.name.lower() == "llama.dll" or p.name.lower().startswith("ggml")]
    names = [p.name.lower() for p in native]
    required = {"llama.dll", "ggml.dll", "ggml-base.dll", "ggml-vulkan.dll"}
    if not required.issubset(names) or not any(name.startswith("ggml-cpu") for name in names):
        raise ValueError(f"incomplete llama/ggml runtime DLL set: {out}")
    if len(set(names)) != len(names):
        raise ValueError(f"duplicate llama/ggml DLL basenames: {out}")
    for dll in native:
        result = subprocess.run([readobj, "--coff-imports", str(dll)], check=True, capture_output=True, text=True)
        imports = re.findall(r"^\s*Name:\s*(\S+)", result.stdout, re.M)
        shared = [name for name in imports if re.fullmatch(
            r"(?:ucrtbase|api-ms-win-crt-[\w-]+|(?:vcruntime|msvcp|concrt)140(?:_\w+)?)\.dll", name, re.I)]
        debug = [name for name in imports if re.fullmatch(r"(?:ucrtbased|(?:vcruntime|msvcp|concrt)140\w*d)\.dll", name, re.I)]
        if not shared or debug:
            raise ValueError(f"{dll.name} does not import the shared release CRT; rebuild all llama/ggml DLLs (imports: {', '.join(imports)})")
        print(f"shared CRT verified: {dll.name}: {', '.join(shared)}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("prepare", "verify"))
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    try:
        (prepare if args.action == "prepare" else verify)(args.directory)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Windows CRT validation failed: {error}\n")


if __name__ == "__main__":
    main()
