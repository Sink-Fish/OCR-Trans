import type { Paragraph, PipelineResult } from "./api";

const CLOSING = "，。、；：！？）》」』】,.;:!?)]}%";

function isCjk(ch: string): boolean {
  const c = ch.codePointAt(0)!;
  return (c >= 0x2e80 && c <= 0x9fff) || (c >= 0xf900 && c <= 0xfaff) || (c >= 0xff00 && c <= 0xffef) || (c >= 0x3000 && c <= 0x303f);
}

/** Split into breakable tokens: single CJK chars, Latin words (with trailing space). */
function tokens(text: string): string[] {
  const out: string[] = [];
  let word = "";
  for (const ch of text) {
    if (isCjk(ch)) {
      if (word) out.push(word), (word = "");
      out.push(ch);
    } else if (ch === " ") {
      out.push(word + " ");
      word = "";
    } else word += ch;
  }
  if (word) out.push(word);
  return out;
}

/** Index of the translated paragraph under image point (x, y), or -1. */
export function paragraphAt(res: PipelineResult, x: number, y: number): number {
  return res.paragraphs.findIndex((p) => {
    const [bx, by, bw, bh] = p.bbox;
    return x >= bx && x <= bx + bw && y >= by && y <= by + bh;
  });
}

type TextBox = Paragraph["bbox"];

function lineBoxes(p: Paragraph): TextBox[] {
  if (!p.lines.length) return [p.bbox];
  return p.lines.map(({ quad }) => {
    const xs = quad.map(([x]) => x);
    const ys = quad.map(([, y]) => y);
    const x = Math.min(...xs);
    const y = Math.min(...ys);
    return [x, y, Math.max(...xs) - x, Math.max(...ys) - y];
  });
}

/** Keep translated text within the number of source rows and leave any overflow on the last row to be fitted. */
function wrapRows(ctx: CanvasRenderingContext2D, parts: string[], boxes: TextBox[]): string[] {
  const lines: string[] = [];
  let cur = "";
  for (const t of parts) {
    const next = cur + t;
    if (cur && lines.length + 1 < boxes.length && !CLOSING.includes(t[0]) && ctx.measureText(next.trimEnd()).width > boxes[lines.length][2]) {
      lines.push(cur.trimEnd());
      cur = t.trimStart();
    } else cur = next;
  }
  if (cur.trim()) lines.push(cur.trimEnd());
  return lines;
}

/** Draw one translated paragraph over the original (coordinates are image pixels). */
function drawParagraph(ctx: CanvasRenderingContext2D, p: Paragraph, font: string, baseSize: number) {
  const boxes = lineBoxes(p).filter(([, , w, h]) => w > 0 && h > 0);
  if (!boxes.length) return;

  if (p.fill !== "inpaint") {
    // Cover only the original text boxes so row and column separators stay visible.
    ctx.save();
    ctx.fillStyle = p.bg;
    for (const [x, y, w, h] of boxes) {
      if (p.fill === "translucent") {
        ctx.globalAlpha = 0.78;
        ctx.beginPath();
        ctx.roundRect(x, y, w, h, Math.min(8, h / 4));
        ctx.fill();
      } else ctx.fillRect(x, y, w, h);
    }
    ctx.restore();
  }

  const parts = tokens(p.translation!.replace(/\s+/g, " ").trim());
  let size = Math.min(baseSize, ...boxes.map(([, , , h]) => h));
  const minSize = size * 0.55;
  let lines: string[] = [];
  for (;;) {
    ctx.font = `${size}px ${font}`;
    lines = wrapRows(ctx, parts, boxes);
    if (size <= minSize || lines.every((line, i) => ctx.measureText(line).width <= boxes[i][2])) break;
    size = Math.max(minSize, size * 0.9);
  }
  ctx.fillStyle = p.fg;
  ctx.textBaseline = "alphabetic";
  lines.forEach((line, i) => {
    const [x, y, w, h] = boxes[i];
    const { actualBoundingBoxAscent, actualBoundingBoxDescent } = ctx.measureText(line);
    ctx.save();
    ctx.beginPath();
    ctx.rect(x, y, w, h);
    ctx.clip();
    ctx.fillText(line, x, y + (h + actualBoundingBoxAscent - actualBoundingBoxDescent) / 2, w);
    ctx.restore();
  });
}

/** Image with every translated paragraph replaced in place. `img` should be the inpainted ("clean") image. */
export function renderTranslated(img: CanvasImageSource & { width: number; height: number }, res: PipelineResult, font: string): HTMLCanvasElement {
  const c = document.createElement("canvas");
  c.width = img.width;
  c.height = img.height;
  const ctx = c.getContext("2d")!;
  ctx.drawImage(img, 0, 0);
  // paragraphs whose line heights are within ~25% share one base font size, so body text looks uniform
  const heights = res.paragraphs.map((p) => p.line_height);
  for (const p of res.paragraphs) {
    if (!p.translation) continue;
    const peers = heights.filter((h) => h / p.line_height < 1.25 && p.line_height / h < 1.25).sort((a, b) => a - b);
    const median = peers[Math.floor(peers.length / 2)] ?? p.line_height;
    drawParagraph(ctx, p, font, median * 0.72);
  }
  return c;
}
