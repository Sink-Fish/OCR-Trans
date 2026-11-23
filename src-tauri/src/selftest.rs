//! Overlay lifecycle self-test (dev hook, scripts/check_overlay_keys.sh):
//!
//!   OCRTRANS_TEST_OVERLAY_KEYS=1 OCRTRANS_TEST_SHOT=<png>
//!
//! Drives the selection overlay through every way it can end — Esc, a capture that fails while an overlay is up
//! (hotkey pressed again), a failing capture on its own, a selection clipped to nothing, a finished selection, one
//! of our own windows taking the key (must not end it), the focus going to another app, many quick cycles — and checks after each step that the overlay is gone (or kept,
//! where it should be) and that this process holds no plain key (Enter, Esc, 1–4) as a system-wide hotkey.
//! Writes `keytest …` lines to the log file and quits.

use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::state::{state, LockExt, Mode};
use crate::windows;

fn wait(cond: impl Fn() -> bool, ms: u64) -> bool {
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(ms) {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    cond()
}

/// Plain keys held as global hotkeys by this process (Carbon's view, see hotkeys::plain_keys_held).
fn held(app: &AppHandle) -> Vec<&'static str> {
    #[cfg(target_os = "macos")]
    return crate::on_main_thread(app, |_| crate::hotkeys::plain_keys_held()).unwrap_or_else(|| vec!["(main thread busy)"]);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        vec![]
    }
}

fn token(app: &AppHandle) -> Option<u32> {
    state(app).shot.lock_safe().as_ref().map(|s| s.token)
}

fn ready(app: &AppHandle) -> bool {
    state(app).shot.lock_safe().as_ref().is_some_and(|s| s.ready)
}

fn visible(app: &AppHandle) -> bool {
    #[cfg(target_os = "macos")]
    return windows::overlay_onscreen(app);
    #[cfg(not(target_os = "macos"))]
    app.get_webview_window("overlay").and_then(|w| w.is_visible().ok()).unwrap_or(false)
}

/// Exercise the real WindowServer check by ordering out a just-painted native window.
#[cfg(target_os = "macos")]
pub(crate) static ORDER_OUT_NEXT_OVERLAY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// No capture left and no overlay on screen, nothing being captured.
fn gone(app: &AppHandle) -> bool {
    token(app).is_none() && !visible(app) && !state(app).capturing.load(std::sync::atomic::Ordering::SeqCst)
}

/// Start a capture (the hotkey's path) and wait until its page has painted.
fn capture(app: &AppHandle) -> bool {
    let t0 = Instant::now();
    let before = token(app);
    windows::start_capture(app, Mode::Select);
    // a new capture replaces one still up: wait for the new one, not the old one that is still ready
    let ok = wait(|| !state(app).capturing.load(std::sync::atomic::Ordering::SeqCst)
        && token(app).is_some() && token(app) != before && ready(app) && visible(app), 4000);
    if !ok {
        crate::log_info(&format!(
            "keytest note: capture not ready after {} ms (capture={:?} ready={} visible={} capturing={})",
            t0.elapsed().as_millis(),
            token(app),
            ready(app),
            visible(app),
            state(app).capturing.load(std::sync::atomic::Ordering::SeqCst)
        ));
    }
    ok
}

fn failing_capture(app: &AppHandle) {
    windows::FAIL_NEXT_CAPTURE.store(true, std::sync::atomic::Ordering::SeqCst);
    windows::start_capture(app, Mode::Select);
    wait(|| !state(app).capturing.load(std::sync::atomic::Ordering::SeqCst), 4000);
}

