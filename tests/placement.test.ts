// run: node --test tests/*.test.ts  (Node ≥ 23 strips TypeScript types natively)
import { test } from "node:test";
import assert from "node:assert/strict";
import { edgeVisibility, FRAME_PAD, pinAnchor, pinLayout, selectionHandles, toolbarPosition, TOOLBAR_H, TOOLBAR_W, type Box } from "../src/lib/placement.ts";

const VW = 1728;
const VH = 1117;

// ---- selection action bar (overlay) ----
test("action bar: below the selection, right-aligned", () => {
  const p = toolbarPosition({ left: 500, top: 200, width: 600, height: 300 }, VW, VH);
  assert.equal(p.top, 508);
  assert.equal(p.left, 1100 - TOOLBAR_W);
});

test("action bar: flips above when the selection reaches the bottom", () => {
  const p = toolbarPosition({ left: 100, top: 700, width: 800, height: 410 }, VW, VH);
  assert.equal(p.top, 700 - TOOLBAR_H - 8);
});

test("action bar: goes inside a full-screen selection", () => {
  const p = toolbarPosition({ left: 0, top: 0, width: VW, height: VH }, VW, VH);
  assert.ok(p.top >= 8 && p.top + TOOLBAR_H <= VH - 8);
  assert.ok(p.left >= 8 && p.left + TOOLBAR_W <= VW - 8);
});

test("action bar: tiny selection at the left edge stays on screen", () => {
  const p = toolbarPosition({ left: 2, top: 50, width: 20, height: 20 }, VW, VH);
  assert.equal(p.left, 8);
});

test("action bar: measured size near the bottom-right corner", () => {
  const size = { w: 560, h: 48 };
  const p = toolbarPosition({ left: 1600, top: 1050, width: 120, height: 60 }, VW, VH, size);
  assert.ok(p.left + size.w <= VW - 8 && p.left >= 8, `left ${p.left}`);
  assert.ok(p.top + size.h <= VH - 8 && p.top >= 8, `top ${p.top}`);
  assert.equal(p.top, 1050 - 48 - 8, "above the selection");
});

// ---- pin window layout ----
const WORK: Box = { x: 0, y: 25, w: 1728, h: 1010 }; // menu bar on top, Dock at the bottom
const TB = { w: 430, h: 88 }; // two rows (annotation mode)

test("a saved pin reopens on screen after the capture display is unplugged", () => {
  const display = { x: 0, y: 0, w: 1728, h: 1117 };
  const img = { x: -1400, y: 350, w: 600, h: 200 };
  const moved = pinAnchor(img, display, WORK);
  assert.deepEqual(moved, { x: 564, y: 430, w: 600, h: 200 });
  onScreen(pinLayout(moved, TB, WORK), moved);
  const visible = { ...img, x: 100, y: 0 };
  assert.deepEqual(pinAnchor(visible, display, WORK), visible, "keep a capture anchored at the menu bar");
});

function onScreen(l: ReturnType<typeof pinLayout>, img: Box) {
  const tbx = l.window.x + l.toolbar.x;
  const tby = l.window.y + l.toolbar.y;
  assert.ok(tbx >= WORK.x && tbx + TB.w <= WORK.x + WORK.w, `toolbar x ${tbx}`);
  assert.ok(tby >= WORK.y && tby + TB.h <= WORK.y + WORK.h, `toolbar y ${tby}`);
  // the image never moves on screen
  assert.equal(l.window.x + l.image.x, img.x);
  assert.equal(l.window.y + l.image.y, img.y);
  // window encloses both
  assert.ok(l.window.w >= Math.max(l.image.x + img.w, l.toolbar.x + TB.w));
  assert.ok(l.window.h >= Math.max(l.image.y + img.h, l.toolbar.y + TB.h));
  framed(l, img);
}

/** 0.5.5: the window keeps FRAME_PAD around the image on all four sides, so the frame's hairline is never clipped. */
function framed(l: ReturnType<typeof pinLayout>, img: Box) {
  assert.ok(l.image.x >= FRAME_PAD, `left ${l.image.x}`);
  assert.ok(l.image.y >= FRAME_PAD, `top ${l.image.y}`);
  assert.ok(l.window.w - (l.image.x + img.w) >= FRAME_PAD, `right ${l.window.w - (l.image.x + img.w)}`);
  assert.ok(l.window.h - (l.image.y + img.h) >= FRAME_PAD, `bottom ${l.window.h - (l.image.y + img.h)}`);
}

test("pin: middle of the screen → toolbar below, left-aligned", () => {
  const img = { x: 400, y: 300, w: 600, h: 200 };
  const l = pinLayout(img, TB, WORK);
  assert.equal(l.placement, "below");
  assert.equal(l.toolbar.x, FRAME_PAD, "left-aligned with the image");
  onScreen(l, img);
});

