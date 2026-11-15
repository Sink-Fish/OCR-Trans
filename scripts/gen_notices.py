#!/usr/bin/env python3
"""Write src-tauri/licenses/THIRD-PARTY-NOTICES.txt: the licence texts the distributed app must carry.

Covers the Rust crates linked into the app on macOS and Windows (from `cargo metadata`, licence files read from the
local cargo registry), the native C/C++ libraries they vendor (CTranslate2 and the third-party code it compiles,
llama.cpp/ggml, SentencePiece), the ONNX Runtime library shipped next to the app (with Microsoft's full
ThirdPartyNotices), the web UI's npm runtime code, the bundled and downloadable models, and the logo font.
Texts that are not in the local registry or node_modules (crates without a licence file, the WebView2 SDK, ONNX
Runtime's notices, the Source Han Sans licence) are kept in scripts/notices/. See docs/LICENSE_INVENTORY.md.
Run after dependency changes: python3 scripts/gen_notices.py
"""
import glob
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "src-tauri", "licenses", "THIRD-PARTY-NOTICES.txt")
EXTRA = os.path.join(ROOT, "scripts", "notices")
# PATENTS: rav1e's Alliance for Open Media Patent License must travel with binaries too
LICENSE_NAMES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT", "UNLICENSE", "PATENTS")
PLATFORMS = ("aarch64-apple-darwin", "x86_64-pc-windows-msvc")

HEADER = """\
OCR Trans — third-party notices
===============================

OCR Trans © 2026 SinkFish. Original material is available under the OCR Trans Noncommercial License 1.0 above.

OCR Trans includes, links or downloads the third-party components listed below. Each component is used under its
own licence, reproduced here; those licences (not the OCR Trans licence terms) govern the components, and nothing in
the OCR Trans terms restricts the rights they grant. The model files are shipped as ordinary, unencrypted files.

Apache License 2.0 §4(d): none of the Apache-2.0 components below ships a NOTICE file (checked: every Rust crate
in the registry, CTranslate2 and its third-party code, llama.cpp, SentencePiece, PaddleOCR, RapidOCR, Qwen3).
Microsoft's own notice files for ONNX Runtime and the WebView2 SDK are reproduced in full in their sections.
The full Apache License 2.0 text appears in the Rust crates section.

Contents: models · logo font · ONNX Runtime · native libraries · Windows-only components · web UI · Rust crates
"""

MODELS = """\
Models
======

PP-OCRv5 mobile text detection and recognition (ch_PP-OCRv5_det_mobile, ch_PP-OCRv5_rec_mobile)
  PaddleOCR — Copyright (c) 2016 PaddlePaddle Authors. All Rights Reserved. https://github.com/PaddlePaddle/PaddleOCR
  ONNX conversion by RapidOCR — Copyright (c) 2021 RapidOCR Authors.
  Source: https://www.modelscope.cn/models/RapidAI/RapidOCR (revision v3.10.0, onnx/PP-OCRv5/det and rec)
  Licence: Apache License 2.0 (full text in the Rust crates section).
  Converted from the official PaddleOCR models to ONNX by RapidOCR; not modified by OCR Trans.
  Bundled with the app (models/ocr/det.onnx, models/ocr/rec.onnx).

PP-OCRv5 character dictionary (ppocr/utils/dict/ppocrv5_dict.txt)
  PaddleOCR — Copyright (c) 2016 PaddlePaddle Authors. Licence: Apache License 2.0. Unmodified.
  Bundled with the app (models/ocr/dict.txt).

opus-mt-en-zh (OPUS-MT / Tatoeba-Challenge model eng-zho, release opus-2020-07-17)
  Creator: Language Technology Research Group at the University of Helsinki (Helsinki-NLP; Jörg Tiedemann).
  Source: https://huggingface.co/Helsinki-NLP/opus-mt-en-zh (revision 408d9bc410a388e1d9aef112a2daba955b945255);
  original release: https://object.pouta.csc.fi/Tatoeba-MT-models/eng-zho/opus-2020-07-17.zip
  Licence: Creative Commons Attribution 4.0 International (CC BY 4.0) — https://creativecommons.org/licenses/by/4.0/
  Changes: the copy bundled with the app (models/opus-mt-en-zh) was converted to the CTranslate2 format with int8
  weight quantization; the copy that 设置 → 模型 can download instead is the CTranslate2 float16 conversion by gaudi
  (https://huggingface.co/gaudi/opus-mt-en-zh-ctranslate2, revision dcd22168f08b99dd34c62bc2195e31dc2f04e90b).
  No other changes. The model is provided as-is, without warranties of any kind (Section 5 of the licence).
  Note: the Hugging Face card labels this model apache-2.0; the original release ships a CC BY 4.0 LICENSE, which
  OCR Trans follows.
  Please cite: J. Tiedemann and S. Thottingal, "OPUS-MT — Building open translation services for the World",
  EAMT 2020; J. Tiedemann, "The Tatoeba Translation Challenge", WMT 2020.

M2M100 418M (Meta AI) — downloaded from Hugging Face at the user's request (设置 → 模型), not included in the app
  Copyright (c) Facebook, Inc. and its affiliates. Licence: MIT — https://huggingface.co/facebook/m2m100_418M,
  https://github.com/facebookresearch/fairseq/blob/main/LICENSE
  CTranslate2 float16 conversion by michaelfeil: https://huggingface.co/michaelfeil/ct2fast-m2m100_418M
  (revision 06bd83294674f3737eb929e501aa5beafaa229d8).

Qwen3-1.7B (Qwen team, Alibaba Cloud) — downloaded from Hugging Face at the user's request, not included in the app
  Copyright 2024 Alibaba Cloud. Licence: Apache License 2.0 — https://huggingface.co/Qwen/Qwen3-1.7B/blob/main/LICENSE
  GGUF Q4_K_M quantization by unsloth: https://huggingface.co/unsloth/Qwen3-1.7B-GGUF
  (revision d7f544eead698dbd1f15126ef60b45a1e1933222).
"""

