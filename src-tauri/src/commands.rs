use std::io::Cursor;

use ocrtrans_core::PipelineResult;
use serde::Serialize;
use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;

use crate::state::{state, LockExt, Mode};
use crate::{hotkeys, models, settings, windows};

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Run a command body, turning a panic into an error message instead of letting it reach the UI thread.
fn guard<T>(f: impl FnOnce() -> CmdResult<T>) -> CmdResult<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|p| {
        let msg = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default();
        Err(format!("内部错误：{msg}"))
    })
}

/// Frontend diagnostics: stderr and the app's log file (errors from the pages used to reach stderr only, so a
/// user's log never showed why a selection failed).
#[tauri::command]
pub fn log(window: tauri::WebviewWindow, msg: String) {
    crate::log_info(&format!("[{}] {msg}", window.label()));
}

// Commands that create windows are async: a sync command runs on the main thread, and building a window from
// there deadlocks on Windows (wry#583). Async commands also keep the work (crop, history, PNG decode) off the UI thread.

#[tauri::command]
pub fn overlay_current(app: AppHandle) -> Option<serde_json::Value> {
    windows::overlay_payload(&app)
}

#[tauri::command]
pub async fn overlay_ready(app: AppHandle, token: Option<u32>, fetch_ms: Option<f64>, paint_ms: Option<f64>) -> CmdResult<()> {
    windows::overlay_ready(&app, token, fetch_ms.zip(paint_ms)).map_err(err)
}

#[tauri::command]
pub fn overlay_failed(app: AppHandle, token: u32) {
    windows::overlay_failed(&app, token);
}

#[tauri::command]
pub fn overlay_interaction(app: AppHandle, token: u32, active: bool) -> bool {
    windows::overlay_interaction(&app, token, active)
}

#[tauri::command]
pub fn overlay_cancel(app: AppHandle, token: Option<u32>) {
    windows::overlay_cancel(&app, token);
}

#[tauri::command]
pub async fn overlay_done(app: AppHandle, token: Option<u32>, x: f64, y: f64, w: f64, h: f64, action: Option<String>) -> CmdResult<()> {
    guard(|| windows::overlay_done(&app, token, x, y, w, h, action.as_deref()).map_err(err)).inspect_err(|e| crate::log_error(&format!("selection failed: {e}")))
}

#[tauri::command]
pub fn start_capture(app: AppHandle, mode: Mode) {
    windows::start_capture(&app, mode);
}

#[derive(Serialize)]
pub struct PinInfo {
    width: u32,
    height: u32,
    scale: f32,
    font_family: String,
}

#[tauri::command]
pub async fn pin_info(app: AppHandle, id: u32) -> CmdResult<PinInfo> {
    let st = state(&app);
    let font_family = st.settings.lock_safe().font_family.clone();
    let pins = st.pins.lock_safe();
    let p = pins.get(&id).ok_or("pin not found")?;
    Ok(PinInfo { width: p.image.width(), height: p.image.height(), scale: p.scale, font_family })
}

