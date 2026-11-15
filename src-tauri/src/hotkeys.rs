use tauri::AppHandle;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::state::{state, LockExt, Mode};
use crate::windows;

fn bindings(app: &AppHandle) -> Vec<(String, Mode)> {
    let s = state(app).settings.lock_safe().clone();
    vec![(s.hotkey_capture, Mode::Select), (s.hotkey_pin, Mode::Pin), (s.hotkey_ocr, Mode::Ocr), (s.hotkey_translate, Mode::Translate)]
}

/// A global shortcut takes its key away from every other app for as long as it is registered, so only real
/// combinations are accepted: Command / Control / Option plus a key, or an F-key (alone or with any modifier).
/// Never a bare Enter, Esc, digit or letter, and not Shift + key (that would swallow capitals). 0.5.6 registered Enter, Esc and 1–4 globally while
/// the selection overlay was up: when macOS put its screen-recording confirmation in front of the overlay, Enter
/// went to the overlay instead of the confirmation or any other app, and the capture looked hung.
pub fn allowed_global(sc: &Shortcut) -> bool {
    let f_key = matches!(
        sc.key,
        Code::F1 | Code::F2 | Code::F3 | Code::F4 | Code::F5 | Code::F6 | Code::F7 | Code::F8 | Code::F9 | Code::F10 | Code::F11 | Code::F12
            | Code::F13 | Code::F14 | Code::F15 | Code::F16 | Code::F17 | Code::F18 | Code::F19 | Code::F20
    );
    f_key || sc.mods.intersects(Modifiers::SUPER | Modifiers::CONTROL | Modifiers::ALT | Modifiers::META)
}

/// (Re-)register the configured shortcuts; returns human-readable errors.
///
/// Runs on the main thread: the plugin's `unregister_all` keeps its shortcut table locked while it waits for the
/// main thread, and the main thread locks that table to dispatch every key press — called from a worker (saving
/// settings) at the moment a hotkey fired, the two waited on each other forever.
pub fn register_all(app: &AppHandle) -> Vec<String> {
    crate::on_main_thread(app, register_all_now).unwrap_or_else(|| vec!["快捷键注册超时（主线程无响应）".into()])
}

fn register_all_now(app: &AppHandle) -> Vec<String> {
    let gs = app.global_shortcut();
    let mut errors = vec![];
    if let Err(e) = gs.unregister_all() {
        errors.push(format!("释放旧快捷键失败：{e}"));
    }
    for (key, _) in bindings(app) {
        if key.trim().is_empty() {
            continue;
        }
        match key.parse::<Shortcut>() {
            Ok(sc) if !allowed_global(&sc) => errors.push(format!("{key}: 需要 ⌘ / ⌃ / ⌥ 组合键（单独的按键会让其他应用收不到它）")),
            Ok(sc) => {
                if let Err(e) = gs.register(sc) {
                    errors.push(format!("{key}: {e}"));
                } else {
                    crate::log_info(&format!("hotkey registered: {key}"));
                }
            }
            Err(e) => errors.push(format!("{key}: {e}")),
        }
    }
    errors
}

/// Rebuild native registrations without restarting the app or losing open pins.
pub fn repair(app: &AppHandle, reason: &str, notify: bool) {
    let errors = register_all(app);
    if errors.is_empty() {
        crate::log_info(&format!("hotkeys restored: {reason}"));
        if notify {
            windows::show_toast(app, "快捷键已重新注册，请再次按截图快捷键", "info");
        }
    } else {
        crate::log_error(&format!("hotkey recovery ({reason}) failed: {errors:?}"));
        windows::show_toast_for(app, &format!("快捷键恢复失败：{}。请在设置中检查组合键。", errors.join("；")), "error", 8000);
    }
}

fn resumed_after_pause(previous: std::time::SystemTime, now: std::time::SystemTime) -> bool {
    // Wall time includes sleep on macOS; Instant uses a clock that may pause during sleep.
    now.duration_since(previous).is_ok_and(|gap| gap >= std::time::Duration::from_secs(20))
}