FONT = """\
Logo font
=========

The character 文 in the OCR Trans logo is derived from Source Han Sans CN Bold, version 2.005
(https://github.com/adobe-fonts/source-han-sans, tag 2.005R), converted to vector outlines. No font file is included
in the app. Licence of the font software (SIL Open Font License 1.1):
"""

ORT = """\
ONNX Runtime 1.31.0 (runtime/libonnxruntime.dylib on macOS, onnxruntime.dll on Windows; loaded at run time)
=========================================================================================================
MIT License

Copyright (c) Microsoft Corporation

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the Software without restriction, including without limitation the
rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit
persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the
Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

ONNX Runtime third-party notices (ThirdPartyNotices.txt of the v1.31.0 release, reproduced in full):
"""

# (title, licence, crate, paths relative to the crate dir) for C/C++ code compiled into the app; vendored code that
# is not compiled with our features (CUDA, tests, the CTranslate2 CLI, llama.cpp's server and multimodal tools) is
# left out — see docs/LICENSE_INVENTORY.md
NATIVE = [
    ("CTranslate2 4.8 (linked statically through ct2rs)", "MIT", "ct2rs", ["CTranslate2/LICENSE"]),
    ("ct2rs (Rust bindings for CTranslate2)", "MIT", None, [os.path.join(EXTRA, "ct2rs-0.10.2-LICENSE.txt")]),
    ("Ruy matrix multiplication library (CTranslate2 GEMM backend)", "Apache-2.0", "ct2rs",
     ["CTranslate2/third_party/ruy/LICENSE"]),
    ("cpuinfo (vendored by Ruy)", "BSD-2-Clause", "ct2rs", ["CTranslate2/third_party/ruy/third_party/cpuinfo/LICENSE"]),
    ("clog (vendored by cpuinfo)", "BSD-2-Clause", "ct2rs",
     ["CTranslate2/third_party/ruy/third_party/cpuinfo/deps/clog/LICENSE"]),
    ("cpu_features (CTranslate2 CPU detection)", "Apache-2.0", "ct2rs", ["CTranslate2/third_party/cpu_features/LICENSE"]),
    ("spdlog (CTranslate2 logging, header-only)", "MIT", "ct2rs", ["CTranslate2/third_party/spdlog/LICENSE"]),
    ("BS::thread_pool (header-only, used by CTranslate2)", "MIT", None, [os.path.join(EXTRA, "BS_thread_pool-LICENSE.txt")]),
    ("SIMD_Utils avx512_mathfun.h (header-only, used by CTranslate2)", "BSD-2-Clause", None,
     [os.path.join(EXTRA, "simd_utils-LICENSE.txt")]),
    ("avx_mathfun.h / neon_mathfun.h (header-only, used by CTranslate2; licence as stated in the headers)",
     "MIT (Marian modifications) + zlib (Garberoglio, Pommier)", "ct2rs", ["header:CTranslate2/third_party/avx_mathfun.h",
                                                                          "header:CTranslate2/third_party/neon_mathfun.h"]),
    ("llama.cpp / ggml (linked statically through llama-cpp-sys-2)", "MIT", "llama-cpp-sys-2", ["llama.cpp/LICENSE"]),
    ("SentencePiece (linked statically through sentencepiece-sys)", "Apache-2.0", "sentencepiece-sys", ["source/LICENSE"]),
    ("Abseil (vendored by SentencePiece)", "Apache-2.0", "sentencepiece-sys", ["source/third_party/absl/LICENSE"]),
    ("Protocol Buffers lite (vendored by SentencePiece)", "BSD-3-Clause", "sentencepiece-sys",
     ["source/third_party/protobuf-lite/LICENSE"]),
    ("Darts-clone (vendored by SentencePiece)", "BSD-2-Clause", "sentencepiece-sys",
     ["source/third_party/darts_clone/LICENSE"]),
    ("esaxx (vendored by SentencePiece)", "MIT", "sentencepiece-sys", ["source/third_party/esaxx/LICENSE"]),
]

