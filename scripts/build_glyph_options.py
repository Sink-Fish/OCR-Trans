"""Generate the numbered “文” glyph options for the logo (docs/brand/glyphs/) and glyph-options.html.

Each option keeps the v0.5 mint frame (tile, corner brackets, colours) and only swaps the glyph. Font-based
options are converted to plain SVG outlines here, so no font file is ever shipped or needed at runtime.

    uv run --no-project --python 3.12 --with fonttools --with pillow python scripts/build_glyph_options.py <fonts dir>

The fonts dir is a scratch folder with the downloaded OFL fonts (see docs/brand/LICENSES.md).
Rendering uses `resvg`.
"""
import base64
import subprocess
import sys
import tempfile
from pathlib import Path

from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parent.parent
BRAND = ROOT / "docs" / "brand"
OUT = BRAND / "glyphs"
FONTS = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("fonts")

MINT = "#4FB98C"
MINT_SMALL = "#43A67C"
INK = "#1F4D3B"
WEN = 0x6587

# (number, title, font file, optical box size on the 1024 grid, licence note)
FONT_OPTIONS = [
    (1, "思源黑体 Bold · Source Han Sans", "sans/SubsetOTF/CN/SourceHanSansCN-Bold.otf", 372),
    (2, "思源黑体 Medium · Source Han Sans", "sans/SubsetOTF/CN/SourceHanSansCN-Medium.otf", 380),
    (3, "思源宋体 SemiBold · Source Han Serif", "serif/SubsetOTF/CN/SourceHanSerifCN-SemiBold.otf", 392),
    (4, "霞鹜文楷 Medium · LXGW WenKai", "LXGWWenKai-Medium.ttf", 396),
    (5, "得意黑 · Smiley Sans", "smiley/SmileySans-Oblique.ttf", 400),
]
GEOMETRIC = (6, "几何圆角“文” · 手工绘制", None, 0)

BRACKETS_1024 = "M262 392V320a58 58 0 0 1 58-58h72M632 262h72a58 58 0 0 1 58 58v72M762 632v72a58 58 0 0 1-58 58h-72M392 762h-72a58 58 0 0 1-58-58v-72"
BRACKETS_32 = "M5 10.5V7.5a2.5 2.5 0 0 1 2.5-2.5h3M21.5 5h3A2.5 2.5 0 0 1 27 7.5v3M27 21.5v3a2.5 2.5 0 0 1-2.5 2.5h-3M10.5 27h-3A2.5 2.5 0 0 1 5 24.5v-3"
BRACKETS_1024_NO_BL = "M262 392V320a58 58 0 0 1 58-58h72M632 262h72a58 58 0 0 1 58 58v72M762 632v72a58 58 0 0 1-58 58h-72"
BRACKETS_32_NO_BL = "M5 10.5V7.5a2.5 2.5 0 0 1 2.5-2.5h3M21.5 5h3A2.5 2.5 0 0 1 27 7.5v3M27 21.5v3a2.5 2.5 0 0 1-2.5 2.5h-3"
BRACKETS_22_NO_BL = "M2.6 7V4.8a2.2 2.2 0 0 1 2.2-2.2H7M15 2.6h2.2a2.2 2.2 0 0 1 2.2 2.2V7M19.4 15v2.2a2.2 2.2 0 0 1-2.2 2.2H15"
BRACKETS_22 = "M2.6 7V4.8a2.2 2.2 0 0 1 2.2-2.2H7M15 2.6h2.2a2.2 2.2 0 0 1 2.2 2.2V7M19.4 15v2.2a2.2 2.2 0 0 1-2.2 2.2H15M7 19.4H4.8a2.2 2.2 0 0 1-2.2-2.2V15"

# hand-tuned geometric 文 (option 6): slanted dot, long bar, asymmetric 撇 / 捺 crossing low, round ends
GEO_STROKES_1024 = [
    "M500 312l22 46",
    "M372 414h280",
    "M606 414C590 540 520 640 380 716",
    "M430 470C470 590 548 668 664 712",
]


def glyph_outline(font_path: Path):
    font = TTFont(font_path)
    gs = font.getGlyphSet()
    name = font.getBestCmap()[WEN]
    bp = BoundsPen(gs)
    gs[name].draw(bp)
    return font, gs, name, bp.bounds  # bounds in font units (y up)


