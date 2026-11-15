//! Screenshot / translation history: the last `MAX` captures with thumbnails, kept in `<app data>/history/`.
//!
//! Layout: `index.json` (newest first) plus `<id>.png` (full capture) and `<id>.thumb.png` per item.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, mpsc};

use anyhow::{anyhow, Context, Result};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::{state, LockExt};

pub const MAX: usize = 50;
const THUMB: u32 = 320;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: u64,
    /// unix seconds
    pub created: u64,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    /// global logical rect the capture came from
    pub rect: [f64; 4],
    pub mode: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub translation: String,
}

#[derive(Default)]
struct PendingText {
    text: String,
    translation: String,
}

/// Tracks only captures being saved and serialises index read-modify-write cycles.
static PENDING: Mutex<BTreeMap<u64, PendingText>> = Mutex::new(BTreeMap::new());
static LAST_ID: Mutex<u64> = Mutex::new(0);

/// Only the owner can read the history (screenshots may show passwords, chats, mail).
fn private(p: &std::path::Path, dir: bool) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(if dir { 0o700 } else { 0o600 }));
    }
    let _ = (p, dir);
}

pub fn dir(app: &AppHandle) -> PathBuf {
    let base = std::env::var("OCRTRANS_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|_| app.path().app_data_dir())
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("history")
}

fn read_index(d: &Path, pending: &BTreeMap<u64, PendingText>) -> Result<Vec<Item>> {
    match std::fs::read(d.join("index.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes).context("历史索引损坏，可从备份和截图恢复"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if d.join("index.backup.json").try_exists()? {
                anyhow::bail!("历史索引缺失，可以从备份恢复");
            }
            if d.try_exists()? {
                for entry in std::fs::read_dir(d)? {
                    let path = entry?.path();
                    if let Some(id) = image_id(&path) {
                        if !pending.contains_key(&id) {
                            anyhow::bail!("已发现历史截图，请恢复索引");
                        }
                    }
                }
            }
            Ok(Vec::new())
        },
        Err(e) => Err(e).context("历史索引读取失败"),
    }
}

fn replace_file(d: &Path, name: &str, data: &[u8]) -> Result<()> {
    let tmp = d.join(format!("{name}.tmp"));
    std::fs::write(&tmp, data)?;
    private(&tmp, false);
    std::fs::rename(tmp, d.join(name))?;
    Ok(())
}

fn write_index(d: &Path, items: &[Item]) -> Result<()> {
    std::fs::create_dir_all(d)?;
    private(d, true);
    let bytes = serde_json::to_vec(items)?;
    replace_file(d, "index.backup.json", &bytes)?;
    replace_file(d, "index.json", &bytes)
}

fn image_id(path: &Path) -> Option<u64> {
    if path.extension()?.to_str()? != "png" { return None; }
    path.file_stem()?.to_str()?.parse().ok()
}

