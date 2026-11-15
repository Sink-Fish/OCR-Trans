#!/usr/bin/env bash
# Install a built app to /Applications (or ~/Applications if /Applications is not writable without admin rights),
# replacing the old copy atomically, then launch it from there. Only one registered copy is left, so Launch
# Services and the privacy settings (TCC) see a single "OCR Trans".
#   ./scripts/install_mac.sh "target/release/bundle/macos/OCR Trans.app"
set -euo pipefail
cd "$(dirname "$0")/.."
SRC=${1:?app bundle}
NAME="OCR Trans.app"
[ -x "$SRC/Contents/MacOS/ocr-trans" ] && [ -f "$SRC/Contents/Info.plist" ] || {
  echo "not a complete OCR Trans app bundle: $SRC" >&2; exit 1
}
codesign --verify --deep --strict "$SRC"
LSREG=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
DEST_DIR=/Applications
if ! { [ -w "$DEST_DIR" ] && { [ ! -e "$DEST_DIR/$NAME" ] || [ -w "$DEST_DIR/$NAME" ]; }; }; then
  DEST_DIR="$HOME/Applications"
  mkdir -p "$DEST_DIR"
  echo "note: /Applications is not writable without admin rights — installing to $DEST_DIR"
fi
DEST="$DEST_DIR/$NAME"

# Prepare and verify before interrupting the running app. Preserve the old bundle if the swap fails.
STAGE=$(mktemp -d "$DEST_DIR/.ocrtrans-install.XXXXXX")
cleanup() {
  if [ -e "$STAGE/old.app" ]; then
    if [ ! -e "$DEST" ]; then
      mv "$STAGE/old.app" "$DEST" || { echo "restore the previous app from $STAGE/old.app" >&2; return; }
    else
      echo "previous app preserved at $STAGE/old.app" >&2
      return
    fi
  fi
  rm -rf "$STAGE"
}
trap cleanup EXIT
ditto "$SRC" "$STAGE/new.app"
codesign --verify --deep --strict "$STAGE/new.app"

# quit every running copy gracefully (SIGTERM → the app frees its models and exits, 0.5.6+), wait up to 10 s
pids=$(pgrep -f "/$NAME/Contents/MacOS/ocr-trans" || true)
if [ -n "$pids" ]; then
  kill -TERM $pids 2>/dev/null || true
  for _ in $(seq 1 20); do
    # (|| true: once every process is gone the loop's status is non-zero, which set -e would treat as an error)
    alive=$(for p in $pids; do kill -0 "$p" 2>/dev/null && echo "$p"; done || true)
    [ -z "$alive" ] && break
    sleep 0.5
  done
  [ -n "${alive:-}" ] && { echo "OCR Trans did not quit (pids $alive) — not replacing it" >&2; exit 1; }
fi

[ ! -e "$DEST" ] || mv "$DEST" "$STAGE/old.app"
mv "$STAGE/new.app" "$DEST"
rm -rf "$STAGE/old.app"

# Builds used to run from release/ inside the project and found the project's models/ (Qwen3, M2M100) next to it.
# Installed in /Applications they would not: if no models folder is configured, point the setting at this one.
SETTINGS="$HOME/Library/Application Support/com.sinkfish.ocrtrans/settings.json"
if [ -f "$SETTINGS" ] && [ -d "$PWD/models" ]; then
  python3 - "$SETTINGS" "$PWD/models" <<'PY'
import json, os, sys, tempfile
path, models = sys.argv[1], sys.argv[2]
try:
    with open(path) as fh:
        s = json.load(fh)
    if not isinstance(s, dict):
        raise ValueError("expected a settings object")
except (OSError, ValueError) as e:
    print(f"settings left unchanged: {e}", file=sys.stderr)
    sys.exit(0)
if not s.get("models_dir"):
    s["models_dir"] = models
    fd, tmp = tempfile.mkstemp(prefix=".settings-install-", dir=os.path.dirname(path))
    try:
        with os.fdopen(fd, "w") as fh:
            json.dump(s, fh, ensure_ascii=False, indent=2)
            fh.flush()
            os.fsync(fh.fileno())
        os.replace(tmp, path)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)
    print(f"models folder set to {models} (设置 → 常规 → 自定义模型文件夹)")
PY
fi

# forget stale copies (build outputs) so nothing else gets launched or listed in the privacy settings
for stale in "$PWD/release/$NAME" "$PWD/target/release/bundle/macos/$NAME" "$PWD/target/debug/bundle/macos/$NAME"; do
  [ -e "$stale" ] && "$LSREG" -u "$stale" 2>/dev/null || true
done
"$LSREG" -f "$DEST"
# An already loaded login agent can retain launch constraints for the previous
# executable. Reload it after the swap so the installed app starts under its
# own service (Tahoe also uses the launching app to attribute menu bar items).
login_plist="$HOME/Library/LaunchAgents/OCR Trans.plist"
gui_domain="gui/$(id -u)"
login_service="$gui_domain/OCR Trans"
started_by_agent=false
if [ -f "$login_plist" ] &&
   [ "$(/usr/libexec/PlistBuddy -c 'Print :ProgramArguments:0' "$login_plist" 2>/dev/null)" = "$DEST/Contents/MacOS/ocr-trans" ] &&
   [ "$(/usr/libexec/PlistBuddy -c 'Print :Label' "$login_plist" 2>/dev/null)" = "OCR Trans" ] &&
   [ "$(/usr/libexec/PlistBuddy -c 'Print :RunAtLoad' "$login_plist" 2>/dev/null)" = true ] &&
   launchctl print "$login_service" >/dev/null 2>&1; then
  if launchctl bootout "$login_service" && launchctl bootstrap "$gui_domain" "$login_plist"; then
    for _ in $(seq 1 20); do
      if lsof -t -a -c ocr-trans "$(dirname "$SETTINGS")/instance.lock" >/dev/null 2>&1; then
        started_by_agent=true
        break
      fi
      sleep 0.25
    done
  fi
fi
if [ "$started_by_agent" = false ]; then open "$DEST"; fi
echo "installed and launched: $DEST ($(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$DEST/Contents/Info.plist"))"
