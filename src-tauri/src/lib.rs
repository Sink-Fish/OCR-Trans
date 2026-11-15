mod commands;
pub mod capture_worker;
mod geometry;
mod history;
mod hotkeys;
mod instance;
mod models;
mod paths;
#[cfg(feature = "llm")]
pub mod quality_worker;
mod selftest;
mod settings;
mod state;
mod windows;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;

use state::{AppState, LockExt, Mode};

fn tray_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let s = state::state(app).settings.lock_safe().clone();
    let item = |id: &str, text: &str, accel: Option<&str>| MenuItem::with_id(app, id, text, true, accel);
    Menu::with_items(
        app,
        &[
            &item("select", &format!("截图…\t{}", pretty_hotkey(&s.hotkey_capture)), None)?,
            &PredefinedMenuItem::separator(app)?,
            &item("translate", &format!("直接截图翻译\t{}", pretty_hotkey(&s.hotkey_translate)), None)?,
            &item("ocr", &format!("直接截图识字\t{}", pretty_hotkey(&s.hotkey_ocr)), None)?,
            &item("pin", &format!("直接截图钉图\t{}", pretty_hotkey(&s.hotkey_pin)), None)?,
            &PredefinedMenuItem::separator(app)?,
            &item("history", "截图历史…", None)?,
            &item("close_pins", "关闭所有钉图", None)?,
            &item("settings", "设置…", None)?,
            &item("repair_hotkeys", "修复快捷键", None)?,
            &item("models", "模型管理…", None)?,
            &item("onboarding", "新手引导 / 权限检查…", None)?,
            &PredefinedMenuItem::separator(app)?,
            &item("quit", "退出", None)?,
        ],
    )
}

/// "Alt+Shift+O" → "⌥⇧O" on macOS (menu labels).
fn pretty_hotkey(k: &str) -> String {
    if !cfg!(target_os = "macos") {
        return k.to_string();
    }
    k.split('+')
        .map(|p| match p {
            "Command" | "Cmd" | "Super" => "⌘".to_string(),
            "Control" | "Ctrl" => "⌃".to_string(),
            "Alt" | "Option" => "⌥".to_string(),
            "Shift" => "⇧".to_string(),
            "CommandOrControl" => "⌘".to_string(),
            other => other.to_string(),
        })
        .collect()
}

/// Rebuild the tray menu (its labels show the current hotkeys).
pub fn refresh_tray(app: &tauri::AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("main"), tray_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

// Test profiles isolate files and shortcuts, but AppKit assigns menu items by bundle ID.
// Never give a parallel test process a status item belonging to the installed application.
#[cfg(target_os = "macos")]
fn isolated_menu_bar_profile() -> bool {
    std::env::var_os("OCRTRANS_TEST_PROFILE").is_some()
}

fn build_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    if isolated_menu_bar_profile() {
        return Ok(());
    }
    let menu = tray_menu(app)?;
    let mut icon = app.default_window_icon().cloned();
    // (joined component by component: "icons/tray.png" in one piece kept its "/" in a Windows path)
    if let Some(p) = paths::resource_dir(app) {
        if let Ok(i) = tauri::image::Image::from_path(p.join("icons").join("tray.png")) {
            icon = Some(i);
        }
    }
    let mut b = TrayIconBuilder::with_id("main")
        .tooltip("OCR 翻译")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "select" => windows::start_capture(app, Mode::Select),
            "pin" => windows::start_capture(app, Mode::Pin),
            "ocr" => windows::start_capture(app, Mode::Ocr),
            "translate" => windows::start_capture(app, Mode::Translate),
            "close_pins" => windows::close_all_pins(app),
            "history" => windows::open_history(app),
            "settings" => windows::open_settings(app, Some("general")),
            "repair_hotkeys" => hotkeys::repair(app, "tray", true),
            "onboarding" => windows::open_onboarding(app),
            "models" => windows::open_settings(app, Some("models")),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(i) = icon {
        b = b.icon(i).icon_as_template(cfg!(target_os = "macos"));
    }
    let _tray = b.build(app)?;
    #[cfg(target_os = "macos")]
    name_status_item(&_tray)?;
    Ok(())
}

// Keep the existing Item-0 position when macOS reconnects the menu bar.
#[cfg(target_os = "macos")]
fn name_status_item(tray: &tauri::tray::TrayIcon) -> tauri::Result<()> {
    tray.with_inner_tray_icon(|inner| {
        if let Some(item) = inner.ns_status_item() {
            item.setAutosaveName(Some(&objc2_foundation::NSString::from_str("Item-0")));
        }
    })
}

