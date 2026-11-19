//! Screenshot / translation history: the last `MAX` captures with thumbnails, kept in `<app data>/history/`.
//!
//! Layout: `index.json` (newest first) plus `<id>.png` (full capture) and `<id>.thumb.png` per item.

use std::collections::BTreeMap;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translations: Option<Vec<Option<String>>>,
}

#[derive(Default)]
struct PendingText {
    translations: Option<Vec<Option<String>>>,
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
    crate::atomic_file::bytes(&d.join(name), data)
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
        let image = match image::open(&path) {
            Ok(image) => image,
            Err(image::ImageError::IoError(e)) if e.kind() != std::io::ErrorKind::UnexpectedEof => return Err(e.into()),
            Err(e) => {
                crate::log_error(&format!("历史恢复跳过损坏的截图 {id}：{e}"));
                continue;
            }
        };
        let (width, height) = (image.width(), image.height());
        let thumb = d.join(format!("{id}.thumb.png"));
        if !thumb.try_exists()? {
            image.thumbnail(THUMB, THUMB).save(&thumb)?;
            private(&thumb, false);
        }
        recovered.push(known.remove(&id).unwrap_or_else(|| Item {
            id, created: id / 1000, width, height, scale: 1.0, rect: [0.0, 0.0, width as f64, height as f64],
            mode: "pin".into(), text: String::new(), translation: String::new(), translations: None,
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

fn unlink(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("无法删除 {}，请重试", path.display())),
    }
}

fn remove_files(d: &Path, id: u64) -> Result<()> {
    unlink(&d.join(format!("{id}.png")))?;
    unlink(&d.join(format!("{id}.thumb.png")))
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
        translation: String::new(), translations: None,
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
    let encode = |image: &RgbaImage| -> Result<Vec<u8>> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png)?;
        Ok(bytes.into_inner())
    };
    let full = encode(image)?;
    let (w, h) = image.dimensions();
    let k = (THUMB as f32 / w.max(h) as f32).min(1.0);
    let thumb = image::imageops::thumbnail(image, ((w as f32 * k) as u32).max(1), ((h as f32 * k) as u32).max(1));
    let thumb = encode(&thumb)?;
    let (days, mb) = {
        let st = state(app);
        let settings = st.settings.lock_safe();
        (settings.history_days, settings.history_mb)
    };
    let mut pending = PENDING.lock_safe();
    commit_capture(&d, &mut pending, item, &full, &thumb, days, mb)?;
    drop(pending);
    let _ = app.emit("history-changed", ());
    Ok(())
}

fn commit_capture(d: &Path, pending: &mut BTreeMap<u64, PendingText>, mut item: Item, full: &[u8], thumb: &[u8], days: u32, mb: u32) -> Result<()> {
    if !pending.contains_key(&item.id) { return Ok(()); }
    let mut items = read_index(d, pending)?;
    std::fs::create_dir_all(d)?;
    private(d, true);
    let id = item.id;
    anyhow::ensure!(!d.join(format!("{id}.png")).try_exists()? && !items.iter().any(|i| i.id == id), "历史编号重复，已保留原记录");
    replace_file(d, &format!("{id}.png"), full)?;
    if let Err(e) = replace_file(d, &format!("{id}.thumb.png"), thumb) {
        remove_files(d, id)?;
        return Err(e);
    }
    let text = pending.remove(&item.id).context("待保存记录已取消")?;
    item.text = text.text;
    item.translation = text.translation;
    item.translations = text.translations;
    items.push(item);
    items.sort_unstable_by_key(|i| std::cmp::Reverse(i.id));
    let mut publish = || -> Result<()> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
        let keep = retention_count(d, &items, days, mb, now)?;
        let expired = items.split_off(keep);
        for old in expired {
            if let Err(e) = remove_files(d, old.id) {
                crate::log_error(&format!("历史清理失败：{e:#}"));
                items.push(old); // retain a visible record and retry on the next save
            }
        }
        write_index(d, &items)
    };
    if let Err(e) = publish() {
        remove_files(d, id)?;
        return Err(e);
    }
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
pub fn update_text(app: &AppHandle, id: u64, result: &ocrtrans_core::PipelineResult, translated: bool) -> Result<()> {
    let text = result.plain_text();
    let values = || {
        let flat = result.paragraphs.iter().map(|p| p.translation.as_deref().unwrap_or(&p.text)).collect::<Vec<_>>().join("\n");
        let rows = result.paragraphs.iter().map(|p| p.translation.clone()).collect();
        (flat, Some(rows))
    };
    let mut pending = PENDING.lock_safe();
    if let Some(capture) = pending.get_mut(&id) {
        capture.text = text;
        if translated { (capture.translation, capture.translations) = values(); }
        return Ok(());
    }
    let d = dir(app);
    let mut items = read_index(&d, &pending)?;
    let Some(it) = items.iter_mut().find(|i| i.id == id) else { return Ok(()) };
    it.text = text;
    if translated { (it.translation, it.translations) = values(); }
    write_index(&d, &items)?;
    drop(pending);
    let _ = app.emit("history-changed", ());
    Ok(())
}

