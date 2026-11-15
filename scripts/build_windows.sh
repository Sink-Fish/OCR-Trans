#!/usr/bin/env bash
# Cross-compile the Windows x64 installer (NSIS .exe) on macOS with cargo-xwin.
#
# Needs: brew install llvm lld nsis; cargo install --locked cargo-xwin; rustup target add x86_64-pc-windows-msvc.
# cargo-xwin downloads the MSVC CRT + Windows SDK on first use and accepts Microsoft's licence for them on your
# behalf. Review the applicable Visual Studio and Build Tools terms before running it.
#
# * The Rust executable and static CTranslate2 use +crt-static (/MT). The shared llama/ggml DLLs use /MD:
#   they pass FILE* between DLLs and must share the CRT's file-descriptor state. Their import libraries do not
#   contain /MT or /MD object code, so this does not change CTranslate2's static CRT requirement.
#   tauri.windows.conf.json turns off Tauri's own "static vcruntime + dynamic
#   UCRT" link flags (build.windows.staticVCRuntime = false): they exclude libucrt.lib while cargo-xwin, seeing
#   +crt-static, excludes ucrt.lib, and with both gone strlen, round etc. are undefined.
# * ct2rs 0.10.2's build script decides Windows vs macOS with cfg!(), i.e. by the HOST, so cross-compiling from a
#   Mac neither links its .lib files nor sets the MSVC flags. scripts/patches/ct2rs-target-cfg.patch makes it read
#   the target instead; the patched copy lives in target/ and is used only for this build (Cargo.lock is restored).
# * The installer checks for the external Microsoft Visual C++ x64 runtime before installing.
# * llama.cpp (high-quality tier) is built as DLLs with its backends loaded at run time: the CPU code for several
#   instruction sets (ggml-cpu-*.dll, the best one for the processor is picked) and Vulkan (ggml-vulkan.dll, for
#   GPUs). The Vulkan backend needs, at build time, a Vulkan SDK layout: Homebrew's vulkan-headers, spirv-headers
#   and glslc (brew install shaderc vulkan-headers glslang) plus an import library for vulkan-1.dll, made here with
#   llvm-dlltool from the headers. ggml builds its shader generator for the Mac by itself. All the DLLs are put into
#   the installer next to the exe. scripts/patches/llama-cpp-sys-2-clang-cl.patch, applied to a copy in target/
#   like the ct2rs one: two things ggml's CMake assumes of MSVC that clang-cl does not do — -mavxvnni for the
#   AVX-VNNI CPU build (MSVC needs no flag for its intrinsics) and advapi32 for the backend DLLs (MSVC links it by
#   default; the CPU backend reads the processor name from the registry) — and llama.cpp's "common" helper library
#   left out (unused; as a DLL it would need OpenSSL for Windows).
# Output: release/OCR-Trans_<version>_x64-setup.exe
# --bundle-only reuses existing llama.cpp DLLs (for app-only changes after a full build).
set -euo pipefail
cd "$(dirname "$0")/.."
case "${1:-}" in ""|--bundle-only) ;; *) echo "usage: $0 [--bundle-only]" >&2; exit 2;; esac
[ "$#" -le 1 ] || { echo "usage: $0 [--bundle-only]" >&2; exit 2; }
export PATH="/opt/homebrew/opt/llvm/bin:/opt/homebrew/opt/lld/bin:$PATH"
# Homebrew's makensis 3.13 aborts with std::bad_alloc on any `Unicode true` script under the C locale
export LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8
for t in cargo-xwin makensis clang-cl lld-link llvm-rc llvm-dlltool llvm-readobj glslc; do
  command -v "$t" >/dev/null || { echo "missing $t (see the header of this script)" >&2; exit 1; }
