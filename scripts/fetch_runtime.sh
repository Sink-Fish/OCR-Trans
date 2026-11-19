#!/usr/bin/env bash
# Fetch the pinned Microsoft runtime for each supported OS.
set -euo pipefail
cd "$(dirname "$0")/.."
platform=${1:-$(uname -s)-$(uname -m)}
url=""
member=""
case "$platform" in
  Darwin-arm64)
    ./scripts/fetch_runtime.sh Darwin-arm64-Ventura
    package=onnxruntime-osx-arm64-1.31.0.tgz
    library=libonnxruntime.dylib
    archive_sha=1031b36dd3bdaa8d976a6130d9cf2a05e9be320b93ae804f8427b91dc3432afc
    library_sha=83973572676032b4ba9418afed0b51f45599d35bb4e4628ca8ae1954c8362e31
    ;;
  Darwin-arm64-Ventura)
    package=microsoft.ml.onnxruntime.1.18.1.nupkg
    url="https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime/1.18.1/$package"
    library=libonnxruntime-ventura.dylib
    member=runtimes/osx-arm64/native/libonnxruntime.dylib
    archive_sha=e1fdeb5359407c948569905b4eb76af50b8bdb38b2bd3f2ad6516fe4a7186d6a
    library_sha=cf0c4da3d0ccae9b71cc854333e716d162869a3c42f9017b142408542fcec9b7
    ;;
  Linux-x86_64)
    package=onnxruntime-linux-x64-1.31.0.tgz
    library=libonnxruntime.so
    archive_sha=cc5c72baf5ae5c8238a6841f0897227be2d02826b9cf98eaf02fdefaeeb03a57
    library_sha=469849977cb60744ea160927e1a3a0643fd4c55c0aff743251c6d293402e334e
    ;;
  Windows-x64|MINGW*_NT*-x86_64|MSYS_NT*-x86_64)
    package=onnxruntime-win-x64-1.31.0.zip
    library=onnxruntime.dll
    archive_sha=3958d8a44984160692c1b4ffd0de3cb48f8c39f9d34bb31e433db06e30b82195
    library_sha=0c42dc4e278f0968fdf9ebf1b1e54a74c657a9942e70803e453c91993451cf4c
    ;;
  *) echo "Unsupported runtime platform: $platform" >&2; exit 1 ;;
esac
hash_file() { shasum -a 256 "$1" | cut -d' ' -f1; }
mkdir -p runtime
dest="runtime/$library"
if [ -f "$dest" ] && [ "$(hash_file "$dest")" = "$library_sha" ]; then exit 0; fi
stage=$(mktemp -d "runtime/.prepare.XXXXXX")
trap 'rm -rf "$stage"' EXIT
curl -fL --retry 3 "${url:-https://github.com/microsoft/onnxruntime/releases/download/v1.31.0/$package}" -o "$stage/$package"
[ "$(hash_file "$stage/$package")" = "$archive_sha" ] || { echo "ONNX Runtime archive checksum mismatch" >&2; exit 1; }
case "$package" in
  *.nupkg) unzip -qo "$stage/$package" "$member" -d "$stage" ;;
  *.zip) unzip -q "$stage/$package" -d "$stage" ;;
  *.tgz) tar xzf "$stage/$package" -C "$stage" ;;
esac
extracted="$stage/${member:-${package%.*}/lib/$library}"
[ "$(hash_file "$extracted")" = "$library_sha" ] || { echo "ONNX Runtime library checksum mismatch" >&2; exit 1; }
cp -L "$extracted" "$stage/verified"
mv -f "$stage/verified" "$dest"
