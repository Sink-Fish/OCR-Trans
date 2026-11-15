#!/usr/bin/env bash
# Regenerate all app / tray icons from the brand SVGs in docs/brand/.
#   ./scripts/build_icons.sh                 # primary logo
#   ./scripts/build_icons.sh logo-alt-a      # use an alternative tile (small sizes keep the primary cut)
#   ./scripts/build_icons.sh option-3        # use “文” glyph option 3 from docs/brand/glyphs/ (tile, small cut, tray)
# Needs `resvg` (cargo install resvg) and uv (for Pillow, to pack the .ico).
set -euo pipefail
cd "$(dirname "$0")/.."
LOGO="${1:-logo-primary}"
B=docs/brand
OUT=src-tauri/icons
T=$(mktemp -d)
MAIN="$B/$LOGO.svg"
SMALL="$B/logo-small.svg"
TRAY="$B/tray-template.svg"
if [ -f "$B/glyphs/$LOGO.svg" ]; then
  MAIN="$B/glyphs/$LOGO.svg"
  SMALL="$B/glyphs/$LOGO-small.svg"
  TRAY="$B/glyphs/$LOGO-tray.svg"
fi
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
# brand PNG exports
mkdir -p "$B/png"
for f in logo-primary logo-alt-a logo-alt-b; do for s in 1024 256 64; do resvg -w $s -h $s "$B/$f.svg" "$B/png/$f-$s.png"; done; done
for s in 16 32; do resvg -w $s -h $s "$B/logo-small.svg" "$B/png/logo-small-$s.png"; done
resvg -w 44 -h 44 "$B/tray-template.svg" "$B/png/tray-template-44.png"
rm -rf "$T"
echo "icons written to $OUT"