/// OCR (and optionally translate) a pin; results are cached on the pin.
/// `engine`: "fast" | "quality"; defaults to the engine chosen in settings.
#[tauri::command]
pub async fn pin_process(app: AppHandle, window: tauri::WebviewWindow, id: u32, translate: bool, engine: Option<String>) -> CmdResult<PipelineResult> {
    if std::env::var_os("OCRTRANS_DEBUG").is_some() {
        crate::perf_log(&format!("pin_process call: pin {id} translate={translate}"));
    }
    use std::sync::{Arc, OnceLock};
    use std::time::Instant;
    static SLOTS: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    let permit = SLOTS.get_or_init(|| Arc::new(tokio::sync::Semaphore::new(8))).clone()
        .try_acquire_owned().map_err(|_| "处理队列已满，请稍后再试".to_string())?;
    let owner = window.label().to_owned();
    let (generation, revision) = {
        let st = state(&app);
        let mut pins = st.pins.lock_safe();
        let pin = pins.get_mut(&id).ok_or("钉图已关闭")?;
        if translate { pin.translation_generation = pin.translation_generation.wrapping_add(1); }
        (pin.translation_generation, pin.edit_revision)
    };
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<PipelineResult> {
        let _permit = permit;
        use image::buffer::ConvertBuffer;
        let st = state(&app);
        let settings = st.settings.lock_safe().clone();
        let engine = engine.or_else(|| std::env::var("OCRTRANS_ENGINE").ok()).unwrap_or(settings.engine);
        let started = Instant::now();
        let epoch = st.model_generation.load(std::sync::atomic::Ordering::SeqCst);
        let cancelled = || app.get_webview_window(&owner).is_none() || st.model_generation.load(std::sync::atomic::Ordering::SeqCst) != epoch || ocrtrans_core::is_exiting() || st.pins.lock_safe().get(&id).is_none_or(|p| {
            p.edit_revision != revision || (translate && p.translation_generation != generation)
        });
        let stage = |message: &str| {
            if translate && !cancelled() {
                let _ = app.emit_to(format!("pin-{id}"), "pin-busy", message);
                let _ = app.emit_to(format!("ocr-{id}"), "pin-busy", message);
            }
        };
        let (img, cached) = {
            let pins = st.pins.lock_safe();
            let pin = pins.get(&id).ok_or_else(|| anyhow::anyhow!("钉图已关闭"))?;
            if let Some(r) = &pin.ocr {
                if !translate || (pin.translated.as_deref() == Some(engine.as_str()) && pin.translated_glossary == settings.glossary && r.warnings.is_empty()) {
                    return Ok(r.clone());
                }
            }
            (pin.image.clone(), pin.ocr.clone())
        };
        let mut result = match cached {
            Some(r) => r,
            None => {
                stage("正在识字…");
                st.with_pipeline(&app, |p| {
                    anyhow::ensure!(!cancelled(), "任务已取消");
                    let mut pins = st.pins.lock_safe();
                    if let Some(r) = pins.get(&id).and_then(|pin| pin.ocr.as_ref()) { return Ok(r.clone()); }
                    drop(pins);
                    let r = p.recognize(&img.convert())?;
                    pins = st.pins.lock_safe();
                    let pin = pins.get_mut(&id).ok_or_else(|| anyhow::anyhow!("钉图已关闭"))?;
                    anyhow::ensure!(pin.edit_revision == revision, "识字结果已失效");
                    pin.ocr = Some(r.clone());
                    if let Some(h) = pin.history_id { crate::history::update_text(&app, h, &r.plain_text(), None); }
                    Ok(r)
                })?
            }
        };
        if !translate {
            crate::perf_log(&format!("pin {id} ocr: {} ms total", started.elapsed().as_millis()));
            return Ok(result);
        }
        for paragraph in &mut result.paragraphs { paragraph.translation = None; }
        result.warnings.clear();
        let translation_start = Instant::now();
        #[cfg(feature = "llm")]
        let mut used_llm = false;
        #[cfg(not(feature = "llm"))]
        let used_llm = false;
        if engine == "quality" {
            let todo: Vec<_> = result.paragraphs.iter().enumerate().filter_map(|(i, p)| match p.lang {
                ocrtrans_core::Lang::En => Some((i, "en")), ocrtrans_core::Lang::Ja => Some((i, "ja")), _ => None,
            }).collect();
            if !todo.is_empty() {
                #[cfg(feature = "llm")]
                {
                    let response = (|| -> anyhow::Result<_> {
                        let _assets = st.model_access.read().unwrap_or_else(|e| e.into_inner());
                        let model = models::locate(&app, "qwen3-1.7b").ok_or_else(|| anyhow::anyhow!("未安装 Qwen3 模型"))?;
                        let mut request = crate::quality_worker::Request {
                            model: model.join("model.gguf"), cpu: settings.llm_device == "cpu" || settings.llm_gpu_failed.is_some(),
                            paragraphs: todo.iter().map(|(i, lang)| (lang.to_string(), result.paragraphs[*i].text.clone())).collect(),
                            glossary: settings.glossary.clone(),
                        };
                        let deadline = started + std::time::Duration::from_secs(240);
                        let progress = |done, total| stage(&format!("高质量翻译 {done}/{total} 段"));
                        stage("正在等待或加载高质量模型…");
                        let first = crate::quality_worker::translate(&request, deadline, cancelled, progress);
                        match first {
                            Err(e) if !request.cpu && !cancelled() && Instant::now() < deadline => {
                                stage("正在使用 CPU 重试…");
                                request.cpu = true;
                                let mut reply = crate::quality_worker::translate(&request, deadline, cancelled, progress)?;
                                reply.gpu_failure = Some(format!("自动模式失败，CPU 重试成功：{e:#}"));
                                Ok(reply)
                            }
                            other => other,
                        }
                    })();
                    anyhow::ensure!(!cancelled(), "翻译已取消");
                    match response {
                        Ok(reply) => {
                            if let Some(reason) = reply.gpu_failure {
                                ocrtrans_core::llm::set_gpu_blocked(Some(reason));
                                remember_gpu_failure(&app);
                            }
                            let mut completed = 0;
                            for ((i, _), value) in todo.iter().zip(reply.translations) {
                                if let Some(text) = value {
                                    let text = ocrtrans_core::translate::tidy_zh(&ocrtrans_core::translate::replace_terms(&text, &settings.glossary));
                                    if !text.is_empty() { result.paragraphs[*i].translation = Some(text); completed += 1; }
                                }
                            }
                            used_llm = completed > 0;
                            if completed < todo.len() { result.warnings.push(format!("{} 段改用快速翻译", todo.len() - completed)); }
                        }
                        Err(e) => result.warnings.push(format!("高质量翻译失败，已改用快速翻译：{e:#}")),
                    }
                }
                #[cfg(not(feature = "llm"))]
                result.warnings.push("此版本未启用高质量模型，已改用快速翻译".into());
            }
        }
        anyhow::ensure!(!cancelled(), "翻译已取消");
        st.with_fast(&app, |fast| {
            anyhow::ensure!(!cancelled(), "翻译已取消");
            ocrtrans_core::Pipeline::finish_translation(fast, &mut result, &settings.glossary, used_llm, translation_start)
        })?;
        anyhow::ensure!(!cancelled(), "翻译已取消");
        let clean = ocrtrans_core::Pipeline::clean_background(&img.convert(), &mut result);
        let clean = image::DynamicImage::ImageRgb8(clean).into_rgba8();
        let mut pins = st.pins.lock_safe();
        let pin = pins.get_mut(&id).ok_or_else(|| anyhow::anyhow!("钉图已关闭"))?;
        if pin.edit_revision != revision { return pin.ocr.clone().ok_or_else(|| anyhow::anyhow!("译文已修改")); }
        anyhow::ensure!(pin.translation_generation == generation, "翻译已取消");
        pin.clean = Some(Arc::new(clean));
        pin.clean_png = None;
        pin.ocr = Some(result.clone());
        pin.translated = Some(engine);
        pin.translated_glossary = settings.glossary;
        if let Some(h) = pin.history_id {
            let text = result.paragraphs.iter().map(|p| p.translation.as_deref().unwrap_or(&p.text)).collect::<Vec<_>>().join("\n");
            crate::history::update_text(&app, h, &result.plain_text(), Some(&text));
        }
        crate::perf_log(&format!("pin {id} translate: {} ms total, {} paragraphs", started.elapsed().as_millis(), result.paragraphs.len()));
        Ok(result)
    })
    .await
    .map_err(err)?
    .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn pin_cancel(app: AppHandle, id: u32) {
    if let Some(pin) = state(&app).pins.lock_safe().get_mut(&id) {
        pin.translation_generation = pin.translation_generation.wrapping_add(1);
    }
}

