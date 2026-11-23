//! Overlay (region selection), pin windows, OCR popup and the settings window.

use std::sync::atomic::Ordering;

use anyhow::{anyhow, Result};
use image::RgbaImage;
use ocrtrans_core::capture;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::capture_worker::Frame;
use crate::geometry;
use crate::state::{state, LockExt, Mode, Pin, Shot};


#[cfg(target_os = "macos")]
mod mac {
    extern "C" {
        fn ocr_input_presses() -> u64;
        fn ocr_window_is_onscreen(number: isize) -> bool;
    }
    pub fn input_presses() -> u64 {
        unsafe { ocr_input_presses() }
    }

    pub fn is_onscreen(w: &tauri::WebviewWindow) -> bool {
        use objc2::{msg_send, runtime::AnyObject};
        let Ok(ptr) = w.ns_window() else { return false };
        unsafe {
            let number: isize = msg_send![ptr as *mut AnyObject, windowNumber];
            ocr_window_is_onscreen(number)
        }
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    pub fn has_screen_access() -> bool {
        unsafe { CGPreflightScreenCaptureAccess() }
    }
    pub fn request_screen_access() -> bool {
        unsafe { CGRequestScreenCaptureAccess() }
    }

    /// Bring our (menu-bar, Accessory) app to the front and make `w` the key window. A global hotkey fires while
    /// another app is frontmost; without this the overlay shows but receives no keys and eats the first click.
    pub fn activate_key(w: &tauri::WebviewWindow) {
        use objc2::runtime::{AnyClass, AnyObject};
        use objc2::{msg_send, sel};
        let Ok(ptr) = w.ns_window() else { return };
        unsafe {
            let cls = AnyClass::get(c"NSApplication").unwrap();
            let app: *mut AnyObject = msg_send![cls, sharedApplication];
            let new_api: bool = msg_send![app, respondsToSelector: sel!(activate)];
            if new_api {
                let _: () = msg_send![app, activate]; // macOS 14+
            }
            let _: () = msg_send![app, activateIgnoringOtherApps: true];
            let ns = ptr as *mut AnyObject;
            let _: () = msg_send![ns, makeKeyAndOrderFront: std::ptr::null::<AnyObject>()];
        }
    }

    /// Where the keyboard focus is, for the overlay guard. Main thread only.
    pub fn focus_detail(w: &tauri::WebviewWindow) -> super::Focus {
        use objc2::msg_send;
        use objc2::runtime::{AnyClass, AnyObject};
        let Ok(ptr) = w.ns_window() else { return super::Focus::default() };
        unsafe {
            let ns = ptr as *mut AnyObject;
            let overlay_key: bool = msg_send![ns, isKeyWindow];
            let cls = AnyClass::get(c"NSApplication").unwrap();
            let app: *mut AnyObject = msg_send![cls, sharedApplication];
            let active: bool = msg_send![app, isActive];
            let kw: *mut AnyObject = msg_send![app, keyWindow];
            let mut frontmost = 0;
            if let Some(wcls) = AnyClass::get(c"NSWorkspace") {
                let ws: *mut AnyObject = msg_send![wcls, sharedWorkspace];
                let front: *mut AnyObject = msg_send![ws, frontmostApplication];
                if !front.is_null() {
                    frontmost = msg_send![front, processIdentifier];
                }
            }
            let hidden: bool = msg_send![app, isHidden];
            super::Focus { active, overlay_key, own_other_key: !kw.is_null() && kw != ns, frontmost, hidden }
        }
    }

    /// Process id of the frontmost (active) app, if it is not us.
    pub fn frontmost_other_pid() -> Option<i32> {
        use objc2::msg_send;
        use objc2::runtime::{AnyClass, AnyObject};
        unsafe {
            let cls = AnyClass::get(c"NSWorkspace")?;
            let ws: *mut AnyObject = msg_send![cls, sharedWorkspace];
            let front: *mut AnyObject = msg_send![ws, frontmostApplication];
            if front.is_null() {
                return None;
            }
            let pid: i32 = msg_send![front, processIdentifier];
            (pid > 0 && pid as u32 != std::process::id()).then_some(pid)
        }
    }

    /// If we are the active app, hand the focus to the app with `pid` (macOS 14+ honours that only when the active
    /// app yields to it first). Main thread only.
    pub fn give_focus_to(pid: i32) {
        use objc2::runtime::{AnyClass, AnyObject};
        use objc2::{msg_send, sel};
        unsafe {
            let cls = AnyClass::get(c"NSApplication").unwrap();
            let app: *mut AnyObject = msg_send![cls, sharedApplication];
            let active: bool = msg_send![app, isActive];
            if !active {
                return; // someone else already has the focus (e.g. a system prompt): leave it there
            }
            let Some(ra) = AnyClass::get(c"NSRunningApplication") else { return };
            let other: *mut AnyObject = msg_send![ra, runningApplicationWithProcessIdentifier: pid];
            if other.is_null() {
                return;
            }
            if msg_send![app, respondsToSelector: sel!(yieldActivationToApplication:)] {
                let _: () = msg_send![app, yieldActivationToApplication: other];
            }
            let _: bool = msg_send![other, activateWithOptions: 0usize];
        }
    }

    /// Stop being the active app, as when another app or a system prompt takes the focus: `deactivate`, or with
    /// `hide` the stronger `hide:` (also orders our windows out; undone with `hide = false`). Self-test helper.
    /// Main thread only.
    pub fn give_up_focus(hide: Option<bool>) {
        use objc2::msg_send;
        use objc2::runtime::{AnyClass, AnyObject};
        unsafe {
            let cls = AnyClass::get(c"NSApplication").unwrap();
            let app: *mut AnyObject = msg_send![cls, sharedApplication];
            match hide {
                None => {
                    let _: () = msg_send![app, deactivate];
                }
                Some(true) => {
                    let _: () = msg_send![app, hide: std::ptr::null::<AnyObject>()];
                }
                Some(false) => {
                    let _: () = msg_send![app, unhideWithoutActivation];
                }
            }
        }
    }

    /// Pins follow all Spaces. Each new selection belongs on the user's active Space.
    pub fn raise_level(w: &tauri::WebviewWindow, level: isize) {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        use objc2_app_kit::{NSWindowAnimationBehavior, NSWindowCollectionBehavior as Behavior};
        if let Ok(ptr) = w.ns_window() {
            let ns = ptr as *mut AnyObject;
            let common = Behavior::FullScreenAuxiliary | Behavior::Stationary;
            unsafe {
                let behavior = if w.label() == "overlay" {
                    let _: () = msg_send![ns, setAnimationBehavior: NSWindowAnimationBehavior::None];
                    common | Behavior::MoveToActiveSpace | Behavior::CanJoinAllApplications
                } else {
                    common | Behavior::CanJoinAllSpaces
                };
                let _: () = msg_send![ns, setLevel: level];
                let _: () = msg_send![ns, setCollectionBehavior: behavior];
            }
        }
    }
}

pub fn has_screen_access() -> bool {
    #[cfg(target_os = "macos")]
    return mac::has_screen_access();
    #[cfg(not(target_os = "macos"))]
    true
}

pub fn request_screen_access() {
    #[cfg(target_os = "macos")]
    {
        if !mac::request_screen_access() {
            let _ = std::process::Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
                .spawn();
        }
    }
}

/// AppKit calls must happen on the main thread.
fn raise(w: &WebviewWindow, level: isize) {
    #[cfg(target_os = "macos")]
    {
        let win = w.clone();
        let _ = w.run_on_main_thread(move || mac::raise_level(&win, level));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (w, level);
}

/// Global mouse position in xcap's coordinate space (points on macOS, physical pixels on Windows).
/// Read straight from the OS: Tauri's `cursor_position()` round-trips through the main thread and can block
/// while another window is busy.
pub(crate) fn os_cursor() -> Option<(f64, f64)> {
    #[cfg(target_os = "macos")]
    {
        #[repr(C)]
        struct CGPoint {
            x: f64,
            y: f64,
        }
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGEventCreate(source: *const std::ffi::c_void) -> *const std::ffi::c_void;
            fn CGEventGetLocation(event: *const std::ffi::c_void) -> CGPoint;
        }
        #[link(name = "CoreFoundation", kind = "framework")]
        extern "C" {
            fn CFRelease(cf: *const std::ffi::c_void);
        }
        unsafe {
            let ev = CGEventCreate(std::ptr::null());
            if ev.is_null() {
                return None;
            }
            let p = CGEventGetLocation(ev);
            CFRelease(ev);
            Some((p.x, p.y))
        }
    }
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
        let mut p = POINT { x: 0, y: 0 };
        (unsafe { GetCursorPos(&mut p) } != 0).then_some((p.x as f64, p.y as f64))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    None
}

/// Monitor under the mouse cursor (falls back to primary), with logical geometry, plus its rect as xcap reports it
/// (physical pixels on Windows, points on macOS).
pub(crate) fn monitor_under_cursor() -> Result<(capture::MonitorInfo, [i32; 4])> {
    monitor_at_point(os_cursor()).map(|(monitor, raw, _)| (monitor, raw))
}

pub(crate) fn monitor_at_point(point: Option<(f64, f64)>) -> Result<(capture::MonitorInfo, [i32; 4], Vec<[i32; 4]>)> {
    let mons = capture::monitors()?;
    let rects: Vec<_> = mons.iter().map(|m| [m.x, m.y, m.width as i32, m.height as i32]).collect();
    let idx = point
        .and_then(|p| rects.iter().position(|&rect| geometry::display_contains(rect, p)))
        .or_else(|| mons.iter().position(|m| m.is_primary))
        .unwrap_or(0);
    let mut m = mons.get(idx).cloned().ok_or_else(|| anyhow!("no monitor"))?;
    let raw = rects[idx];
    if cfg!(target_os = "windows") {
        // xcap reports physical pixels on Windows; windows are placed in logical units
        let s = m.scale_factor.max(0.5) as f64;
        m.x = (m.x as f64 / s).round() as i32;
        m.y = (m.y as f64 / s).round() as i32;
        m.width = (m.width as f64 / s).round() as u32;
        m.height = (m.height as f64 / s).round() as u32;
    }
    Ok((m, raw, rects))
}

/// Hotkey / tray entry point: freeze the screen under the cursor and show the selection overlay.
pub fn start_capture(app: &AppHandle, mode: Mode) {
    // The pointer may move while the capture process starts or enumerates displays.
    let cursor = os_cursor();
    let started = std::time::Instant::now();
    if state(app).capturing.swap(true, Ordering::SeqCst) {
        return;
    }
    let handle = app.clone();
    // Prepare and start the worker together. A caller timing out must not strand the capture flag.
    if let Err(e) = app.run_on_main_thread(move || begin_capture(&handle, mode, started, cursor)) {
        state(app).capturing.store(false, Ordering::SeqCst);
        crate::log_error(&format!("could not schedule capture: {e}"));
    }
}

fn begin_capture(app: &AppHandle, mode: Mode, started: std::time::Instant, cursor: Option<(f64, f64)>) {
    prepare_capture(app);
    let handle = app.clone();
    let worker = std::thread::Builder::new().name("capture".into()).spawn(move || {
        let app = handle;
        // a panic here must not leave `capturing` set: every later hotkey press would be ignored silently
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| capture_and_show(&app, mode, started, cursor)))
            .unwrap_or_else(|_| Err(anyhow!("内部错误")));
        if let Err(e) = res {
            crate::log_error(&format!("capture failed: {e:#}"));
            // nothing of a failed capture may stay on screen (its window is shown before the page paints)
            overlay_cancel(&app, None);
            show_toast_for(&app, &format!("截屏失败：{e}"), "error", 6000);
        }
        // Finish failure cleanup before accepting another hotkey; otherwise the old cancel can hide its frame.
        state(&app).capturing.store(false, Ordering::SeqCst);
    });
    if let Err(e) = worker {
        overlay_cancel(app, None);
        state(app).capturing.store(false, Ordering::SeqCst);
        crate::log_error(&format!("capture failed to start: {e}"));
        show_toast(app, "无法启动截图，请重试", "error");
    }
}

