#!/usr/bin/env bash
# Regression check (macOS): the selection outline must be visible on all four sides — at 1x and 2x, for short,
# narrow, tiny and full-screen selections and ones touching the screen edges. Renders the real overlay page (dist/)
# in WebKit with a fake Tauri bridge (scripts/dev/overlay_mock.js) and measures the mint pixels along each edge.
#
#   ./scripts/check_overlay_frame.sh            # uses the existing dist/ (run `pnpm build` first after CSS changes)
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=$(mktemp -d)
SRV=""
trap '[ -z "$SRV" ] || kill "$SRV" 2>/dev/null || true; rm -rf "$OUT"' EXIT
swiftc -O scripts/dev/webkit_snap.swift -o "$OUT/snap" 2>/dev/null
PORT=$((20000 + RANDOM % 20000))
(cd dist && exec python3 -m http.server $PORT --bind 127.0.0.1 >/dev/null 2>&1) &
SRV=$!
sleep 1
W=900 H=600
CASES="normal:300,200,700,450 short:300,200,700,226 narrow:300,100,322,450 tiny:300,200,316,214 full:0,0,$W,$H topleft:0,0,400,300 bottomright:500,300,$W,$H"
for dpr in 1 2; do
  for c in $CASES; do
    name=${c%%:*} sel=${c#*:}
    DPR=$dpr "$OUT/snap" "http://127.0.0.1:$PORT/index.html?view=overlay&sel=$sel" "$OUT/$name@$dpr.png" $W $H scripts/dev/overlay_mock.js 1.2 >/dev/null
    echo "$name $dpr $sel $OUT/$name@$dpr.png"
  done
done > "$OUT/cases.txt"
uv run -q --no-project --python 3.12 --with pillow python - "$OUT/cases.txt" <<'PY'
import sys
from PIL import Image
mint = lambda p: p[1] > p[0] + 40 and p[1] > 150
bad = 0
for line in open(sys.argv[1]):
    name, dpr, sel, path = line.split()
    d = int(dpr); x0, y0, x1, y1 = [int(v) * d for v in sel.split(",")]
    im = Image.open(path).convert("RGB")
    x1, y1 = min(x1, im.width), min(y1, im.height)
    edges = {
        "top": [im.getpixel((x, y0)) for x in range(x0, x1)],
        "bottom": [im.getpixel((x, y1 - 1)) for x in range(x0, x1)],
        "left": [im.getpixel((x0, y)) for y in range(y0, y1)],
        "right": [im.getpixel((x1 - 1, y)) for y in range(y0, y1)],
    }
    cov = {k: sum(map(mint, v)) / len(v) for k, v in edges.items()}
    ok = all(c >= 0.95 for c in cov.values())
    bad += not ok
    print(f"{'ok ' if ok else 'BAD'} {name:12} @{d}x  " + "  ".join(f"{k} {100 * c:5.1f}%" for k, c in cov.items()))
sys.exit(1 if bad else 0)
PY
