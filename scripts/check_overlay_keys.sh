#!/usr/bin/env bash
# Regression check (macOS, real app): the selection overlay never holds Enter / Esc / 1–4 as system-wide hotkeys,
# and no way out of it (Esc, a failing capture, the hotkey pressed again, a selection clipped to nothing, a finished
# selection, another app taking the focus, many quick cycles) leaves the overlay or a key behind. Runs the app's
# built-in self-test (src-tauri/src/selftest.rs) on a test image — no real screenshot is taken — and reads the
# result from the log. The overlay flashes on screen for a few seconds.
#
#   ./scripts/check_overlay_keys.sh                       # the installed /Applications/OCR Trans.app
#   ./scripts/check_overlay_keys.sh "path/to/OCR Trans.app"
#
# A running copy of the app is quit first (SIGTERM) and started again afterwards.
set -euo pipefail
cd "$(dirname "$0")/.."
APP=${1:-/Applications/OCR Trans.app}
LOG="$HOME/Library/Logs/com.sinkfish.ocrtrans/ocr-trans.log"
SHOT="$PWD/testdata/screen_full_zh.png"
[ -d "$APP" ] || { echo "no app at $APP" >&2; exit 2; }
APP=$(cd "$APP" && pwd)
RESTORE_APPS=()
OPEN_PID=""

# Remember each original bundle, not just whether something was running: the test may use a different build.
restore() {
  if [ -n "$OPEN_PID" ] && kill -0 "$OPEN_PID" 2>/dev/null; then
    while IFS= read -r pid; do
      [ -z "$pid" ] || kill -TERM "$pid" 2>/dev/null || true
    done < <(pgrep -f "/OCR Trans.app/Contents/MacOS/ocr-trans" | while read -r pid; do
      command=$(ps -p "$pid" -o comm=)
      [ "$command" != "$APP/Contents/MacOS/ocr-trans" ] || echo "$pid"
    done)
  fi
  for bundle in ${RESTORE_APPS[@]+"${RESTORE_APPS[@]}"}; do open "$bundle"; done
}
trap restore EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

was_running=$(pgrep -f "/OCR Trans.app/Contents/MacOS/ocr-trans" || true)
if [ -n "$was_running" ]; then
  for pid in $was_running; do
    command=$(ps -p "$pid" -o comm=)
    RESTORE_APPS+=("${command%/Contents/MacOS/ocr-trans}")
  done
  kill -TERM $was_running 2>/dev/null || true
  for _ in $(seq 1 20); do pgrep -f "/OCR Trans.app/Contents/MacOS/ocr-trans" >/dev/null || break; sleep 0.5; done
  if pgrep -f "/OCR Trans.app/Contents/MacOS/ocr-trans" >/dev/null; then
    echo "the original app did not quit; self-test cancelled" >&2; exit 1
  fi
fi

mkdir -p "$(dirname "$LOG")"
[ -f "$LOG" ] || touch "$LOG"
START=$(wc -l < "$LOG")
# launched through Launch Services (not as a child of this shell) so the app is its own responsible process for
# the privacy settings, exactly like a normal start
open -n -W "$APP" --env OCRTRANS_TEST_OVERLAY_KEYS=1 --env OCRTRANS_TEST_SHOT="$SHOT" &
OPEN_PID=$!
for _ in $(seq 1 120); do
  tail -n +"$((START + 1))" "$LOG" | grep -q "keytest result:" && break
  sleep 0.5
done
# the app quits by itself after the result; if it hangs (or the self-test died), stop it instead of waiting forever
for _ in $(seq 1 20); do kill -0 $OPEN_PID 2>/dev/null || break; sleep 0.5; done
if kill -0 $OPEN_PID 2>/dev/null; then
  echo "the app did not quit after the self-test — stopping it" >&2
  test_pids=""
  for pid in $(pgrep -f "/OCR Trans.app/Contents/MacOS/ocr-trans" || true); do
    [ "$(ps -p "$pid" -o comm=)" != "$APP/Contents/MacOS/ocr-trans" ] || test_pids="$test_pids $pid"
  done
  [ -z "$test_pids" ] || kill -TERM $test_pids 2>/dev/null || true
  sleep 3
  for pid in $test_pids; do
    [ "$(ps -p "$pid" -o comm= || true)" != "$APP/Contents/MacOS/ocr-trans" ] || kill -KILL "$pid" 2>/dev/null || true
  done
fi
wait $OPEN_PID 2>/dev/null || true
OUT=$(tail -n +"$((START + 1))" "$LOG" | grep -E "keytest|start [0-9]|error:" || true)
echo "$OUT"
echo "$OUT" | grep -q "keytest result:" || echo "no self-test result in the log" >&2

echo "$OUT" | grep -q "keytest result: PASS"