fn prepare_capture(app: &AppHandle) {
    if let Some(pid) = frontmost_other_pid(app) {
        FOCUS_BEFORE.store(pid, Ordering::SeqCst);
        LAST_OTHER_APP.store(pid, Ordering::SeqCst);
    } else if state(app).shot.lock_safe().is_none() {
        // Immediately after cancellation, AppKit can still be handing activation back to the last app.
        let own_window_focused = app.webview_windows().values().any(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false));
        let before = if own_window_focused { 0 } else { LAST_OTHER_APP.load(Ordering::SeqCst) };
        FOCUS_BEFORE.store(before, Ordering::SeqCst);
    }
    drop_overlay(app, None);
}

fn capture_and_show(app: &AppHandle, mode: Mode, started: std::time::Instant, cursor: Option<(f64, f64)>) -> Result<()> {
    let frame = crate::capture_worker::capture(cursor)?;
    if FAIL_NEXT_CAPTURE.swap(false, Ordering::SeqCst) {
        return Err(anyhow!("模拟的截屏失败（自检）"));
    }
    let capture_ms = started.elapsed().as_millis();
    // A new webview must be built off the main thread on Windows.
    let w = overlay_window(app)?;
    crate::on_main_thread(app, move |app| {
        // Showing a Win32 window can activate it before the page acknowledges the frame.
        if cfg!(target_os = "windows")
            && frontmost_other_pid(app).is_some_and(|pid| pid != FOCUS_BEFORE.load(Ordering::SeqCst))
        {
            return Err(anyhow!("截图期间已切换到其他应用，请重试"));
        }
        present_frame(app, &w, frame, mode, started, capture_ms, true)
    }).ok_or_else(|| anyhow!("截图界面没有响应"))?
}

