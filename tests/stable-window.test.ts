// Regression guard for the 0.5.1 OCR-popup crash: getCurrentWindow() inside a component returns a new object per
// render; used as a hook dependency it re-ran effects forever (OCR + clipboard + show/focus loop → WebKit crash).
// Pages must use the module-level `win` from src/lib/win.ts.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const SRC = join(import.meta.dirname, "..", "src");

function files(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? files(join(dir, e.name)) : /\.tsx?$/.test(e.name) ? [join(dir, e.name)] : [],
  );
}

test("getCurrentWindow() is only called once, in src/lib/win.ts", () => {
  for (const f of files(SRC)) {
    if (f.endsWith(join("lib", "win.ts"))) continue;
    const src = readFileSync(f, "utf8");
    assert.ok(!/getCurrentWindow\s*\(/.test(src), `${f} calls getCurrentWindow(); import { win } from lib/win instead`);
  }
});

test("no hook lists `win` as a dependency", () => {
  for (const f of files(SRC)) {
    const src = readFileSync(f, "utf8");
    assert.ok(!/\[[^\]]*\bwin\b[^\]]*\]\s*\)/.test(src), `${f}: \`win\` in a dependency list`);
  }
});
