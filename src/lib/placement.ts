/** Pure layout math for the floating toolbars (unit-tested in tests/placement.test.ts). */
export const TOOLBAR_W = 452;
export const TOOLBAR_H = 40;

export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * Selection action bar (overlay): below the selection's right edge; above it if there is no room below; inside it
 * if neither fits. `size` is the measured bar size (defaults to the nominal size).
 */
export function toolbarPosition(r: Rect, vw: number, vh: number, size: { w: number; h: number } = { w: TOOLBAR_W, h: TOOLBAR_H }): { left: number; top: number } {
  const gap = 8;
  const { w, h } = size;
  const left = Math.max(8, Math.min(r.left + r.width - w, vw - w - 8));
  let top = r.top + r.height + gap;
  if (top + h > vh - 8) top = r.top - h - gap;
  if (top < 8) top = Math.min(vh - h - 8, r.top + r.height - h - gap);
  return { left, top: Math.max(8, top) };
}

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface PinLayout {
  /** pin window frame in screen coordinates */
  window: Box;
  /** image and toolbar offsets inside the window */
  image: { x: number; y: number };
  toolbar: { x: number; y: number };
  placement: "below" | "above" | "inside";
}

/**
 * Room (CSS px) the pin window keeps around the image for the frame's outer hairline (styles.css .pin-frame). Up to
 * 0.5.4 the window was exactly image + toolbar and clipped the frame on every side but the toolbar's.
 */
export const FRAME_PAD = 2;

/** Reopen a capture at its original anchor when the image is still visible on its display. */
export function pinAnchor(img: Box, display: Box | null, work: Box): Box {
  if (display && img.x < display.x + display.w && img.x + img.w > display.x
    && img.y < display.y + display.h && img.y + img.h > display.y) return img;
  return {
    ...img,
    x: work.x + Math.max(0, (work.w - img.w) / 2),
    y: work.y + Math.max(0, (work.h - img.h) / 2),
  };
}

/**
 * Pin window layout. The image never moves on screen (it sits exactly over what was captured); the toolbar goes
 * below it, else above it, else inside its bottom edge, and is clamped horizontally (and vertically when inside)
 * to the monitor's visible work area. The window is the bounding box of the image plus `pad` on every side, and the
 * toolbar.
 */
export function pinLayout(img: Box, tb: { w: number; h: number }, work: Box, gap = 8, margin = 6, pad = FRAME_PAD): PinLayout {
  const minX = work.x + margin;
  const maxX = work.x + work.w - margin;
  const minY = work.y + margin;
  const maxY = work.y + work.h - margin;

  let placement: PinLayout["placement"];
  let ty: number;
  if (img.y + img.h + gap + tb.h <= maxY) {
    placement = "below";
    ty = img.y + img.h + gap;
  } else if (img.y - gap - tb.h >= minY) {
    placement = "above";
    ty = img.y - gap - tb.h;
  } else {
    placement = "inside";
    // overlay the bottom of the image, but stay within the visible area
    ty = Math.min(img.y + img.h - tb.h - gap, maxY - tb.h);
    ty = Math.max(ty, minY);
  }
  // left-align with the image, clamped into the work area (a toolbar wider than the screen pins to the left edge)
  let tx = Math.min(img.x, maxX - tb.w);
  tx = Math.max(tx, minX);

  const x0 = Math.min(img.x - pad, tx);
  const y0 = Math.min(img.y - pad, ty);
  const x1 = Math.max(img.x + img.w + pad, tx + tb.w);
  const y1 = Math.max(img.y + img.h + pad, ty + tb.h);
  return {
    window: { x: x0, y: y0, w: x1 - x0, h: y1 - y0 },
    image: { x: img.x - x0, y: img.y - y0 },
    toolbar: { x: tx - x0, y: ty - y0 },
    placement,
  };
}

/** Handle size (CSS px) and the shortest edge that still gets a midpoint handle. */
export const HANDLE_D = 8;
export const MID_HANDLE_MIN = 64;

/**
 * Resize handles for a selection, as fractions of its rect: always the corners; edge midpoints only when that edge
 * is long enough. On a short text-line selection three handles stacked on a 30 px side edge used to hide most of
 * the side's outline (0.5.3: "the outline only shows top and bottom").
 */
export function selectionHandles(width: number, height: number): [number, number][] {
  const h: [number, number][] = [
    [0, 0],
    [1, 0],
    [1, 1],
    [0, 1],
  ];
  if (width >= MID_HANDLE_MIN) h.push([0.5, 0], [0.5, 1]);
  if (height >= MID_HANDLE_MIN) h.push([0, 0.5], [1, 0.5]);
  return h;
}

/**
 * Fraction of each edge (top, right, bottom, left) not covered by a handle. A corner handle covers half its
 * diameter on each of its two edges, so an edge's coverage is (handles on it - 1) diameters.
 */
export function edgeVisibility(width: number, height: number): [number, number, number, number] {
  const hs = selectionHandles(width, height);
  const free = (len: number, pred: (p: [number, number]) => boolean) =>
    Math.max(0, 1 - ((hs.filter(pred).length - 1) * HANDLE_D) / Math.max(len, 1));
  return [free(width, ([, y]) => y === 0), free(height, ([x]) => x === 1), free(width, ([, y]) => y === 1), free(height, ([x]) => x === 0)];
}