/// Replace one paragraph's translation (user edit in the pin).
/// A GPU that failed during this translation is remembered in the settings, so later runs go straight to the CPU.
#[cfg(feature = "llm")]
fn remember_gpu_failure(app: &AppHandle) {
    let _g = SETTINGS_WRITE.lock_safe();
    let (_, failed) = ocrtrans_core::llm_device_state();
    let Some(reason) = failed else { return };
    let st = state(app);
    let value = {
        let mut s = st.settings.lock_safe();
        if s.llm_gpu_failed.is_some() {
            return;
        }
        s.llm_gpu_failed = Some(reason);
        s.clone()
    };
    if let Err(e) = settings::save(app, &value) {
        crate::log_error(&format!("saving the GPU failure: {e}"));
    }
}

#[derive(Serialize)]
pub struct LlmInfo {
    /// what the quality model ran on last; None until it was loaded in this run
    running_on: Option<String>,
    /// why the GPU is not used (it failed); None = it may be used
    gpu_failed: Option<String>,
    /// e.g. "4 个性能核 / 共 14 核 · avx2 fma f16c · 4 线程"
    cpu: String,
}

/// Hardware summary for 设置 → 翻译.
#[tauri::command]
pub async fn llm_info() -> LlmInfo {
    let (running_on, gpu_failed) = ocrtrans_core::llm_device_state();
    #[cfg(feature = "llm")]
    let running_on = crate::quality_worker::device().or(running_on);
    let c = ocrtrans_core::cpu::info();
    let (gen, batch) = ocrtrans_core::cpu::llm_threads();
    let cores = if c.hybrid { format!("{} 个性能核 / 共 {} 核", c.performance_cores, c.physical_cores) } else { format!("{} 核", c.physical_cores) };
    let threads = if gen == batch { format!("{gen} 线程") } else { format!("{gen}/{batch} 线程") };
    let feat = if c.features.is_empty() { String::new() } else { format!(" · {}", c.features) };
    LlmInfo { running_on, gpu_failed, cpu: format!("{cores}{feat} · {threads}") }
}