def placed_path(gs, name, bounds, box: float, cx: float, cy: float) -> str:
    """Path of the glyph scaled so its larger side is `box`, centred on (cx, cy) in a y-down SVG space."""
    x0, y0, x1, y1 = bounds
    s = box / max(x1 - x0, y1 - y0)
    tx = cx - s * (x0 + x1) / 2
    ty = cy + s * (y0 + y1) / 2  # flip y
    sp = SVGPathPen(gs, ntos=lambda v: f"{v:.1f}".rstrip("0").rstrip("."))
    gs[name].draw(TransformPen(sp, (s, 0, 0, -s, tx, ty)))
    return sp.getCommands()


def tile_1024(glyph_svg: str, n, title: str, brackets: str = BRACKETS_1024) -> str:
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
  <!-- OCR Trans logo, glyph option {n}: {title}. Glyph converted to outlines; no font is shipped. -->
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#FAFDFB"/><stop offset="1" stop-color="#E4F3EA"/></linearGradient>
    <filter id="shadow" x="-10%" y="-10%" width="120%" height="125%"><feDropShadow dx="0" dy="12" stdDeviation="14" flood-color="#12301F" flood-opacity="0.15"/></filter>
  </defs>
  <rect x="100" y="100" width="824" height="824" rx="184" fill="url(#bg)" filter="url(#shadow)"/>
  <rect x="100.5" y="100.5" width="823" height="823" rx="183.5" fill="none" stroke="#12301F" stroke-opacity="0.07"/>
  <path d="{brackets}" fill="none" stroke="{MINT}" stroke-width="44" stroke-linecap="round" stroke-linejoin="round"/>
  {glyph_svg}
</svg>
"""


def tile_32(glyph_svg: str, n, brackets: str = BRACKETS_32) -> str:
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32" width="32" height="32">
  <!-- 16–32 px cut, glyph option {n} -->
  <rect x="0.5" y="0.5" width="31" height="31" rx="8" fill="#F1FAF5" stroke="#12301F" stroke-opacity="0.12"/>
  <path d="{brackets}" fill="none" stroke="{MINT_SMALL}" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/>
  {glyph_svg}
</svg>
"""


def tray_22(glyph_svg: str, n, brackets: str = BRACKETS_22) -> str:
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 22 22" width="22" height="22">
  <!-- macOS menu-bar template, glyph option {n}: black + alpha only -->
  <path d="{brackets}" fill="none" stroke="#000" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/>
  {glyph_svg}
</svg>
"""


def geo_glyph(scale: float, offset: float, width: float, color: str) -> str:
    """Option 6 strokes, transformed from the 1024 grid into a smaller viewBox."""
    t = f'transform="translate({offset} {offset}) scale({scale})"' if scale != 1 else ""
    return f'<g {t} fill="none" stroke="{color}" stroke-width="{width}" stroke-linecap="round" stroke-linejoin="round"><path d="{" ".join(GEO_STROKES_1024)}"/></g>'


def recorded(gs, name):
    from fontTools.pens.recordingPen import RecordingPen

    rp = RecordingPen()
    gs[name].draw(rp)
    return rp.value


def path_from_recording(cmds, s, tx, ty) -> str:
    """SVG path from recorded pen commands with the same transform as placed_path."""
    f = lambda p: f"{s * p[0] + tx:.1f} {ty - s * p[1]:.1f}"
    out = []
    for op, args in cmds:
        if op == "moveTo":
            out.append("M" + f(args[0]))
        elif op == "lineTo":
            out.append("L" + f(args[0]))
        elif op == "curveTo":
            out.append("C" + " ".join(f(a) for a in args))
        elif op == "qCurveTo":
            out.append("Q" + " ".join(f(a) for a in args))
        elif op in ("closePath", "endPath"):
            out.append("Z")
    return "".join(out)


def transform_for(bounds, box, cx, cy):
    x0, y0, x1, y1 = bounds
    s = box / max(x1 - x0, y1 - y0)
    return s, cx - s * (x0 + x1) / 2, cy + s * (y0 + y1) / 2


def without_dot(cmds):
    """Source Han Sans 文 is one outline with the dot merged into the bar (bar top at y=681): drop the dot's
    segments so the outline runs straight along the bar's top edge."""
    first_close = next(i for i, (op, _) in enumerate(cmds) if op == "closePath")
    outer, rest = cmds[: first_close + 1], cmds[first_close + 1 :]
    # outer: M(540,860) L(412,822) C(..→469,681) | L(44,681) … L(960,681) | L(524,681) L(609,708) C(…→540,860) Z
    start = next(i for i, (op, a) in enumerate(outer) if op == "lineTo" and a[0] == (44, 681))
    end = next(i for i, (op, a) in enumerate(outer) if op == "lineTo" and a[0] == (960, 681))
    body = [("moveTo", ((469, 681),))] + outer[start : end + 1] + [("closePath", ())]
    return body + rest