/// Install a frame and its display geometry together on the UI thread.
fn present_frame(app: &AppHandle, w: &WebviewWindow, frame: Frame, mode: Mode, started: std::time::Instant, capture_ms: u128, allow_rebuild: bool) -> Result<()> {
    let needs_activation = !w.is_visible()?;
    let monitor = &frame.monitor;
    let raw = frame.raw;
    if cfg!(target_os = "windows") {
        // Move first so the DPI transition does not rescale the final physical size.
        let position = tauri::PhysicalPosition::new(raw[0], raw[1]);
        w.set_position(position)?;
        w.set_size(tauri::PhysicalSize::new(raw[2] as u32, raw[3] as u32))?;
        w.set_position(position)?;
    } else {
        // Size first: AppKit keeps the bottom-left corner fixed when resizing.
        w.set_size(LogicalSize::new(monitor.width as f64, monitor.height as f64))?;
        w.set_position(LogicalPosition::new(monitor.x as f64, monitor.y as f64))?;
    }
    #[cfg(target_os = "macos")]
    mac::raise_level(w, 1000);
    // Showing the native window can fail. Keep the previous token until the replacement is ordered.
    if needs_activation { w.show()?; }
    let st = state(app);
    let token = st.next_id.fetch_add(1, Ordering::Relaxed);
    {
        let mut shot = st.shot.lock_safe();
        let previous_frame = shot.take().map(|s| s.frame);
        *shot = Some(Shot {
            frame, previous_frame, mode, token, started, capture_ms, ready: false, interacting: false, needs_activation,
            #[cfg(target_os = "macos")]
            input_presses: mac::input_presses(),
        });
    }
    guard_overlay(app, token, !needs_activation, allow_rebuild);
    if let Some(payload) = overlay_payload(app) {
        app.emit_to("overlay", "overlay-start", payload)?;
    }
    Ok(())
}

/// Capture the destination without hiding the old frame. An Escape, a new drag or a new hotkey
/// can invalidate the request while the worker is running.
fn switch_monitor(app: &AppHandle, token: u32, cursor: (f64, f64)) -> bool {
    if state(app).switching_monitor.swap(true, Ordering::SeqCst) {
        return false;
    }
    let handle = app.clone();
    let spawn = std::thread::Builder::new().name("capture-monitor".into()).spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            capture_other_monitor(&handle, token, cursor)
        })).unwrap_or_else(|_| Err(anyhow!("切换显示器时发生内部错误")));
        state(&handle).switching_monitor.store(false, Ordering::SeqCst);
        if let Err(e) = result {
            crate::log_error(&format!("overlay monitor switch failed: {e:#}"));
        }
    });
    if let Err(e) = spawn {
        state(app).switching_monitor.store(false, Ordering::SeqCst);
        crate::log_error(&format!("could not start monitor switch: {e}"));
    }
    true
}

fn capture_other_monitor(app: &AppHandle, token: u32, cursor: (f64, f64)) -> Result<()> {
    let started = std::time::Instant::now();
    let cached = {
        let st = state(app);
        let shot = st.shot.lock_safe();
        let Some(s) = shot.as_ref().filter(|s| s.token == token && s.ready && !s.interacting) else { return Ok(()) };
        s.previous_frame.as_ref().filter(|f| geometry::display_contains(f.raw, cursor)).cloned()
    };
    let (frame, capture_ms) = match cached {
        Some(frame) => (frame, 0),
        None => (crate::capture_worker::capture(Some(cursor))?, started.elapsed().as_millis()),
    };
    crate::on_main_thread(app, move |app| {
        // The pointer can cross back, the user can start selecting, or another app can become active
        // before capture completes. Keep the existing screen in those cases.
        if state(app).capturing.load(Ordering::SeqCst)
            || !os_cursor().is_some_and(|p| geometry::display_contains(frame.raw, p))
        {
            return Ok(());
        }
        let Some(w) = app.get_webview_window("overlay") else { return Ok(()) };
        let focus = window_focus_detail(&w);
        if !focus.active || !focus.overlay_key {
            return Ok(());
        }
        let mode = state(app).shot.lock_safe().as_ref()
            .filter(|s| s.token == token && s.ready && !s.interacting && s.frame.raw != frame.raw)
            .map(|s| s.mode);
        if let Some(mode) = mode {
            present_frame(app, &w, frame, mode, started, capture_ms, true)?;
        }
        Ok(())
    }).ok_or_else(|| anyhow!("截图界面没有响应"))?
}

/// A newly created (or recovered) page pulls the current frame after registering its listeners, so it cannot
/// miss the event sent while its JavaScript was still loading.
pub(crate) fn overlay_payload(app: &AppHandle) -> Option<serde_json::Value> {
    let st = state(app);
    let shot = st.shot.lock_safe();
    let s = shot.as_ref()?;
    let settings = st.settings.lock_safe();
    Some(serde_json::json!({
        "token": s.token, "mode": s.mode, "pw": s.frame.image.width(), "ph": s.frame.image.height(),
        "autoTranslate": settings.auto_translate, "defaultAction": settings.default_action,
        "bench": std::env::var_os("OCRTRANS_BENCH").is_some()
    }))
}