WINDOWS = [
    ("Microsoft WebView2 SDK 1.0.3800.47 — WebView2LoaderStatic.lib, linked statically through webview2-com-sys",
     "BSD-3-Clause-style (Microsoft)", [os.path.join(EXTRA, "WebView2-SDK-1.0.3800.47-LICENSE.txt"),
                                        os.path.join(EXTRA, "WebView2-SDK-1.0.3800.47-NOTICE.txt")]),
    ("webview2-com, webview2-com-sys 0.39.1", "MIT", [os.path.join(EXTRA, "webview2-rs-LICENSE.txt")]),
    ("Vulkan-Headers 1.4.363 (vulkan.h / vulkan.hpp, compiled into ggml-vulkan.dll; used under the MIT option)",
     "Apache-2.0 OR MIT", [os.path.join(EXTRA, "Vulkan-Headers-1.4.363-LICENSE.md")]),
    ("SPIRV-Headers 1.4.363 (spirv.hpp, compiled into ggml-vulkan.dll)", "MIT (Khronos)",
     [os.path.join(EXTRA, "SPIRV-Headers-1.4.363-LICENSE.txt")]),
]

# Rust crates that ship no licence file of their own: the upstream licence text
CRATE_EXTRA = {
    "ct2rs": "ct2rs-0.10.2-LICENSE.txt",
    "objc2": "objc2-LICENSE-MIT.txt",
    "block2": "objc2-LICENSE-MIT.txt",
    "objc2-encode": "objc2-LICENSE-MIT.txt",
    "objc2-foundation": "objc2-LICENSE-MIT.txt",
    "webview2-com": "webview2-rs-LICENSE.txt",
    "webview2-com-sys": "webview2-rs-LICENSE.txt",
}

# npm code that ends up in the shipped web bundle (src/ imports + what the bundler injects)
NPM = ["react", "react-dom", "scheduler", "@tauri-apps/api", "@tauri-apps/plugin-autostart", "@tauri-apps/plugin-opener"]
NPM_BUILD_HELPERS = ["rolldown", "vite"]


def licence_files(d, depth=1):
    out = []
    for root, dirs, files in os.walk(d):
        if root[len(d):].count(os.sep) >= depth:
            dirs[:] = []
        for f in sorted(files):
            if f.upper().startswith(LICENSE_NAMES) and not f.endswith((".cmake", ".py", ".rs", ".h", ".cc")):
                out.append(os.path.join(root, f))
    return out


def read(p):
    with open(p, encoding="utf-8", errors="replace") as fh:
        return fh.read().strip()


def header_licence(p):
    """The licence comment at the top of a C/C++ header (header-only code that ships no licence file)."""
    text = read(p)
    start = text.find("/*")
    end = text.find("*/", start)
    if start < 0 or end < 0:
        sys.exit(f"no licence comment at the top of {p}")
    return text[start + 2:end].strip("\n")


def section(title, paths, licence=None):
    parts = [title, "=" * min(len(title), 110)]
    if licence:
        parts.append(f"Licence: {licence}")
    for p in paths:
        if p.startswith(HEADER_PREFIX):
            h = p[len(HEADER_PREFIX):]
            parts.append(f"--- {os.path.basename(h)} (licence comment at the top of the file) ---\n{header_licence(h)}")
        else:
            parts.append(f"--- {os.path.basename(p)} ---\n{read(p)}")
    if len(parts) == (3 if licence else 2):
        parts.append("(no licence file in the package; see the licence named above)")
    return "\n".join(parts)


HEADER_PREFIX = "header:"


def need(path):
    """`path` (a licence file), or "header:<path>" for a header whose top comment carries the licence."""
    real = path[len(HEADER_PREFIX):] if path.startswith(HEADER_PREFIX) else path
    if not os.path.exists(real):
        sys.exit(f"missing licence text: {real}")
    return path


def linked_crates(platform):
    """Crates linked into the app for one target: walk the resolved graph from the app package (normal deps)."""
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--offline", "--filter-platform", platform], cwd=ROOT))
    workspace = set(meta["workspace_members"])
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    pkgs = {p["id"]: p for p in meta["packages"]}
    app = next(p["id"] for p in meta["packages"] if p["name"] == "ocr-trans")
    seen, todo = set(), [app]
    while todo:
        i = todo.pop()
        if i in seen:
            continue
        seen.add(i)
        for d in nodes[i]["deps"]:
            if any(k["kind"] is None for k in d["dep_kinds"]):
                todo.append(d["pkg"])
    return {i: pkgs[i] for i in seen if i not in workspace}


