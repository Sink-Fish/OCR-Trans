#!/usr/bin/env bash
# Regression check (macOS): a pin's frame must be visible, and the same, on all four sides — at 1x and 2x, on white
# and light-green captures, with the toolbar below, above or inside the image, for a short text line and an image
# wider than the toolbar — and must never be drawn over what sits on the image (the toolbar when it is inside, the
# 翻译中… pill). Renders the real pin page (dist/) in WebKit with a fake Tauri bridge (scripts/dev/pin_mock.js),
# resizes the window to the size the page asks for (so whatever the real pin window clips is clipped here too) and
# measures each edge of the image: the mint line just inside it and the dark hairline just outside it.
#
#   ./scripts/check_pin_frame.sh [dist dir]    # default dist/ (run `pnpm build` first after CSS changes)
#   KEEP=<dir> ./scripts/check_pin_frame.sh    # also keep the screenshots
set -euo pipefail
cd "$(dirname "$0")/.."
DIST=$(cd "${1:-dist}" && pwd)
OUT=$(mktemp -d)
SRV=""
trap '[ -z "$SRV" ] || kill "$SRV" 2>/dev/null || true; rm -rf "$OUT"' EXIT
swiftc -O scripts/dev/webkit_snap.swift -o "$OUT/snap" 2>/dev/null
PORT=$((20000 + RANDOM % 20000))
(cd "$DIST" && exec python3 -m http.server $PORT --bind 127.0.0.1 >/dev/null 2>&1) &
SRV=$!
sleep 1
# name:image:desktop colour behind the window:image size:where it was captured (CSS px, mock screen 1440x900, menu
# bar 25):extra query
CASES="white:white:#ffffff:320,60:400,300:
mint:mint:#dff3e7:320,60:400,300:
above:white:#ffffff:320,60:400,820:
short:mint:#dff3e7:120,26:400,300:
wide:white:#ffffff:600,80:400,300:
inside:mint:#dff3e7:200,860:300,30:
busy:mint:#dff3e7:240,26:400,300:&translate=1&busy=1"
for dpr in 1 2; do
  while IFS=: read -r name img bg size at extra; do
    png="$OUT/$name@${dpr}x.png"
    q="view=pin&id=1&img=$img&bg=${bg/\#/%23}&size=$size&at=$at${extra:-&translate=0}"
    # (|| true: a page that never sizes its window makes webkit_snap exit 1; that is reported below, per case)
    out=$(DPR=$dpr FIT=1 "$OUT/snap" "http://127.0.0.1:$PORT/index.html?$q" "$png" "${size%,*}" "${size#*,}" scripts/dev/pin_mock.js 1.2 2>&1 || true)
    report=$(sed -n 's/^report //p' <<<"$out")
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$dpr" "$bg" "$png" "${report:-}" "$(tr '\n\t' '  ' <<<"$out")"
  done <<<"$CASES"