/// Dev hook for the self-test (selftest.rs): the next capture fails after taking the screenshot.
pub static FAIL_NEXT_CAPTURE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(target_os = "macos")]
pub(crate) fn overlay_onscreen(app: &AppHandle) -> bool {
    let Some(w) = app.get_webview_window("overlay") else { return false };
    crate::on_main_thread(app, move |_| mac::is_onscreen(&w)).unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn can_rebuild_overlay(app: &AppHandle, w: &WebviewWindow, token: u32) -> bool {
    let f = mac::focus_detail(w);
    let before = FOCUS_BEFORE.load(Ordering::SeqCst);
    !f.hidden && (f.frontmost == std::process::id() as i32 || (before > 0 && f.frontmost == before))
        && state(app).shot.lock_safe().as_ref().is_some_and(|s|
            s.token == token && !s.interacting && s.input_presses == mac::input_presses())
}

/// A cached NSWindow can stay bound to an obsolete Space even after AppKit orders it front.
/// Replace that native window once, using the original frozen pixels. Every UI mutation
/// checks the token so a newer hotkey, an actual app switch or Escape wins the race.
#[cfg(target_os = "macos")]
fn rebuild_offscreen_overlay(app: &AppHandle, token: u32, allow_rebuild: bool) -> Result<()> {
    let retired = crate::on_main_thread(app, move |app| -> Result<bool> {
        let Some(w) = app.get_webview_window("overlay") else { return Ok(false) };
        if !state(app).shot.lock_safe().as_ref().is_some_and(|s| s.token == token) || mac::is_onscreen(&w) {
            return Ok(false);
        }
        if !can_rebuild_overlay(app, &w, token) {
            overlay_cancel(app, Some(token));
            return Ok(false);
        }
        w.destroy()?;
        anyhow::ensure!(allow_rebuild, "重建后截图窗口仍未出现在当前桌面");
        Ok(true)
    }).ok_or_else(|| anyhow!("截图界面没有响应"))??;
    if !retired { return Ok(()); }
    crate::log_info(&format!("overlay {token} painted offscreen; rebuilding on the active Space"));
    // Tauri removes the destroyed window from its registry on the next event-loop turn.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while app.get_webview_window("overlay").is_some() {
        if !state(app).shot.lock_safe().as_ref().is_some_and(|s| s.token == token) { return Ok(()); }
        anyhow::ensure!(std::time::Instant::now() < deadline, "无法重建截图窗口");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    crate::on_main_thread(app, move |app| -> Result<()> {
        let shot = state(app).shot.lock_safe().as_ref().filter(|s| s.token == token)
            .map(|s| (s.frame.clone(), s.mode, s.started, s.capture_ms));
        let Some((frame, mode, started, capture_ms)) = shot else { return Ok(()) };
        let w = overlay_window(app)?;
        if !can_rebuild_overlay(app, &w, token) {
            overlay_cancel(app, Some(token));
            return Ok(());
        }
        present_frame(app, &w, frame, mode, started, capture_ms, false)
    }).ok_or_else(|| anyhow!("截图界面没有响应"))?
}

/// A stalled page, explicitly hiding the app, or a different foreground app can end a capture.
/// On macOS the source app can regain activation while the overlay is still visible and accepts a drag.
///
/// One of our own windows taking the key (the previous capture's pin or text window showing itself) is not a loss:
/// the overlay takes the key back.
///
/// The overlay covers the whole screen above everything else (screen-saver level), so whatever took the focus would
/// sit invisible behind it. In 0.5.6 that was macOS's own confirmation to keep allowing screen recording (asked
/// again from time to time for apps that capture the screen directly): it took the focus, could neither be seen
/// nor answered, and every following capture asked again.
fn guard_overlay(app: &AppHandle, token: u32, was_key: bool, _allow_rebuild: bool) {
    let app = app.clone();
    let before = FOCUS_BEFORE.load(Ordering::SeqCst);
    let me = std::process::id() as i32;
    #[cfg(target_os = "macos")]
    let mut retried_source = false;
    #[cfg(target_os = "macos")]
    let mut confirmed_onscreen = false;
    std::thread::spawn(move || {
        let t0 = std::time::Instant::now();
        let (mut was_key, mut lost, mut retried) = (was_key, 0, false);
        let mut seen = None;
        let mut target = None;
        let mut attempted = false;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(75));
            let cursor = os_cursor();
            let (ready, off_display, destination) = {
                let st = state(&app);
                let shot = st.shot.lock_safe();
                let Some(s) = shot.as_ref().filter(|s| s.token == token) else { return };
                let off_display = cursor.is_some_and(|p| !geometry::display_contains(s.frame.raw, p));
                let destination = cursor.filter(|_| s.ready && !s.interacting && off_display)
                    .and_then(|p| s.frame.displays.iter().copied().find(|&r| geometry::display_contains(r, p)));
                (s.ready, off_display, destination)
            };
            if !ready && t0.elapsed() >= std::time::Duration::from_secs(2) {
                if reset_overlay(&app, token) {
                    crate::log_error("overlay page did not paint within 2 s — cancelled");
                    show_toast(&app, "截图界面没有响应，已自动重置，请重试", "error");
                    return;
                }
                continue;
            }
            let Some(f) = overlay_focus_detail(&app) else { continue };
            #[cfg(target_os = "macos")]
            if f.hidden && was_key {
                if overlay_cancel(&app, Some(token)) {
                    crate::log_info("overlay cancelled after app was hidden");
                }
                return;
            }
            if seen != Some(f) {
                crate::log_info(&format!(
                    "overlay focus at {} ms: app active={}, overlay key window={}, other own window key={}, frontmost pid={} (us {me}, before {before})",
                    t0.elapsed().as_millis(),
                    f.active,
                    f.overlay_key,
                    f.own_other_key,
                    f.frontmost
                ));
                seen = Some(f);
            }
            let foreign = f.frontmost > 0 && f.frontmost != me && f.frontmost != before;
            #[cfg(target_os = "macos")]
            if ready && !confirmed_onscreen && !f.hidden && !foreign {
                if !overlay_onscreen(&app) {
                    if t0.elapsed() < std::time::Duration::from_millis(400) { continue; }
                    if let Err(e) = rebuild_offscreen_overlay(&app, token, _allow_rebuild) {
                        if overlay_cancel(&app, Some(token)) {
                            crate::log_error(&format!("offscreen overlay {token}: {e:#}"));
                            show_toast(&app, "截图窗口未能显示在当前桌面，请重试", "error");
                        }
                        return;
                    }
                    continue;
                }
                confirmed_onscreen = true;
                crate::log_info(&format!("overlay {token} confirmed onscreen by WindowServer"));
            }
            #[cfg(target_os = "macos")]
            if !f.active && before > 0 && f.frontmost == before {
                // A key press, click or pointer movement can rule out taking the keyboard
                // back. None of these invalidates the visible selection or its first click.
                if ready && was_key && !retried_source {
                    retried_source = true;
                    let _ = crate::on_main_thread(&app, move |app| retry_source_focus(app, token, before));
                }
                lost = 0;
                continue;
            }
            // NSWorkspace and the key window can briefly disagree during activation.
            // Keep a working selection while it still owns the keyboard.
            let gone = if f.active && f.overlay_key {
                was_key = true;
                false
            } else if foreign {
                true
            } else if !ready {
                false
            } else if f.active && f.own_other_key && off_display {
                // A window the user clicked on the other screen keeps its focus.
                true
            } else if (f.active && f.own_other_key) || (!was_key && !retried) {
                // Reclaim the key from a late pin window, or retry the initial activation once.
                let _ = crate::on_main_thread(&app, move |app| focus_overlay(app, token));
                retried = true;
                false
            } else {
                // activation still pending or refused (the first click activates the overlay) — or lost after it had it
                was_key
            };
            lost = if gone { lost + 1 } else { 0 };
            let limit = if cfg!(target_os = "macos") && !foreign { 9 } else { 3 };
            if lost >= limit && overlay_cancel(&app, Some(token)) {
                let name = foreground_app_name(f.frontmost);
                crate::log_error(&format!(
                    "overlay lost the focus ({}) {} ms after the hotkey — cancelled",
                    if foreign { format!("{} (pid {}) came to the front", name.as_deref().unwrap_or("app"), f.frontmost) } else { "our app is no longer active".into() },
                    t0.elapsed().as_millis()
                ));
                let message = match name {
                    Some(name) => format!("截图已取消，{name} 已切到前台。"),
                    None => "截图已取消，其他窗口已获得焦点。".into(),
                };
                show_toast_for(&app, &message, "info", 3000);
                return;
            }
            let destination = destination.filter(|_| f.active && f.overlay_key);
            if target != destination {
                target = destination;
                attempted = false;
            } else if !attempted && destination.is_some() {
                if let Some(cursor) = cursor {
                    attempted = switch_monitor(&app, token, cursor);
                }
            }
        }
    });
}

/// Remove a nonresponsive page. Check the token again on the UI thread before changing the window.
fn reset_overlay(app: &AppHandle, token: u32) -> bool {
    crate::on_main_thread(app, move |app| {
        if !state(app).shot.lock_safe().as_ref().is_some_and(|s| s.token == token && !s.ready) {
            return false;
        }
        drop_overlay(app, Some(token));
        if let Some(w) = app.get_webview_window("overlay") {
            let _ = w.destroy();
        }
        give_focus_back(app);
        true
    }).unwrap_or(false)
}

