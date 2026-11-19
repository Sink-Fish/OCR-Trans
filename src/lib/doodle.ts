/** Annotation shapes, all in image-pixel coordinates. */
export type Shape =
  | { kind: "pen"; color: string; width: number; points: [number, number][] }
  | { kind: "highlight"; color: string; width: number; points: [number, number][] }
  | { kind: "rect"; color: string; width: number; x0: number; y0: number; x1: number; y1: number }
  | { kind: "ellipse"; color: string; width: number; x0: number; y0: number; x1: number; y1: number }
  | { kind: "arrow"; color: string; width: number; x0: number; y0: number; x1: number; y1: number }
  | { kind: "text"; color: string; size: number; x: number; y: number; text: string }
  | { kind: "marker"; color: string; size: number; x: number; y: number; n: number }
  | { kind: "mosaic"; block: number; x0: number; y0: number; x1: number; y1: number };

export type Tool = "move" | "select" | "pen" | "highlight" | "rect" | "ellipse" | "arrow" | "text" | "marker" | "mosaic";

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

function norm(x0: number, y0: number, x1: number, y1: number): Box {
  return { x: Math.min(x0, x1), y: Math.min(y0, y1), w: Math.abs(x1 - x0), h: Math.abs(y1 - y0) };
}

export function mosaicCovers(bbox: number[], shapes: Shape[]): boolean {
  const [x, y, w, h] = bbox;
  return shapes.some(s => {
    if (s.kind !== "mosaic") return false;
    const b = norm(s.x0, s.y0, s.x1, s.y1);
    return x < b.x + b.w && b.x < x + w && y < b.y + b.h && b.y < y + h;
  });
}

let measureCtx: CanvasRenderingContext2D | null = null;
function textWidth(s: Extract<Shape, { kind: "text" }>, font: string): number {
  measureCtx ??= document.createElement("canvas").getContext("2d");
  if (!measureCtx) return s.text.length * s.size * 0.6;
  measureCtx.font = `${s.size}px ${font}`;
  return Math.max(...s.text.split("\n").map((l) => measureCtx!.measureText(l).width));
}

/** Bounding box of a shape (image pixels). */
export function bounds(s: Shape, font = "sans-serif"): Box {
  switch (s.kind) {
    case "pen":
    case "highlight": {
      const xs = s.points.map((p) => p[0]);
      const ys = s.points.map((p) => p[1]);
      const pad = s.kind === "highlight" ? s.width * 2 : s.width / 2;
      const x = Math.min(...xs) - pad;
      const y = Math.min(...ys) - pad;
      return { x, y, w: Math.max(...xs) - x + pad, h: Math.max(...ys) - y + pad };
    }
    case "text":
      return { x: s.x, y: s.y, w: textWidth(s, font), h: s.text.split("\n").length * s.size * 1.25 };
    case "marker":
      return { x: s.x - s.size, y: s.y - s.size, w: s.size * 2, h: s.size * 2 };
    default:
      return norm(s.x0, s.y0, s.x1, s.y1);
  }
}

function distToSegment(px: number, py: number, ax: number, ay: number, bx: number, by: number): number {
  const dx = bx - ax;
  const dy = by - ay;
  const len = dx * dx + dy * dy;
  const t = len ? Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / len)) : 0;
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
}

/** Does point (x, y) touch the shape? `tol` in image pixels. */
export function hitTest(s: Shape, x: number, y: number, tol: number, font = "sans-serif"): boolean {
  switch (s.kind) {
    case "pen":
    case "highlight": {
      const w = (s.kind === "highlight" ? s.width * 4 : s.width) / 2 + tol;
      return s.points.some((p, i) => (i ? distToSegment(x, y, s.points[i - 1][0], s.points[i - 1][1], p[0], p[1]) : Math.hypot(x - p[0], y - p[1])) <= w);
    }
    case "arrow":
      return distToSegment(x, y, s.x0, s.y0, s.x1, s.y1) <= s.width / 2 + tol;
    case "rect": {
      const b = norm(s.x0, s.y0, s.x1, s.y1);
      const inside = x >= b.x - tol && x <= b.x + b.w + tol && y >= b.y - tol && y <= b.y + b.h + tol;
      const nearEdge = Math.min(Math.abs(x - b.x), Math.abs(x - b.x - b.w), Math.abs(y - b.y), Math.abs(y - b.y - b.h)) <= s.width / 2 + tol;
      return inside && nearEdge;
    }
    case "ellipse": {
      const b = norm(s.x0, s.y0, s.x1, s.y1);
      const rx = b.w / 2;
      const ry = b.h / 2;
      if (rx < 1 || ry < 1) return false;
      const d = Math.hypot((x - b.x - rx) / rx, (y - b.y - ry) / ry);
      return Math.abs(d - 1) * Math.min(rx, ry) <= s.width / 2 + tol;
    }
    default: {
      // text, marker, mosaic: anywhere inside the box
      const b = bounds(s, font);
      return x >= b.x - tol && x <= b.x + b.w + tol && y >= b.y - tol && y <= b.y + b.h + tol;
    }
  }
}

