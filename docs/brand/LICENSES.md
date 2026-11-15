# Brand artwork — sources and licences

**In use since 0.5.1:** the app logo (icons, tray template, in-app mark) uses option **1** — 文 from
**Source Han Sans CN Bold** (思源黑体), © 2014–2025 Adobe, licensed under the SIL Open Font License 1.1
(<https://openfontlicense.org>), Reserved Font Name “Source”. The glyph is a converted outline; no font file ships.
The same attribution appears in the app under 设置 → 关于.

All logo artwork is original vector work. For the “文” glyph options 1–5 (docs/brand/glyphs/, see
glyph-options.html), the outline of the single character 文 (U+6587) was taken from the fonts below and converted
to plain SVG paths by `scripts/build_glyph_options.py`. **No font file is included in the app or the repository.**

All fonts used are licensed under the **SIL Open Font License 1.1**, which permits using and modifying glyph
outlines in artwork, including commercial logos (the OFL governs the font software itself; artwork made with it
is not restricted). Each font's embedded licence string (name ID 13) was checked before use.

| option | font | version | official source (downloaded 2026-10-27) | licence |
|---|---|---|---|---|
| 1 | Source Han Sans CN **Bold** (思源黑体) | 2.005 | https://github.com/adobe-fonts/source-han-sans/releases/tag/2.005R (`19_SourceHanSansCN.zip`) | SIL OFL 1.1 — © Adobe, Reserved Font Name “Source” |
| 2 | Source Han Sans CN **Medium** (思源黑体) | 2.005 | same as above | SIL OFL 1.1 |
| 3 | Source Han Serif CN **SemiBold** (思源宋体) | 2.003 | https://github.com/adobe-fonts/source-han-serif/releases/tag/2.003R (`14_SourceHanSerifCN.zip`) | SIL OFL 1.1 — © Adobe, Reserved Font Name “Source” |
| 4 | LXGW WenKai **Medium** (霞鹜文楷) | 1.530 | https://github.com/lxgw/LxgwWenKai/releases/tag/v1.530 (`LXGWWenKai-Medium.ttf`) | SIL OFL 1.1 |
| 5 | Smiley Sans Oblique (得意黑) | 2.0.1 | https://github.com/atelier-anchor/smiley-sans/releases/tag/v2.0.1 (`smiley-sans-v2.0.1.zip`) | SIL OFL 1.1 |
| 6 | — hand-drawn geometric strokes | — | original | © SinkFish, subject to the project licence and its brand terms |

Not used: PingFang, Hiragino, STHeiti or any other system / commercial font.

The Reserved Font Name clause only restricts naming *modified fonts*; it does not apply to a logo, and the
project does not distribute a font.

To regenerate the options, download the fonts above into a scratch folder (never into the app) and run:

```bash
uv run --no-project --python 3.12 --with fonttools --with pillow python scripts/build_glyph_options.py /path/to/fonts
```