/// The app that had the focus when the current capture started (0: none, or it was us).
static FOCUS_BEFORE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
/// The app that had the focus when the latest capture started (kept after a pin or text window opened from it).
static LAST_OTHER_APP: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// Process id of the frontmost app if it is not us.
pub(crate) fn frontmost_other_pid(app: &AppHandle) -> Option<i32> {
    #[cfg(target_os = "macos")]
    return crate::on_main_thread(app, |_| mac::frontmost_other_pid()).flatten();
    #[cfg(target_os = "windows")]
    {
        let _ = app;
        let pid = windows_foreground_pid();
        (pid > 0 && pid as u32 != std::process::id()).then_some(pid)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = app;
        None
    }
}

#[cfg(target_os = "windows")]
fn windows_foreground_pid() -> i32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), &mut pid) };
    pid as i32
}

/// Hand the focus to the app with `pid` if we hold it (see mac::give_focus_to).
pub(crate) fn give_focus_to(app: &AppHandle, pid: i32) {
    #[cfg(target_os = "macos")]
    let _ = crate::on_main_thread(app, move |_| mac::give_focus_to(pid));
    #[cfg(not(target_os = "macos"))]
    let _ = (app, pid);
}

/// The overlay ended without opening a window of ours (cancelled, copied, saved): give the keyboard back to the app
/// the user was in. Otherwise our app stays active with no window (a menu-bar app is not deactivated when its last
/// window hides), and what the user types next — Enter included — goes nowhere until they click their app.
fn give_focus_back(app: &AppHandle) {
    let pid = FOCUS_BEFORE.swap(0, Ordering::SeqCst);
    if pid != 0 {
        give_focus_to(app, pid);
    }
}

fn restore_selection_focus(app: &AppHandle, pid: i32) {
    if pid != 0 {
        let _ = crate::on_main_thread(app, move |app| {
            if !state(app).capturing.load(Ordering::SeqCst) && state(app).shot.lock_safe().is_none() {
                give_focus_to(app, pid);
            }
        });
    }
}

/// Self-test hook: our app stops being the active one (see mac::give_up_focus).
#[cfg(target_os = "macos")]
pub(crate) fn give_up_focus(app: &AppHandle, hide: Option<bool>) {
    let _ = crate::on_main_thread(app, move |_| mac::give_up_focus(hide));
}

fn foreground_app_name(pid: i32) -> Option<String> {
    #[cfg(target_os = "macos")]
    if pid > 0 && pid != std::process::id() as i32 {
        let app = objc2_app_kit::NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?;
        return app.localizedName().map(|name| name.to_string());
    }
    let _ = pid;
    None
}

/// Where the keyboard focus is while the overlay is up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Focus {
    /// our app is the active one
    pub active: bool,
    #[cfg(target_os = "macos")]
    pub hidden: bool,
    /// the overlay is our key window
    pub overlay_key: bool,
    /// another window of ours is the key window (a pin or text window of the previous capture showing itself, …)
    pub own_other_key: bool,
    /// process id of the frontmost app (0: unknown)
    pub frontmost: i32,
}

/// None: no overlay, or the main thread did not answer in time. Waits for the main thread; call without holding a
/// lock.
pub(crate) fn overlay_focus_detail(app: &AppHandle) -> Option<Focus> {
    let w = app.get_webview_window("overlay")?;
    crate::on_main_thread(app, move |_| window_focus_detail(&w))
}

fn window_focus_detail(w: &WebviewWindow) -> Focus {
    #[cfg(target_os = "macos")]
    return mac::focus_detail(w);
    #[cfg(not(target_os = "macos"))]
    {
        let key = w.is_focused().unwrap_or(false);
        #[cfg(target_os = "windows")]
        {
            let frontmost = windows_foreground_pid();
            let active = frontmost as u32 == std::process::id();
            Focus { active, overlay_key: key, own_other_key: active && !key, frontmost }
        }
        #[cfg(not(target_os = "windows"))]
        Focus { active: key, overlay_key: key, own_other_key: false, frontmost: 0 }
    }
}

/// Recheck on the UI thread: a second hotkey, an intentional switch, or a system
/// dialog arriving after the guard's observation must not activate this window.
#[cfg(target_os = "macos")]
fn retry_source_focus(app: &AppHandle, token: u32, before: i32) {
    let Some(w) = app.get_webview_window("overlay") else { return };
    let eligible = state(app).shot.lock_safe().as_ref().is_some_and(|s|
        s.token == token && s.ready && s.input_presses == mac::input_presses()
            && os_cursor().is_some_and(|p| geometry::display_contains(s.frame.raw, p))
    );
    if !eligible || !w.is_visible().unwrap_or(false) {
        return;
    }
    let focus = mac::focus_detail(&w);
    if !focus.active && !focus.hidden && focus.frontmost == before {
        mac::activate_key(&w);
        crate::log_info("retried overlay focus after source app reactivated");
    }
}

/// Only the current overlay can take focus. Leave a newly foreground app or permission prompt alone.
fn focus_overlay(app: &AppHandle, token: u32) {
    let shot = state(app).shot.lock_safe().as_ref()
        .filter(|s| s.token == token && s.ready)
        .map(|s| (s.needs_activation, s.frame.raw));
    let Some((needs_activation, raw)) = shot else { return };
    let Some(w) = app.get_webview_window("overlay") else { return };
    let f = window_focus_detail(&w);
    let before = FOCUS_BEFORE.load(Ordering::SeqCst);
    if (f.active && f.overlay_key)
        || (!f.active && !needs_activation)
        || (f.own_other_key && os_cursor().is_some_and(|p| !geometry::display_contains(raw, p)))
        || (f.frontmost > 0 && f.frontmost as u32 != std::process::id() && f.frontmost != before)
    {
        return;
    }
    #[cfg(target_os = "macos")]
    mac::activate_key(&w);
    #[cfg(not(target_os = "macos"))]
    let _ = w.set_focus();
}

/// (our app is active, the overlay is its key window).
pub(crate) fn overlay_focus(app: &AppHandle) -> Option<(bool, bool)> {
    overlay_focus_detail(app).map(|f| (f.active, f.overlay_key))
}

fn overlay_window(app: &AppHandle) -> Result<WebviewWindow> {
    if let Some(w) = app.get_webview_window("overlay") {
        return Ok(w);
    }
    let w = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("index.html?view=overlay".into()))
        .title("ocr-trans overlay")
        .accept_first_mouse(true)
        .transparent(true)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .visible(false)
        .focused(false)
        .build()?;
    raise(&w, 1000); // NSScreenSaverWindowLevel: above the menu bar
    Ok(w)
}

/// Pre-create the overlay so the first capture is fast.
pub fn warm_up(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = overlay_window(&app) {
            crate::log_error(&format!("preparing the capture window: {e:#}"));
        }
    });
}

