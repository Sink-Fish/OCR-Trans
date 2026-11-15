// Guards for the 0.5.6 fixes from the 0.5.4 audit: the bugs were in wiring (sync commands, permission scopes,
// keyboard handlers), so these tests read the sources the way the audit did.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(import.meta.dirname, "..");
const read = (p: string) => readFileSync(join(ROOT, p), "utf8");

test("commands that create windows are async (sync commands deadlock on Windows, wry#583)", () => {
  const rs = read("src-tauri/src/commands.rs");
  for (const name of ["overlay_ready", "overlay_done", "open_pin", "open_text_window", "open_settings", "open_history", "copy_png"]) {
    assert.match(rs, new RegExp(`pub async fn ${name}\\(`), `${name} must be async`);
  }
});

test("打开下载目录 is allowed by the opener scope", () => {
  const cap = JSON.parse(read("src-tauri/capabilities/default.json"));
  const open = cap.permissions.find((p: unknown) => typeof p === "object" && (p as { identifier: string }).identifier === "opener:allow-open-path");
  assert.ok(open, "opener:allow-open-path needs a scope: without one every path is refused");
  assert.ok(open.allow.some((a: { path: string }) => a.path === "$APPDATA/models"));
  assert.ok(!cap.permissions.includes("opener:allow-open-path"), "no unscoped copy left");
});

test("pin editors ignore the Enter that confirms an input-method composition", () => {
  const pin = read("src/views/Pin.tsx");
  const enters = [...pin.matchAll(/if \(e\.key === "Enter" && !e\.shiftKey\)/g)].length;
  const guards = [...pin.matchAll(/if \(e\.nativeEvent\.isComposing \|\| e\.keyCode === 229\) return;/g)].length;
  assert.equal(enters, 2);
  assert.equal(guards, enters, "every Enter-to-submit handler checks isComposing first");
});

test("asking for an open pin again focuses it instead of failing on the duplicate label", () => {
  const rs = read("src-tauri/src/windows.rs");
  const body = rs.slice(rs.indexOf("pub fn open_pin_with"));
  assert.ok(body.indexOf('get_webview_window(&format!("pin-{id}"))') < body.indexOf("WebviewWindowBuilder::new"));
});

test("windows save settings field by field (a stale copy must not undo other windows' changes)", () => {
  for (const view of ["src/views/Settings.tsx", "src/views/Onboarding.tsx"]) {
    assert.ok(!read(view).includes("api.setSettings("), `${view} writes the whole settings object`);
    assert.ok(read(view).includes("api.patchSettings("), view);
  }
});

test("a bare S does not pin", () => {
  const ov = read("src/views/Overlay.tsx");
  assert.ok(!/k === "p" \|\| k === "s"/.test(ov), "a bare S no longer pins");
});

test("model downloads name fixed revisions and are verified", () => {
  // (the Rust unit tests at the end spell out the forbidden URLs themselves)
  const rs = read("src-tauri/src/models.rs").split("#[cfg(test)]")[0];
  assert.ok(!/resolve\/(main|master)\//.test(rs), "no moving branch in a download URL");
  const specs = [...rs.matchAll(/FileSpec \{\s*url:[\s\S]*?sha256: (?:None|Some\("[0-9a-f]*"\))/g)].map((m) => m[0]);
  assert.ok(specs.length >= 12, `${specs.length} files`);
  for (const f of specs) assert.match(f, /size: [\d_]+/, f);
  for (const big of ["det.onnx", "rec.onnx", "model.bin", "model.gguf", "source.spm", "target.spm", "sentencepiece.bpe.model"]) {
    for (const f of specs.filter((s) => s.includes(`path: "${big}"`))) assert.match(f, /sha256: Some\("[0-9a-f]{64}"\)/, f);
  }
});

test("the app ships its third-party notices", () => {
  const conf = JSON.parse(read("src-tauri/tauri.conf.json"));
  assert.equal(conf.bundle.resources["licenses/THIRD-PARTY-NOTICES.txt"], "licenses/THIRD-PARTY-NOTICES.txt");
  const notices = read("src-tauri/licenses/THIRD-PARTY-NOTICES.txt");
  for (const name of ["ONNX Runtime", "CTranslate2", "llama.cpp", "SentencePiece", "opus-mt-en-zh", "CC BY 4.0", "Qwen3-1.7B", "react"]) {
    assert.ok(notices.includes(name), name);
  }
});

test("pins: only the latest translation request updates the view; pinch zooms instead of fading", () => {
  const pin = read("src/views/Pin.tsx");
  assert.match(pin, /if \(my !== request\.current\) return;/);
  assert.match(pin, /if \(e\.altKey \|\| e\.metaKey\) \{/);
});

test("pin annotation drags keep pointer capture and PNG export composes only committed artwork", () => {
  const pin = read("src/views/Pin.tsx");
  assert.match(pin, /setPointerCapture\(e\.pointerId\)/);
  assert.match(pin, /onLostPointerCapture=.*cancelPointer/);
  const png = pin.slice(pin.indexOf("async function pngBytes"), pin.indexOf("function openText"));
  assert.match(png, /drawAnnotations\(canvas, base, shapes, font\)/);
  assert.doesNotMatch(png, /canvasRef|drawSelection/);
});

test("clearing history invalidates captures still being saved", () => {
  const history = read("src-tauri/src/history.rs");
  assert.match(history, /PENDING\.lock_safe\(\)\.insert\(id, PendingText::default\(\)\);/);
  assert.match(history, /if !pending\.contains_key\(&item\.id\) \{[\s\S]*?remove_files\(app, item\.id\);\s*return Ok\(\(\)\);/);
  assert.match(history.slice(history.indexOf("pub fn clear")), /pending\.clear\(\)/);
});
