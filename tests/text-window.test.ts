// Guards for the merged text window (0.5.3): one window for 识字, no silent clipboard writes, and no effect that can
// re-run OCR / clipboard / focus in a loop the way the 0.5.1 popup did.
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(import.meta.dirname, "..");
const read = (p: string) => readFileSync(join(ROOT, p), "utf8");
const TW = read("src/views/TextWindow.tsx");

test("the old OCR popup and the pin's own copy-text path are gone", () => {
  assert.ok(!existsSync(join(ROOT, "src/views/OcrPopup.tsx")));
  const pin = read("src/views/Pin.tsx");
  assert.ok(!pin.includes("api.copyText("), "the pin copies text only through the text window");
  assert.ok(pin.includes("api.openTextWindow("), "the pin's 识字 button opens the text window");
  const rs = read("src-tauri/src/windows.rs");
  assert.ok(!rs.includes("open_ocr_popup"), "one Rust entry point: open_text_window");
  assert.match(rs, /"ocr" => open_text_window\(/);
});

test("only an explicit OCR retry can restart a text-window effect", () => {
  const deps = [...TW.matchAll(/use(?:Layout)?Effect\([\s\S]*?\n {2}\}, \[([^\]]*)\]\);/g)].map((m) => m[1].trim());
  assert.ok(deps.length >= 3, `found ${deps.length} effects`);
  for (const d of deps) assert.ok(d === "" || d === "id" || d === "id, ocrAttempt", `unexpected effect dependencies [${d}]`);
  assert.equal(deps.filter((d) => d === "id, ocrAttempt").length, 1);
  assert.match(TW, /function retryOcr\(\)[\s\S]*?setOcrAttempt\(\(n\) => n \+ 1\);/);
});

test("the text window writes the clipboard only on user actions or with auto-copy on", () => {
  // every copyText call sits in a user handler (copy / copyAllAndClose) or behind shouldAutoCopy()
  const calls = [...TW.matchAll(/api\.copyText\(/g)].map((m) => m.index!);
  assert.ok(calls.length >= 1);
  for (const at of calls) {
    const before = TW.slice(Math.max(0, at - 400), at);
    const lastFn = [...before.matchAll(/(async function (\w+)|shouldAutoCopy\()/g)].at(-1)?.[0] ?? "";
    assert.ok(/copy\b|copyAllAndClose|shouldAutoCopy\(/.test(lastFn), `copyText outside a user action: …${before.slice(-120)}`);
  }
});