/// `timings` = page-side milliseconds (event→fetched, fetched→painted), for the perf log.
pub fn overlay_ready(app: &AppHandle, token: Option<u32>, timings: Option<(f64, f64)>) -> Result<()> {
    let timing = crate::on_main_thread(app, move |app| {
        let (token, timing) = {
            let st = state(app);
            let mut shot = st.shot.lock_safe();
            let s = shot.as_mut().filter(|s| !s.ready && token.is_none_or(|t| t == s.token))?;
            s.ready = true;
            let (fetch, paint) = timings.unwrap_or_default();
            let timing = format!(
                "hotkey→overlay painted {} ms (capture {} ms, page fetch {:.0} ms, paint {:.0} ms, {}x{})",
                s.started.elapsed().as_millis(), s.capture_ms, fetch, paint, s.frame.image.width(), s.frame.image.height()
            );
            (s.token, timing)
        };
        focus_overlay(app, token);
        #[cfg(target_os = "macos")]
        if crate::selftest::ORDER_OUT_NEXT_OVERLAY.swap(false, Ordering::SeqCst) {
            if let Some(w) = app.get_webview_window("overlay") { let _ = w.hide(); }
        }
        Some(timing)
    }).ok_or_else(|| anyhow!("截图界面没有响应"))?;
    if let Some(timing) = timing {
        crate::perf_log(&timing);
    }
    Ok(())
}

pub fn overlay_failed(app: &AppHandle, token: u32) {
    if reset_overlay(app, token) {
        show_toast(app, "截图画面加载失败，已重置截图界面，请重试", "error");
    }
}

/// Main-thread command. Only the active frame can claim or release selection interaction.
pub(crate) fn overlay_interaction(app: &AppHandle, token: u32, active: bool) -> bool {
    let st = state(app);
    let mut shot = st.shot.lock_safe();
    if let Some(s) = shot.as_mut().filter(|s| s.token == token) {
        s.interacting = active;
        #[cfg(target_os = "macos")]
        if active {
            // The webview accepted this click as part of the selection.
            s.input_presses = mac::input_presses();
        }
        return true;
    }
    false
}

/// Cancel the selection. With `token`, only if it still refers to the current screenshot.
pub fn overlay_cancel(app: &AppHandle, token: Option<u32>) -> bool {
    crate::on_main_thread(app, move |app| {
        if drop_overlay(app, token) {
            give_focus_back(app);
            true
        } else {
            false
        }
    }).unwrap_or(false)
}

/// Main thread only, so taking the frame and hiding its window are ordered with showing the next one.
fn drop_overlay(app: &AppHandle, token: Option<u32>) -> bool {
    let shot = {
        let st = state(app);
        let mut shot = st.shot.lock_safe();
        if token.is_some_and(|t| shot.as_ref().is_none_or(|s| s.token != t)) {
            return false;
        }
        shot.take()
    };
    hide_overlay(app, shot.map(|s| s.token));
    true
}

fn hide_overlay(app: &AppHandle, token: Option<u32>) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.hide();
        if let Some(token) = token {
            let _ = app.emit_to("overlay", "overlay-cancelled", token);
        }
    }
}

/// Region chosen in overlay-local logical coordinates. `action` (from the selection toolbar) overrides the
/// capture mode: "translate" | "ocr" | "pin" | "doodle" | "copy" | "save".
///
/// On an error the overlay stays only while its capture is still usable (a selection clipped to nothing); otherwise
/// it is cancelled, so a page that failed can never leave a blank full-screen window behind.
pub fn overlay_done(app: &AppHandle, token: Option<u32>, x: f64, y: f64, w: f64, h: f64, action: Option<&str>) -> Result<()> {
    let r = finish_selection(app, token, x, y, w, h, action);
    if r.is_err() {
        let live = state(app).shot.lock_safe().as_ref().is_some_and(|s| token.is_none_or(|t| t == s.token));
        if !live {
            overlay_cancel(app, token);
        }
    }
    r
}

fn finish_selection(app: &AppHandle, token: Option<u32>, x: f64, y: f64, w: f64, h: f64, action: Option<&str>) -> Result<()> {
    let st = state(app);
    let (shot, c, focus_before) = crate::on_main_thread(app, move |app| -> Result<_> {
        let (shot, crop) = {
            let st = state(app);
            let mut g = st.shot.lock_safe();
            let s = g.as_ref().ok_or_else(|| anyhow!("截图已失效，请重新截图"))?;
            if token.is_some_and(|t| s.token != t) {
                return Err(anyhow!("截图已被新的截图替换"));
            }
            // A too-small selection keeps the overlay and the capture.
            let crop = geometry::selection_to_crop([x, y, w, h], (s.frame.monitor.x as f64, s.frame.monitor.y as f64), s.frame.monitor.width as f64, s.frame.image.dimensions())
                .ok_or_else(|| anyhow!("选区太小"))?;
            #[cfg(windows)]
            let crop = {
                let mut crop = crop;
                crop.rect[0] = (f64::from(s.frame.raw[0]) + f64::from(crop.px)) / crop.scale;
                crop.rect[1] = (f64::from(s.frame.raw[1]) + f64::from(crop.py)) / crop.scale;
                crop
            };
            (g.take().expect("checked above"), crop)
        };
        hide_overlay(app, Some(shot.token));
        Ok((shot, crop, FOCUS_BEFORE.swap(0, Ordering::SeqCst)))
    }).ok_or_else(|| anyhow!("截图界面没有响应"))??;
    let crop = std::sync::Arc::new(image::imageops::crop_imm(&*shot.frame.image, c.px, c.py, c.pw, c.ph).to_image());
    drop(shot.frame);
    drop(shot.previous_frame); // release both full-screen frames right away
    let action = resolve_action(shot.mode, action, &st.settings.lock_safe().default_action);
    let hid = crate::history::add(app, &crop, c.scale as f32, c.rect, action);
    match action {
        "copy" => {
            use tauri_plugin_clipboard_manager::ClipboardExt;
            let (w, h) = crop.dimensions();
            let result = app.clipboard().write_image(&tauri::image::Image::new(crop.as_raw(), w, h));
            restore_selection_focus(app, focus_before);
            if let Err(e) = result {
                show_toast(app, &format!("复制截图失败：{e}"), "error");
                return Err(e.into());
            }
            show_toast(app, "截图已复制到剪贴板", "ok");
            return Ok(());
        }
        "save" => {
            // the save dialog must not block the main thread (this runs inside a sync command)
            let app = app.clone();
            std::thread::spawn(move || {
                use tauri_plugin_dialog::DialogExt;
                let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
                let name = format!("ocr-trans-{stamp}.png");
                let mut dlg = app.dialog().file().add_filter("PNG", &["png"]).set_file_name(&name);
                if let Ok(d) = app.path().picture_dir() {
                    dlg = dlg.set_directory(d);
                }
                let chosen = with_pins_lowered(&app, || dlg.blocking_save_file()).and_then(|p| p.into_path().ok());
                restore_selection_focus(&app, focus_before);
                if let Some(p) = chosen {
                    match crate::atomic_file::replace(&p, |f| Ok(crop.write_to(f, image::ImageFormat::Png)?)) {
                        Ok(()) => show_toast(&app, &format!("已保存 {}", p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()), "ok"),
                        Err(e) => show_toast(&app, &format!("保存失败：{e}"), "error"),
                    }
                }
            });
            return Ok(());
        }
        _ => {}
    }
    // a pin or text window takes the focus now: the user works there
    let id = add_pin(app, crop, c.scale as f32, c.rect)?;
    if let Some(p) = state(app).pins.lock_safe().get_mut(&id) {
        p.history_id = hid;
    }
    let opened = match action {
        "ocr" => open_text_window(app, id, false),
        "translate" => open_pin_with(app, id, true, None),
        "doodle" => open_pin_with(app, id, false, Some("pen")),
        _ => open_pin_with(app, id, false, None),
    };
    if let Err(e) = opened {
        st.pins.lock_safe().remove(&id);
        restore_selection_focus(app, focus_before);
        show_toast(app, &format!("打开截图结果失败：{e}"), "error");
        return Err(e);
    }
    Ok(())
}