fn recover_index(d: &Path, pending: &BTreeMap<u64, PendingText>) -> Result<()> {
    if read_index(d, pending).is_ok() {
        return Ok(());
    }
    let backup: Vec<Item> = match std::fs::read(d.join("index.backup.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
        Err(e) => return Err(e.into()),
    };
    let mut known: BTreeMap<_, _> = backup.into_iter().map(|item| (item.id, item)).collect();
    let mut recovered = vec![];
    for entry in std::fs::read_dir(d)? {
        let entry = entry?;
        let path = entry.path();
        let Some(id) = image_id(&path) else { continue };
        if pending.contains_key(&id) { continue; }
        let (width, height) = image::image_dimensions(&path).context("无法读取历史截图尺寸")?;
        let thumb = d.join(format!("{id}.thumb.png"));
        if !thumb.try_exists()? {
            let image = image::open(&path)?;
            image.thumbnail(THUMB, THUMB).save(&thumb)?;
            private(&thumb, false);
        }
        recovered.push(known.remove(&id).unwrap_or_else(|| Item {
            id, created: id / 1000, width, height, scale: 1.0, rect: [0.0, 0.0, width as f64, height as f64],
            mode: "pin".into(), text: String::new(), translation: String::new(),
        }));
    }
    recovered.sort_unstable_by_key(|item| std::cmp::Reverse(item.id));
    recovered.truncate(MAX);
    match std::fs::read(d.join("index.json")) {
        Ok(bytes) => replace_file(d, "index.damaged.json", &bytes)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
        Err(e) => return Err(e.into()),
    }
    write_index(d, &recovered)
}

pub fn recover(app: &AppHandle) -> Result<()> {
    let pending = PENDING.lock_safe();
    recover_index(&dir(app), &pending)?;
    drop(pending);
    let _ = app.emit("history-changed", ());
    Ok(())
}

fn remove_files(app: &AppHandle, id: u64) {
    let d = dir(app);
    let _ = std::fs::remove_file(d.join(format!("{id}.png")));
    let _ = std::fs::remove_file(d.join(format!("{id}.thumb.png")));
}

/// Make an existing history folder private (files written before 0.5.6 were world-readable).
pub fn secure(app: &AppHandle) {
    let d = dir(app);
    if d.exists() {
        private(&d, true);
    }
}

pub fn list(app: &AppHandle) -> Result<Vec<Item>> {
    let pending = PENDING.lock_safe();
    read_index(&dir(app), &pending)
}

/// Record a new capture (runs on a background thread; returns the id immediately). `None` when the user turned
/// the history off (设置 → 保存截图历史).
pub fn add(app: &AppHandle, image: &Arc<RgbaImage>, scale: f32, rect: [f64; 4], mode: &str) -> Option<u64> {
    if !state(app).settings.lock_safe().keep_history {
        return None;
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let id = {
        let mut last = LAST_ID.lock_safe();
        *last = now.max(last.saturating_add(1));
        *last
    };
    let item = Item {
        id,
        created: now / 1000,
        width: image.width(),
        height: image.height(),
        scale,
        rect,
        mode: mode.into(),
        text: String::new(),
        translation: String::new(),
    };
    PENDING.lock_safe().insert(id, PendingText::default());
    type SaveJob = (AppHandle, Arc<RgbaImage>, Item);
    static ENCODER: OnceLock<mpsc::SyncSender<SaveJob>> = OnceLock::new();
    let queue = ENCODER.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<SaveJob>(2);
        std::thread::spawn(move || {
            for (app, image, item) in rx {
                let id = item.id;
                if let Err(e) = save(&app, &image, item) {
                    let mut pending = PENDING.lock_safe();
                    pending.remove(&id);
                    remove_files(&app, id);
                    crate::log_error(&format!("history: {e:#}"));
                }
            }
        });
        tx
    });
    if queue.send((app.clone(), Arc::clone(image), item)).is_err() {
        PENDING.lock_safe().remove(&id);
        crate::log_error("历史保存线程已退出");
        return None;
    }
    Some(id)
}

fn save(app: &AppHandle, image: &RgbaImage, item: Item) -> Result<()> {
    let d = dir(app);
    std::fs::create_dir_all(&d)?;
    private(&d, true);
    let full = d.join(format!("{}.png", item.id));
    image.save(&full)?;
    private(&full, false);
    let (w, h) = image.dimensions();
    let k = (THUMB as f32 / w.max(h) as f32).min(1.0);
    let thumb = image::imageops::thumbnail(image, ((w as f32 * k) as u32).max(1), ((h as f32 * k) as u32).max(1));
    let thumb_path = d.join(format!("{}.thumb.png", item.id));
    thumb.save(&thumb_path)?;
    private(&thumb_path, false);
    let mut item = item;
    let mut pending = PENDING.lock_safe();
    if !pending.contains_key(&item.id) {
        // The user cleared or deleted this capture while its image was being encoded.
        remove_files(app, item.id);
        return Ok(());
    };
    let mut items = read_index(&dir(app), &pending)?;
    let text = pending.remove(&item.id).context("待保存记录已取消")?;
    item.text = text.text;
    item.translation = text.translation;
    items.retain(|i| i.id != item.id);
    items.push(item);
    items.sort_unstable_by_key(|i| std::cmp::Reverse(i.id));
    let (days, mb) = {
        let state = state(app);
        let settings = state.settings.lock_safe();
        (settings.history_days, settings.history_mb)
    };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
    let keep = retention_count(&d, &items, days, mb, now)?;
    let old = items.split_off(keep);
    write_index(&dir(app), &items)?;
    for old in old {
        remove_files(app, old.id);
    }
    drop(pending);
    let _ = app.emit("history-changed", ());
    Ok(())
}