impl Item {
    pub fn restore(&self, r: &mut ocrtrans_core::PipelineResult) -> Result<()> {
        let incompatible = || anyhow!("此记录的段落与当前识字结果不同，原译文已保留，可在历史中复制或手动选择引擎重译");
        if self.text != r.plain_text() { return Err(incompatible()); }
        let rows = match &self.translations {
            Some(rows) => rows.clone(),
            None if r.paragraphs.len() == 1 => vec![Some(self.translation.clone())],
            None => self.translation.split('\n').map(|v| Some(v.to_owned())).collect(),
        };
        if rows.len() != r.paragraphs.len() { return Err(incompatible()); }
        for (p, row) in r.paragraphs.iter_mut().zip(rows) { p.translation = row; }
        r.engine = "history".into();
        Ok(())
    }
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
    delete_item(&dir(app), &mut pending, id)?;
    drop(pending);
    let _ = app.emit("history-changed", ());
    Ok(())
}

fn remove_index_debris(d: &Path) -> Result<()> {
    for name in ["index.damaged.json", "index.json.tmp", "index.backup.json.tmp", "index.damaged.json.tmp"] {
        unlink(&d.join(name))?;
    }
    // Same-directory temporary files left behind by a terminated atomic writer.
    if d.try_exists()? {
        for entry in std::fs::read_dir(d)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with(".ocrtrans-write-") { unlink(&entry.path())?; }
        }
    }
    Ok(())
}

fn delete_item(d: &Path, pending: &mut BTreeMap<u64, PendingText>, id: u64) -> Result<()> {
    let mut items = read_index(d, pending)?;
    pending.remove(&id);
    remove_files(d, id)?;
    remove_index_debris(d)?;
    items.retain(|item| item.id != id);
    write_index(d, &items)
}

pub fn clear(app: &AppHandle) -> Result<()> {
    let mut pending = PENDING.lock_safe();
    clear_history(&dir(app), &mut pending)?;
    drop(pending);
    let _ = app.emit("history-changed", ());
    Ok(())
}

