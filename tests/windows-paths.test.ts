// 0.6.1: on Windows Tauri reports the app's folder as \\?\D:\…; CTranslate2 opens <dir> + "/model.bin", which a
// \\?\ path does not allow, so the bundled translation model failed and 设置 showed the \\?\ form. These guards read
// the sources (the path logic itself has Rust unit tests in paths.rs and settings.rs).
import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(import.meta.dirname, "..");
const read = (p: string) => readFileSync(join(ROOT, p), "utf8");
const code = (p: string) => read(p).replace(/\/\/.*$/gm, "");

test("the resource folder is only taken through paths::resource_dir (ordinary form)", () => {
  for (const f of readdirSync(join(ROOT, "src-tauri/src")).filter((n) => n.endsWith(".rs") && n !== "paths.rs")) {
    assert.ok(!/\.resource_dir\(\)|BaseDirectory::Resource/.test(code(`src-tauri/src/${f}`)), `${f} uses Tauri's resource path directly`);
  }
  assert.match(code("src-tauri/src/paths.rs"), /app\.path\(\)\.resource_dir\(\)\.ok\(\)\.map\(\|p\| plain\(&p\)\)/);
  assert.match(code("src-tauri/src/models.rs"), /crate::paths::resource_dir\(app\)/);
  assert.match(code("src-tauri/src/models.rs"), /crate::paths::plain\(Path::new\(&d\)\)/, "a saved custom folder too");
});

test("resource files are joined component by component", () => {
  const lib = code("src-tauri/src/lib.rs");
  assert.match(lib, /\.join\("icons"\)\.join\("tray\.png"\)/);
  assert.ok(!/join\("icons\/tray\.png"\)/.test(lib));
  assert.match(code("src-tauri/src/commands.rs"), /\.join\("licenses"\)\.join\("THIRD-PARTY-NOTICES\.txt"\)/);
  assert.ok(!read("src/views/Settings.tsx").includes("resolveResource("), "the notices file is opened by the backend");
});

test("saved model folders are cleaned on load and on save", () => {
  const st = code("src-tauri/src/settings.rs");
  assert.match(st, /clean_models_dir\(s\.models_dir\.as_deref\(\), cfg!\(windows\), true\)/);
  assert.match(code("src-tauri/src/commands.rs"), /clean_models_dir\(value\.models_dir\.as_deref\(\), cfg!\(windows\), false\)/);
});

test("Windows manifest keeps Common Controls v6 and adds the UTF-8 code page", () => {
  const m = read("src-tauri/windows/app.manifest");
  assert.match(m, /name="Microsoft\.Windows\.Common-Controls"/);
  assert.match(m, /<activeCodePage xmlns="http:\/\/schemas\.microsoft\.com\/SMI\/2019\/WindowsSettings">UTF-8<\/activeCodePage>/);
  assert.match(read("src-tauri/build.rs"), /app_manifest\(include_str!\("windows\/app\.manifest"\)\)/);
});
