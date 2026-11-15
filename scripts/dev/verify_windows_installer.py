"""Extract the NSIS installer and compare its payload with the exact release inputs."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def sha(path):
    with path.open('rb') as stream:
        digest = hashlib.sha256()
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
        return digest.hexdigest()


def verify(installer, native, executable, version, sevenzip):
    with tempfile.TemporaryDirectory(prefix="ocrtrans-unpack-") as tmp:
        unpacked = Path(tmp)
        subprocess.run([sevenzip, "x", "-y", "-o" + tmp, str(installer)], check=True,
                       stdout=subprocess.DEVNULL)
        native_files = [*native.glob("bin/*.dll"), *native.glob("backends/*.dll")]
        names = {p.name.lower() for p in native_files}
        if len(names) != len(native_files) or not {"ggml-vulkan.dll", "ggml.dll", "ggml-base.dll", "llama.dll"} <= names or not any(n.startswith("ggml-cpu-") for n in names):
            raise ValueError("Incomplete or duplicate model libraries")
        report = {}
        for original in native_files:
            target = unpacked / original.name
            if sha(target) != sha(original):
                raise ValueError(f"Incorrect packaged DLL: {original.name}")
            imports = subprocess.check_output(["llvm-readobj", "--coff-imports", str(target)], text=True)
            modules = re.findall(r"^\s*Name:\s*(\S+)", imports, re.M)
            if not any(n.lower().startswith("api-ms-win-crt-") or n.lower() == "ucrtbase.dll" for n in modules):
                raise ValueError(f"Missing shared UCRT in {target.name}")
            report[target.name] = {"sha256": sha(target), "imports": modules}
        resources = json.loads((ROOT / "src-tauri/tauri.windows.conf.json").read_text())["bundle"]["resources"]
        for source, dest in resources.items():
            p = ROOT / "src-tauri" / source
            files = p.rglob("*") if p.is_dir() else [p]
            for original in files:
                if not original.is_file():
                    continue
                target = unpacked / dest
                if p.is_dir():
                    target = target / original.relative_to(p)
                if sha(original) != sha(target):
                    raise ValueError(f"Incorrect packaged resource: {target.relative_to(unpacked)}")
                report[str(target.relative_to(unpacked))] = {"sha256": sha(target)}
        if any(unpacked.rglob("vc_redist.x64.exe")):
            raise ValueError("Unexpected Microsoft redistributable in installer")
        binary = unpacked / "ocr-trans.exe"
        image = binary.read_bytes()
        header = struct.unpack_from("<I", image, 0x3c)[0]
        if struct.unpack_from("<H", image, header + 4)[0] != 0x8664 or version.encode("utf-16le") not in image:
            raise ValueError("Unexpected desktop executable architecture or version")
        nss = b"__TAURI_BUNDLE_TYPE_VAR_NSS"
        unk = b"__TAURI_BUNDLE_TYPE_VAR_UNK"
        built = executable.read_bytes()
        if image.count(nss) != 1 or image.replace(nss, unk) != built.replace(nss, unk):
            raise ValueError("Packaged executable differs beyond the expected NSIS bundle marker")
        return {"installer_sha256": sha(installer), "executable_sha256": sha(binary), "version": version,
                "dll_count": len(native_files), "payload": report}


if __name__ == "__main__":
    args = argparse.ArgumentParser()
    for name in ("installer", "native", "executable"):
        args.add_argument(name, type=Path)
    args.add_argument("version")
    args.add_argument("--sevenzip", default="7zz")
    values = args.parse_args()
    print(json.dumps(verify(**vars(values)), indent=2))