done
rustup target add x86_64-pc-windows-msvc >/dev/null
./scripts/fetch_runtime.sh Windows-x64
[ -f models/ocr/rec.onnx ] && [ -f models/opus-mt-en-zh/model.bin ] || {
  echo "runtime/onnxruntime.dll or bundled models missing: run scripts/check_windows.sh / scripts/fetch_models.sh" >&2
  exit 1
}
# patched ct2rs (see above)
CT2_SRC=$(ls -d "${CARGO_HOME:-$HOME/.cargo}"/registry/src/*/ct2rs-0.10.2 | head -1)
CT2=target/xwin-patch/ct2rs-0.10.2
CT2_MARK="$CT2/.patched-$(shasum scripts/patches/ct2rs-target-cfg.patch | cut -c1-12)"
if [ ! -f "$CT2_MARK" ]; then
  rm -rf "$CT2" && mkdir -p "$(dirname "$CT2")"
  cp -R "$CT2_SRC" "$CT2"
  patch -d "$CT2" -p1 < scripts/patches/ct2rs-target-cfg.patch
  touch "$CT2_MARK"
fi

# patched llama-cpp-sys-2 (see above)
LL_SRC=$(ls -d "${CARGO_HOME:-$HOME/.cargo}"/registry/src/*/llama-cpp-sys-2-0.1.160 | head -1)
LL=target/xwin-patch/llama-cpp-sys-2-0.1.160
LL_MARK="$LL/.patched-$(shasum scripts/patches/llama-cpp-sys-2-clang-cl.patch | cut -c1-12)"
if [ ! -f "$LL_MARK" ]; then
  rm -rf "$LL" && mkdir -p "$(dirname "$LL")"
  cp -R "$LL_SRC" "$LL"
  patch -d "$LL" -p1 < scripts/patches/llama-cpp-sys-2-clang-cl.patch
  touch "$LL_MARK"
fi

LOCK_BACKUP=$(mktemp "$PWD/target/xwin-patch/Cargo.lock.XXXXXX")
cp Cargo.lock "$LOCK_BACKUP"
trap 'cp "$LOCK_BACKUP" Cargo.lock && rm -f "$LOCK_BACKUP"' EXIT

export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS="-C target-feature=+crt-static"
# Override the parent Rust +crt-static only for the shared llama.cpp CMake build. ggml_fopen creates a FILE*
# in ggml-base.dll which llama.dll consumes; separate /MT CRT instances cannot safely share that object.
export LLAMA_STATIC_CRT=0
# Upstream uses always_configure(false), so changing this environment variable alone leaves the old /MT cache.
# Bundle-only is read-only here and will reject incompatible artifacts at the verification gate below.
if [ "${1:-}" != "--bundle-only" ]; then
  python3 scripts/dev/windows_crt.py prepare target/x86_64-pc-windows-msvc/release/build
fi
# a minimal Vulkan SDK for the cross build (see above)
VK="$PWD/target/xwin-vulkan-sdk"
VK_HEADERS=$(brew --prefix vulkan-headers)/include
SPIRV_HEADERS=$(brew --prefix spirv-headers)/include
[ -f "$VK_HEADERS/vulkan/vulkan.hpp" ] || { echo "vulkan-headers missing (brew install vulkan-headers)" >&2; exit 1; }
[ -f "$SPIRV_HEADERS/spirv/unified1/spirv.hpp" ] || { echo "spirv-headers missing (brew install glslang)" >&2; exit 1; }
# Include/ as in the SDK: the Vulkan headers and SPIRV-Headers (ggml-vulkan includes spirv/unified1/spirv.hpp)
[ -L "$VK/Include" ] && rm "$VK/Include"
mkdir -p "$VK/Lib" "$VK/Bin" "$VK/Include"
for d in vulkan vk_video; do ln -sfn "$VK_HEADERS/$d" "$VK/Include/$d"; done
ln -sfn "$SPIRV_HEADERS/spirv" "$VK/Include/spirv"
ln -sf "$(command -v glslc)" "$VK/Bin/glslc"
python3 scripts/dev/vulkan_def.py "$VK_HEADERS/vulkan/vulkan_core.h" "$VK_HEADERS/vulkan/vulkan_win32.h" > "$VK/vulkan-1.def"
llvm-dlltool -m i386:x86-64 -D vulkan-1.dll -d "$VK/vulkan-1.def" -l "$VK/Lib/vulkan-1.lib"
export VULKAN_SDK="$VK"

