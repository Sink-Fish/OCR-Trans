use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Settings schema version (0 = files written before 0.4.0; a missing field must read as 0).
    #[serde(default)]
    pub version: u32,
    /// Main global shortcut: select a region, then pick an action from the toolbar.
    pub hotkey_capture: String,
    /// Optional direct shortcuts (unassigned by default since 0.4.0).
    pub hotkey_pin: String,
    pub hotkey_ocr: String,
    pub hotkey_translate: String,
    /// Releasing the mouse immediately translates in place and pins (skips the action toolbar).
    pub auto_translate: bool,
    /// Action run by Enter on the toolbar: "translate", "pin" or "ocr".
    pub default_action: String,
    /// Extra folder to look for models in (optional).
    pub models_dir: Option<String>,
    /// Download source: "auto" (huggingface.co then hf-mirror.com), "hf" or "hf-mirror".
    pub mirror: String,
    /// Font used to draw translations.
    pub font_family: String,
    /// Copy all recognised text as soon as 识字 finishes (the pre-0.5.3 behaviour). Off: copy only when asked.
    pub ocr_auto_copy: bool,
    /// Default translation engine: "fast" (opus-mt / M2M100) or "quality" (Qwen3 LLM).
    pub engine: String,
    /// User glossary: source term -> Chinese, applied by both engines.
    pub glossary: Vec<(String, String)>,
    /// Set once the first-run onboarding window has been completed.
    pub onboarded: bool,
    /// Keep the last 50 captures (image, text, translation) in the history folder.
    pub keep_history: bool,
    pub history_days: u32,
    pub history_mb: u32,
    /// Hardware for the quality model: "auto" (a usable GPU, else the CPU) or "cpu".
    pub llm_device: String,
    /// Why the GPU is not used for the quality model (it failed once); None = may be tried. Cleared by 重新尝试显卡.
    pub llm_gpu_failed: Option<String>,
    /// Release the quality model after this many idle minutes (0 = keep it loaded).
    pub llm_idle_minutes: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            hotkey_capture: "Alt+Shift+O".into(),
            hotkey_pin: String::new(),
            hotkey_ocr: String::new(),
            hotkey_translate: String::new(),
            auto_translate: false,
            default_action: "translate".into(),
            models_dir: None,
            mirror: "auto".into(),
            font_family: "\"PingFang SC\", \"Microsoft YaHei\", \"Noto Sans CJK SC\", sans-serif".into(),
            ocr_auto_copy: false,
            engine: "fast".into(),
            glossary: vec![],
            onboarded: false,
            keep_history: true,
            history_days: 0,
            history_mb: 0,
            llm_device: "auto".into(),
            llm_gpu_failed: None,
            llm_idle_minutes: 5,
        }
    }
}

pub const CURRENT_VERSION: u32 = 2;

/// Bring settings written by older versions up to date.
/// 0.4.0: one capture hotkey (+ action toolbar) replaces the three mode hotkeys. The old ⌥⇧O becomes the capture
/// hotkey; old hotkeys still at their factory defaults are released (they become optional, unassigned), while
/// hotkeys the user customised are kept as direct shortcuts.
pub fn migrate(mut s: Settings) -> Settings {
    if s.version < 2 {
        let old_defaults = ["Alt+Shift+S", "Alt+Shift+O", "Alt+Shift+T"];
        let capture = if s.hotkey_ocr.trim().is_empty() || s.hotkey_ocr == "Alt+Shift+O" { "Alt+Shift+O".to_string() } else { s.hotkey_ocr.clone() };
        for k in [&mut s.hotkey_pin, &mut s.hotkey_ocr, &mut s.hotkey_translate] {
            if old_defaults.contains(&k.as_str()) || *k == capture {
                k.clear();
            }
        }
        s.hotkey_capture = capture;
        if s.default_action.is_empty() {
            s.default_action = "translate".into();
        }
        s.version = 2;
    }
    s
}