#[tauri::command]
pub async fn pin_set_translation(app: AppHandle, id: u32, index: usize, text: String) -> CmdResult<()> {
    let st = state(&app);
    let mut pins = st.pins.lock_safe();
    let p = pins.get_mut(&id).ok_or("钉图已关闭")?;
    let r = p.ocr.as_mut().ok_or("段落不存在")?;
    r.paragraphs.get_mut(index).ok_or("段落不存在")?.translation = Some(text);
    p.edit_revision = p.edit_revision.wrapping_add(1);
    // keep the history entry in sync with the user's edit
    if let Some(h) = p.history_id {
        let tr = r.paragraphs.iter().map(|p| p.translation.clone().unwrap_or_else(|| p.text.clone())).collect::<Vec<_>>().join("\n");
        crate::history::update_text(&app, h, &r.plain_text(), Some(&tr));
    }
    Ok(())
}

#[tauri::command]
pub async fn open_pin(app: AppHandle, id: u32, translate: bool) -> CmdResult<()> {
    windows::open_pin(&app, id, translate).map_err(err)
}

#[tauri::command]
pub async fn open_text_window(app: AppHandle, id: u32, translated: bool) -> CmdResult<()> {
    windows::open_text_window(&app, id, translated).map_err(err)
}

/// Only ever called on a user action (or with "识字后自动复制全部" on); logged so tests can check that.
#[tauri::command]
pub async fn copy_text(app: AppHandle, text: String) -> CmdResult<()> {
    crate::perf_log(&format!("clipboard: copy_text {} chars", text.chars().count()));
    app.clipboard().write_text(text).map_err(err)
}

/// Small transient toast pill (e.g. after "复制并关闭", when the window that asked is already gone).
#[tauri::command]
pub fn toast(app: AppHandle, msg: String, kind: Option<String>) {
    windows::show_toast(&app, &msg, kind.as_deref().unwrap_or("ok"));
}

fn png_from_request(request: &Request<'_>) -> CmdResult<Vec<u8>> {
    match request.body() {
        InvokeBody::Raw(b) => Ok(b.clone()),
        _ => Err("expected raw PNG body".into()),
    }
}

/// Copy a PNG (rendered by the pin canvas, incl. doodles/translation) to the clipboard.
#[tauri::command]
pub async fn copy_png(app: AppHandle, request: Request<'_>) -> CmdResult<()> {
    let png = png_from_request(&request)?;
    guard(|| {
        let img = image::load_from_memory(&png).map_err(err)?.to_rgba8();
        let (w, h) = img.dimensions();
        let raw = img.into_raw();
        app.clipboard().write_image(&tauri::image::Image::new(&raw, w, h)).map_err(err)
    })
}

#[tauri::command]
pub async fn save_png(app: AppHandle, request: Request<'_>) -> CmdResult<Option<String>> {
    let png = png_from_request(&request)?;
    let name = format!("ocr-trans-{}.png", chrono_like_stamp());
    let mut dlg = app.dialog().file().add_filter("PNG", &["png"]).set_file_name(&name);
    if let Ok(d) = app.path().picture_dir() {
        dlg = dlg.set_directory(d);
    }
    let Some(path) = windows::with_pins_lowered(&app, || dlg.blocking_save_file()) else { return Ok(None) };
    let path = path.into_path().map_err(err)?;
    std::fs::write(&path, png).map_err(err)?;
    Ok(Some(path.display().to_string()))
}

