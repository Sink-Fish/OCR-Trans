// Text window: mapping between the editor's text and the image (Live-Text style selection).
import { test } from "node:test";
import assert from "node:assert/strict";
import { buildTextMap, dragRange, hitTest, imageSelectionText, lineRangeAt, rangePolys, shouldAutoCopy, charWeight } from "../src/lib/textmap.ts";

type Q = [[number, number], [number, number], [number, number], [number, number]];
const box = (x: number, y: number, w: number, h: number): Q => [
  [x, y],
  [x + w, y],
  [x + w, y + h],
  [x, y + h],
];
const line = (x: number, y: number, w: number, h: number, text: string) => ({ quad: box(x, y, w, h), text, score: 0.9 });
const para = (text: string, lines: ReturnType<typeof line>[]) => ({ text, lines }) as any;

// two paragraphs: "Hello world" / "foo bar" on two lines that join to "foo bar baz", plus a Chinese line
const R = {
  paragraphs: [
    para("Hello world", [line(10, 10, 110, 20, "Hello world")]),
    para("foo bar baz", [line(10, 50, 70, 20, "foo bar"), line(10, 80, 30, 20, "baz")]),
    para("你好世界", [line(10, 120, 80, 20, "你好世界")]),
  ],
};

test("text equals the editor text (paragraphs joined by newlines)", () => {
  const m = buildTextMap(R);
  assert.equal(m.text, "Hello world\nfoo bar baz\n你好世界");
  assert.deepEqual(
    m.lines.map((l) => m.text.slice(l.start, l.end)),
    ["Hello world", "foo bar", "baz", "你好世界"],
  );
  for (const l of m.lines) {
    assert.equal(l.stops.length, l.end - l.start + 1);
    assert.equal(l.stops[0], 0);
    assert.equal(l.stops.at(-1), 1);
  }
});

test("de-hyphenated line breaks still map", () => {
  const m = buildTextMap({ paragraphs: [para("an example here", [line(0, 0, 50, 10, "an exam-"), line(0, 12, 50, 10, "ple here")])] });
  assert.deepEqual(
    m.lines.map((l) => m.text.slice(l.start, l.end)),
    ["an exam", "ple here"],
  );
});

test("hit test lands on character boundaries", () => {
  const m = buildTextMap(R);
  assert.equal(hitTest(m, 5, 20), 0); // left of the first line → its start
  assert.equal(hitTest(m, 200, 20), "Hello world".length); // right of it → its end
  const mid = hitTest(m, 65, 20)!;
  assert.ok(mid > 3 && mid < 8, `middle of "Hello world" → ${mid}`);
  // Chinese: four equal-width characters over 80 px → boundary every 20 px
  const zh = m.text.indexOf("你");
  assert.equal(hitTest(m, 31, 130), zh + 1);
  assert.equal(hitTest(m, 49, 130), zh + 2);
  // far below everything → end of the last line
  assert.equal(hitTest(m, 500, 900), m.text.length);
  assert.equal(hitTest(buildTextMap({ paragraphs: [] }), 1, 1), null);
});

test("drag selects across lines in reading order, in either direction", () => {
  const m = buildTextMap(R);
  const fwd = dragRange(m, [70, 15], [25, 85])!;
  const back = dragRange(m, [25, 85], [70, 15])!;
  assert.deepEqual(fwd, back);
  const s = m.text.slice(...fwd);
  assert.ok(s.startsWith("world") || s.startsWith(" world") || s.startsWith("o world"), s);
  assert.ok(s.includes("\nfoo bar"), s);
  assert.ok(s.endsWith("b") || s.endsWith("ba"), s);
});

test("double-click range is the whole line", () => {
  const m = buildTextMap(R);
  const r = lineRangeAt(m, 40, 60)!;
  assert.equal(m.text.slice(...r), "foo bar");
});

test("highlight polygons follow the selected part of each line", () => {
  const m = buildTextMap(R);
  const zh = m.text.indexOf("你");
  const polys = rangePolys(m, zh + 1, zh + 3); // 好世
  assert.equal(polys.length, 1);
  const [[x0, y0], [x1], [, y2]] = polys[0];
  assert.equal(x0, 30);
  assert.equal(x1, 70);
  assert.equal(y0, 120);
  assert.equal(y2, 140);
  // a range spanning the paragraph break touches two lines; an empty range none
  assert.equal(rangePolys(m, 6, 15).length, 2);
  assert.equal(rangePolys(m, 4, 4).length, 0);
});

test("rotated lines are handled in their own frame", () => {
  const q: Q = [
    [0, 0],
    [100, 100],
    [90, 110],
    [-10, 10],
  ];
  const m = buildTextMap({ paragraphs: [para("abcd", [{ quad: q, text: "abcd", score: 1 }])] });
  assert.equal(hitTest(m, 50, 52), 2);
});

test("character widths: CJK wide, Latin narrow", () => {
  assert.equal(charWeight("文"), 1);
  assert.equal(charWeight("あ"), 1);
  assert.ok(charWeight("a") < 1);
  assert.ok(charWeight(" ") < charWeight("a"));
});

test("dragging to the end of a rare CJK character selects the whole character", () => {
  const text = "A𠮷";
  const m = buildTextMap({ paragraphs: [para(text, [line(0, 0, 100, 20, text)])] });
  assert.deepEqual(dragRange(m, [0, 10], [100, 10]), [0, text.length]);
});

test("OCR never touches the clipboard unless the user turned auto-copy on", () => {
  assert.equal(shouldAutoCopy(undefined, "text"), false);
  assert.equal(shouldAutoCopy(false, "text"), false);
  assert.equal(shouldAutoCopy(true, "  "), false);
  assert.equal(shouldAutoCopy(true, "text"), true);
});

test("an image selection cannot copy stale OCR text after edits", () => {
  const map = buildTextMap(R);
  assert.equal(imageSelectionText(map, map.text, [0, 5]), "Hello");
  assert.equal(imageSelectionText(map, "Edited text", [0, 5]), "");
  assert.equal(imageSelectionText(map, map.text, [0, 0]), "");
});