DOT_CENTER = (510, 770)  # font units: centre of the removed dot
TAIL_MID = (61.5, -31.5)  # font units: midpoint of the 撇 tail's end edge (25,25)–(98,-88)
TAIL_DIR = (157, 39)  # tangent pointing back into the stroke (font units, y up)


def handle_svg(s, tx, ty, r, ring, small=False):
    cx, cy = s * DOT_CENTER[0] + tx, ty - s * DOT_CENTER[1]
    if small:
        return f'<circle cx="{cx:.2f}" cy="{cy:.2f}" r="{r:.2f}" fill="{MINT_SMALL}"/>'
    return f'<circle cx="{cx:.1f}" cy="{cy:.1f}" r="{r:.1f}" fill="#FFFFFF" stroke="{MINT}" stroke-width="{ring:.1f}"/>'


def flowing_bracket(s, tx, ty, x_arm, y_top, *_):
    """1c: the bottom-left bracket's arm comes down and sweeps right into the 撇 tail.

    The tail's own direction, extended backwards, meets the frame's left arm about two thirds of the way down,
    so the arm bends there (a quarter turn) and arrives on the tail's tangent: one continuous stroke.
    Drawn under the glyph so the join is hidden inside the tail."""
    mx, my = s * TAIL_MID[0] + tx, ty - s * TAIL_MID[1]
    n = (TAIL_DIR[0] ** 2 + TAIL_DIR[1] ** 2) ** 0.5
    dx, dy = TAIL_DIR[0] / n, -TAIL_DIR[1] / n  # SVG direction pointing into the stroke
    span = mx - x_arm
    bend = my - 0.55 * span  # where the arm starts to turn
    c1 = (x_arm, my - 0.05 * span)
    c2 = (mx - dx * 0.55 * span, my - dy * 0.55 * span)
    end = (mx + dx * 0.25 * span, my + dy * 0.25 * span)  # tuck the end into the tail
    return f"M{x_arm:.2f} {y_top:.2f}V{bend:.2f}C{c1[0]:.2f} {c1[1]:.2f} {c2[0]:.2f} {c2[1]:.2f} {end[0]:.2f} {end[1]:.2f}"


def render(svg: Path, size: int) -> bytes:
    with tempfile.NamedTemporaryFile(suffix=".png") as f:
        subprocess.run(["resvg", "-w", str(size), "-h", str(size), str(svg), f.name], check=True)
        return Path(f.name).read_bytes()