fn chrono_like_stamp() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    s.to_string()
}

#[tauri::command]
pub async fn close_all_pins(app: AppHandle) {
    windows::close_all_pins(&app);
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> settings::Settings {
    state(&app).settings.lock_safe().clone()
}

/// Save settings; re-registers hotkeys. Returns hotkey registration errors (if any).
#[tauri::command]
pub async fn set_settings(app: AppHandle, value: settings::Settings) -> CmdResult<Vec<String>> {
    let _g = SETTINGS_WRITE.lock_safe();
    apply_settings(&app, value).map(|saved| saved.errors)
}

/// Serialises read-modify-write cycles of the settings (patches from several windows).
static SETTINGS_WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Serialize)]
pub struct SettingsSaved {
    settings: settings::Settings,
    errors: Vec<String>,
}

/// Change only the given fields (what the calling window edited) and save; returns the full new settings.
#[tauri::command]
pub async fn patch_settings(app: AppHandle, patch: serde_json::Map<String, serde_json::Value>) -> CmdResult<SettingsSaved> {
    let _g = SETTINGS_WRITE.lock_safe();
    let current = state(&app).settings.lock_safe().clone();
    let value = settings::patched(&current, patch).map_err(err)?;
    apply_settings(&app, value)
}

fn apply_settings(app: &AppHandle, value: settings::Settings) -> CmdResult<SettingsSaved> {
    let mut value = value;
    value.models_dir = settings::clean_models_dir(value.models_dir.as_deref(), cfg!(windows), false);
    // Publish only after the replacement file has been committed.
    settings::save(app, &value).map_err(err)?;
    let (models_changed, device_changed, hotkeys_changed) = {
        let st = state(app);
        let mut s = st.settings.lock_safe();
        let changed = (
            s.models_dir != value.models_dir,
            s.llm_device != value.llm_device || s.llm_gpu_failed != value.llm_gpu_failed,
            s.hotkey_capture != value.hotkey_capture || s.hotkey_pin != value.hotkey_pin
                || s.hotkey_ocr != value.hotkey_ocr || s.hotkey_translate != value.hotkey_translate,
        );
        *s = value.clone();
        changed
    };
    if device_changed {
        ocrtrans_core::set_llm_device(value.llm_device == "cpu", value.llm_gpu_failed.clone());
    }
    if models_changed {
        state(app).invalidate_pipeline();
    } else if device_changed {
        state(app).unload_llm();
    }
    let errors = if hotkeys_changed {
        let errors = hotkeys::register_all(app);
        crate::refresh_tray(app);
        errors
    } else {
        Vec::new()
    };
    Ok(SettingsSaved { settings: value, errors })
}

#[tauri::command]
pub async fn model_status(app: AppHandle) -> Vec<models::ModelStatus> {
    models::status(&app)
}

#[tauri::command]
pub async fn model_verify(app: AppHandle, id: String) -> CmdResult<String> {
    tauri::async_runtime::spawn_blocking(move || models::verify_installed(&app, &id)).await.map_err(err)?.map_err(err)
}