#[cfg(target_os = "macos")]
fn check_status_item(app: &tauri::AppHandle, reopen: bool) {
    if isolated_menu_bar_profile() {
        return;
    }
    let result = (|| -> tauri::Result<()> {
        let Some(tray) = app.tray_by_id("main") else {
            return build_tray(app);
        };
        // A restarted Control Center can leave a valid-looking local button whose
        // remote menu bar host has gone away. Reattach once to the new host.
        static LAST_HOST: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
        let hosts = objc2_app_kit::NSRunningApplication::runningApplicationsWithBundleIdentifier(
            &objc2_foundation::NSString::from_str("com.apple.controlcenter"),
        );
        let host_changed = hosts.firstObject().is_some_and(|host| {
            let pid = host.processIdentifier();
            let last = LAST_HOST.swap(pid, std::sync::atomic::Ordering::Relaxed);
            last != 0 && last != pid
        });
        let attached = tray.with_inner_tray_icon(move |inner| {
            let Some(item) = inner.ns_status_item() else { return false };
            if !item.isVisible() {
                if !reopen { return true; } // honour an explicitly hidden item during background checks
                item.setVisible(true);
            }
            !host_changed && !reopen && inner.rect().is_some()
        })?;
        if !attached {
            // set_visible(true) alone reuses a stale NSStatusItem if the handle still exists.
            tray.set_visible(false)?;
            tray.set_visible(true)?;
            name_status_item(&tray)?;
            log_info(&format!("reattached menu bar item (reopen={reopen}, host changed={host_changed})"));
        }
        Ok(())
    })();
    if let Err(e) = result {
        log_error(&format!("checking menu bar item: {e}"));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(hotkeys::plugin())
        // images for the webviews; served from a worker thread so encoding / copying a full Retina frame never
        // blocks the main (UI) thread
        .register_asynchronous_uri_scheme_protocol("shot", |ctx, req, responder| {
            let app = ctx.app_handle().clone();
            let path = req.uri().path().to_string();
            std::thread::spawn(move || {
                let resp = tauri::http::Response::builder().header("Access-Control-Allow-Origin", "*");
                let resp = match commands::serve_shot(&app, &path) {
                    Some((body, mime)) => resp.header("Content-Type", mime).header("Cache-Control", "no-store").body(body),
                    None => resp.status(404).body(Vec::new()),
                };
                if let Ok(r) = resp {
                    responder.respond(r);
                }
            });
        })
        .setup(|app| {
            let instance_dir = std::env::var_os("OCRTRANS_TEST_PROFILE").map(std::path::PathBuf::from)
                .map(Ok).unwrap_or_else(|| app.path().app_config_dir())?;
            match instance::acquire(&instance_dir)? {
                Some(guard) => { app.manage(guard); }
                None => {
                    eprintln!("OCR Trans is already running for this profile");
                    std::process::exit(0);
                }
            }
            install_panic_log(app.handle());
            ocrtrans_core::set_logger(log_info);
            #[cfg(target_os = "macos")]
            disable_app_nap();
            // login item: if it is on, rewrite it with this copy's path (the app moved to /Applications in 0.5.4)
            {
                use tauri_plugin_autostart::ManagerExt;
                let al = app.autolaunch();
                if std::env::var_os("OCRTRANS_TEST_PROFILE").is_none() && al.is_enabled().unwrap_or(false) {
                    let _ = al.enable();
                }
            }
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let handle = app.handle().clone();
            let loaded = settings::load(&handle);
            ocrtrans_core::set_llm_device(loaded.llm_device == "cpu", loaded.llm_gpu_failed.clone());
            app.manage(AppState::new(loaded));
            build_tray(&handle)?;
            #[cfg(target_os = "macos")]
            check_status_item(&handle, false);
            history::secure(&handle);
            log_info(&format!(
                "start {} pid={} (screen recording permission: {})",
                app.package_info().version,
                std::process::id(),
                if windows::has_screen_access() { "yes" } else { "NO" }
            ));
            // (this also drops every shortcut not in the settings: only the configured combinations are ever held)
            let errs = hotkeys::register_all(&handle);
            if !errs.is_empty() {
                log_error(&format!("hotkey errors: {errs:?}"));
                // a hotkey that silently stops working looks like a hang: say so (设置 → 常规 shows it as well)
                windows::show_toast_for(&handle, &format!("快捷键没有注册成功：{}。请在 设置 → 常规 里换一个组合。", errs.join("；")), "error", 8000);
            }
            hotkeys::start_recovery_watch(&handle);
            windows::warm_up(&handle);
            // SIGTERM (install_mac.sh, `kill`, logout) quits through the normal exit path: models freed, then _exit
            #[cfg(unix)]
            {
                let h = handle.clone();
                tauri::async_runtime::spawn(async move {
                    use tokio::signal::unix::{signal, SignalKind};
                    if let Ok(mut term) = signal(SignalKind::terminate()) {
                        term.recv().await;
                        h.exit(0);
                    }
                });
            }
            {
                let h = handle.clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                    state::state(&h).unload_idle();
                });
            }
            let onboarded = state::state(&handle).settings.lock_safe().onboarded;
            let testing = std::env::var_os("OCRTRANS_TEST_CAPTURE").is_some() || std::env::var_os("OCRTRANS_TEST_PIN").is_some();
            if (!onboarded && !testing) || std::env::var_os("OCRTRANS_FORCE_ONBOARDING").is_some() {
                windows::open_onboarding(&handle);
            } else if models::locate(&handle, "ocr").is_none() {
                windows::open_settings(&handle, Some("models"));
            } else if let Ok(tab) = std::env::var("OCRTRANS_OPEN_SETTINGS") {
                windows::open_settings(&handle, Some(&tab));
            }
            // developer hook: OCRTRANS_TEST_POPUP=<png>:<runs>:<js|safe|hold> — open the 识字 text window on a test image,
            // translate, close (old window.close() vs the safe close), repeat. Used to chase the 0.5.1 WebKit crash.
            if let Ok(spec) = std::env::var("OCRTRANS_TEST_POPUP") {
                let h = handle.clone();
                std::thread::spawn(move || {
                    let mut it = spec.split(':');
                    let (path, runs, mode) = (it.next().unwrap_or_default().to_string(), it.next().and_then(|n| n.parse().ok()).unwrap_or(5u32), it.next().unwrap_or("safe").to_string());
                    let Ok(img) = image::open(&path) else { return };
                    let img = img.to_rgba8();
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    for i in 0..runs {
                        let rect = [140.0, 520.0, img.width() as f64 / 2.0, img.height() as f64 / 2.0];
                        let Ok(id) = windows::add_pin(&h, img.clone(), 2.0, rect) else { break };
                        let _ = windows::open_text_window_with(&h, id, false, Some(&mode));
                        perf_log(&format!("popup test {i} ({mode})"));
                        std::thread::sleep(std::time::Duration::from_millis(2500));
                    }
                    perf_log("popup test done");
                });
            }
            if let Ok(msg) = std::env::var("OCRTRANS_TEST_TOAST") {
                let h = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    windows::show_toast(&h, &msg, "ok");
                });
            }
            if std::env::var_os("OCRTRANS_OPEN_HISTORY").is_some() {
                windows::open_history(&handle);
            }
            // developer hook: OCRTRANS_TEST_PIN=<png>[:translate] opens a pin from an image file
            if let Ok(spec) = std::env::var("OCRTRANS_TEST_PIN") {
                let (path, tr) = spec.strip_suffix(":translate").map(|p| (p.to_string(), true)).unwrap_or((spec, false));
                if let Ok(img) = image::open(&path) {
                    let img = img.to_rgba8();
                    // OCRTRANS_TEST_PIN_AT=x,y places it (e.g. near a screen edge to check the toolbar layout)
                    let at: Vec<f64> = std::env::var("OCRTRANS_TEST_PIN_AT").unwrap_or_default().split(',').filter_map(|v| v.parse().ok()).collect();
                    let (x, y) = if at.len() == 2 { (at[0], at[1]) } else { (120.0, 120.0) };
                    let rect = [x, y, img.width() as f64 / 2.0, img.height() as f64 / 2.0];
                    let id = windows::add_pin(&handle, img, 2.0, rect)?;
                    let _ = windows::open_pin(&handle, id, tr);
                }
            }
            // developer hook: OCRTRANS_TEST_DOWNLOAD=<model id> downloads a model at startup and logs the result
            if let Ok(id) = std::env::var("OCRTRANS_TEST_DOWNLOAD") {
                let h = handle.clone();
                tauri::async_runtime::spawn(async move {
                    let t = std::time::Instant::now();
                    let r = models::download(h, id.clone()).await;
                    eprintln!("download {id}: {r:?} in {:.1}s", t.elapsed().as_secs_f64());
                });
            }
            // developer hook: OCRTRANS_TEST_OVERLAY_KEYS=1 (with OCRTRANS_TEST_SHOT=<png>) runs the overlay lifecycle
            // self-test (selftest.rs, scripts/check_overlay_keys.sh) and quits
            if std::env::var_os("OCRTRANS_TEST_OVERLAY_KEYS").is_some() {
                let h = handle.clone();
                std::thread::spawn(move || selftest::overlay_keys(&h));
            }
            // developer hook: OCRTRANS_TEST_CAPTURE=<pin|ocr|translate>[@x,y,w,h] runs the capture flow and
            // (optionally) completes the selection, so the whole path can be exercised without a keyboard
            if let Ok(spec) = std::env::var("OCRTRANS_TEST_CAPTURE") {
                let h = handle.clone();
                std::thread::spawn(move || {
                    let (mode, sel) = spec.split_once('@').map(|(m, r)| (m.to_string(), Some(r.to_string()))).unwrap_or((spec, None));
                    let mode = match mode.as_str() {
                        "ocr" => Mode::Ocr,
                        "translate" => Mode::Translate,
                        "select" => Mode::Select,
                        _ => Mode::Pin,
                    };
                    let delay = std::env::var("OCRTRANS_TEST_DELAY").ok().and_then(|v| v.parse().ok()).unwrap_or(2);
                    std::thread::sleep(std::time::Duration::from_secs(delay));
                    windows::start_capture(&h, mode);
                    // "@cancel": take a real capture and cancel it after 4 s if it is still up (checks what happens
                    // around the capture, e.g. macOS's screen-recording confirmation, without keeping anything)
                    if sel.as_deref() == Some("cancel") {
                        std::thread::sleep(std::time::Duration::from_secs(4));
                        if state::state(&h).shot.lock_safe().is_some() {
                            log_info("test capture still up after 4 s — cancelled");
                            windows::overlay_cancel(&h, None);
                        }
                        return;
                    }
                    if let Some(r) = sel {
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        let v: Vec<f64> = r.split(',').filter_map(|x| x.parse().ok()).collect();
                        if v.len() == 4 {
                            let action = std::env::var("OCRTRANS_TEST_ACTION").ok();
                            if let Err(e) = windows::overlay_done(&h, None, v[0], v[1], v[2], v[3], action.as_deref()) {
                                eprintln!("test select failed: {e:#}");
                            }
                        }
                    }
                });
            }
            Ok(())
        })
        .on_window_event(|win, ev| {
            if let tauri::WindowEvent::Destroyed = ev {
                let label = win.label().to_string();
                let app = win.app_handle();
                if let Some(id) = label.strip_prefix("pin-").or(label.strip_prefix("ocr-")) {
                    // drop the pin data once neither its pin nor its OCR popup is open
                    let other = if label.starts_with("pin-") { format!("ocr-{id}") } else { format!("pin-{id}") };
                    if app.get_webview_window(&other).is_none() {
                        if let Ok(id) = id.parse::<u32>() {
                            state::state(app).pins.lock_safe().remove(&id);
                        }
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::log,
            commands::overlay_current,
            commands::overlay_ready,
            commands::overlay_failed,
            commands::overlay_interaction,
            commands::overlay_cancel,
            commands::overlay_done,
            commands::start_capture,
            commands::pin_info,
            commands::pin_process,
            commands::pin_cancel,
            commands::pin_set_translation,
            commands::open_pin,
            commands::copy_text,
            commands::open_text_window,
            commands::toast,
            commands::copy_png,
            commands::save_png,
            commands::close_all_pins,
            commands::get_settings,
            commands::set_settings,
            commands::patch_settings,
            commands::model_status,
            commands::model_verify,
            commands::model_download,
            commands::model_cancel,
            commands::model_delete,
            commands::models_folder,
            commands::pick_folder,
            commands::env_info,
            commands::third_party_notices,
            commands::diagnostics,
            commands::save_diagnostics,
            commands::release_probe,
            commands::open_third_party_notices,
            commands::llm_info,
            commands::request_screen_access,
            commands::restart_app,
            commands::open_settings,
            commands::close_self,
            commands::open_history,
            commands::history_list,
            commands::history_recover,
            commands::history_pin,
            commands::history_delete,
            commands::history_clear,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, ev| match ev {
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => {
                check_status_item(app, true);
                windows::open_settings(app, Some("general"));
            }
            // keep running in the tray when the last window closes
            tauri::RunEvent::ExitRequested { api, code, .. } if code.is_none() => api.prevent_exit(),
            // Quitting (tray 退出, ⌘Q, logout, Apple-event quit): free the models first, then leave without C++ static
            // teardown. With the Qwen model still loaded, llama.cpp's Metal backend aborts in its static destructor
            // (GGML_ASSERT rsets->data count == 0) — the 0.5.1 quit crash. Reproduced and covered by
            // crates/core/tests/exit.rs.
            tauri::RunEvent::Exit => {
                // stop a running translation first: otherwise quitting waited for it to finish (0.5.4)
                ocrtrans_core::abort_work();
                state::state(app).invalidate_pipeline();
                perf_log("exit: models released");
                ocrtrans_core::exit_now(0);
            }
            _ => {}
        });
}

static LOG_FILE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Timing line for regressions: stderr + `<log dir>/ocr-trans.log`.
pub fn perf_log(msg: &str) {
    write_log("perf", msg);
}

/// Something went wrong (capture failed, overlay cancelled, …): stderr + the log file.
pub fn log_error(msg: &str) {
    write_log("error", msg);
}

pub fn log_info(msg: &str) {
    write_log("info", msg);
}

fn write_log(kind: &str, msg: &str) {
    eprintln!("[{kind}] {msg}");
    if let Some(f) = LOG_FILE.get() {
        use std::io::Write;
        static WRITER: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = WRITER.lock().unwrap_or_else(|e| e.into_inner());
        if std::fs::metadata(f).is_ok_and(|meta| meta.len() >= 4 * 1024 * 1024) {
            let _ = std::fs::rename(f, f.with_extension("previous.log"));
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(f) {
            let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let _ = writeln!(f, "[{ts}] {kind}: {msg}");
        }
    }
}

/// Run `f` on the main (event loop) thread and wait for its result; directly when already there. None if the main
/// thread did not get to it within 5 s.
pub fn on_main_thread<T: Send + 'static>(app: &tauri::AppHandle, f: impl FnOnce(&tauri::AppHandle) -> T + Send + 'static) -> Option<T> {
    if std::thread::current().name() == Some("main") {
        return Some(f(app));
    }
    let timeout = std::time::Duration::from_secs(5);
    let deadline = std::time::Instant::now() + timeout;
    let (tx, rx) = std::sync::mpsc::channel();
    let h = app.clone();
    app.run_on_main_thread(move || {
        // If the event loop was blocked, an expired action must not mutate newer UI state when it recovers.
        if std::time::Instant::now() < deadline {
            let _ = tx.send(f(&h));
        }
    })
    .ok()?;
    rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())).ok()
}

/// Append panics (with location) to `<app log dir>/ocr-trans.log` in addition to stderr.
fn install_panic_log(app: &tauri::AppHandle) {
    let Some(dir) = std::env::var_os("OCRTRANS_TEST_PROFILE").map(std::path::PathBuf::from).or_else(|| app.path().app_log_dir().ok()) else { return };
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("ocr-trans.log");
    let _ = LOG_FILE.set(file.clone());
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&file) {
            let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let _ = writeln!(f, "[{ts}] panic: {info}");
        }
        default(info);
    }));
}

/// Opt out of App Nap. As a menu-bar utility we are "in the background" almost all the time; App Nap then
/// throttles our timers and the hidden overlay's web process, so the first hotkey after a while felt frozen.
/// Costs nothing while idle (no work is scheduled), only removes the throttling.
#[cfg(target_os = "macos")]
fn disable_app_nap() {
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2::msg_send;
    unsafe {
        let Some(cls) = AnyClass::get(c"NSProcessInfo") else { return };
        let info: *mut AnyObject = msg_send![cls, processInfo];
        let reason: *mut AnyObject = {
            let ns_string = AnyClass::get(c"NSString").unwrap();
            msg_send![ns_string, stringWithUTF8String: c"Global hotkeys must respond instantly".as_ptr()]
        };
        // NSActivityUserInitiatedAllowingIdleSystemSleep
        let options: u64 = 0x00FF_FFFF & !(1u64 << 20);
        let token: *mut AnyObject = msg_send![info, beginActivityWithOptions: options, reason: reason];
        // keep the activity for the whole process lifetime
        let _: *mut AnyObject = msg_send![token, retain];
    }
}
