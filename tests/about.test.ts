// 设置 → 关于: developer line, optional website link, and the full third-party notices inside the app.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { DEVELOPER, WEBSITE_URL, websiteLink } from "../src/lib/about.ts";

const ROOT = join(import.meta.dirname, "..");
const read = (p: string) => readFileSync(join(ROOT, p), "utf8");

test("developer is SinkFish", () => {
  assert.equal(DEVELOPER, "SinkFish");
});

test("an empty website hides the link", () => {
  assert.equal(websiteLink(""), null);
  assert.equal(websiteLink("   "), null);
  // the shipped constant is either empty (hidden) or a valid http(s) address
  assert.ok(WEBSITE_URL === "" || websiteLink(WEBSITE_URL) !== null);
});

test("only http(s) addresses become links", () => {
  assert.deepEqual(websiteLink("https://sinkfish.example/"), { href: "https://sinkfish.example/", label: "sinkfish.example" });
  assert.deepEqual(websiteLink(" https://example.com/ocr-trans "), { href: "https://example.com/ocr-trans", label: "example.com/ocr-trans" });
  assert.equal(websiteLink("http://example.com")?.href, "http://example.com/");
  for (const bad of ["javascript:alert(1)", "file:///etc/passwd", "example.com", "mailto:a@b.c"]) {
    assert.equal(websiteLink(bad), null, bad);
  }
});

test("the notices viewer reads the bundled file on both platforms", () => {
  const rs = read("src-tauri/src/commands.rs");
  assert.match(rs, /pub async fn third_party_notices\(/);
  assert.match(read("src-tauri/src/lib.rs"), /commands::third_party_notices,/);
  for (const conf of ["src-tauri/tauri.conf.json", "src-tauri/tauri.windows.conf.json"]) {
    const res = JSON.parse(read(conf)).bundle.resources;
    assert.equal(res["licenses/THIRD-PARTY-NOTICES.txt"], "licenses/THIRD-PARTY-NOTICES.txt", conf);
  }
});

test("the notices carry the attributions the licences ask for", () => {
  const n = read("src-tauri/licenses/THIRD-PARTY-NOTICES.txt");
  assert.match(n, /Helsinki-NLP/);
  assert.match(n, /creativecommons\.org\/licenses\/by\/4\.0/);
  assert.match(n, /Source Han Sans/);
  assert.match(n, /SIL Open Font License/);
  assert.match(n, /NOTICE/);
});

test("the notices are complete: header licences, MPL sources, patents", () => {
  const n = read("src-tauri/licenses/THIRD-PARTY-NOTICES.txt");
  assert.match(n, /avx_mathfun\.h \(licence comment at the top of the file\)/);
  assert.match(n, /Marcin Junczys-Dowmunt/);
  assert.match(n, /Mozilla Public License 2\.0 crates are used unmodified; their source code is available at:/);
  assert.match(n, /Alliance for Open Media Patent License/);
  assert.match(n, /ONNX Runtime third-party notices/);
  assert.ok(read("docs/LICENSE_INVENTORY.md").includes("scripts/gen_notices.py"));
});

test("the public source and installed notices carry the noncommercial terms", () => {
  assert.equal(JSON.parse(read("package.json")).license, "SEE LICENSE IN LICENSE");
  for (const toml of ["src-tauri/Cargo.toml", "crates/core/Cargo.toml"]) {
    assert.match(read(toml), /^license = "LicenseRef-OCR-Trans-Noncommercial-1.0"$/m, toml);
  }
  const terms = read("LICENSE").trim();
  assert.match(terms, /企业内部业务/);
  assert.ok(read("src-tauri/licenses/THIRD-PARTY-NOTICES.txt").includes(terms));
});

test("the Windows installer requires the shared Visual C++ runtime before installing the app", () => {
  const conf = JSON.parse(read("src-tauri/tauri.windows.conf.json"));
  assert.equal(conf.bundle.windows.nsis.installerHooks, "windows/installer-hooks.nsh");
  const nsh = read("src-tauri/windows/installer-hooks.nsh");
  assert.match(nsh, /!insertmacro OCRTRANS_VCREDIST_PRESENT/);
  assert.match(nsh, /https:\/\/learn\.microsoft\.com\/cpp\/windows\/latest-supported-vc-redist/);
  assert.match(nsh, /SetErrorLevel 1603/);
  assert.doesNotMatch(nsh, /^\s*File\s|vc_redist\.x64\.exe/mi);
});