def npm_dir(name):
    d = os.path.join(ROOT, "node_modules", name)
    if not os.path.isdir(d):
        d = next(iter(glob.glob(os.path.join(ROOT, "node_modules", ".pnpm", name.replace("/", "+") + "@*", "node_modules", name))), d)
    return d


def npm_section(name):
    d = npm_dir(name)
    lic = "?"
    if os.path.exists(os.path.join(d, "package.json")):
        with open(os.path.join(d, "package.json")) as fh:
            pj = json.load(fh)
        lic = pj.get("license", "?")
        name = f"{name} {pj.get('version', '')}"
    return section(name, licence_files(d), lic)


def main():
    # one notices file for both installers: the union of what macOS and Windows link
    per_platform = {pl: linked_crates(pl) for pl in PLATFORMS}
    crates = {}
    for found in per_platform.values():
        crates.update(found)
    only = {pl: {p["name"] for i, p in found.items() if not any(i in o for q, o in per_platform.items() if q != pl)}
            for pl, found in per_platform.items()}
    crates = sorted(crates.values(), key=lambda p: (p["name"], p["version"]))
    reg = {p["name"]: os.path.dirname(p["manifest_path"]) for p in crates}

    out = [read(os.path.join(ROOT, "LICENSE")), "", HEADER, MODELS, FONT + read(need(os.path.join(EXTRA, "SourceHanSans-2.005-LICENSE.txt"))), ""]
    out.append(ORT + read(need(os.path.join(EXTRA, "onnxruntime-1.31.0-ThirdPartyNotices.txt"))))
    out.append("")

    out.append("Native libraries compiled into the app\n======================================\n")
    for title, lic, crate, rel in NATIVE:
        base = reg.get(crate, "/none") if crate else ""
        paths = [HEADER_PREFIX + os.path.join(base, r[len(HEADER_PREFIX):]) if r.startswith(HEADER_PREFIX) else os.path.join(base, r) for r in rel]
        out.append(section(title, [need(p) for p in paths], lic))
        out.append("")

    out.append("Windows-only components\n=======================\n")
    for title, lic, paths in WINDOWS:
        out.append(section(title, [need(p) for p in paths], lic))
        out.append("")

    out.append("Web UI (npm packages bundled into the app's web view)\n=====================================================\n")
    for name in NPM:
        out.append(npm_section(name))
        out.append("")
    out.append("Small runtime helpers injected by the bundler (module preload polyfill, CommonJS interop):\n")
    for name in NPM_BUILD_HELPERS:
        out.append(npm_section(name))
        out.append("")

    # Rust crates: identical licence texts (most crates carry the same MIT / Apache-2.0 files) are printed once,
    # each preceded by the crates that ship it
    groups, bare, mpl = {}, [], []
    for p in crates:
        files = licence_files(os.path.dirname(p["manifest_path"]))
        if not files and p["name"] in CRATE_EXTRA:
            files = [need(os.path.join(EXTRA, CRATE_EXTRA[p["name"]]))]
        spdx = p.get("license") or p.get("license_file") or "?"
        where = " [macOS only]" if p["name"] in only["aarch64-apple-darwin"] else (
            " [Windows only]" if p["name"] in only["x86_64-pc-windows-msvc"] else "")
        name = f"{p['name']} {p['version']} ({spdx}){where}"
        if "MPL" in spdx:
            mpl.append(f"  {p['name']} {p['version']}: https://crates.io/crates/{p['name']}/{p['version']}"
                       + (f" — {p['repository']}" if p.get("repository") else ""))
        if files:
            for t in (read(f) for f in files):
                groups.setdefault(" ".join(t.split()), (t, []))[1].append(name)
        else:
            bare.append(name)
    out.append(f"Rust crates ({len(crates)})\n===============")
    out.append("Crates linked on macOS and/or Windows (some are used only while compiling; listing them is harmless).")
    if mpl:
        out.append("Mozilla Public License 2.0 crates are used unmodified; their source code is available at:")
        out.extend(mpl)
    out.append("")
    for text, names in sorted(groups.values(), key=lambda g: (-len(g[1]), g[1][0])):
        out.append("Used by: " + ", ".join(sorted(set(names))))
        out.append(text)
        out.append("-" * 80)
    if bare:
        out.append("Crates that ship no licence file (their SPDX licence is named; its standard text appears above):")
        out.append(", ".join(bare))

    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w", encoding="utf-8") as fh:
        fh.write("\n".join(out).rstrip() + "\n")
    print(f"wrote {OUT}: {len(crates)} crates, {os.path.getsize(OUT) // 1024} KB")


if __name__ == "__main__":
    sys.exit(main())
