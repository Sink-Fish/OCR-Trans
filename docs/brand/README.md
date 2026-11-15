# OCR Trans brand

Calm and simple: neutral greys, one ink-blue accent (`#3456A6`, `#7F98DA` in dark mode), system fonts only.
Logo 中的「文」采用已转曲的思源黑体字形，来源与许可见 [LICENSES.md](LICENSES.md)。

| file | what |
|---|---|
| `logo-primary.svg` | **current app icon** (0.5.1) — mint selection corner brackets framing 文 from Source Han Sans Bold (OFL, outlined), on the macOS 824/1024 icon grid |
| `logo-v050-geometric.svg`, `logo-v040-blue.svg` | earlier logos, kept for comparison |
| `glyphs/`, `glyph-options.html` | the 文 options 1–6, 1b, 1c the logo was chosen from (option 1) |
| `LICENSES.md` | font sources and licences |
| `logo-alt-a.svg` | alternative A — "A → 文" |
| `logo-alt-b.svg` | alternative B — brackets around three text lines, the last one ink blue |
| `logo-small.svg` | 16–32 px cut of the primary mark (full-bleed tile, heavier strokes) used for small PNG/ICO sizes |
| `logo-mark.svg` | the mark without the tile (in-app headers; `src/lib/Logo.tsx` inlines the same paths) |
| `tray-template.svg` | macOS menu-bar template icon (black + alpha, tinted by the system) |
| `png/` | rendered PNGs |

Design tokens (colours, type, radius 8 px, 1 px borders, shadows, 150 ms motion) live in `src/theme.css`.

Switch the app icon to another variant and regenerate every asset (.icns, .ico, all Tauri PNGs, tray icon):

```bash
./scripts/build_icons.sh logo-alt-a     # or logo-alt-b, or no argument for the primary logo
```

Requires resvg and uv.