pub fn overlay_keys(app: &AppHandle) {
    std::thread::sleep(Duration::from_secs(2)); // setup and the overlay page's first load
    let mut fails: Vec<String> = vec![];
    let mut check = |name: &str, ok: bool, detail: String| {
        crate::log_info(&format!("keytest {} {name} {detail}", if ok { "ok  " } else { "FAIL" }));
        if !ok {
            fails.push(name.to_string());
        }
    };
    let none_held = |app: &AppHandle| {
        let h = held(app);
        (h.is_empty(), format!("held={h:?}"))
    };

    // 0. the probe itself sees a real registration (otherwise every later "nothing held" would prove nothing)
    let gs = app.global_shortcut();
    let reg = gs.register("Enter");
    let seen = held(app);
    let _ = gs.unregister("Enter");
    let after = held(app);
    check("probe sees a registered Enter", reg.is_ok() && seen.contains(&"Return") && after.is_empty(), format!("register={reg:?} seen={seen:?} after={after:?}"));

    let (ok, d) = none_held(app);
    check("idle: no plain key held", ok, d);

    // A missing registration must be recoverable without a process restart. Run on the UI
    // thread, just like the tray command, including the native plugin calls.
    let recovered = crate::on_main_thread(app, |app| {
        let key = state(app).settings.lock_safe().hotkey_capture.clone();
        let gs = app.global_shortcut();
        if key.is_empty() || gs.unregister(key.as_str()).is_err() || gs.is_registered(key.as_str()) {
            return false;
        }
        crate::hotkeys::repair(app, "self-test: registration lost", false);
        gs.is_registered(key.as_str())
    }).unwrap_or(false);
    check("lost hotkey registration is restored without restart", recovered, String::new());

    #[cfg(target_os = "macos")]
    if crate::isolated_menu_bar_profile() {
        let absent = crate::on_main_thread(app, |app| {
            crate::check_status_item(app, true);
            app.tray_by_id("main").is_none()
        }).unwrap_or(false);
        check("isolated profile has no system menu item after reopen", absent, String::new());
    } else {
        let recovered = crate::on_main_thread(app, |app| {
            let tray = app.tray_by_id("main")?;
            tray.set_visible(false).ok()?;
            if tray.rect().ok()?.is_some() { return Some(false); }
            crate::check_status_item(app, false);
            Some(tray.rect().ok()?.is_some())
        }).flatten().unwrap_or(false);
        check("detached status item reattaches", recovered, String::new());
    }

    // 1. overlay up: the keys are the page's, not global
    let up = capture(app);
    check("overlay shows", up, String::new());
    let (ok, d) = none_held(app);
    check("overlay up: no plain key held", ok, d);

    // 2. Esc (the page cancels with its token)
    windows::overlay_cancel(app, token(app));
    let g = wait(|| gone(app), 1500);
    let (ok, d) = none_held(app);
    check("Esc: overlay gone, no plain key held", g && ok, d);

    #[cfg(target_os = "macos")]
    {
        let first_token = state(app).next_id.load(std::sync::atomic::Ordering::Relaxed);
        ORDER_OUT_NEXT_OVERLAY.store(true, std::sync::atomic::Ordering::SeqCst);
        let up = capture(app);
        let replacement = token(app);
        check("offscreen painted window is replaced on the same capture request",
            up && replacement.is_some_and(|t| t != first_token),
            format!("initial={first_token}, onscreen={replacement:?}"));
        windows::overlay_cancel(app, replacement);
    }

    // 2b. …and the keyboard goes back to the app the user was in (Finder stands in for it), not to our windowless app
    if let Some(finder) = std::process::Command::new("pgrep").args(["-x", "Finder"]).output().ok().and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<i32>().ok()) {
        windows::give_focus_to(app, finder);
        wait(|| windows::frontmost_other_pid(app) == Some(finder), 1500);
        let before = windows::frontmost_other_pid(app);
        capture(app);
        let took = wait(|| windows::overlay_focus(app) == Some((true, true)), 2500);
        windows::overlay_cancel(app, token(app));
        let back = wait(|| windows::frontmost_other_pid(app) == before, 1500);
        check(
            "Esc: the focus returns to the app the user was in",
            before.is_some() && took && back,
            format!("before={before:?} overlay focused={took} now={:?}", windows::frontmost_other_pid(app)),
        );

        let focused = || windows::overlay_focus(app) == Some((true, true));
        capture(app);
        let original = token(app);
        let mut recovered = false;
        if let Some(pid) = before {
            if wait(focused, 2500) {
                // Let the guard observe the key window, then reproduce AppKit giving
                // the source app focus without a key press or click.
                std::thread::sleep(Duration::from_millis(250));
                windows::give_focus_to(app, pid);
                let lost = wait(|| windows::frontmost_other_pid(app) == before, 1000);
                recovered = lost && wait(focused, 1600) && token(app) == original && visible(app);
                if recovered {
                    // Model an additional key press which the overlay did not consume.
                    // This may veto activating the window, but the frame must remain selectable.
                    #[cfg(target_os = "macos")]
                    if let Some(shot) = state(app).shot.lock_safe().as_mut() {
                        shot.input_presses = shot.input_presses.wrapping_sub(1);
                    }
                    windows::give_focus_to(app, pid);
                }
            }
        }
        check("source app reactivated: same selection regains focus", recovered, format!("before={before:?}"));
        let source_active = wait(|| windows::frontmost_other_pid(app) == before, 1000);
        std::thread::sleep(Duration::from_millis(1400));
        check("source reactivation after input keeps the same selectable frame",
            recovered && source_active && token(app) == original && visible(app), String::new());
        windows::overlay_cancel(app, token(app));
    }

    // 3. hotkey pressed again while the overlay is up, and that capture fails (0.5.6 left the first one's state
    //    and keys behind, with its window hidden)
    capture(app);
    failing_capture(app);
    let g = wait(|| gone(app), 1500);
    let (ok, d) = none_held(app);
    check("second press + failing capture: overlay gone, no plain key held", g && ok, d);

    // 4. a failing capture with nothing on screen
    failing_capture(app);
    let g = wait(|| gone(app), 1500);
    let (ok, d) = none_held(app);
    check("failing capture: nothing left on screen, no plain key held", g && ok, d);

    // 5. a selection clipped to nothing keeps the overlay (and its capture) for another try; then Esc
    capture(app);
    let t = token(app);
    let r = windows::overlay_done(app, t, -500.0, -500.0, 100.0, 100.0, None);
    let kept = r.is_err() && token(app) == t && visible(app);
    check("selection clipped to nothing: overlay kept", kept, format!("{r:?}"));
    windows::overlay_cancel(app, t);
    check("…then Esc: overlay gone", wait(|| gone(app), 1500), String::new());

    // 6. a finished selection (钉图): the overlay hides, a pin opens; closing it gives the keyboard back to the app
    //    the user was in. The test's history entry is removed again.
    let before = windows::frontmost_other_pid(app);
    capture(app);
    let r = windows::overlay_done(app, token(app), 100.0, 100.0, 240.0, 140.0, Some("pin"));
    let g = wait(|| gone(app), 1500);
    let (ok, d) = none_held(app);
    check("finished selection: overlay gone, no plain key held", r.is_ok() && g && ok, format!("{r:?} {d}"));
    wait(|| app.webview_windows().iter().any(|(l, w)| l.starts_with("pin-") && w.is_visible().unwrap_or(false)), 3000);
    let hids: Vec<u64> = state(app).pins.lock_safe().values().filter_map(|p| p.history_id).collect();
    windows::close_all_pins(app);
    for id in hids {
        let _ = crate::history::delete(app, id);
    }
    if before.is_some() {
        let back = wait(|| windows::frontmost_other_pid(app) == before, 1500);
        check("closing the last pin: the focus returns to the app the user was in", back, format!("before={before:?} now={:?}", windows::frontmost_other_pid(app)));
    }

    // 6b. one of our own windows taking the key while the overlay is up (the previous capture's pin showing itself
    //     late) is not a focus loss: the overlay stays and takes the key back
    wait(|| !app.webview_windows().keys().any(|l| l.starts_with("pin-")), 3000);
    capture(app);
    let t = token(app);
    if wait(|| windows::overlay_focus(app) == Some((true, true)), 2500) {
        let img = image::RgbaImage::from_pixel(160, 90, image::Rgba([40, 120, 200, 255]));
        let id = windows::add_pin(app, img, 2.0, [200.0, 200.0, 80.0, 45.0]).expect("test image fits pin budget");
        let _ = windows::open_pin(app, id, false);
        let pin_shown = wait(|| app.get_webview_window(&format!("pin-{id}")).and_then(|w| w.is_visible().ok()).unwrap_or(false), 3000);
        std::thread::sleep(Duration::from_millis(800));
        let kept = token(app) == t && visible(app);
        let back = wait(|| windows::overlay_focus(app) == Some((true, true)), 1500);
        check("own pin window taking the key: overlay kept and focused again", pin_shown && kept && back, format!("pin shown={pin_shown} kept={kept} refocused={back}"));
        windows::overlay_cancel(app, token(app));
        windows::close_all_pins(app);
        wait(|| !app.webview_windows().keys().any(|l| l.starts_with("pin-")), 3000);
    } else {
        check("overlay becomes the key window of the active app", false, format!("{:?}", windows::overlay_focus(app)));
        windows::overlay_cancel(app, token(app));
    }

    // An explicitly hidden application must not take the key back.
    #[cfg(target_os = "macos")]
    {
        capture(app);
        let active = wait(|| windows::overlay_focus(app) == Some((true, true)), 2500);
        std::thread::sleep(Duration::from_millis(300));
        windows::give_up_focus(app, Some(true));
        let hidden = active && wait(|| gone(app), 3000);
        windows::give_up_focus(app, Some(false));
        check("explicitly hiding the app cancels the selection", hidden, String::new());
        windows::overlay_cancel(app, token(app));
    }

    // 8. many quick cycles through every ending
    let mut stuck = vec![];
    for i in 0..24 {
        if !capture(app) {
            stuck.push(format!("#{i}: capture did not open"));
        }
        match i % 4 {
            0 => {
                windows::overlay_cancel(app, token(app));
            }
            1 => failing_capture(app),
            2 => {
                if !capture(app) {
                    stuck.push(format!("#{i}: replacement capture did not open"));
                }
                windows::overlay_cancel(app, token(app));
            }
            _ => {
                let _ = windows::overlay_done(app, token(app), -500.0, -500.0, 50.0, 50.0, None);
                windows::overlay_cancel(app, None);
            }
        }
        if !wait(|| gone(app), 1500) {
            stuck.push(format!("#{i}: capture={:?} visible={} capturing={}", token(app), visible(app), state(app).capturing.load(std::sync::atomic::Ordering::SeqCst)));
            windows::overlay_cancel(app, None);
        }
    }
    let (ok, d) = none_held(app);
    check("24 quick cycles: nothing left on screen, no plain key held", stuck.is_empty() && ok, format!("{d} stuck={stuck:?}"));

    capture(app);
    let old = token(app);
    capture(app);
    let current = token(app);
    let stale_ignored = old.is_some()
        && current.is_some()
        && current != old
        && !windows::overlay_cancel(app, old)
        && windows::overlay_ready(app, old, None).is_ok()
        && token(app) == current
        && ready(app)
        && visible(app);
    check("stale cancel and ready do not affect a new capture", stale_ignored, String::new());
    windows::overlay_cancel(app, current);

    // A hung native capture must release the flag, and the next request must work without restarting the app.
    crate::capture_worker::STALL_NEXT.store(true, std::sync::atomic::Ordering::SeqCst);
    windows::start_capture(app, Mode::Select);
    let recovered = wait(|| gone(app), 11000);
    check("hung capture: timed out and reset", recovered, String::new());
    let next = capture(app);
    check("capture after native timeout works", next, String::new());
    windows::overlay_cancel(app, token(app));

    // Simulate a dead webview being removed: a fresh page must pull the frame even if its initial event was lost.
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.destroy();
    }
    let removed = wait(|| app.get_webview_window("overlay").is_none(), 2000);
    let fresh = capture(app);
    check("capture after overlay recreation works", removed && fresh, String::new());
    windows::overlay_cancel(app, token(app));

    #[cfg(target_os = "macos")]
    if std::env::var_os("OCRTRANS_TEST_OVERLAY_MONITORS").is_some() {
        monitor_handoff(app, &mut check);
    }

    crate::log_info(&format!("keytest result: {}", if fails.is_empty() { "PASS".to_string() } else { format!("FAIL {fails:?}") }));
    app.exit(0);
}