fn clear_history(d: &Path, pending: &mut BTreeMap<u64, PendingText>) -> Result<()> {
    pending.clear();
    if !d.try_exists()? { return Ok(()); }
    for entry in std::fs::read_dir(d)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.strip_suffix(".thumb.png").or_else(|| name.strip_suffix(".png")).is_some_and(|id| id.parse::<u64>().is_ok()) {
            unlink(&entry.path())?;
        }
    }
    remove_index_debris(d)?;
    write_index(d, &[])
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
            rect: [1.0, 1.0, 1.5, 1.0], mode: "ocr".into(), text: "preserved".into(), translation: "译文".into(), translations: None };
        write_index(&d, &[item]).unwrap();
        RgbaImage::new(3, 2).save(d.join("1000.png")).unwrap();
        std::fs::write(d.join("3000.png"), []).unwrap();
        let bytes = std::fs::read(d.join("1000.png")).unwrap();
        std::fs::write(d.join("4000.png"), &bytes[..bytes.len()/2]).unwrap();
        std::fs::write(d.join("4000.thumb.png"), &bytes).unwrap();
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
        let mut pending = BTreeMap::from([(5000, PendingText::default())]);
        commit_capture(&d, &mut pending, test_item(5000), &bytes, &bytes, 0, 0).unwrap();
        assert_eq!(read_index(&d, &pending).unwrap().len(), 3);
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn history_limits_keep_newest_items_and_account_for_thumbnails() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("ocrtrans-retention-{nonce}"));
        std::fs::create_dir(&dir).unwrap();
        let item = |id, created| Item {
            id, created, width: 1, height: 1, scale: 1.0, rect: [0.0; 4],
            mode: "pin".into(), text: String::new(), translation: String::new(), translations: None,
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
    fn clear_cancels_queued_writers_and_removes_orphans() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let mut pending = BTreeMap::from([(200, PendingText::default())]);
        for name in ["100.png", "100.thumb.png", "index.damaged.json", ".ocrtrans-write-orphan", "notes.txt"] {
            std::fs::write(d.join(name), b"private").unwrap();
        }
        clear_history(d, &mut pending).unwrap();
        commit_capture(d, &mut pending, test_item(200), b"late", b"late", 0, 0).unwrap();
        for name in ["100.png","100.thumb.png","200.png","200.thumb.png","index.damaged.json",".ocrtrans-write-orphan"] {
            assert!(!d.join(name).exists());
        }
        assert!(d.join("notes.txt").exists());
    }

    fn test_item(id: u64) -> Item {
        Item { id, created: 1, width: 1, height: 1, scale: 1.0, rect: [0.0; 4],
            mode: "pin".into(), text: String::new(), translation: String::new(), translations: None }
    }

    #[test]
    fn corrected_translation_roundtrips_through_index_and_legacy_history() {
        use ocrtrans_core::{PipelineResult, Paragraph, Lang};
        let paragraph = |text: &str| Paragraph { text: text.into(), lang: Lang::En, translation: None,
            bbox: [0.; 4], lines: vec![], line_height: 20., bg: String::new(), fg: String::new(), fill: None };
        let mut result = PipelineResult { width: 10, height: 10, paragraphs: vec![paragraph("Hello"),paragraph("world")],
            engine: String::new(), warnings: vec![], timings: Default::default() };
        let d = tempfile::tempdir().unwrap();
        let mut item = test_item(1);
        item.text = result.plain_text();
        item.translation = "人工校对\n保留第二行\nworld".into();
        item.translations = Some(vec![Some("人工校对\n保留第二行".into()), None]);
        write_index(d.path(), &[item]).unwrap();
        let mut item = read_index(d.path(), &BTreeMap::new()).unwrap().pop().unwrap();
        item.restore(&mut result).unwrap();
        assert_eq!(result.paragraphs[0].translation.as_deref(), Some("人工校对\n保留第二行"));
        assert!(result.paragraphs[1].translation.is_none());
        item.translations = None;
        assert!(item.restore(&mut result).is_err());
        item.translation = "人工校对\n第二段".into();
        item.restore(&mut result).unwrap();
        assert_eq!(result.paragraphs[1].translation.as_deref(), Some("第二段"));
        result.paragraphs.pop();
        assert!(item.restore(&mut result).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn finder_locked_screenshot_remains_listed_until_unlocked() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        RgbaImage::new(3, 2).save(d.join("1.png")).unwrap();
        write_index(d, &[test_item(1)]).unwrap();
        let flags = |mode| std::process::Command::new("chflags").arg(mode).arg(d.join("1.png")).status().unwrap().success();
        let mut pending = BTreeMap::new();
        assert!(flags("uchg"));
        let failed = delete_item(d, &mut pending, 1).is_err();
        assert!(flags("nouchg"));
        assert!(failed);
        assert_eq!(read_index(d, &pending).unwrap().len(), 1);
        delete_item(d, &mut pending, 1).unwrap();
        assert!(!d.join("1.png").exists());
    }

    #[test]
    fn failed_delete_and_clear_are_visible_and_retryable() {
        for clear in [false,true] {
            let dir = tempfile::tempdir().unwrap();
            let d = dir.path();
            let mut pending = BTreeMap::new();
            write_index(d, &[test_item(1)]).unwrap();
            std::fs::write(d.join("index.damaged.json"), b"old OCR text").unwrap();
            std::fs::create_dir(d.join("1.thumb.png")).unwrap();
            let delete = |p: &mut _| if clear { clear_history(d, p) } else { delete_item(d, p, 1) };
            assert!(delete(&mut pending).is_err());
            assert_eq!(read_index(d, &pending).unwrap().len(), 1);
            std::fs::remove_dir(d.join("1.thumb.png")).unwrap();
            delete(&mut pending).unwrap();
            assert!(read_index(d, &pending).unwrap().is_empty());
            assert_eq!(std::fs::read(d.join("index.backup.json")).unwrap(), b"[]");
            assert!(!d.join("index.damaged.json").exists());
        }
    }
}
