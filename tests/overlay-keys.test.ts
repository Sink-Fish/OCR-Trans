// 0.5.7: Enter / Esc / 1–4 registered as global shortcuts while the selection overlay was up (0.5.6) took those keys
// from every other app — including macOS's screen-recording confirmation, which the overlay also covered — and the
// overlay could outlive its capture. These guards read the sources; scripts/check_overlay_keys.sh runs the real app.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(import.meta.dirname, "..");
const read = (p: string) => readFileSync(join(ROOT, p), "utf8");
/** Rust source without comments, so explanations of the old behaviour don't trip the checks. */
const code = (p: string) => read(p).replace(/\/\/.*$/gm, "");

test("the only global shortcuts are the configured hotkeys", () => {
  const hk = code("src-tauri/src/hotkeys.rs");
  assert.ok(!/OVERLAY_KEYS|set_overlay_keys|overlay-key/.test(hk), "no overlay keys registered globally");
  // every registration goes through register_all_now, which takes the keys from the settings only
  const regs = [...hk.matchAll(/gs\.register\(/g)].length;
  assert.equal(regs, 1, "one register call (the configured hotkeys)");
  assert.match(hk, /Ok\(sc\) if !allowed_global\(&sc\)/, "bare keys refused even if typed into settings.json");
  // nothing else touches the plugin (selftest.rs registers Enter once, on purpose, to prove its probe works)
  for (const f of readdirSync(join(ROOT, "src-tauri/src")).filter((n) => n.endsWith(".rs") && n !== "hotkeys.rs" && n !== "selftest.rs")) {
    assert.ok(!/global_shortcut\(\)|\.on_shortcuts?\(/.test(code(`src-tauri/src/${f}`)), `${f} uses the global-shortcut plugin`);
  }
});

test("the overlay page reads its keys from its own window only", () => {
  const ov = read("src/views/Overlay.tsx");
  assert.ok(!ov.includes('"overlay-key"'), "no keys forwarded from global shortcuts");
  assert.match(ov, /window\.addEventListener\("keydown", f\)/);
});

test("hotkeys are (re-)registered on the main thread", () => {
  const hk = code("src-tauri/src/hotkeys.rs");
  assert.match(hk, /pub fn register_all\(app: &AppHandle\) -> Vec<String> \{\s*crate::on_main_thread\(app, register_all_now\)/);
});

test("every way out of the overlay hides it and cancels its capture", () => {
  const rs = code("src-tauri/src/windows.rs");
  const capture = rs.slice(rs.indexOf("fn prepare_capture"), rs.indexOf("fn overlay_window"));
  assert.match(capture, /drop_overlay\(app, None\)[\s\S]*capture_worker::capture/, "an overlay still up is cancelled before a new capture");
  assert.match(capture, /guard_overlay\(app, token,/);
  const start = rs.slice(rs.indexOf("pub fn start_capture"), rs.indexOf("fn capture_and_show"));
  assert.match(start, /if let Err\(e\) = res \{[\s\S]*overlay_cancel\(&app, None\)/, "a failed capture leaves nothing on screen");
  const guard = rs.slice(rs.indexOf("fn guard_overlay"), rs.indexOf("fn overlay_window"));
  assert.match(guard, /overlay_focus_detail\(&app\)/, "focus is watched");
  assert.match(guard, /let foreign = f\.frontmost > 0 && f\.frontmost != me && f\.frontmost != before;/, "another app in front counts even before the overlay had the key");
  assert.match(guard, /f\.active && f\.own_other_key[\s\S]*focus_overlay\(app, token\)/, "our own window taking the key is not a loss");
  assert.match(guard, /lost >= limit && overlay_cancel\(&app, Some\(token\)\)/, "the overlay gives way when the focus is taken");
  const ready = rs.slice(rs.indexOf("pub fn overlay_ready"), rs.indexOf("pub fn overlay_cancel"));
  assert.match(ready, /t == s\.token[\s\S]*focus_overlay\(app, token\)/, "only the current frame gets focus");
  assert.doesNotMatch(ready, /\.show\(/, "a ready notification cannot show a window");
  const done = rs.slice(rs.indexOf("pub fn overlay_done"), rs.indexOf("fn finish_selection"));
  assert.match(done, /if !live \{\s*overlay_cancel\(app, token\)/, "a failed selection without a usable capture cancels");
});

test("a failed selection keeps the page usable instead of blanking a kept overlay", () => {
  const ov = read("src/views/Overlay.tsx");
  const run = ov.slice(ov.indexOf("function run("), ov.indexOf("function finish("));
  assert.ok(!run.includes(".finally(clear)"), "clearing after an error left a blank full-screen window");
  assert.match(run, /\.then\([\s\S]*if \(token\.current === my\) clear\(\)/);
  assert.match(run, /\.catch\([\s\S]*if \(token\.current !== my\) return;[\s\S]*busy\.current = false/);
  assert.match(ov, /listen<number>\("overlay-cancelled",[\s\S]*if \(token\.current !== payload\) return;/);
});

test("errors reach the log file, not only stderr", () => {
  const lib = read("src-tauri/src/lib.rs");
  assert.match(lib, /pub fn log_error\(msg: &str\)/);
  const rs = read("src-tauri/src/windows.rs");
  assert.match(rs, /crate::log_error\(&format!\("capture failed: \{e:#\}"\)\)/);
  assert.match(read("src-tauri/src/commands.rs"), /pub fn log\(window: tauri::WebviewWindow, msg: String\) \{\s*crate::log_info/);
});

test("the hotkey recorder refuses keys that would be taken from every app", () => {
  const st = read("src/views/Settings.tsx");
  assert.match(st, /if \(!mods\.some\(\(m\) => m !== "Shift"\) && !key\.startsWith\("F"\)\) return null;/);
});

test("the keyboard goes back to the user's app when the overlay ends without a window of ours", () => {
  const rs = code("src-tauri/src/windows.rs");
  const cancel = rs.slice(rs.indexOf("pub fn overlay_cancel"), rs.indexOf("fn drop_overlay"));
  assert.match(cancel, /if drop_overlay\(app, token\) \{\s*give_focus_back\(app\);/);
  const finish = rs.slice(rs.indexOf("fn finish_selection"), rs.indexOf("pub fn resolve_action"));
  assert.match(finish, /write_image[\s\S]*restore_selection_focus\(app, focus_before\)/, "复制");
  assert.match(finish, /blocking_save_file[\s\S]*restore_selection_focus\(&app, focus_before\)/, "保存");
  assert.match(finish, /FOCUS_BEFORE\.swap\(0, Ordering::SeqCst\)[\s\S]*let id = add_pin/, "a pin / text window keeps the focus");
  const start = rs.slice(rs.indexOf("pub fn start_capture"), rs.indexOf("fn capture_and_show"));
  assert.match(start, /if let Some\(pid\) = frontmost_other_pid\(app\) \{\s*FOCUS_BEFORE\.store\(pid/);
  // only while we still hold the focus: a system prompt that took it keeps it
  assert.match(rs, /let active: bool = msg_send!\[app, isActive\];\s*if !active \{\s*return;/);
});

test("closing the last pin or text window gives the keyboard back; the toast never takes it", () => {
  const rs = code("src-tauri/src/windows.rs");
  const close = rs.slice(rs.indexOf("pub fn close_safely"), rs.indexOf("pub fn close_all_pins"));
  assert.match(close, /!ours_left[\s\S]*give_focus_to\(app, pid\)/);
  const toast = rs.slice(rs.indexOf("pub fn show_toast_for"), rs.indexOf("pub fn with_pins_lowered"));
  assert.match(toast, /\.focusable\(false\)/);
});