/// Which action a finished selection runs: the toolbar's choice, else the hotkey's mode
/// (the unified capture without a choice falls back to the default action).
pub fn resolve_action(mode: Mode, chosen: Option<&str>, default_action: &str) -> &'static str {
    const KNOWN: [&str; 6] = ["translate", "ocr", "pin", "doodle", "copy", "save"];
    if let Some(a) = chosen.and_then(|c| KNOWN.iter().find(|k| **k == c)) {
        return a;
    }
    match mode {
        Mode::Pin => "pin",
        Mode::Ocr => "ocr",
        Mode::Translate => "translate",
        Mode::Select => match default_action {
            "ocr" => "ocr",
            "pin" => "pin",
            _ => "translate",
        },
    }
}

pub fn add_pin(app: &AppHandle, image: impl Into<std::sync::Arc<RgbaImage>>, scale: f32, rect: [f64; 4]) -> Result<u32> {
    let image = image.into();
    let st = state(app);
    let mut pins = st.pins.lock_safe();
    // Reserve space for the translated image as well as the original. Never evict an open pin.
    let pixels = pins.values().map(|p| p.image.as_raw().len() as u64).sum::<u64>() + image.as_raw().len() as u64;
    anyhow::ensure!(pixels * 2 <= 512 * 1024 * 1024, "钉图图像已达到内存上限，请关闭部分钉图后重试");
    let id = st.next_id.fetch_add(1, Ordering::Relaxed);
    pins.insert(id, Pin { image, scale, rect, ocr: None, translated: None, translated_glossary: vec![], edit_revision: 0, translation_generation: 0, clean: None, original_png: None, clean_png: None, history_id: None, saved_translation: None });
    Ok(id)
}

pub fn open_pin(app: &AppHandle, id: u32, translate: bool) -> Result<()> {
    open_pin_with(app, id, translate, None)
}

/// `tool`: annotation tool to start with (e.g. "pen" for the 涂鸦 action).
pub fn open_pin_with(app: &AppHandle, id: u32, translate: bool, tool: Option<&str>) -> Result<()> {
    // one window per pin: asking again (e.g. 钉图 in the text window opened from this pin) brings it forward
    // instead of failing on the duplicate label (0.5.4)
    if let Some(w) = app.get_webview_window(&format!("pin-{id}")) {
        let _ = w.show();
        let _ = w.set_focus();
        if translate {
            let _ = app.emit_to(format!("pin-{id}"), "pin-show-translation", ());
        }
        return Ok(());
    }
    let (rect, _) = {
        let st = state(app);
        let pins = st.pins.lock_safe();
        let p = pins.get(&id).ok_or_else(|| anyhow!("no pin {id}"))?;
        (p.rect, p.scale)
    };
    let demo = if std::env::var_os("OCRTRANS_TEST_DOODLES").is_some() { "&demo=1" } else { "" };
    let tool = tool.map(|t| format!("&tool={t}")).unwrap_or_default();
    let probe = if std::env::var_os("OCRTRANS_TEST_PROFILE").is_some() { "&verify=1" } else { "" };
    let url = format!("index.html?view=pin&id={id}&translate={}{demo}{tool}{probe}", translate as u8);
    let w = WebviewWindowBuilder::new(app, format!("pin-{id}"), WebviewUrl::App(url.into()))
        .title("钉图")
        .accept_first_mouse(true)
        .decorations(false)
        .transparent(true)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .visible(false)
        .position(rect[0], rect[1])
        // image size for now; the page measures its toolbar and lays the window out (lib/placement.ts pinLayout)
        // before showing it
        .inner_size(rect[2].max(1.0), rect[3].max(1.0))
        .build()?;
    raise(&w, 8); // above normal floating windows, below the selection overlay
    Ok(())
}

/// The one text window (识字): from the action bar (key 2), the direct hotkey and the pin's 识字 button.
/// One per capture: asking again focuses the open window instead of making a second one.
pub fn open_text_window(app: &AppHandle, id: u32, translated: bool) -> Result<()> {
    open_text_window_with(app, id, translated, None)
}

/// `autotest`: dev hook — the page translates and then closes itself ("js" = window.close(), "safe" = close_self).
pub fn open_text_window_with(app: &AppHandle, id: u32, translated: bool, autotest: Option<&str>) -> Result<()> {
    let label = format!("ocr-{id}");
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    let (rect, capture_scale) = state(app).pins.lock_safe().get(&id).map(|p| (p.rect, p.scale)).ok_or_else(|| anyhow!("no pin"))?;
    let (work, scale) = work_area_at(app, rect, capture_scale).unwrap_or(([0.0, 0.0, 1440.0, 900.0], 1.0));
    let selection = if cfg!(windows) { rect.map(|v| v * f64::from(capture_scale) / scale) } else { rect };
    let [x, y, width, height] = crate::geometry::text_window_frame(selection, work);
    let url = format!("index.html?view=ocr&id={id}&translated={}{}", translated as u8, autotest.map(|m| format!("&autotest={m}")).unwrap_or_default());
    let w = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .title("识字")
        .accept_first_mouse(true)
        .always_on_top(true)
        .resizable(true)
        .min_inner_size(420.0, 300.0)
        .inner_size(width, height)
        .position(x, y)
        .visible(false)
        .build()?;
    #[cfg(windows)]
    {
        w.set_position(tauri::PhysicalPosition::new((x * scale).round() as i32, (y * scale).round() as i32))?;
        w.set_size(tauri::PhysicalSize::new((width * scale).round() as u32, (height * scale).round() as u32))?;
    }
    raise(&w, 8);
    Ok(())
}

/// Visible work area (menu bar and Dock excluded) of the monitor under a logical point, in logical coordinates.
fn work_area_at(app: &AppHandle, rect: [f64;4], capture_scale: f32) -> Option<([f64; 4], f64)> {
    let factor = if cfg!(windows) { f64::from(capture_scale) } else { 1.0 };
    let (x, y) = ((rect[0] + rect[2] / 2.0) * factor, (rect[1] + rect[3] / 2.0) * factor);
    let mons = app.available_monitors().ok()?;
    let rects: Vec<[f64; 4]> = mons
        .iter()
        .map(|m| {
            let s = if cfg!(windows) { 1.0 } else { m.scale_factor() };
            [m.position().x as f64 / s, m.position().y as f64 / s, m.size().width as f64 / s, m.size().height as f64 / s]
        })
        .collect();
    // no monitors (a display reconfiguring) → None, not an index panic
    let m = crate::geometry::monitor_at(&rects, x, y).and_then(|i| mons.get(i)).or(mons.first())?;
    let (s, wa) = (m.scale_factor(), m.work_area());
    Some(([wa.position.x as f64 / s, wa.position.y as f64 / s, wa.size.width as f64 / s, wa.size.height as f64 / s], s))
}