export function translate(s: Shape, dx: number, dy: number): Shape {
  switch (s.kind) {
    case "pen":
    case "highlight":
      return { ...s, points: s.points.map(([x, y]) => [x + dx, y + dy] as [number, number]) };
    case "text":
    case "marker":
      return { ...s, x: s.x + dx, y: s.y + dy };
    default:
      return { ...s, x0: s.x0 + dx, y0: s.y0 + dy, x1: s.x1 + dx, y1: s.y1 + dy };
  }
}

/** Pointer-up can arrive at a new position without a final pointer-move (including coalesced input). */
export function finishShape(s: Shape, x: number, y: number): Shape {
  switch (s.kind) {
    case "pen":
    case "highlight": {
      const last = s.points[s.points.length - 1];
      return last?.[0] === x && last?.[1] === y ? s : { ...s, points: [...s.points, [x, y]] };
    }
    case "text":
    case "marker":
      return s;
    default:
      return { ...s, x1: x, y1: y };
  }
}

/** Topmost shape under the point, or -1. */
export function shapeAt(shapes: Shape[], x: number, y: number, tol: number, font: string): number {
  for (let i = shapes.length - 1; i >= 0; i--) if (hitTest(shapes[i], x, y, tol, font)) return i;
  return -1;
}

export function nextMarkerNumber(shapes: Shape[]): number {
  return shapes.reduce((n, s) => (s.kind === "marker" ? Math.max(n, s.n) : n), 0) + 1;
}

/** Pixelate a region of `ctx` using `source` as the pixels to blur. */
function mosaic(ctx: CanvasRenderingContext2D, source: CanvasImageSource, s: Extract<Shape, { kind: "mosaic" }>) {
  const { x, y, w, h } = norm(s.x0, s.y0, s.x1, s.y1);
  if (w < 1 || h < 1) return;
  const b = Math.max(2, s.block);
  const sw = Math.max(1, Math.ceil(w / b));
  const sh = Math.max(1, Math.ceil(h / b));
  const tmp = document.createElement("canvas");
  tmp.width = sw;
  tmp.height = sh;
  const t = tmp.getContext("2d")!;
  t.imageSmoothingEnabled = true;
  t.drawImage(source, x, y, w, h, 0, 0, sw, sh);
  ctx.save();
  ctx.beginPath();
  ctx.rect(x, y, w, h);
  ctx.clip();
  ctx.imageSmoothingEnabled = false;
  ctx.drawImage(tmp, 0, 0, sw, sh, x, y, sw * b, sh * b);
  ctx.restore();
}

function polyline(ctx: CanvasRenderingContext2D, pts: [number, number][]) {
  ctx.beginPath();
  pts.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
  if (pts.length === 1) ctx.lineTo(pts[0][0] + 0.1, pts[0][1]);
  ctx.stroke();
}

function contrastText(hex: string): string {
  const v = parseInt(hex.replace("#", "").padEnd(6, "0").slice(0, 6), 16);
  const l = 0.299 * ((v >> 16) & 255) + 0.587 * ((v >> 8) & 255) + 0.114 * (v & 255);
  return l > 160 ? "#000" : "#fff";
}

