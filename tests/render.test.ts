import { test } from "node:test";
import assert from "node:assert/strict";
import type { Paragraph, PipelineResult, TextLine } from "../src/lib/api.ts";
import { renderTranslated } from "../src/lib/render.ts";

function sourceLine(x: number, y: number, w: number, h: number): TextLine {
  return { quad: [[x, y], [x + w, y], [x + w, y + h], [x, y + h]], text: "Source", score: 0.99 };
}

function paint(paragraph: Paragraph) {
  const draws: { text: string; x: number; y: number; maxWidth: number }[] = [];
  const clips: number[][] = [];
  const ctx = {
    font: "",
    drawImage() {},
    save() {},
    restore() {},
    beginPath() {},
    clip() {},
    rect(...box: number[]) { clips.push(box); },
    measureText(text: string) {
      return { width: [...text].length * parseFloat(this.font), actualBoundingBoxAscent: 8, actualBoundingBoxDescent: 2 };
    },
    fillText(text: string, x: number, y: number, maxWidth: number) { draws.push({ text, x, y, maxWidth }); },
  };
  const canvas = { width: 0, height: 0, getContext: () => ctx };
  const res: PipelineResult = { width: 150, height: 100, paragraphs: [paragraph], timings: {}, warnings: [], engine: "nmt" };
  const document = Object.getOwnPropertyDescriptor(globalThis, "document");
  Object.defineProperty(globalThis, "document", { configurable: true, value: { createElement: () => canvas } });
  try {
    renderTranslated({ width: res.width, height: res.height } as HTMLCanvasElement, res, "sans-serif");
    return { draws, clips };
  } finally {
    if (document) Object.defineProperty(globalThis, "document", document);
    else Reflect.deleteProperty(globalThis, "document");
  }
}

test("a long translation stays on the original table row and inside its source box", () => {
  const text = "这个译文比原来的表格文字长很多";
  const { draws, clips } = paint({
    bbox: [10, 20, 26, 14], lines: [sourceLine(10, 20, 26, 14)], line_height: 14,
    text: "Source", translation: text, lang: "en", bg: "#ffffff", fg: "#000000", fill: "inpaint",
  });
  assert.deepEqual(draws, [{ text, x: 10, y: 30, maxWidth: 26 }]);
  assert.deepEqual(clips, [[10, 20, 26, 14]]);
});

test("a wrapped paragraph uses the original line starts and heights", () => {
  const { draws } = paint({
    bbox: [5, 6, 37, 38], lines: [sourceLine(12, 6, 30, 12), sourceLine(5, 32, 37, 12)], line_height: 12,
    text: "Source Source", translation: "甲乙丙丁戊己庚", lang: "en", bg: "#ffffff", fg: "#000000", fill: "inpaint",
  });
  assert.deepEqual(draws.map(({ x, y, maxWidth }) => [x, y, maxWidth]), [[12, 15, 30], [5, 41, 37]]);
  assert.equal(draws.map(({ text }) => text).join(""), "甲乙丙丁戊己庚");
});
