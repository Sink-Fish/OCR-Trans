use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{anyhow, Result};
use image::RgbaImage;
use ocrtrans_core::{Pipeline, PipelineResult, Translators};
use tauri::{AppHandle, Manager};

use crate::{capture_worker::Frame, models};
use crate::settings::Settings;

/// Lock that survives a panic in another holder (a poisoned mutex must never take the whole app down).
pub trait LockExt<T> {
    fn lock_safe(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_safe(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// select a region, then choose the action on the toolbar
    Select,
    Pin,
    Ocr,
    Translate,
}

/// The frozen full-monitor screenshot shown by the selection overlay.
pub struct Shot {
    pub frame: Frame,
    /// Reuse the frozen frame when going back to the previous display. Keep one extra image at most.
    pub previous_frame: Option<Frame>,
    pub mode: Mode,
    pub token: u32,
    /// when the hotkey / tray action started this capture (for the perf log)
    pub started: std::time::Instant,
    /// ms spent taking the screenshot
    pub capture_ms: u128,
    /// the page has painted the screenshot
    pub ready: bool,
    /// A drag or completed selection keeps this frame on its original display.
    pub interacting: bool,
    /// The initial frame opens a hidden window. A cross-screen frame moves the existing key window.
    pub needs_activation: bool,
    /// Input at presentation or the last pointer press accepted by this selection.
    #[cfg(target_os = "macos")]
    pub input_presses: u64,
}

pub struct Pin {
    pub image: Arc<RgbaImage>,
    /// physical pixels per logical point
    pub scale: f32,
    /// logical screen rect where the region was taken
    pub rect: [f64; 4],
    pub ocr: Option<PipelineResult>,
    /// Engine of the cached translation ("fast" / "quality"), if translated.
    pub translated: Option<String>,
    /// The glossary is part of the translation cache key, including an empty glossary.
    pub translated_glossary: Vec<(String, String)>,
    /// Manual edits made while another window is translating must win over that older work.
    pub edit_revision: u64,
    pub translation_generation: u64,
    /// Image with the original text under translated paragraphs erased (inpainted).
    pub clean: Option<Arc<RgbaImage>>,
    pub original_png: Option<Arc<Vec<u8>>>,
    pub clean_png: Option<Arc<Vec<u8>>>,
    /// Entry in the history list this pin came from / was recorded as.
    pub history_id: Option<u64>,
    pub saved_translation: Option<crate::history::Item>,
}

pub struct AppState {
    pipeline: Mutex<Option<Pipeline>>,
    fast: Mutex<Option<Translators>>,
    pub model_access: std::sync::RwLock<()>,
    pub model_generation: AtomicU32,
    pub shot: Mutex<Option<Shot>>,
    pub pins: Mutex<HashMap<u32, Pin>>,
    pub next_id: AtomicU32,
    /// A screen capture is being taken (before the overlay shows).
    pub capturing: AtomicBool,
    /// Keep automatic monitor changes single-flight without blocking the hotkey.
    pub switching_monitor: AtomicBool,
    pub settings: Mutex<Settings>,
    pub downloads: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl AppState {
    pub fn new(settings: Settings) -> Self {
        Self {
            pipeline: Mutex::new(None),
            fast: Mutex::new(None),
            model_access: Default::default(),
            model_generation: AtomicU32::new(0),
            shot: Mutex::new(None),
            pins: Mutex::new(HashMap::new()),
            next_id: AtomicU32::new(1),
            capturing: AtomicBool::new(false),
            switching_monitor: AtomicBool::new(false),
            settings: Mutex::new(settings),
            downloads: Mutex::new(HashMap::new()),
        }
    }

    /// Called every minute: release the quality model (memory, and the GPU's memory) after 设置's idle minutes
    /// (never blocks a running OCR/translation; 0 = keep it).
    pub fn unload_idle(&self) {
        let minutes = self.settings.lock_safe().llm_idle_minutes;
        #[cfg(feature = "llm")]
        crate::quality_worker::idle(minutes);
        let _ = minutes;
    }

    pub fn unload_llm(&self) {
        self.model_generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        #[cfg(feature = "llm")]
        crate::quality_worker::release();
    }

    pub fn invalidate_pipeline(&self) {
        let _ = self.change_model_files(|| Ok(()));
    }

    pub fn change_model_files<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<T> {
        self.model_generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _exclusive = self.model_access.write().unwrap_or_else(|e| e.into_inner());
        #[cfg(feature = "llm")]
        crate::quality_worker::release();
        let mut pipeline = self.pipeline.lock_safe();
        let mut fast = self.fast.lock_safe();
        *pipeline = None;
        *fast = None;
        f()
    }

    pub fn with_fast<T>(&self, app: &AppHandle, f: impl FnOnce(&mut Translators) -> Result<T>) -> Result<T> {
        let mut fast = self.fast.lock_safe();
        f(fast.get_or_insert_with(|| Translators::with_dirs(models::locate(app, "opus-mt-en-zh"), models::locate(app, "m2m100_418m"))))
    }

    /// Run `f` with the (lazily loaded) pipeline.
    pub fn with_pipeline<T>(&self, app: &AppHandle, f: impl FnOnce(&mut Pipeline) -> Result<T>) -> Result<T> {
        let mut guard = self.pipeline.lock_safe();
        if guard.is_none() {
            let t = std::time::Instant::now();
            let ocr = models::locate(app, "ocr").ok_or_else(|| anyhow!("未安装文字识别模型，请在“设置 → 模型”中下载"))?;
            *guard = Some(Pipeline::with_parts(&ocr, Translators::with_dirs(None, None))?);
            crate::perf_log(&format!("models loaded in {} ms", t.elapsed().as_millis()));
        }
        f(guard.as_mut().unwrap())
    }
}

pub fn state(app: &AppHandle) -> tauri::State<'_, AppState> {
    app.state::<AppState>()
}