test("pin: near the bottom edge → toolbar above (the reported bug)", () => {
  const img = { x: 300, y: 860, w: 900, h: 160 };
  const l = pinLayout(img, TB, WORK);
  assert.equal(l.placement, "above");
  onScreen(l, img);
});

test("pin: near the right edge → toolbar shifted left to stay on screen", () => {
  const img = { x: 1600, y: 300, w: 110, h: 80 };
  const l = pinLayout(img, TB, WORK);
  assert.equal(l.placement, "below");
  assert.ok(l.window.x + l.toolbar.x + TB.w <= WORK.w - 6 + 0.001);
  onScreen(l, img);
});

test("pin: bottom-right corner, small selection → above and shifted left", () => {
  const img = { x: 1650, y: 960, w: 60, h: 40 };
  const l = pinLayout(img, TB, WORK);
  assert.equal(l.placement, "above");
  onScreen(l, img);
});

test("pin: top edge, no room above, room below", () => {
  const img = { x: 20, y: 30, w: 300, h: 100 };
  const l = pinLayout(img, TB, WORK);
  assert.equal(l.placement, "below");
  onScreen(l, img);
});

test("pin: full-height selection → toolbar inside the image, on screen", () => {
  const img = { x: 0, y: 25, w: 1728, h: 1010 };
  const l = pinLayout(img, TB, WORK);
  assert.equal(l.placement, "inside");
  onScreen(l, img);
});

test("pin: left edge → toolbar never goes off the left side", () => {
  const img = { x: -40, y: 400, w: 200, h: 100 }; // partly off-screen after a drag
  const l = pinLayout(img, TB, WORK);
  assert.ok(l.window.x + l.toolbar.x >= WORK.x);
  framed(l, img);
});

// 0.5.5: "the pin's green frame only shows at the bottom" — the window was exactly image + toolbar, so the frame
// drawn outside the image was clipped on the three sides not facing the toolbar.
test("pin: room for the frame on all four sides, toolbar below / above / inside / wider than the image", () => {
  const cases: [Box, string][] = [
    [{ x: 400, y: 300, w: 600, h: 200 }, "below"],
    [{ x: 300, y: 860, w: 900, h: 160 }, "above"],
    [{ x: 0, y: 25, w: 1728, h: 1010 }, "inside"],
    [{ x: 800, y: 500, w: 40, h: 26 }, "below"], // short text line, toolbar much wider than the image
  ];
  for (const [img, placement] of cases) {
    const l = pinLayout(img, TB, WORK);
    assert.equal(l.placement, placement);
    framed(l, img);
    assert.equal(l.window.x + l.image.x, img.x, "the image still does not move");
    assert.equal(l.window.y + l.image.y, img.y);
  }
  // exactly FRAME_PAD where nothing else widens the window: above the image when the toolbar is below, and so on
  const below = pinLayout({ x: 400, y: 300, w: 600, h: 200 }, TB, WORK);
  assert.deepEqual([below.image.x, below.image.y, below.window.w - 600 - below.image.x], [FRAME_PAD, FRAME_PAD, FRAME_PAD]);
  const above = pinLayout({ x: 300, y: 860, w: 900, h: 160 }, TB, WORK);
  assert.equal(above.window.h - (above.image.y + 160), FRAME_PAD);
});

test("pin: tiny selection keeps a usable window", () => {
  const img = { x: 800, y: 500, w: 12, h: 10 };
  const l = pinLayout(img, TB, WORK);
  assert.ok(l.window.w >= TB.w && l.window.h >= img.h + TB.h);
  onScreen(l, img);
});

// 0.5.4: the selection outline must stay visible on all four sides. Handles no longer pile up on short edges
// (rendering itself is checked in WebKit by scripts/check_overlay_frame.sh).
test("short or narrow selections get no midpoint handles on their short edges", () => {
  assert.equal(selectionHandles(600, 30).length, 6); // corners + top/bottom midpoints
  assert.ok(!selectionHandles(600, 30).some(([x, y]) => y === 0.5 && (x === 0 || x === 1)));
  assert.ok(!selectionHandles(24, 400).some(([x, y]) => x === 0.5 && (y === 0 || y === 1)));
  assert.equal(selectionHandles(400, 300).length, 8);
  assert.equal(selectionHandles(10, 10).length, 4);
});

test("most of every edge is plain outline, whatever the selection's shape", () => {
  for (const [w, h] of [
    [600, 26],
    [26, 600],
    [40, 40],
    [64, 64],
    [1728, 1080],
    [300, 63],
  ]) {
    for (const v of edgeVisibility(w, h)) assert.ok(v >= 0.69, `${w}×${h}: edge visibility ${v.toFixed(2)}`);
  }
  // the 0.5.3 layout (three 10 px handles on a 30 px side edge) left a third of the edge
  assert.ok(1 - (2 * 10) / 30 < 0.69);
});
