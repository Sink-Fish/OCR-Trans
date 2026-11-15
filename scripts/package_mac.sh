#!/usr/bin/env bash
# Build the release .app, sign it with the local identity (scripts/sign_mac.sh), wrap it in a .dmg with hdiutil
# (no Finder/AppleScript automation needed) and install it to /Applications (scripts/install_mac.sh).
# Output: release/OCR-Trans_<version>_<arch>.dmg and /Applications/OCR Trans.app.  `--no-install` skips installing.
set -euo pipefail
cd "$(dirname "$0")/.."
case "${1:-}" in ""|--no-install) ;; *) echo "usage: $0 [--no-install]" >&2; exit 2;; esac
[ "$#" -le 1 ] || { echo "usage: $0 [--no-install]" >&2; exit 2; }
./scripts/fetch_runtime.sh
[ -f models/ocr/rec.onnx ] && [ -f models/opus-mt-en-zh/model.bin ] || ./scripts/fetch_models.sh
pnpm install --frozen-lockfile
pnpm tauri build --bundles app -- --locked
VER=$(node -p "require('./src-tauri/tauri.conf.json').version")
APP="target/release/bundle/macos/OCR Trans.app"
# stable signature (same designated requirement every build → Screen Recording permission survives updates)
./scripts/sign_mac.sh "$APP"
mkdir -p release
STAGE=$(mktemp -d)
DMG_TMP=""
trap 'rm -rf "$STAGE"; [ -z "$DMG_TMP" ] || rm -f "$DMG_TMP"' EXIT
ditto "$APP" "$STAGE/OCR Trans.app"
ln -s /Applications "$STAGE/Applications"
DMG="release/OCR-Trans_${VER}_$(uname -m).dmg"
# Publish only a complete, verified image; a failed build must preserve the last good release.
DMG_TMP="release/.OCR-Trans_${VER}_$(uname -m).$$.dmg"
hdiutil create -volname "OCR Trans" -srcfolder "$STAGE" -format UDZO "$DMG_TMP" >/dev/null
hdiutil verify "$DMG_TMP" >/dev/null
mv -f "$DMG_TMP" "$DMG"
DMG_TMP=""
ls -lh "$DMG"
# install to /Applications (quits the running app, swaps the bundle, launches the new one); --no-install skips it
if [ "${1:-}" != "--no-install" ]; then
  ./scripts/install_mac.sh "$APP"
  # release/ keeps only the disk image: the old runnable copy there (pre-0.5.4 builds ran from it) would confuse
  # Launch Services and the privacy settings; install_mac.sh has unregistered it
  if [ -e "release/OCR Trans.app" ] && ! pgrep -f "release/OCR Trans.app/Contents/MacOS" >/dev/null; then rm -rf "release/OCR Trans.app"; fi
fi