#[tauri::command]
pub async fn model_download(app: AppHandle, id: String) -> CmdResult<()> {
    models::download(app, id).await.map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn model_cancel(app: AppHandle, id: String) {
    models::cancel(&app, &id);
}

#[tauri::command]
pub async fn model_delete(app: AppHandle, id: String) -> CmdResult<()> {
    models::delete(&app, &id).map_err(err)
}

#[tauri::command]
pub async fn models_folder(app: AppHandle) -> String {
    let d = models::user_models_dir(&app);
    let _ = std::fs::create_dir_all(&d);
    d.display().to_string()
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Option<String> {
    app.dialog().file().blocking_pick_folder().and_then(|p| p.into_path().ok()).map(|p| crate::paths::plain(&p).display().to_string())
}

#[derive(Serialize)]
pub struct Env {
    screen_access: bool,
    platform: &'static str,
    version: String,
    /// forced theme ("light" / "dark") from OCRTRANS_THEME, for screenshots and testing
    theme: Option<String>,
}

#[tauri::command]
pub async fn env_info(app: AppHandle) -> Env {
    Env {
        screen_access: windows::has_screen_access(),
        platform: std::env::consts::OS,
        version: app.package_info().version.to_string(),
        theme: std::env::var("OCRTRANS_THEME").ok().filter(|t| t == "light" || t == "dark"),
    }
}

/// Return a bounded local preview. The user decides whether to save or share it.
#[tauri::command]
pub async fn diagnostics(app: AppHandle) -> CmdResult<String> {
    use std::io::{Read, Seek, SeekFrom};
    let folder = app.path().app_log_dir().map_err(err)?;
    let settings = state(&app).settings.lock_safe().clone();
    let mut report = format!("OCR Trans {}  {} {}\nengine={}  device={}\n",
        app.package_info().version, std::env::consts::OS, std::env::consts::ARCH,
        settings.engine, settings.llm_device);
    for name in ["ocr-trans.previous.log", "ocr-trans.log"] {
        let mut file = match std::fs::File::open(folder.join(name)) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(err(e)),
        };
        let offset = file.metadata().map_err(err)?.len().saturating_sub(64 * 1024);
        file.seek(SeekFrom::Start(offset)).map_err(err)?;
        let mut bytes = Vec::new();
        file.take(64 * 1024).read_to_end(&mut bytes).map_err(err)?;
        report.push_str(&format!("\n{name}\n"));
        report.push_str(&String::from_utf8_lossy(&bytes));
    }
    Ok(report)
}

#[tauri::command]
pub async fn save_diagnostics(app: AppHandle, report: String) -> CmdResult<bool> {
    use tauri_plugin_dialog::DialogExt;
    if report.len() > 512 * 1024 { return Err("诊断文件过大".into()); }
    tauri::async_runtime::spawn_blocking(move || {
        let dialog = app.dialog().file().add_filter("Text", &["txt"]).set_file_name("OCRTrans-diagnostics.txt");
        let path = windows::with_pins_lowered(&app, || dialog.blocking_save_file());
        let Some(path) = path else { return Ok(false); };
        std::fs::write(path.into_path().map_err(err)?, report).map_err(err)?;
        Ok(true)
    }).await.map_err(err)?
}

/// Test-profile handshake after React has drawn the translated canvas.
#[tauri::command]
pub fn release_probe(window: tauri::WebviewWindow, engine: String, warnings: usize) {
    if std::env::var_os("OCRTRANS_TEST_PROFILE").is_some() && window.label().starts_with("pin-") {
        crate::log_info(&format!("release-check displayed engine={engine} warnings={warnings}"));
        window.app_handle().exit(0);
    }
}

/// The bundled licence texts (licenses/THIRD-PARTY-NOTICES.txt), shown in full under 设置 → 关于.
#[tauri::command]
pub async fn third_party_notices(app: AppHandle) -> CmdResult<String> {
    let p = notices_path(&app)?;
    std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))
}

fn notices_path(app: &AppHandle) -> CmdResult<std::path::PathBuf> {
    Ok(crate::paths::resource_dir(app).ok_or("no resource folder")?.join("licenses").join("THIRD-PARTY-NOTICES.txt"))
}

/// Open THIRD-PARTY-NOTICES.txt in the system's text viewer (the ordinary path: some viewers reject `\\?\` ones).
#[tauri::command]
pub async fn open_third_party_notices(app: AppHandle) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_path(notices_path(&app)?.display().to_string(), None::<&str>).map_err(err)
}

#[tauri::command]
pub async fn history_list(app: AppHandle) -> CmdResult<Vec<crate::history::Item>> {
    crate::history::list(&app).map_err(err)
}

#[tauri::command]
pub async fn history_recover(app: AppHandle) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || crate::history::recover(&app)).await.map_err(err)?.map_err(err)
}

/// Re-open a history item as a pin at the place it was captured.
#[tauri::command]
pub async fn history_pin(app: AppHandle, id: u64, translate: bool) -> CmdResult<()> {
    let (img, item) = crate::history::load_image(&app, id).map_err(err)?;
    let pid = windows::add_pin(&app, img, item.scale, item.rect).map_err(err)?;
    if let Some(p) = state(&app).pins.lock_safe().get_mut(&pid) {
        p.history_id = Some(id);
    }
    if let Err(e) = windows::open_pin(&app, pid, translate) {
        state(&app).pins.lock_safe().remove(&pid);
        return Err(err(e));
    }
    Ok(())
}

