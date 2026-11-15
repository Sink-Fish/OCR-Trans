#!/usr/bin/env bash
# Type-check the whole app for Windows x64 from macOS/Linux, without a Windows SDK.
#
# * Native C++ translation backends (CTranslate2, SentencePiece, llama.cpp) need MSVC headers, so they are
#   switched off with --no-default-features; everything else (all of our Rust code, Tauri, xcap's Windows
#   capture path, ONNX Runtime bindings, plugins) is checked.
# * Tauri's build script compiles the .exe icon/manifest with llvm-rc; `cargo check` never links, so a tiny shim
#   that only creates the expected output file is enough here. Real Windows builds use the real rc.exe.
# * We deliberately don't use cargo-xwin: it downloads the MSVC CRT/SDK and auto-accepts Microsoft's licence.
set -euo pipefail
cd "$(dirname "$0")/.."
T=""
SHIM=""
trap '[ -z "$T" ] || rm -rf "$T"; [ -z "$SHIM" ] || rm -rf "$SHIM"' EXIT
rustup target add x86_64-pc-windows-msvc >/dev/null
./scripts/fetch_runtime.sh Windows-x64
SHIM=$(mktemp -d)
cat > "$SHIM/llvm-rc" <<'SH'
#!/bin/sh
# fake llvm-rc for `cargo check`: answer the version probe, otherwise create the /fo output file
for a in "$@"; do [ "$a" = "/?" ] && { echo "OVERVIEW: Resource Converter"; echo "  /no-preprocess"; exit 0; }; done
while [ $# -gt 0 ]; do [ "$1" = "/fo" ] && { : > "$2"; exit 0; }; shift; done
exit 0
SH
chmod +x "$SHIM/llvm-rc"
PATH="$SHIM:$PATH" cargo check --target x86_64-pc-windows-msvc -p ocr-trans --no-default-features "$@"