fn retention_count(d: &Path, newest_first: &[Item], days: u32, mb: u32, now: u64) -> Result<usize> {
    let mut total = 0u64;
    for (index, item) in newest_first.iter().take(MAX).enumerate() {
        if days != 0 && now.saturating_sub(item.created) > u64::from(days) * 86400 { return Ok(index); }
        if mb != 0 {
            for suffix in ["png", "thumb.png"] {
                match std::fs::metadata(d.join(format!("{}.{suffix}", item.id))) {
                    Ok(meta) => total = total.saturating_add(meta.len()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
                    Err(e) => return Err(e.into()),
                }
            }
            if total > u64::from(mb) * 1024 * 1024 { return Ok(index); }
        }
    }
    Ok(newest_first.len().min(MAX))
}

/// Store OCR text / translation for an item once they are known.
pub fn update_text(app: &AppHandle, id: u64, text: &str, translation: Option<&str>) {
    let mut pending = PENDING.lock_safe();
    if let Some(capture) = pending.get_mut(&id) {
        capture.text = text.into();
        if let Some(t) = translation {
            capture.translation = t.into();
        }
        return;
    }
    let mut items = match read_index(&dir(app), &pending) {
        Ok(items) => items,
        Err(e) => {
            crate::log_error(&format!("history text: {e:#}"));
            return;
        }
    };
    let Some(it) = items.iter_mut().find(|i| i.id == id) else { return };
    it.text = text.into();
    if let Some(t) = translation {
        it.translation = t.into();
    }
    if let Err(e) = write_index(&dir(app), &items) {
        crate::log_error(&format!("history text: {e:#}"));
        return;
    }
    drop(pending);
    let _ = app.emit("history-changed", ());
}

pub fn load_image(app: &AppHandle, id: u64) -> Result<(RgbaImage, Item)> {
    let item = list(app)?.into_iter().find(|i| i.id == id).ok_or_else(|| anyhow!("记录不存在"))?;
    let img = image::open(dir(app).join(format!("{id}.png")))?.to_rgba8();
    Ok((img, item))
}

pub fn thumb_bytes(app: &AppHandle, id: u64) -> Option<Vec<u8>> {
    std::fs::read(dir(app).join(format!("{id}.thumb.png"))).ok()
}

pub fn delete(app: &AppHandle, id: u64) -> Result<()> {
    let mut pending = PENDING.lock_safe();
    let mut items = read_index(&dir(app), &pending)?;
    items.retain(|i| i.id != id);
    write_index(&dir(app), &items)?;
    if pending.remove(&id).is_none() {
        remove_files(app, id);
    }
    drop(pending);
    let _ = app.emit("history-changed", ());
    Ok(())
}

pub fn clear(app: &AppHandle) -> Result<()> {
    let mut pending = PENDING.lock_safe();
    write_index(&dir(app), &[])?;
    let damaged = dir(app).join("index.damaged.json");
    if damaged.try_exists()? { std::fs::remove_file(damaged)?; }
    let saving = pending.keys().copied().collect();
    pending.clear();
    // Include screenshots left behind by an interrupted save or an unreadable index. Active encoders
    // remove their own files when they discover that their pending entry was cleared.
    let result = clear_images(&dir(app), &saving);
    drop(pending);
    let _ = app.emit("history-changed", ());
    result
}

fn clear_images(dir: &std::path::Path, saving: &BTreeSet<u64>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(id) = name.strip_suffix(".thumb.png").or_else(|| name.strip_suffix(".png")).and_then(|id| id.parse::<u64>().ok()) else { continue };
        if !saving.contains(&id) && entry.file_type()?.is_file() {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damaged_index_blocks_writes_and_recovers_existing_capture() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let d = std::env::temp_dir().join(format!("ocrtrans-recovery-{nonce}"));
        std::fs::create_dir(&d).unwrap();
        assert!(read_index(&d, &BTreeMap::new()).unwrap().is_empty());
        let item = Item { id: 1000, created: 1, width: 3, height: 2, scale: 2.0,
            rect: [1.0, 1.0, 1.5, 1.0], mode: "ocr".into(), text: "preserved".into(), translation: "译文".into() };
        write_index(&d, &[item]).unwrap();
        RgbaImage::new(3, 2).save(d.join("1000.png")).unwrap();
        RgbaImage::new(4, 5).save(d.join("2000.png")).unwrap();
        std::fs::write(d.join("index.json"), b"{broken").unwrap();
        assert!(read_index(&d, &BTreeMap::new()).is_err());
        recover_index(&d, &BTreeMap::new()).unwrap();
        let items = read_index(&d, &BTreeMap::new()).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!((items[0].id, items[0].width, items[0].height), (2000, 4, 5));
        assert_eq!(items[1].text, "preserved");
        assert_eq!(items[1].translation, "译文");
        assert_eq!(std::fs::read(d.join("index.damaged.json")).unwrap(), b"{broken");
        std::fs::remove_file(d.join("index.json")).unwrap();
        assert!(read_index(&d, &BTreeMap::new()).is_err());
        recover_index(&d, &BTreeMap::new()).unwrap();
        assert_eq!(read_index(&d, &BTreeMap::new()).unwrap().len(), 2);
        std::fs::remove_file(d.join("index.json")).unwrap();
        std::fs::remove_file(d.join("index.backup.json")).unwrap();
        assert!(read_index(&d, &BTreeMap::new()).is_err());
        recover_index(&d, &BTreeMap::new()).unwrap();
        assert_eq!(read_index(&d, &BTreeMap::new()).unwrap().len(), 2);
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn history_limits_keep_newest_items_and_account_for_thumbnails() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("ocrtrans-retention-{nonce}"));
        std::fs::create_dir(&dir).unwrap();
        let item = |id, created| Item {
            id, created, width: 1, height: 1, scale: 1.0, rect: [0.0; 4],
            mode: "pin".into(), text: String::new(), translation: String::new(),
        };
        let now = 30 * 86400;
        let items = [item(3, now), item(2, now - 86400), item(1, now - 8 * 86400)];
        assert_eq!(retention_count(&dir, &items, 0, 0, now).unwrap(), 3);
        assert_eq!(retention_count(&dir, &items, 7, 0, now).unwrap(), 2);
        for capture in &items {
            std::fs::File::create(dir.join(format!("{}.png", capture.id))).unwrap().set_len(500_000).unwrap();
            std::fs::File::create(dir.join(format!("{}.thumb.png", capture.id))).unwrap().set_len(32_000).unwrap();
        }
        assert_eq!(retention_count(&dir, &items, 0, 1, now).unwrap(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn clearing_includes_orphans_but_leaves_active_encoders_to_clean_up() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("ocrtrans-history-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        for name in ["100.png", "100.thumb.png", "200.png", "200.thumb.png", "index.json", "notes.txt"] {
            std::fs::write(dir.join(name), b"test").unwrap();
        }
        clear_images(&dir, &BTreeSet::from([200])).unwrap();
        assert!(!dir.join("100.png").exists());
        assert!(!dir.join("100.thumb.png").exists());
        assert!(dir.join("200.png").exists());
        assert!(dir.join("200.thumb.png").exists());
        assert!(dir.join("index.json").exists());
        assert!(dir.join("notes.txt").exists());
        clear_images(&dir, &BTreeSet::new()).unwrap();
        assert!(!dir.join("200.png").exists());
        assert!(!dir.join("200.thumb.png").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