pnpm install --frozen-lockfile
CARGO_PATCH="patch.crates-io.ct2rs.path=\"$PWD/$CT2\""
CARGO_PATCH_LL="patch.crates-io.llama-cpp-sys-2.path=\"$PWD/$LL\""
# 1. compile (the llama.cpp DLLs only exist afterwards) …
pnpm build
if [ "${1:-}" != "--bundle-only" ]; then
  cargo xwin build --release --target x86_64-pc-windows-msvc -p ocr-trans --config "$CARGO_PATCH" --config "$CARGO_PATCH_LL" \
    --message-format=json-render-diagnostics > target/xwin-patch/build-messages.jsonl
  # Use the directory Cargo actually used, never a different cached build selected by its mtime.
  python3 - <<'PY' > target/xwin-patch/llama-out.txt
import json, sys
outputs = []
for line in open('target/xwin-patch/build-messages.jsonl'):
    try:
        item = json.loads(line)
    except ValueError:
        continue
    if (item.get('reason') == 'build-script-executed' and 'llama-cpp-sys-2' in item.get('package_id', '')
            and '/x86_64-pc-windows-msvc/' in item.get('out_dir', '')):
        outputs.append(item['out_dir'])
if not outputs:
    sys.exit('Cargo did not report the Windows llama.cpp output directory')
print(outputs[-1])
PY
fi
# (absolute: Tauri resolves resource paths from src-tauri/)
[ -s target/xwin-patch/llama-out.txt ] || { echo "no recorded llama.cpp build; run without --bundle-only first" >&2; exit 1; }
OUT=$(cat target/xwin-patch/llama-out.txt)
python3 scripts/dev/windows_crt.py verify "$OUT"
DLLS=()
for dll in "$OUT"/bin/*.dll "$OUT"/backends/*.dll; do [ ! -f "$dll" ] || DLLS+=("$dll"); done
printf '%s\n' ${DLLS[@]+"${DLLS[@]}"} | grep -q "ggml-cpu" || { echo "no ggml-cpu backend DLLs in $OUT" >&2; exit 1; }
for required in llama.dll ggml.dll ggml-base.dll ggml-vulkan.dll; do
  [ -s "$OUT/bin/$required" ] || [ -s "$OUT/backends/$required" ] || {
    echo "missing required runtime DLL: $required ($OUT)" >&2; exit 1
  }
done
# 2. … then bundle with every DLL next to the exe
python3 -c 'import json, os, sys; print(json.dumps({"bundle": {"resources": {p: os.path.basename(p) for p in sys.argv[1:]}}}, indent=2))' "${DLLS[@]}" \
  > target/xwin-patch/bundle-dlls.json
echo "bundling: $(for d in "${DLLS[@]}"; do basename "$d"; done | tr '\n' ' ')"
pnpm tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis --config target/xwin-patch/bundle-dlls.json -- \
  --config "$CARGO_PATCH" --config "$CARGO_PATCH_LL"

VER=$(node -p "require('./src-tauri/tauri.conf.json').version")
EXE="target/x86_64-pc-windows-msvc/release/bundle/nsis/OCR Trans_${VER}_x64-setup.exe"
[ -s "$EXE" ] || { echo "missing installer for version $VER: $EXE" >&2; exit 1; }
mkdir -p release
cp "$EXE" "release/.OCR-Trans_${VER}_x64-setup.exe.part"
mv -f "release/.OCR-Trans_${VER}_x64-setup.exe.part" "release/OCR-Trans_${VER}_x64-setup.exe"
ls -lh "release/OCR-Trans_${VER}_x64-setup.exe"