done > "$OUT/cases.tsv"
[ -n "${KEEP:-}" ] && mkdir -p "$KEEP" && cp "$OUT"/*.png "$KEEP"/
uv run -q --no-project --python 3.12 --with pillow python - "$OUT/cases.tsv" <<'PY'
import json, sys
from PIL import Image

mint = lambda p: p[1] > p[0] + 40 and p[1] > 150
lum = lambda p: 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]


def in_rounded(x, y, rect, r, inset=0.0):
    """Whether pixel (x, y) is inside the rounded rectangle `rect` (device px) shrunk by `inset`."""
    x0, y0, w, h = rect
    x0, y0, x1, y1 = x0 + inset, y0 + inset, x0 + w - inset, y0 + h - inset
    cx, cy = x + 0.5, y + 0.5
    if not (x0 <= cx <= x1 and y0 <= cy <= y1):
        return False
    r = max(0.0, min(r - inset, (x1 - x0) / 2, (y1 - y0) / 2))
    qx = min(max(cx, x0 + r), x1 - r)
    qy = min(max(cy, y0 + r), y1 - r)
    return (cx - qx) ** 2 + (cy - qy) ** 2 <= r * r


bad = 0
for line in open(sys.argv[1]):
    name, dpr, bg, path, report, out = line.rstrip("\n").split("\t")
    d = int(dpr)
    if not report:
        print(f"BAD {name:6} @{d}x  the page did not lay its window out (webkit_snap: {out.strip() or 'no output'})"); bad += 1; continue
    r = json.loads(report)
    im = Image.open(path).convert("RGB")
    W, H = im.size
    bgc = tuple(int(bg[i:i + 2], 16) for i in (1, 3, 5))
    px = lambda x, y: im.getpixel((x, y)) if 0 <= x < W and 0 <= y < H else None  # None: clipped by the window
    left, top, w, h = (round(v * d) for v in r["stage"])
    x0, y0, x1, y1 = left, top, left + w, top + h
    covers = [([v * d for v in c["rect"]], c["radius"] * d) for c in r["covers"]]
    # what sits on the image: near it (its shadow, rounded corners) the frame is not measured; well inside it the
    # frame must not show
    near = lambda x, y: any(in_rounded(x, y, rc, 0, -6 * d) for rc, _ in covers)
    under = lambda x, y: any(in_rounded(x, y, rc, rad, 2 * d) for rc, rad in covers)
    skip = 8 * d  # the image's rounded corners (6 px radius)
    # per edge: (positions along it, pixel position at depth k: k = 1 is the image's outermost pixel row / column,
    # k = 0 the first one outside it)
    edges = {
        "top": (range(x0 + skip, x1 - skip), lambda t, k: (t, y0 + k - 1)),
        "bottom": (range(x0 + skip, x1 - skip), lambda t, k: (t, y1 - k)),
        "left": (range(y0 + skip, y1 - skip), lambda t, k: (x0 + k - 1, t)),
        "right": (range(y0 + skip, y1 - skip), lambda t, k: (x1 - k, t)),
    }
    res, over = {}, [0, 0]
    for side, (along, at) in edges.items():
        free = [t for t in along if not near(*at(t, 1))]
        for t in along:
            if under(*at(t, 1)):
                over[0] += 1
                over[1] += any((p := px(*at(t, k))) is not None and mint(p) for k in range(1, 2 * d + 1))
        # mint line: within 3 CSS px of the edge, inside or outside the image
        line_ok = sum(any((p := px(*at(t, k))) is not None and mint(p) for k in range(-3 * d + 1, 3 * d + 1)) for t in free)
        # hairline: the first pixel outside the image exists (not clipped), is not mint and is darker than the desktop
        halo_ok = sum((p := px(*at(t, 0))) is not None and not mint(p) and lum(bgc) - lum(p) >= 20 for t in free)
        # thickness of the mint line in the middle of the edge's free part
        mid = free[len(free) // 2] if free else None
        thick = sum(1 for k in range(-3 * d + 1, 3 * d + 1) if mid is not None and (p := px(*at(mid, k))) is not None and mint(p))
        n = len(free) or 1
        res[side] = (line_ok / n if free else 1, halo_ok / n if free else 1, thick if free else None)
    thicks = [v[2] for v in res.values() if v[2] is not None]
    over_frac = over[1] / over[0] if over[0] else 0
    ok = (all(a >= 0.95 and b >= 0.95 for a, b, _ in res.values()) and thicks and max(thicks) - min(thicks) <= 1
          and min(thicks) >= 2 and over_frac <= 0.05)
    bad += not ok
    print(f"{'ok ' if ok else 'BAD'} {name:6} @{d}x  window {W}x{H}  "
          + "  ".join(f"{k} line {100 * a:3.0f}% hairline {100 * b:3.0f}% {'-' if t is None else t}px" for k, (a, b, t) in res.items())
          + (f"  frame over toolbar/pill {100 * over_frac:3.0f}% of {over[0]} px" if over[0] else ""))
sys.exit(1 if bad else 0)
PY