#[tauri::command]
pub async fn history_delete(app: AppHandle, id: u64) -> CmdResult<()> {
    crate::history::delete(&app, id).map_err(err)
}

#[tauri::command]
pub async fn history_clear(app: AppHandle) -> CmdResult<()> {
    crate::history::clear(&app).map_err(err)
}

#[tauri::command]
pub async fn open_history(app: AppHandle) {
    windows::open_history(&app);
}

/// Close the calling window safely: hide it now, destroy it a moment later from the backend.
/// Closing a WKWebView window from its own page (window.close) while WebKit still has a scroll/display update
/// pending for it crashed the app inside WebCore::ScrollingTree::takePendingScrollUpdates (0.5.1, after using the
/// OCR popup). Hiding first lets the display link drain before the view is torn down.
#[tauri::command]
pub fn close_self(window: tauri::WebviewWindow) {
    if std::env::var_os("OCRTRANS_DEBUG").is_some() {
        eprintln!("close_self {}", window.label());
    }
    windows::close_safely(&window);
}

#[tauri::command]
pub async fn open_settings(app: AppHandle, tab: String) {
    windows::open_settings(&app, Some(&tab));
}

/// Screen-recording permission only takes effect after a restart on macOS.
#[tauri::command]
pub fn restart_app(app: AppHandle) {
    // free the models like a normal quit does: restart() exits the process without RunEvent::Exit, and llama.cpp's
    // Metal backend aborts in its static teardown while a model is still loaded
    ocrtrans_core::abort_work();
    state(&app).invalidate_pipeline();
    app.restart();
}

#[tauri::command]
pub fn request_screen_access() {
    windows::request_screen_access();
}

/// Serve screenshots to the webviews: `shot://localhost/overlay/<token>` (raw RGBA) and `shot://localhost/pin/<id>` (PNG).
pub fn serve_shot(app: &AppHandle, path: &str) -> Option<(Vec<u8>, &'static str)> {
    let st = state(app);
    let mut parts = path.trim_start_matches('/').split('/');
    let kind = parts.next()?;
    if kind == "history" {
        let id = parts.next()?.parse::<u64>().ok()?;
        return crate::history::thumb_bytes(app, id).map(|b| (b, "image/png"));
    }
    match (kind, parts.next()?.parse::<u32>().ok()?) {
        ("overlay", token) => {
            let img = st.shot.lock_safe().as_ref().filter(|s| s.token == token)?.frame.image.clone();
            // raw RGBA: no encoding cost, the page puts it straight into an ImageData; copied outside the lock
            Some((img.as_raw().to_vec(), "application/octet-stream"))
        }
        ("pin", id) => {
            let clean = parts.next() == Some("clean");
            let (img, cached) = {
                let pins = st.pins.lock_safe();
                let p = pins.get(&id)?;
                if clean { (p.clean.clone()?, p.clean_png.clone()) }
                else { (p.image.clone(), p.original_png.clone()) }
            };
            if let Some(png) = cached { return Some((png.as_ref().clone(), "image/png")); }
            let mut out = Cursor::new(Vec::new());
            img.write_to(&mut out, image::ImageFormat::Png).ok()?;
            let bytes = out.into_inner();
            if bytes.len() <= 8 * 1024 * 1024 {
                let mut pins = st.pins.lock_safe();
                let cached_size: usize = pins.values().flat_map(|p| [&p.original_png, &p.clean_png])
                    .filter_map(|entry| entry.as_ref()).map(|bytes| bytes.len()).sum();
                if cached_size + bytes.len() <= 16 * 1024 * 1024 {
                    if let Some(p) = pins.get_mut(&id) {
                        if clean && p.clean.as_ref().is_some_and(|current| std::sync::Arc::ptr_eq(current, &img)) {
                            p.clean_png = Some(std::sync::Arc::new(bytes.clone()));
                        } else if !clean {
                            p.original_png = Some(std::sync::Arc::new(bytes.clone()));
                        }
                    }
                }
            }
            Some((bytes, "image/png"))
        }
        _ => None,
    }
}