def b64(data: bytes, mime: str) -> str:
    return f"data:{mime};base64," + base64.b64encode(data).decode()


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    options = []
    for n, title, file, box in FONT_OPTIONS:
        _, gs, name, bounds = glyph_outline(FONTS / file)
        big = placed_path(gs, name, bounds, box, 512, 516)
        small = placed_path(gs, name, bounds, box * 32 / 1024 * 1.06, 16, 16.1)  # a touch larger: small sizes need it
        tray = placed_path(gs, name, bounds, box * 22 / 1024 * 1.08, 11, 11.05)
        (OUT / f"option-{n}.svg").write_text(tile_1024(f'<path d="{big}" fill="{INK}"/>', n, title))
        (OUT / f"option-{n}-small.svg").write_text(tile_32(f'<path d="{small}" fill="{INK}"/>', n))
        (OUT / f"option-{n}-tray.svg").write_text(tray_22(f'<path d="{tray}" fill="#000"/>', n))
        options.append((n, title))
        if n == 1:
            cmds = recorded(gs, name)
            nodot = without_dot(cmds)
            T = {
                "big": transform_for(bounds, box, 512, 516),
                "small": transform_for(bounds, box * 32 / 1024 * 1.06, 16, 16.1),
                "tray": transform_for(bounds, box * 22 / 1024 * 1.08, 11, 11.05),
            }
            # 1b: the dot becomes a selection handle (same look as the overlay's handles)
            t1 = "思源黑体 Bold + 选区手柄作点"
            sb, sm, st = T["big"], T["small"], T["tray"]
            (OUT / "option-1b.svg").write_text(tile_1024(f'<path d="{path_from_recording(nodot, *sb)}" fill="{INK}"/>' + handle_svg(*sb, 38, 22), "1b", t1))
            (OUT / "option-1b-small.svg").write_text(tile_32(f'<path d="{path_from_recording(nodot, *sm)}" fill="{INK}"/>' + handle_svg(*sm, 2.3, 0, small=True), "1b"))
            (OUT / "option-1b-tray.svg").write_text(tray_22(f'<path d="{path_from_recording(nodot, *st)}" fill="#000"/>' + handle_svg(*st, 1.55, 0, small=True).replace(MINT_SMALL, "#000"), "1b"))
            options.append(("1b", t1))
            # 1c: the bottom-left bracket bends into the 撇 tail — frame and glyph become one mark
            t2 = "思源黑体 Bold + 角框连入撇"
            full = lambda tr: path_from_recording(cmds, *tr)
            fb = flowing_bracket(*sb, 262, 632, 700, 70)
            fs = flowing_bracket(*sm, 5, 21.5, 23.6, 2.6)
            ft = flowing_bracket(*st, 2.6, 15, 16.8, 1.9)
            (OUT / "option-1c.svg").write_text(
                tile_1024(f'<path d="{fb}" fill="none" stroke="{MINT}" stroke-width="44" stroke-linecap="round" stroke-linejoin="round"/><path d="{full(sb)}" fill="{INK}"/>', "1c", t2, BRACKETS_1024_NO_BL)
            )
            (OUT / "option-1c-small.svg").write_text(
                tile_32(f'<path d="{fs}" fill="none" stroke="{MINT_SMALL}" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/><path d="{full(sm)}" fill="{INK}"/>', "1c", BRACKETS_32_NO_BL)
            )
            (OUT / "option-1c-tray.svg").write_text(
                tray_22(f'<path d="{ft}" fill="none" stroke="#000" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/><path d="{full(st)}" fill="#000"/>', "1c", BRACKETS_22_NO_BL)
            )
            options.append(("1c", t2))
    n, title, _, _ = GEOMETRIC
    (OUT / f"option-{n}.svg").write_text(tile_1024(geo_glyph(1, 0, 54, INK), n, title))
    (OUT / f"option-{n}-small.svg").write_text(tile_32(geo_glyph(32 / 1024, 0, 54 * 1.08, INK), n))
    (OUT / f"option-{n}-tray.svg").write_text(tray_22(geo_glyph(22 / 1024, 0, 54 * 1.15, "#000"), n))
    options.append((n, title))

    # page
    cards = []
    for n, title in options:
        big = b64(render(OUT / f"option-{n}.svg", 512), "image/png")
        r128 = b64(render(OUT / f"option-{n}.svg", 128), "image/png")
        r32 = b64(render(OUT / f"option-{n}-small.svg", 32), "image/png")
        r16 = b64(render(OUT / f"option-{n}-small.svg", 16), "image/png")
        tray = b64((OUT / f"option-{n}-tray.svg").read_bytes(), "image/svg+xml")
        sizes = f"""
          <figure><img src="{r128}" width="128" height="128"><figcaption>128</figcaption></figure>
          <figure><img src="{r32}" width="32" height="32"><figcaption>32</figcaption></figure>
          <figure><img src="{r16}" width="16" height="16"><figcaption>16</figcaption></figure>
          <figure><img class="zoom" src="{r16}" width="64" height="64"><figcaption>16 放大</figcaption></figure>"""
        cards.append(f"""
      <section class="card">
        <div class="head"><span class="num">{n}</span><h3>{title}</h3></div>
        <div class="row">
          <img class="big" src="{big}" width="256" height="256" alt="option {n} at 512 px">
          <div class="side">
            <div class="bg light"><div class="sizes">{sizes}</div></div>
            <div class="bg dark"><div class="sizes">{sizes}</div></div>
            <div class="bars">
              <div class="menubar light"><span></span><img src="{tray}" width="18" height="18"><span class="clock">10:30</span></div>
              <div class="menubar dark"><span></span><img src="{tray}" width="18" height="18"><span class="clock">10:30</span></div>
            </div>
          </div>
        </div>
      </section>""")
    html = f"""<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>OCR Trans · “文”字形方案</title>
<!-- generated by scripts/build_glyph_options.py; images embedded so the file can be shared -->
<style>
  :root {{ --bg:#f6f8f3; --surface:#fbfcf9; --text:#1e2a24; --text-2:#5a6b61; --border:#dbe5d8; --mint:#6cc9a1; color-scheme: light dark;
          font-family: -apple-system, BlinkMacSystemFont, "PingFang SC", "Segoe UI", "Microsoft YaHei", sans-serif; }}
  @media (prefers-color-scheme: dark) {{ :root {{ --bg:#111814; --surface:#18211c; --text:#e6efe9; --text-2:#98a99f; --border:#2a3832; }} }}
  body {{ margin:0; background:var(--bg); color:var(--text); font-size:14px; line-height:1.55; }}
  main {{ max-width: 1100px; margin: 0 auto; padding: 36px 24px 72px; }}
  h1 {{ font-size: 24px; margin: 0 0 6px; }}
  .lead {{ color: var(--text-2); margin: 0 0 24px; max-width: 760px; }}
  .card {{ background: var(--surface); border: 1px solid var(--border); border-radius: 16px; padding: 20px 24px; margin-bottom: 18px;
           box-shadow: 0 2px 6px rgba(20,45,32,.05), 0 8px 24px rgba(20,45,32,.06); }}
  .head {{ display: flex; align-items: center; gap: 12px; margin-bottom: 12px; }}
  .head h3 {{ margin: 0; font-size: 16px; }}
  .num {{ width: 32px; height: 32px; border-radius: 50%; background: var(--mint); color: #0f3b2a; display: flex; align-items: center; justify-content: center; font-weight: 700; font-size: 16px; }}
  .row {{ display: flex; gap: 24px; align-items: center; flex-wrap: wrap; }}
  .big {{ flex: none; }}
  .side {{ flex: 1; min-width: 300px; display: flex; flex-direction: column; gap: 10px; }}
  .bg {{ border-radius: 12px; padding: 12px 16px; }}
  .bg.light {{ background: #ffffff; border: 1px solid #e6ede3; }}
  .bg.dark {{ background: #1b1d1f; }}
  .sizes {{ display: flex; align-items: flex-end; gap: 22px; }}
  .sizes figure {{ margin: 0; text-align: center; }}
  .sizes img {{ display: block; margin: 0 auto 4px; }}
  .sizes img.zoom {{ image-rendering: pixelated; }}
  figcaption {{ font-size: 11px; color: #7a8b7f; }}
  .bars {{ display: flex; gap: 10px; flex-wrap: wrap; }}
  .menubar {{ display: flex; align-items: center; gap: 14px; height: 26px; padding: 0 12px; border-radius: 8px; font-size: 12px; min-width: 200px; }}
  .menubar span:first-child {{ flex: 1; }}
  .menubar.light {{ background: rgba(236,236,236,.95); color: #111; }}
  .menubar.dark {{ background: #2a2a2d; color: #eee; }}
  .menubar.dark img {{ filter: invert(1); }}
  .note {{ color: var(--text-2); font-size: 13px; margin-top: 28px; }}
  code {{ font-family: ui-monospace, Menlo, monospace; font-size: 12px; }}
</style>
</head>
<body>
<main>
  <h1>“文”字形方案（1–6，另有基于 1 的 1b / 1c）</h1>
  <p class="lead">外框、颜色、圆角角框都和 0.5 一样，只换中间的“文”。每个方案都展示了 512 / 128 / 32 / 16 像素（浅底、深底），以及菜单栏单色图标的效果。字体字形已转换为矢量轮廓，应用里不包含任何字体文件；所用字体均为 SIL OFL 1.1，可用于商业 logo（来源见 <code>docs/brand/LICENSES.md</code>）。</p>
  {''.join(cards)}
  <p class="note">选好后告诉我编号，例如“用 3”，我会运行 <code>./scripts/build_icons.sh option-3</code> 生成全部图标（应用图标、Windows 图标、菜单栏图标）并同步应用内的 logo。</p>
</main>
</body>
</html>
"""
    (BRAND / "glyph-options.html").write_text(html, encoding="utf-8")
    print("options:", ", ".join(f"{n} {t}" for n, t in options))


if __name__ == "__main__":
    main()