export function drawShape(ctx: CanvasRenderingContext2D, s: Shape, source: CanvasImageSource, font: string) {
  ctx.save();
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  switch (s.kind) {
    case "pen":
      ctx.strokeStyle = s.color;
      ctx.lineWidth = s.width;
      polyline(ctx, s.points);
      break;
    case "highlight":
      // translucent wide marker; multiply keeps text underneath readable
      ctx.globalAlpha = 0.38;
      ctx.globalCompositeOperation = "multiply";
      ctx.lineCap = "butt";
      ctx.strokeStyle = s.color;
      ctx.lineWidth = s.width * 4;
      polyline(ctx, s.points);
      break;
    case "rect": {
      const { x, y, w, h } = norm(s.x0, s.y0, s.x1, s.y1);
      ctx.strokeStyle = s.color;
      ctx.lineWidth = s.width;
      ctx.strokeRect(x, y, w, h);
      break;
    }
    case "ellipse": {
      const { x, y, w, h } = norm(s.x0, s.y0, s.x1, s.y1);
      ctx.strokeStyle = s.color;
      ctx.lineWidth = s.width;
      ctx.beginPath();
      ctx.ellipse(x + w / 2, y + h / 2, Math.max(0.5, w / 2), Math.max(0.5, h / 2), 0, 0, Math.PI * 2);
      ctx.stroke();
      break;
    }
    case "arrow": {
      const ang = Math.atan2(s.y1 - s.y0, s.x1 - s.x0);
      const len = Math.hypot(s.x1 - s.x0, s.y1 - s.y0);
      const head = Math.min(len * 0.5, Math.max(10, s.width * 4));
      ctx.strokeStyle = ctx.fillStyle = s.color;
      ctx.lineWidth = s.width;
      ctx.beginPath();
      ctx.moveTo(s.x0, s.y0);
      ctx.lineTo(s.x1 - Math.cos(ang) * head * 0.8, s.y1 - Math.sin(ang) * head * 0.8);
      ctx.stroke();
      ctx.beginPath();
      ctx.moveTo(s.x1, s.y1);
      ctx.lineTo(s.x1 - head * Math.cos(ang - 0.45), s.y1 - head * Math.sin(ang - 0.45));
      ctx.lineTo(s.x1 - head * Math.cos(ang + 0.45), s.y1 - head * Math.sin(ang + 0.45));
      ctx.closePath();
      ctx.fill();
      break;
    }
    case "text":
      ctx.fillStyle = s.color;
      ctx.font = `${s.size}px ${font}`;
      ctx.textBaseline = "top";
      s.text.split("\n").forEach((l, i) => ctx.fillText(l, s.x, s.y + i * s.size * 1.25));
      break;
    case "marker":
      ctx.fillStyle = s.color;
      ctx.beginPath();
      ctx.arc(s.x, s.y, s.size, 0, Math.PI * 2);
      ctx.fill();
      ctx.lineWidth = Math.max(1, s.size * 0.12);
      ctx.strokeStyle = "rgba(255,255,255,0.9)";
      ctx.stroke();
      ctx.fillStyle = contrastText(s.color);
      ctx.font = `bold ${Math.round(s.size * (s.n > 9 ? 1.0 : 1.2))}px -apple-system, "Segoe UI", sans-serif`;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(String(s.n), s.x, s.y + s.size * 0.05);
      break;
    case "mosaic":
      mosaic(ctx, source, s);
      break;
  }
  ctx.restore();
}

/** Paint the screenshot and committed annotations at the original capture resolution. */
export function drawAnnotations(canvas: HTMLCanvasElement, base: HTMLCanvasElement | ImageBitmap, shapes: Shape[], font: string) {
  canvas.width = base.width;
  canvas.height = base.height;
  const ctx = canvas.getContext("2d")!;
  ctx.drawImage(base, 0, 0);
  for (const shape of shapes) drawShape(ctx, shape, canvas, font);
  return ctx;
}

/** Dashed box around the selected shape. */
export function drawSelection(ctx: CanvasRenderingContext2D, b: Box, scale: number) {
  ctx.save();
  ctx.setLineDash([4 * scale, 3 * scale]);
  ctx.lineWidth = Math.max(1, scale);
  ctx.strokeStyle = "#0a84ff";
  const pad = 3 * scale;
  ctx.strokeRect(b.x - pad, b.y - pad, b.w + 2 * pad, b.h + 2 * pad);
  ctx.restore();
}
