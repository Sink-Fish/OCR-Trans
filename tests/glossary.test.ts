import { test } from "node:test";
import assert from "node:assert/strict";
import { formatGlossary, parseGlossary } from "../src/lib/glossary.ts";

test("glossary preserves separators inside translations", () => {
  const entries: [string, string][] = [["equation", "a = b → c"], ["path", "开始 -> 结束"], ["Pin", "钉图"]];
  assert.deepEqual(parseGlossary(formatGlossary(entries)), { entries, invalidLines: [] });
  assert.deepEqual(parseGlossary("one → 一\ntwo -> 二\nthree\t三").entries, [["one", "一"], ["two", "二"], ["three", "三"]]);
});

test("invalid glossary lines are reported instead of silently deleting existing terms", () => {
  assert.deepEqual(parseGlossary("\nPin = 钉图\nincomplete\nSettings =\n= 无原文\n"), {
    entries: [["Pin", "钉图"]], invalidLines: [3, 4, 5],
  });
  assert.deepEqual(parseGlossary(" \n\t"), { entries: [], invalidLines: [] });
});
