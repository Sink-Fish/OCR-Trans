//! Paths shown to the user and handed to native code.
//!
//! On Windows Tauri reports the app's own folder (`resource_dir`, derived from the canonicalized exe path) in the
//! verbatim form `\\?\D:\…`. Windows takes such a path literally: `/` is not a separator there. CTranslate2 opens
//! `<model dir> + "/model.bin"`, so with the bundled models under `\\?\D:\OCR Trans\models` the translation model
//! could not be opened, and 设置 → 模型 showed the `\\?\` form (0.6.0). `plain` turns these paths back into the
//! ordinary form, which every API accepts.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

/// `\\?\D:\x` → `D:\x`, `\\?\UNC\server\share\x` → `\\server\share\x`. None if `s` is not a verbatim drive or UNC
/// path (other verbatim forms such as `\\?\Volume{…}\` have no ordinary equivalent and stay as they are).
pub fn strip_verbatim(s: &str) -> Option<String> {
    let rest = s.strip_prefix(r"\\?\")?;
    if let Some(unc) = rest.strip_prefix(r"UNC\") {
        return Some(format!(r"\\{unc}"));
    }
    let b = rest.as_bytes();
    (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':').then(|| rest.to_string())
}

/// `p` in its ordinary form on Windows (see the module notes); unchanged elsewhere.
pub fn plain(p: &Path) -> PathBuf {
    if cfg!(windows) {
        if let Some(s) = p.to_str().and_then(strip_verbatim) {
            return PathBuf::from(s);
        }
    }
    p.to_path_buf()
}

/// The app's resource folder (bundled models, licences, tray icon) in its ordinary form.
pub fn resource_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().resource_dir().ok().map(|p| plain(&p))
}

/// Whether `s` is an absolute path by the rules of Windows (`windows`) or of macOS / Linux — as text, so the rules
/// of either platform can be checked anywhere.
pub fn is_absolute_for(s: &str, windows: bool) -> bool {
    if windows {
        let b = s.as_bytes();
        let drive = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/');
        drive || s.starts_with(r"\\") || s.starts_with("//")
    } else {
        s.starts_with('/')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbatim_paths_lose_their_prefix() {
        assert_eq!(strip_verbatim(r"\\?\D:\OCR Trans\models").as_deref(), Some(r"D:\OCR Trans\models"));
        assert_eq!(strip_verbatim(r"\\?\D:\软件\OCR 翻译\models\ocr").as_deref(), Some(r"D:\软件\OCR 翻译\models\ocr"));
        assert_eq!(strip_verbatim(r"\\?\c:\Program Files\OCR Trans").as_deref(), Some(r"c:\Program Files\OCR Trans"));
        assert_eq!(strip_verbatim(r"\\?\UNC\nas\共享\models").as_deref(), Some(r"\\nas\共享\models"));
        assert_eq!(strip_verbatim(r"\\?\D:").as_deref(), Some("D:"));
    }

    #[test]
    fn other_paths_are_left_alone() {
        for p in [r"D:\OCR Trans\models", r"\\nas\share", "/Users/example/models", r"\\?\Volume{1234}\models", r"\\?\", ""] {
            assert_eq!(strip_verbatim(p), None, "{p}");
        }
    }

    #[test]
    fn absolute_by_platform() {
        for p in [r"D:\models", "D:/models", r"\\nas\share\models", r"C:\Users\张三\模型"] {
            assert!(is_absolute_for(p, true), "{p}");
            assert!(!is_absolute_for(p, false), "{p} on macOS");
        }
        for p in ["/Users/example/Work/projects/ocr_trans/models", "models", r"D:models", ""] {
            assert!(!is_absolute_for(p, true), "{p} on Windows");
        }
        assert!(is_absolute_for("/Users/example/models", false));
    }

    #[cfg(windows)]
    #[test]
    fn plain_on_windows() {
        assert_eq!(plain(Path::new(r"\\?\D:\OCR Trans\models")), PathBuf::from(r"D:\OCR Trans\models"));
        // what CTranslate2 opens: the model folder + "/model.bin" must be an openable ordinary path
        let dir = plain(Path::new(r"\\?\D:\OCR Trans\models")).join("opus-mt-en-zh");
        assert_eq!(format!("{}/model.bin", dir.display()), r"D:\OCR Trans\models\opus-mt-en-zh/model.bin");
    }
}
