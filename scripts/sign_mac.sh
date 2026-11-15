#!/usr/bin/env bash
# Sign an app bundle with the local identity (scripts/setup_signing.sh), nested code first, no hardened runtime.
#   ./scripts/sign_mac.sh "target/release/bundle/macos/OCR Trans.app"
set -euo pipefail
cd "$(dirname "$0")/.."
APP=${1:?app bundle}
if [ -z "${OCRTRANS_SIGNING_DIR:-}" ]; then ./scripts/setup_signing.sh >/dev/null; fi
identity_dir=${OCRTRANS_SIGNING_DIR:-"$PWD/.signing"}
KC="$identity_dir/ocrtrans-signing.keychain-db"
ID=$(security find-identity "$KC" | awk '/OCR Trans Local Signing/{print $2; exit}')
security unlock-keychain -p "$(cat "$identity_dir/keychain.pass")" "$KC"
# codesign only finds identities in keychains on the search list: add ours for the duration of this script
ORIG=()
ORIG_TEXT=$(security list-keychains -d user)
while IFS= read -r keychain; do
  [ -z "$keychain" ] || ORIG+=("$keychain")
done <<< "$(printf '%s\n' "$ORIG_TEXT" | sed -E 's/^[[:space:]]*"(.*)"[[:space:]]*$/\1/')"
# macOS ships Bash 3.2, where an empty array is considered unset with `set -u`.
trap 'security list-keychains -d user -s ${ORIG[@]+"${ORIG[@]}"}' EXIT
security list-keychains -d user -s ${ORIG[@]+"${ORIG[@]}"} "$KC"
sign() { codesign --force --timestamp=none --keychain "$KC" -s "$ID" "$@"; }
# nested code (dylibs, helpers) before the bundle that seals them
find "$APP/Contents" -type f \( -name "*.dylib" -o -name "*.so" \) -print0 | while IFS= read -r -d '' f; do sign "$f"; done
if [ -d "$APP/Contents/Frameworks" ]; then
  find "$APP/Contents/Frameworks" -maxdepth 1 -name "*.framework" -print0 | while IFS= read -r -d '' f; do sign "$f"; done
fi
sign "$APP"
codesign --verify --deep --strict "$APP"
DR=$(codesign -dr - "$APP" 2>/dev/null | sed -n 's/^designated => //p')
case "$DR" in
  *"certificate leaf"*) echo "signed: $DR" ;;
  *) echo "unexpected designated requirement: $DR" >&2; exit 1 ;;
esac