#[cfg(target_os = "macos")]
fn monitor_handoff(app: &AppHandle, check: &mut impl FnMut(&str, bool, String)) {
    use ocrtrans_core::capture::MonitorInfo;

    fn warp((x, y): (f64, f64)) -> bool {
        #[repr(C)]
        struct Point { x: f64, y: f64 }
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGWarpMouseCursorPosition(point: Point) -> i32;
        }
        (unsafe { CGWarpMouseCursorPosition(Point { x, y }) }) == 0
    }
    fn center(m: &MonitorInfo) -> (f64, f64) {
        (m.x as f64 + m.width as f64 / 2.0, m.y as f64 + m.height as f64 / 2.0)
    }
    fn ready_on(app: &AppHandle, m: &MonitorInfo) -> bool {
        let target = state(app).shot.lock_safe().as_ref().is_some_and(|s| s.ready && s.frame.monitor.id == m.id);
        if !target || windows::overlay_focus(app) != Some((true, true)) {
            return false;
        }
        let Some(w) = app.get_webview_window("overlay") else { return false };
        let (Ok(scale), Ok(position), Ok(size)) = (w.scale_factor(), w.outer_position(), w.inner_size()) else { return false };
        let p = position.to_logical::<f64>(scale);
        let s = size.to_logical::<f64>(scale);
        (p.x - m.x as f64).abs() < 2.0 && (p.y - m.y as f64).abs() < 2.0
            && (s.width - m.width as f64).abs() < 2.0 && (s.height - m.height as f64).abs() < 2.0
    }
    fn detail(app: &AppHandle) -> String {
        let display = state(app).shot.lock_safe().as_ref().map(|s| s.frame.raw);
        let frame = app.get_webview_window("overlay").map(|w| (w.scale_factor(), w.outer_position(), w.inner_size()));
        format!("pointer={:?} display={display:?} frame={frame:?} focus={:?}", windows::os_cursor(), windows::overlay_focus(app))
    }
    let displays = ocrtrans_core::capture::monitors().unwrap_or_default();
    let Some(first) = displays.first() else {
        check("dual-screen displays available", false, String::new());
        return;
    };
    let Some(second) = displays.iter().find(|m| (m.x, m.y) != (first.x, first.y)) else {
        check("dual-screen displays available", false, String::new());
        return;
    };
    let original = windows::os_cursor();
    let started = warp(center(first)) && capture(app) && wait(|| ready_on(app, first), 2000);
    check("overlay initially covers the first display", started, detail(app));
    let initial = token(app);
    let moved = started && warp(center(second));
    let followed = moved && wait(|| token(app) != initial && ready_on(app, second), 9500);
    check("pointer follows to the second display with keyboard focus", followed, detail(app));
    let returned = followed && warp(center(first)) && wait(|| ready_on(app, first), 4000);
    check("pointer follows back to the first display with keyboard focus", returned, String::new());
    if returned && warp(center(second)) && wait(|| ready_on(app, second), 4000) {
        let current = token(app);
        let drag = app.get_webview_window("overlay").is_some_and(|w| w.eval(r#"
            const overlay = document.querySelector('.overlay');
            for (const [type, pos] of [['pointerdown', 100], ['pointermove', 240], ['pointerup', 240]]) {
                overlay.dispatchEvent(new PointerEvent(type, {
                    clientX: pos, clientY: pos, button: 0, bubbles: true, pointerId: 1, isPrimary: true,
                }));
            }
        "#).is_ok());
        let locked = drag && wait(|| state(app).shot.lock_safe().as_ref().is_some_and(|s| s.interacting), 1000);
        let moved = warp(center(first));
        std::thread::sleep(Duration::from_millis(650));
        check("completed selection stays on its display", locked && moved && token(app) == current && ready_on(app, second), String::new());
        let escaped = app.get_webview_window("overlay").is_some_and(|w| {
            w.eval("window.dispatchEvent(new KeyboardEvent('keydown', {key: 'Escape'}))").is_ok()
        });
        check("Escape still reaches the overlay after a display switch", escaped && wait(|| gone(app), 1500), String::new());
    } else {
        check("selection can start on the second display", false, detail(app));
    }
    windows::overlay_cancel(app, token(app));
    if let Some(p) = original {
        warp(p);
    }
}