pub fn open_settings(app: &AppHandle, tab: Option<&str>) {
    // Tray callbacks and setup run on the UI thread. WebView2 creation there can deadlock on Windows.
    let app = app.clone();
    let tab = tab.map(str::to_owned);
    std::thread::spawn(move || open_settings_now(&app, tab.as_deref()));
}

fn open_settings_now(app: &AppHandle, tab: Option<&str>) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.set_focus();
        if let Some(t) = tab {
            let _ = w.emit("settings-tab", t);
        }
        return;
    }
    let url = format!("index.html?view=settings&tab={}", tab.unwrap_or("general"));
    let _ = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App(url.into()))
        .title("OCR Trans · 设置")
        .inner_size(760.0, 620.0)
        .min_inner_size(600.0, 480.0)
        .center()
        .build()
        .map(|w| w.set_focus());
}

static TOAST_GEN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// A small transient pill at the bottom centre of the screen under the cursor ("已复制", "已保存", errors).
/// Click-through, never takes focus, hides itself after ~1.8 s. Also forwarded to the settings banner.
pub fn show_toast(app: &AppHandle, msg: &str, kind: &str) {
    show_toast_for(app, msg, kind, 1800);
}

/// `show_toast` that stays `ms` milliseconds (longer messages).
pub fn show_toast_for(app: &AppHandle, msg: &str, kind: &str, ms: u64) {
    let _ = app.emit("toast", msg);
    let gen = TOAST_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let app2 = app.clone();
    let msg = msg.to_string();
    let kind = kind.to_string();
    // window work hops to the main thread inside Tauri; do it from a helper thread so callers never wait
    std::thread::spawn(move || {
        let app = app2;
        // long messages wrap onto two lines in a wider window
        let (w, h) = if msg.chars().count() > 26 { (560.0, 100.0) } else { (460.0, 72.0) };
        let mon = monitor_under_cursor().ok().map(|(m, _)| m);
        let (mx, my, mw, mh) = mon.map(|m| (m.x as f64, m.y as f64, m.width as f64, m.height as f64)).unwrap_or((0.0, 0.0, 1440.0, 900.0));
        let win = match app.get_webview_window("toast") {
            Some(win) => win,
            None => match WebviewWindowBuilder::new(&app, "toast", WebviewUrl::App("index.html?view=toast".into()))
                .title("toast")
                .decorations(false)
                .transparent(true)
                .resizable(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .shadow(false)
                .focused(false)
                // never the key window: showing it must not take the key from the overlay or a pin
                .focusable(false)
                .visible(false)
                .inner_size(w, h)
                .build()
            {
                Ok(win) => {
                    // the page needs a moment to register its listener the first time
                    std::thread::sleep(std::time::Duration::from_millis(350));
                    win
                }
                Err(_) => return,
            },
        };
        let _ = win.set_ignore_cursor_events(true);
        let _ = win.set_size(LogicalSize::new(w, h));
        let _ = win.set_position(LogicalPosition::new(mx + (mw - w) / 2.0, my + mh - h - 96.0));
        let _ = win.emit("toast-show", serde_json::json!({ "msg": msg, "kind": kind }));
        let _ = win.show();
        raise(&win, 1001);
        std::thread::sleep(std::time::Duration::from_millis(ms));
        if TOAST_GEN.load(Ordering::SeqCst) == gen {
            let _ = win.emit("toast-hide", ());
            std::thread::sleep(std::time::Duration::from_millis(260));
            if TOAST_GEN.load(Ordering::SeqCst) == gen {
                let _ = win.hide();
            }
        }
    });
}

/// Run a blocking file dialog with the pins and text windows moved down to the normal window level: they float
/// above other apps (level 8), and the system save panel opened at normal level behind them.
pub fn with_pins_lowered<T>(app: &AppHandle, f: impl FnOnce() -> T) -> T {
    // Two save panels must not restore the floating windows while the other panel is still open.
    static DIALOG: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _dialog = DIALOG.lock_safe();
    let wins: Vec<WebviewWindow> = app
        .webview_windows()
        .into_iter()
        .filter(|(label, _)| label.starts_with("pin-") || label.starts_with("ocr-"))
        .map(|(_, w)| w)
        .collect();
    for w in &wins {
        let _ = w.set_always_on_top(false);
        raise(w, 0);
    }
    struct Restore(Vec<WebviewWindow>);
    impl Drop for Restore {
        fn drop(&mut self) {
            for w in &self.0 {
                let _ = w.set_always_on_top(true);
                raise(w, 8);
            }
        }
    }
    let _restore = Restore(wins);
    f()
}

pub fn open_history(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || open_history_now(&app));
}

fn open_history_now(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("history") {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, "history", WebviewUrl::App("index.html?view=history".into()))
        .title("截图历史")
        .inner_size(820.0, 620.0)
        .min_inner_size(480.0, 360.0)
        .center()
        .build()
        .map(|w| w.set_focus());
}

pub fn open_onboarding(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || open_onboarding_now(&app));
}

fn open_onboarding_now(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("onboarding") {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(app, "onboarding", WebviewUrl::App("index.html?view=onboarding".into()))
        .title("欢迎使用 OCR Trans")
        .inner_size(560.0, 740.0)
        .resizable(false)
        .center()
        .build()
        .map(|w| w.set_focus());
}

/// Hide now, destroy ~0.7 s later (see commands::close_self). Closing the last pin or text window gives the
/// keyboard back to the app of the latest capture: otherwise our menu-bar app stays active with no window and what
/// the user types next (⌘V of the text just copied, Enter) goes nowhere.
pub fn close_safely(w: &WebviewWindow) {
    let _ = w.hide();
    let app = w.app_handle();
    let pid = LAST_OTHER_APP.load(Ordering::SeqCst);
    let ours_left = app.webview_windows().iter().any(|(l, w)| l != "toast" && w.is_visible().unwrap_or(false));
    if pid != 0 && !ours_left && (w.label().starts_with("pin-") || w.label().starts_with("ocr-")) {
        give_focus_to(app, pid);
    }
    let w = w.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(700));
        let _ = w.destroy();
    });
}

pub fn close_all_pins(app: &AppHandle) {
    for (label, w) in app.webview_windows() {
        if label.starts_with("pin-") || label.starts_with("ocr-") {
            close_safely(&w);
        }
    }
    state(app).pins.lock_safe().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_resolution() {
        assert_eq!(resolve_action(Mode::Select, Some("doodle"), "translate"), "doodle");
        assert_eq!(resolve_action(Mode::Select, None, "ocr"), "ocr");
        assert_eq!(resolve_action(Mode::Select, None, "translate"), "translate");
        assert_eq!(resolve_action(Mode::Select, Some("rm -rf"), "translate"), "translate", "unknown action ignored");
        assert_eq!(resolve_action(Mode::Pin, None, "ocr"), "pin");
        assert_eq!(resolve_action(Mode::Translate, None, "ocr"), "translate");
        assert_eq!(resolve_action(Mode::Select, None, "pin"), "pin", "Enter default can be 钉图");
        assert_eq!(resolve_action(Mode::Select, None, "nonsense"), "translate");
    }
}