/// A custom models folder as it should be stored: trimmed, in ordinary (not `\\?\`) form, None when empty. With
/// `strict` (a saved file being loaded) also None when the path is not absolute on this platform — say a macOS path
/// in a Windows settings file — so the default folders apply again. `windows` picks the platform's rules.
pub fn clean_models_dir(dir: Option<&str>, windows: bool, strict: bool) -> Option<String> {
    let d = dir?.trim();
    let d = crate::paths::strip_verbatim(d).unwrap_or_else(|| d.to_string());
    if d.is_empty() || (strict && !crate::paths::is_absolute_for(&d, windows)) {
        return None;
    }
    Some(d)
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    std::env::var_os("OCRTRANS_TEST_PROFILE").map(PathBuf::from)
        .or_else(|| app.path().app_config_dir().ok()).map(|d| d.join("settings.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    load_path(path(app).as_deref())
}

fn load_path(path: Option<&std::path::Path>) -> Settings {
    let safe_defaults = || Settings { keep_history: false, ..Settings::default() };
    let Some(p) = path else { return safe_defaults() };
    let bytes = match std::fs::read(p) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Settings::default(),
        Err(e) => {
            crate::log_error(&format!("设置读取失败，已关闭历史保存：{e}"));
            return safe_defaults();
        }
    };
    match serde_json::from_slice::<Settings>(&bytes) {
        Ok(s) => {
            let mut s = migrate(s);
            let dir = clean_models_dir(s.models_dir.as_deref(), cfg!(windows), true);
            if dir != s.models_dir {
                crate::log_info(&format!("settings: custom models folder {:?} → {:?}", s.models_dir, dir));
                s.models_dir = dir;
            }
            s
        }
        Err(e) => {
            // Keep the original in place so another launch also fails closed.
            let backup = p.with_extension("broken.json");
            if !backup.exists() { let _ = crate::atomic_file::bytes(&backup, &bytes); }
            crate::log_error(&format!("设置文件损坏，已关闭历史保存：{e}"));
            safe_defaults()
        }
    }
}

/// Write via a temporary file and a rename, so a crash mid-write never leaves a truncated settings.json.
pub fn save(app: &AppHandle, s: &Settings) -> anyhow::Result<()> {
    let p = path(app).ok_or_else(|| anyhow::anyhow!("no config dir"))?;
    std::fs::create_dir_all(p.parent().unwrap())?;
    crate::atomic_file::bytes(&p, &serde_json::to_vec_pretty(s)?)
}

/// `current` with the fields in `patch` replaced (unknown fields are ignored). Windows send only what they changed,
/// so one window's stale copy cannot undo another's changes (onboarding reset 设置 edits in 0.5.4).
pub fn patched(current: &Settings, patch: serde_json::Map<String, serde_json::Value>) -> anyhow::Result<Settings> {
    let mut v = serde_json::to_value(current)?;
    let obj = v.as_object_mut().ok_or_else(|| anyhow::anyhow!("settings are not an object"))?;
    for (k, val) in patch {
        if obj.contains_key(&k) {
            obj.insert(k, val);
        }
    }
    Ok(serde_json::from_value(v)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreadable_settings_keep_history_off_across_restarts() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.json");
        assert!(load_path(Some(&p)).keep_history);
        let corrupt = br#"{"keep_history":false,"history_days":"invalid"}"#;
        std::fs::write(&p, corrupt).unwrap();
        for _ in 0..2 { assert!(!load_path(Some(&p)).keep_history); }
        assert_eq!(std::fs::read(&p).unwrap(), corrupt);
        assert!(!load_path(Some(d.path())).keep_history);
        assert!(!load_path(None).keep_history);
    }

    #[test]
    fn quality_model_defaults_for_old_files() {
        let s: Settings = serde_json::from_str(r#"{"version": 2, "engine": "quality"}"#).unwrap();
        assert_eq!(s.llm_device, "auto");
        assert_eq!(s.llm_gpu_failed, None);
        assert_eq!(s.llm_idle_minutes, 5);
    }

    #[test]
    fn custom_models_folder_is_cleaned_up() {
        let win = |d: &str| clean_models_dir(Some(d), true, true);
        // the \\?\ form Tauri reports on Windows (0.6.0): back to the ordinary path, any drive, spaces, Chinese
        assert_eq!(win(r"\\?\D:\OCR Trans\models").as_deref(), Some(r"D:\OCR Trans\models"));
        assert_eq!(win(r"\\?\E:\软件\OCR 翻译\models").as_deref(), Some(r"E:\软件\OCR 翻译\models"));
        assert_eq!(win(r"\\?\UNC\nas\share\models").as_deref(), Some(r"\\nas\share\models"));
        assert_eq!(win(r"  D:\Program Files\OCR Trans\models  ").as_deref(), Some(r"D:\Program Files\OCR Trans\models"));
        // a macOS path (the dev machine's project folder) or a relative one in a Windows file: defaults apply again
        assert_eq!(win("/Users/example/Work/projects/ocr_trans/models"), None);
        assert_eq!(win("models"), None);
        assert_eq!(win(""), None);
        assert_eq!(clean_models_dir(None, true, true), None);
        // macOS keeps its own absolute paths and drops Windows ones
        assert_eq!(clean_models_dir(Some("/Users/example/Work/projects/ocr_trans/models"), false, true).as_deref(), Some("/Users/example/Work/projects/ocr_trans/models"));
        assert_eq!(clean_models_dir(Some(r"D:\models"), false, true), None);
        // saving from the settings page (not strict): only trimmed and un-prefixed
        assert_eq!(clean_models_dir(Some(r" \\?\D:\m "), true, false).as_deref(), Some(r"D:\m"));
        assert_eq!(clean_models_dir(Some("   "), true, false), None);
    }

    fn old(json: &str) -> Settings {
        migrate(serde_json::from_str(json).unwrap())
    }

    #[test]
    fn patch_changes_only_the_named_fields() {
        let cur = Settings { glossary: vec![("Pin".into(), "钉图".into())], hotkey_ocr: "F2".into(), ..Default::default() };
        let mut patch = serde_json::Map::new();
        patch.insert("onboarded".into(), serde_json::json!(true));
        patch.insert("no_such_field".into(), serde_json::json!(1));
        let s = patched(&cur, patch).unwrap();
        assert!(s.onboarded);
        assert_eq!(s.glossary, cur.glossary, "a stale window's other fields are not written back");
        assert_eq!(s.hotkey_ocr, "F2");
        let mut bad = serde_json::Map::new();
        bad.insert("onboarded".into(), serde_json::json!("yes"));
        assert!(patched(&cur, bad).is_err(), "wrong types are refused, not written");
    }

    #[test]
    fn history_is_kept_by_default_and_for_old_files() {
        assert!(Settings::default().keep_history);
        assert!(old(r#"{"onboarded":true}"#).keep_history);
    }

    #[test]
    fn migrates_0_3_defaults() {
        let s = old(r#"{"hotkey_pin":"Alt+Shift+S","hotkey_ocr":"Alt+Shift+O","hotkey_translate":"Alt+Shift+T","engine":"quality","onboarded":true}"#);
        assert_eq!(s.version, CURRENT_VERSION);
        assert_eq!(s.hotkey_capture, "Alt+Shift+O");
        assert!(s.hotkey_pin.is_empty() && s.hotkey_ocr.is_empty() && s.hotkey_translate.is_empty());
        assert_eq!(s.engine, "quality", "other settings survive");
        assert!(s.onboarded);
        assert!(!s.auto_translate);
        assert_eq!(s.default_action, "translate");
    }

    #[test]
    fn keeps_customised_hotkeys() {
        let s = old(r#"{"hotkey_pin":"F1","hotkey_ocr":"Command+Shift+2","hotkey_translate":"Alt+Shift+T"}"#);
        assert_eq!(s.hotkey_capture, "Command+Shift+2", "custom OCR hotkey becomes the capture hotkey");
        assert_eq!(s.hotkey_pin, "F1");
        assert!(s.hotkey_ocr.is_empty(), "not registered twice");
        assert!(s.hotkey_translate.is_empty());
    }

    #[test]
    fn new_settings_untouched() {
        let s = Settings { hotkey_pin: "F2".into(), ..Settings::default() };
        let m = migrate(s.clone());
        assert_eq!(m.hotkey_pin, "F2");
        assert_eq!(m.hotkey_capture, "Alt+Shift+O");
    }

    #[test]
    fn ocr_never_auto_copies_by_default() {
        assert!(!Settings::default().ocr_auto_copy);
        // files from 0.5.2 and earlier carry the old `ocr_popup` field: ignored, auto-copy stays off
        let s = old(r#"{"version":2,"ocr_popup":true,"onboarded":true}"#);
        assert!(!s.ocr_auto_copy);
        assert!(s.onboarded);
    }
}
