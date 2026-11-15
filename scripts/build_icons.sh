#!/usr/bin/env bash
# Regenerate all app / tray icons from the brand SVGs in docs/brand/.
# Needs `resvg` (cargo install resvg) and uv (for Pillow, to pack the .ico).
set -euo pipefail
cd "$(dirname "$0")/.."
B=docs/brand
OUT=src-tauri/icons
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
MAIN="$B/logo-primary.svg"
SMALL="$B/logo-small.svg"
TRAY="$B/tray-template.svg"
resvg -w 1024 -h 1024 "$MAIN" "$T/app-1024.png"
cp "$T/app-1024.png" "$OUT/app-icon.png"
# icns + all Tauri PNG sizes (+ Windows Square*Logo) from the 1024 master
pnpm -s tauri icon "$T/app-1024.png" -o "$OUT" >/dev/null
rm -rf "$OUT/android" "$OUT/ios"
# small sizes use the heavier full-bleed cut so they stay legible
resvg -w 32 -h 32 "$SMALL" "$OUT/32x32.png"
for s in 16 24 32 48; do resvg -w $s -h $s "$SMALL" "$T/s$s.png"; done
for s in 64 128 256; do resvg -w $s -h $s "$MAIN" "$T/s$s.png"; done
uv run --no-project --python 3.12 --with pillow python - "$T" "$OUT/icon.ico" <<'PY'
import sys
from PIL import Image
t, out = sys.argv[1], sys.argv[2]
imgs = [Image.open(f"{t}/s{s}.png").convert("RGBA") for s in (16, 24, 32, 48, 64, 128, 256)]
imgs[-1].save(out, format="ICO", sizes=[im.size for im in imgs], append_images=imgs[:-1])
PY
# macOS menu-bar template (black + alpha), 22 pt @2x
resvg -w 44 -h 44 "$TRAY" "$OUT/tray.png"
echo "icons written to $OUT"
