#!/usr/bin/env bash
# Exercise the production React bundle in real macOS WebKit with a deterministic Tauri bridge.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=$(mktemp -d)
SRV=""
trap '[ -z "$SRV" ] || kill "$SRV" 2>/dev/null || true; rm -rf "$OUT"' EXIT
swiftc -O scripts/dev/webkit_snap.swift -o "$OUT/snap" 2>/dev/null
PORT=$((20000 + RANDOM % 20000))
(cd dist && exec python3 -m http.server "$PORT" --bind 127.0.0.1 >/dev/null 2>&1) &
SRV=$!
sleep 1
for scenario in settings pin onboarding history models about; do
  view=$scenario
  tab=general
  case "$scenario" in models|about) view=settings; tab=$scenario;; esac
  FIT=1 "$OUT/snap" "http://127.0.0.1:$PORT/index.html?view=$view&tab=$tab&case=$scenario&id=1&translate=1" "$OUT/$scenario.png" 900 750 scripts/dev/frontend_behavior_mock.js 3 > "$OUT/$scenario.log"
done
python3 - "$OUT" <<'PY'
import json, pathlib, sys
for scenario in ("settings", "pin", "onboarding", "history", "models", "about"):
    lines = (pathlib.Path(sys.argv[1]) / f"{scenario}.log").read_text().splitlines()
    report = json.loads(next(line[7:] for line in lines if line.startswith("report ")))
    print(json.dumps(report, ensure_ascii=False))
    if not report.get("passed"):
        sys.exit(1)
PY