pub fn start_recovery_watch(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let mut previous = std::time::SystemTime::now();
        #[cfg(target_os = "macos")]
        let mut tray_checked = std::time::Instant::now();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(5));
            let now = std::time::SystemTime::now();
            let resumed = resumed_after_pause(previous, now);
            #[cfg(target_os = "macos")]
            if resumed || tray_checked.elapsed() >= std::time::Duration::from_secs(30) {
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || crate::check_status_item(&handle, false));
                tray_checked = std::time::Instant::now();
            }
            if resumed {
                repair(&app, "resume after sleep or scheduling pause", false);
            }
            previous = std::time::SystemTime::now();
        }
    });
}

/// Plain keys (no modifier) this process holds as system-wide hotkeys; must always be empty. Asks Carbon itself
/// rather than the plugin's bookkeeping: registering a key this process already holds fails with
/// eventHotKeyExistsErr, a free one is released again at once. Main thread only. Used by the
/// OCRTRANS_TEST_OVERLAY_KEYS self-test (selftest.rs).
#[cfg(target_os = "macos")]
pub fn plain_keys_held() -> Vec<&'static str> {
    use std::ffi::c_void;
    #[repr(C)]
    struct EventHotKeyId {
        signature: u32,
        id: u32,
    }
    #[link(name = "Carbon", kind = "framework")]
    extern "C" {
        fn GetApplicationEventTarget() -> *mut c_void;
        fn RegisterEventHotKey(code: u32, mods: u32, id: EventHotKeyId, target: *mut c_void, options: u32, out: *mut *mut c_void) -> i32;
        fn UnregisterEventHotKey(hotkey: *mut c_void) -> i32;
    }
    const EXISTS: i32 = -9878; // eventHotKeyExistsErr
    // virtual key codes (Events.h): kVK_Return, kVK_ANSI_KeypadEnter, kVK_Escape, kVK_ANSI_1 … 4
    const KEYS: [(&str, u32); 7] = [("Return", 36), ("KeypadEnter", 76), ("Escape", 53), ("1", 18), ("2", 19), ("3", 20), ("4", 21)];
    let mut held = vec![];
    for (i, (name, code)) in KEYS.iter().enumerate() {
        let mut r: *mut c_void = std::ptr::null_mut();
        let id = EventHotKeyId { signature: u32::from_be_bytes(*b"prob"), id: i as u32 + 1 };
        let st = unsafe { RegisterEventHotKey(*code, 0, id, GetApplicationEventTarget(), 0, &mut r) };
        if st == 0 && !r.is_null() {
            unsafe { UnregisterEventHotKey(r) };
        } else if st == EXISTS {
            held.push(*name);
        }
    }
    held
}

pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            for (key, mode) in bindings(app) {
                if key.parse::<Shortcut>().ok().as_ref() == Some(shortcut) {
                    crate::perf_log(&format!("hotkey {key} → {mode:?}"));
                    windows::start_capture(app, mode);
                    break;
                }
            }
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleep_gap_triggers_recovery_but_regular_ticks_and_clock_rollback_do_not() {
        use std::time::{Duration, SystemTime};
        let before = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        assert!(!resumed_after_pause(before, before + Duration::from_secs(5)));
        assert!(resumed_after_pause(before, before + Duration::from_secs(20)));
        assert!(resumed_after_pause(before, before + Duration::from_secs(3600)));
        assert!(!resumed_after_pause(before, before - Duration::from_secs(60)));
    }

    fn ok(s: &str) -> bool {
        allowed_global(&s.parse::<Shortcut>().unwrap())
    }

    #[test]
    fn only_real_combinations_are_registered_globally() {
        for bare in ["Enter", "Escape", "1", "4", "A", "Space", "Shift+A", "Shift+Enter"] {
            assert!(!ok(bare), "{bare} would be taken from every other app");
        }
        for combo in ["Command+Control+A", "Alt+Shift+O", "Control+1", "Command+Enter", "F5", "F13", "Shift+F5"] {
            assert!(ok(combo), "{combo}");
        }
    }
}
