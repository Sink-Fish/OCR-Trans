// Live-Text style mapping between the recognised text (as shown in the text window's editor) and where each
// character sits on the captured image. Pure functions, unit-tested in tests/textmap.test.ts.
import type { PipelineResult, Quad } from "./api";

export type Pt = [number, number];

export interface MappedLine {
  quad: Quad;
  /** [start, end) offsets of this line inside `TextMap.text`. */
  start: number;
  end: number;
  /** Normalised position (0…1 along the line) of every character boundary; length = end - start + 1. */
  stops: number[];
}

export interface TextMap {
  /** Exactly the editor's text: paragraphs joined by "\n" (same as plainText(r, false)). */
  text: string;
  lines: MappedLine[];
}

const WIDE = /[ᄀ-ᅟ⺀-〿぀-꓏가-힣豈-﫿︰-﹏＀-｠￠-￦]/;

/** Rough advance width of a character in ems: CJK / full-width 1, Latin ~0.56, space ~0.33. */
export function charWeight(c: string): number {
  if (c === " ") return 0.33;
  return WIDE.test(c) ? 1 : 0.56;
}

function stopsOf(s: string): number[] {
  const w = [...s].map(charWeight);
  // a string may contain surrogate pairs; keep one stop per UTF-16 unit so offsets match textarea indices
  const units: number[] = [];
  [...s].forEach((c, i) => {
    units.push(w[i]);
    for (let k = 1; k < c.length; k++) units.push(0);
  });
  const total = units.reduce((a, b) => a + b, 0) || 1;
  const out = [0];
  let acc = 0;
  for (const u of units) out.push((acc += u) / total);
  return out;
}

/** Locate every OCR line inside its paragraph's text. Lines that cannot be found (e.g. merged away) are skipped. */
export function buildTextMap(r: Pick<PipelineResult, "paragraphs">): TextMap {
  let text = "";
  const lines: MappedLine[] = [];
  r.paragraphs.forEach((p, pi) => {
    if (pi) text += "\n";
    const base = text.length;
    text += p.text;
    let cursor = 0;
    for (const l of p.lines) {
      let t = l.text.trim();
      if (!t) continue;
      let at = p.text.indexOf(t, cursor);
      if (at < 0 && t.endsWith("-")) {
        // de-hyphenated when the paragraph was joined: "exam-" + "ple" → "example"
        t = t.slice(0, -1);
        at = p.text.indexOf(t, cursor);
      }
      if (at < 0) at = p.text.indexOf(t);
      if (at < 0) continue;
      lines.push({ quad: l.quad, start: base + at, end: base + at + t.length, stops: stopsOf(t) });
      cursor = at + t.length;
    }
  });
  return { text, lines };
}

const sub = (a: Pt, b: Pt): Pt => [a[0] - b[0], a[1] - b[1]];
const dot = (a: Pt, b: Pt) => a[0] * b[0] + a[1] * b[1];
const lerp = (a: Pt, b: Pt, t: number): Pt => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];

/** Position of (x, y) in a line's own frame: s = 0…1 along the line, d = 0…1 across it. */
function local(q: Quad, x: number, y: number) {
  const o = q[0] as Pt;
  const u = sub(q[1] as Pt, o);
  const v = sub(q[3] as Pt, o);
  const p = sub([x, y], o);
  const lu = Math.hypot(u[0], u[1]) || 1;
  const lv = Math.hypot(v[0], v[1]) || 1;
  return { s: dot(p, u) / (lu * lu), d: dot(p, v) / (lv * lv), lu, lv };
}

/** Index of the line nearest to (x, y) in image pixels, or -1 when there is no text. */
export function nearestLine(m: TextMap, x: number, y: number): number {
  let best = -1;
  let bestD = Infinity;
  m.lines.forEach((l, i) => {
    const { s, d, lu, lv } = local(l.quad, x, y);
    const dx = (s < 0 ? -s : s > 1 ? s - 1 : 0) * lu;
    const dy = (d < 0 ? -d : d > 1 ? d - 1 : 0) * lv;
    // being off to the side of a line matters much less than being above / below it
    const dist = dy * 4 + dx;
    if (dist < bestD) {
      bestD = dist;
      best = i;
    }
  });
  return best;
}

/** Text offset (a character boundary) closest to (x, y), or null when there is no text. */
export function hitTest(m: TextMap, x: number, y: number): number | null {
  const i = nearestLine(m, x, y);
  if (i < 0) return null;
  const l = m.lines[i];
  const { s } = local(l.quad, x, y);
  let k = 0;
  let bestD = Infinity;
  l.stops.forEach((t, j) => {
    // An extra UTF-16 stop inside a surrogate pair shares the character's end position.
    if (t === l.stops[j + 1]) return;
    const dd = Math.abs(t - s);
    if (dd < bestD) {
      bestD = dd;
      k = j;
    }
  });
  return l.start + k;
}

/** Selection between two drag points: [start, end) with start ≤ end. */
export function dragRange(m: TextMap, a: Pt, b: Pt): [number, number] | null {
  const p = hitTest(m, a[0], a[1]);
  const q = hitTest(m, b[0], b[1]);
  if (p === null || q === null) return null;
  return p <= q ? [p, q] : [q, p];
}

/** The whole line under (x, y) (double-click). */
export function lineRangeAt(m: TextMap, x: number, y: number): [number, number] | null {
  const i = nearestLine(m, x, y);
  return i < 0 ? null : [m.lines[i].start, m.lines[i].end];
}

/** Highlight polygons (image pixels) covering [start, end), one per touched line. */
export function rangePolys(m: TextMap, start: number, end: number): Pt[][] {
  const out: Pt[][] = [];
  if (end <= start) return out;
  for (const l of m.lines) {
    const a = Math.max(start, l.start) - l.start;
    const b = Math.min(end, l.end) - l.start;
    if (a >= b) continue;
    const [q0, q1, q2, q3] = l.quad as unknown as Pt[];
    const t0 = l.stops[a];
    const t1 = l.stops[b];
    out.push([lerp(q0, q1, t0), lerp(q0, q1, t1), lerp(q3, q2, t1), lerp(q3, q2, t0)]);
  }
  return out;
}

/** Whether the text window should put the text on the clipboard by itself once OCR finishes. */
export function shouldAutoCopy(autoCopySetting: boolean | undefined, text: string): boolean {
  return autoCopySetting === true && text.trim().length > 0;
}

/** Image offsets refer to the original OCR text and become invalid after the editor is changed. */
export function imageSelectionText(map: TextMap, editorText: string, selection: [number, number]): string {
  if (editorText !== map.text) return "";
  return map.text.slice(Math.max(0, selection[0]), Math.max(0, selection[1]));
}
