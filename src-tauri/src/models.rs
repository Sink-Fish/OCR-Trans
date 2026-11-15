//! Model catalogue, discovery and download manager.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::{AppState, LockExt};

pub struct FileSpec {
    /// URL; `{hf}` is replaced by the chosen Hugging Face host, a leading `/` means ModelScope (RapidOCR).
    /// Every URL names a fixed revision, so an upstream change cannot swap the files under us.
    pub url: &'static str,
    pub path: &'static str,
    /// Expected size in bytes.
    pub size: u64,
    /// Expected SHA-256 (hex) for catalogue downloads.
    pub sha256: Option<&'static str>,
}

pub struct ModelSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub license: &'static str,
    pub dir: &'static str,
    /// Minimum files needed for discovery, independent of the download's exact versions.
    pub required: &'static [&'static str],
    pub approx_mb: u32,
    pub files: &'static [FileSpec],
}

const MS: &str = "https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.10.0/onnx/PP-OCRv5";

pub const MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "ocr",
        name: "PP-OCRv5 文字识别（中/英/日）",
        description: "PaddleOCR PP-OCRv5 mobile 检测+识别模型（ONNX），截图识字必需",
        license: "Apache-2.0",
        dir: "ocr",
        required: &["det.onnx", "rec.onnx"],
        approx_mb: 21,
        files: &[
            FileSpec {
                url: "/det/ch_PP-OCRv5_det_mobile.onnx",
                path: "det.onnx",
                size: 4_819_576,
                sha256: Some("4d97c44a20d30a81aad087d6a396b08f786c4635742afc391f6621f5c6ae78ae"),
            },
            FileSpec {
                url: "/rec/ch_PP-OCRv5_rec_mobile.onnx",
                path: "rec.onnx",
                size: 16_631_306,
                sha256: Some("5825fc7ebf84ae7a412be049820b4d86d77620f204a041697b0494669b1742c5"),
            },
        ],
    },
    ModelSpec {
        id: "opus-mt-en-zh",
        name: "英→中 翻译（opus-mt-en-zh）",
        description: "Helsinki-NLP opus-mt-en-zh，CTranslate2 格式，英文翻译必需",
        license: "CC-BY-4.0（需署名 Helsinki-NLP / OPUS-MT）",
        dir: "opus-mt-en-zh",
        required: &["model.bin", "source.spm", "target.spm"],
        approx_mb: 160,
        files: &[
            FileSpec { url: "{hf}/gaudi/opus-mt-en-zh-ctranslate2/resolve/dcd22168f08b99dd34c62bc2195e31dc2f04e90b/config.json", path: "config.json", size: 215, sha256: Some("ce02c0c0d02f285d2ff34c80b0867ccb5c4a3b250a275e6d1d2884f5499a6e46") },
            FileSpec {
                url: "{hf}/gaudi/opus-mt-en-zh-ctranslate2/resolve/dcd22168f08b99dd34c62bc2195e31dc2f04e90b/shared_vocabulary.json",
                path: "shared_vocabulary.json",
                size: 1_303_887,
                sha256: Some("37314a6abb25ed8f8497498aeeb31fcea98de892bf00ff7c2e8c966b26fe0b82"),
            },
            FileSpec {
                url: "{hf}/gaudi/opus-mt-en-zh-ctranslate2/resolve/dcd22168f08b99dd34c62bc2195e31dc2f04e90b/source.spm",
                path: "source.spm",
                size: 806_435,
                sha256: Some("5775ddc9e3ff2fae91554da56468ad35ff56edaba870fea74447bc7234bfdaa8"),
            },
            FileSpec {
                url: "{hf}/gaudi/opus-mt-en-zh-ctranslate2/resolve/dcd22168f08b99dd34c62bc2195e31dc2f04e90b/target.spm",
                path: "target.spm",
                size: 804_600,
                sha256: Some("81dc94efa84e4025ef38d25d5d07429fe41e3eb29d44003f1db6fe98487b0052"),
            },
            FileSpec {
                url: "{hf}/gaudi/opus-mt-en-zh-ctranslate2/resolve/dcd22168f08b99dd34c62bc2195e31dc2f04e90b/model.bin",
                path: "model.bin",
                size: 155_502_615,
                sha256: Some("f24c2bb82368f7de0196882de1d8b644d2aa54ae2439c3142f263de8a64ea2a9"),
            },
        ],
    },
    ModelSpec {
        id: "m2m100_418m",
        name: "日→中 翻译（M2M100 418M）",
        description: "facebook/m2m100_418M，CTranslate2 格式，日文翻译需要（较大）",
        license: "MIT",
        dir: "m2m100_418m",
        required: &["model.bin", "sentencepiece.bpe.model"],
        approx_mb: 975,
        files: &[
            FileSpec { url: "{hf}/michaelfeil/ct2fast-m2m100_418M/resolve/06bd83294674f3737eb929e501aa5beafaa229d8/config.json", path: "config.json", size: 159, sha256: Some("0c2f6fa2057c7264d052fb4a62ba3476eeae70487acddfa8e779a53a00cbf44c") },
            FileSpec {
                url: "{hf}/michaelfeil/ct2fast-m2m100_418M/resolve/06bd83294674f3737eb929e501aa5beafaa229d8/shared_vocabulary.txt",
                path: "shared_vocabulary.txt",
                size: 1_159_903,
                sha256: Some("c1e7e97d9ced6091b49036d2c354dd55c6c90fa85175b0536d13305f9f96c919"),
            },
            FileSpec {
                url: "{hf}/michaelfeil/ct2fast-m2m100_418M/resolve/06bd83294674f3737eb929e501aa5beafaa229d8/sentencepiece.bpe.model",
                path: "sentencepiece.bpe.model",
                size: 2_423_393,
                sha256: Some("d8f7c76ed2a5e0822be39f0a4f95a55eb19c78f4593ce609e2edbc2aea4d380a"),
            },
            FileSpec {
                url: "{hf}/michaelfeil/ct2fast-m2m100_418M/resolve/06bd83294674f3737eb929e501aa5beafaa229d8/model.bin",
                path: "model.bin",
                size: 969_807_284,
                sha256: Some("0b70f12bc9af78a921f2f4b2049aeae745c0a70613a0cd58d3d26bd4571177bd"),
            },
        ],
    },
    ModelSpec {
        id: "qwen3-1.7b",
        name: "高质量翻译（Qwen3-1.7B 大模型，可选）",
        description: "Qwen3-1.7B Q4_K_M，结合整屏上下文翻译；耗时取决于处理器、显卡和文字量。",
        license: "Apache-2.0",
        dir: "qwen3-1.7b",
        required: &["model.gguf"],
        approx_mb: 1107,
        files: &[FileSpec {
            url: "{hf}/unsloth/Qwen3-1.7B-GGUF/resolve/d7f544eead698dbd1f15126ef60b45a1e1933222/Qwen3-1.7B-Q4_K_M.gguf",
            path: "model.gguf",
            size: 1_107_409_472,
            sha256: Some("b139949c5bd74937ad8ed8c8cf3d9ffb1e99c866c823204dc42c0d91fa181897"),
        }],
    },
];

pub fn spec(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|m| m.id == id)
}

/// Where downloads go (`OCRTRANS_DATA_DIR` overrides the app data folder, for testing).
pub fn user_models_dir(app: &AppHandle) -> PathBuf {
    if let Ok(d) = std::env::var("OCRTRANS_DATA_DIR") {
        return PathBuf::from(d).join("models");
    }
    app.path().app_data_dir().map(|d| d.join("models")).unwrap_or_else(|_| PathBuf::from("models"))
}

/// Candidate base folders, highest priority first.
pub fn search_dirs(app: &AppHandle) -> Vec<PathBuf> {
    let mut v = vec![];
    if let Some(d) = app.state::<AppState>().settings.lock_safe().models_dir.clone() {
        if !d.is_empty() {
            v.push(crate::paths::plain(Path::new(&d)));
        }
    }
    v.push(user_models_dir(app));
    // the bundled models; the ordinary path, not Tauri's \\?\ form on Windows (see paths.rs)
    if let Some(r) = crate::paths::resource_dir(app) {
        v.push(r.join("models"));
    }
    // dev builds / an app bundle kept inside the project folder: <ancestor>/models
    if let Ok(exe) = std::env::current_exe() {
        for a in exe.ancestors().skip(1).take(8) {
            v.push(a.join("models"));
        }
    }
    v
}

pub fn locate(app: &AppHandle, id: &str) -> Option<PathBuf> {
    locate_in(spec(id)?, search_dirs(app))
}

fn locate_in(spec: &ModelSpec, roots: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    roots.into_iter().map(|root| root.join(spec.dir)).find(|dir| {
        use ocrtrans_core::translate::{model_complete, ModelKind};
        match spec.id {
            "opus-mt-en-zh" => return model_complete(dir, ModelKind::Marian),
            "m2m100_418m" => return model_complete(dir, ModelKind::M2M100),
            _ => {}
        }
        spec.required.iter().all(|file| std::fs::metadata(dir.join(file)).is_ok_and(|meta| meta.is_file() && meta.len() > 0))
    })
}

#[derive(PartialEq, Eq)]
struct VerifiedFiles {
    directory: PathBuf,
    files: Vec<(u64, std::time::SystemTime)>,
}

impl VerifiedFiles {
    fn inspect(directory: &Path, model: &ModelSpec) -> Result<Self> {
        let mut files = Vec::with_capacity(model.files.len());
        for file in model.files {
            let meta = std::fs::metadata(directory.join(file.path))?;
            files.push((meta.len(), meta.modified()?));
        }
        Ok(Self { directory: directory.to_owned(), files })
    }
}

static VERIFIED: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, VerifiedFiles>>> =
    std::sync::LazyLock::new(Default::default);

/// Explicit user-initiated check, including models imported outside the download manager.
pub fn verify_installed(app: &AppHandle, id: &str) -> Result<String> {
    let model = spec(id).ok_or_else(|| anyhow!("未知模型"))?;
    let directory = locate(app, id).ok_or_else(|| anyhow!("找不到完整的模型文件"))?;
    let observed = VerifiedFiles::inspect(&directory, model)?;
    let cancel = AtomicBool::new(false);
    for file in model.files {
        verify_existing(file, &directory.join(file.path), &cancel)
            .map_err(|e| anyhow!("{} 与发布清单不符：{e}。自定义版本可继续使用；官方模型可重新下载。", file.path))?;
    }
    anyhow::ensure!(observed == VerifiedFiles::inspect(&directory, model)?, "模型文件在校验期间已改变，请重新校验");
    VERIFIED.lock_safe().insert(id.to_owned(), observed);
    Ok(format!("{} 已通过文件大小和清单哈希校验", model.name))
}

fn dir_size(p: &Path) -> u64 {
    std::fs::read_dir(p)
        .map(|rd| rd.flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum())
        .unwrap_or(0)
}

#[derive(Serialize)]
pub struct ModelStatus {
    id: String,
    name: String,
    description: String,
    license: String,
    approx_mb: u32,
    installed: bool,
    verified: bool,
    path: Option<String>,
    size_mb: f64,
    /// true when the model lives in the user download folder (so it can be deleted from the app)
    deletable: bool,
    downloading: bool,
}

pub fn status(app: &AppHandle) -> Vec<ModelStatus> {
    let user = user_models_dir(app);
    let st = app.state::<AppState>();
    let dl = st.downloads.lock_safe();
    MODELS
        .iter()
        .map(|m| {
            let p = locate(app, m.id);
            ModelStatus {
                id: m.id.into(),
                name: m.name.into(),
                description: m.description.into(),
                license: m.license.into(),
                approx_mb: m.approx_mb,
                verified: p.as_ref().is_some_and(|dir| {
                    let verified = VERIFIED.lock_safe();
                    verified.get(m.id).is_some_and(|old| VerifiedFiles::inspect(dir, m).is_ok_and(|now| old == &now))
                }),
                installed: p.is_some(),
                size_mb: p.as_deref().map(|p| dir_size(p) as f64 / 1e6).unwrap_or(0.0),
                deletable: p.as_ref().is_some_and(|p| p.starts_with(&user)),
                path: p.map(|p| p.display().to_string()),
                downloading: dl.contains_key(m.id),
            }
        })
        .collect()
}

#[derive(Clone, Serialize)]
struct Progress<'a> {
    id: &'a str,
    file: &'a str,
    downloaded: u64,
    total: u64,
    done: bool,
    error: Option<String>,
}

fn hf_hosts(mirror: &str) -> Vec<&'static str> {
    match mirror {
        "hf" => vec!["https://huggingface.co"],
        "hf-mirror" => vec!["https://hf-mirror.com"],
        _ => vec!["https://huggingface.co", "https://hf-mirror.com"],
    }
}

fn file_url(f: &FileSpec, host: &str) -> String {
    if f.url.starts_with('/') {
        format!("{MS}{}", f.url)
    } else {
        f.url.replace("{hf}", host)
    }
}

pub async fn download(app: AppHandle, id: String) -> Result<()> {
    let spec = spec(&id).ok_or_else(|| anyhow!("unknown model {id}"))?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let st = app.state::<AppState>();
        let mut dl = st.downloads.lock_safe();
        if dl.contains_key(&id) {
            bail!("already downloading");
        }
        dl.insert(id.clone(), cancel.clone());
    }
    let res = download_inner(&app, spec, &cancel).await;
    let st = app.state::<AppState>();
    st.downloads.lock_safe().remove(&id);
    let _ = app.emit(
        "model-progress",
        Progress { id: &id, file: "", downloaded: 0, total: 0, done: true, error: res.as_ref().err().map(|e| e.to_string()) },
    );
    res
}

async fn download_inner(app: &AppHandle, spec: &ModelSpec, cancel: &AtomicBool) -> Result<()> {
    let mirror = app.state::<AppState>().settings.lock_safe().mirror.clone();
    let dest = user_models_dir(app).join(spec.dir);
    let tmp = user_models_dir(app).join(format!("{}.partial", spec.dir));
    std::fs::create_dir_all(&tmp)?;
    let client = reqwest::Client::builder()
        .user_agent(concat!("ocr-trans/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()?;
    for f in spec.files {
        if cancel.load(Ordering::Relaxed) {
            bail!("已取消");
        }
        let target = tmp.join(f.path);
        // A previous download can have been interrupted by a crash or its partial folder changed on disk.
        // Only reuse a completed file after checking the same size/hash as a fresh transfer.
        if target.exists() && verify_existing(f, &target, cancel).is_ok() {
            continue;
        }
        let mut last_err = None;
        for host in hf_hosts(&mirror) {
            if cancel.load(Ordering::Relaxed) {
                bail!("已取消");
            }
            let url = file_url(f, host);
            match fetch(app, &client, &url, &target, spec.id, f, cancel).await {
                Ok(()) => {
                    last_err = None;
                    break;
                }
                Err(e) => last_err = Some(e),
            }
            if cancel.load(Ordering::Relaxed) || !f.url.contains("{hf}") {
                break;
            }
        }
        if let Some(e) = last_err {
            return Err(e);
        }
    }
    if cancel.load(Ordering::Relaxed) {
        bail!("已取消");
    }
    app.state::<AppState>().change_model_files(|| replace_directory(&tmp, &dest))?;
    Ok(())
}

/// Preserve an existing installation until the complete replacement has been moved into place.
/// A failed rename rolls back; a crash between the two renames can be recovered on the next attempt.
fn replace_directory(tmp: &Path, dest: &Path) -> Result<()> {
    if !tmp.is_dir() {
        bail!("下载的模型目录不存在");
    }
    let backup = dest.with_extension("previous");
    if backup.exists() {
        if !dest.exists() {
            std::fs::rename(&backup, dest)?;
        } else {
            std::fs::remove_dir_all(&backup)?;
        }
    }
    let existed = dest.exists();
    if existed {
        std::fs::rename(dest, &backup)?;
    }
    if let Err(e) = std::fs::rename(tmp, dest) {
        if existed {
            if let Err(restore) = std::fs::rename(&backup, dest) {
                bail!("安装模型失败：{e}；旧模型保留在 {}（恢复失败：{restore}）", backup.display());
            }
        }
        return Err(e.into());
    }
    if existed {
        // Installation succeeded; a backup cleanup error must not tell the user that it failed.
        if let Err(e) = std::fs::remove_dir_all(&backup) {
            crate::log_error(&format!("removing the previous model {}: {e}", backup.display()));
        }
    }
    Ok(())
}

fn verify_existing(spec: &FileSpec, path: &Path, cancel: &AtomicBool) -> Result<()> {
    use std::io::Read;
    use sha2::Digest;
    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    // Reject wrong sizes before hashing a potentially large file.
    if size != spec.size {
        return verify(spec, size, "");
    }
    let mut hash = sha2::Sha256::new();
    if spec.sha256.is_some() {
        let mut buf = [0; 64 * 1024];
        loop {
            if cancel.load(Ordering::Relaxed) {
                bail!("已取消");
            }
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
        }
    }
    verify(spec, size, &format!("{:x}", hash.finalize()))
}

/// A download that receives nothing for this long fails (and the next mirror is tried).
const STALL: Duration = Duration::from_secs(30);

/// Await `fut`, giving up as soon as the user cancels or when it has not finished within `STALL`.
/// Without this a connection that stalls mid-transfer hung the download forever and 取消 did nothing (0.5.4).
async fn watch<F: std::future::Future>(fut: F, cancel: &AtomicBool) -> Result<F::Output> {
    let mut fut = Box::pin(fut);
    let start = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            bail!("已取消");
        }
        match tokio::time::timeout(Duration::from_millis(250), &mut fut).await {
            Ok(v) => return Ok(v),
            Err(_) if start.elapsed() >= STALL => bail!("网络停滞超过 {} 秒", STALL.as_secs()),
            Err(_) => {}
        }
    }
}

/// A downloaded file must have the expected size and (for the large files) SHA-256 before it is used.
fn verify(spec: &FileSpec, got: u64, sha256_hex: &str) -> Result<()> {
    let file = spec.path;
    if got != spec.size {
        bail!("{file} 大小不对（{got} 字节，应为 {} 字节），可能是下载源返回了错误页面或文件已变更", spec.size);
    }
    if spec.sha256.is_some_and(|want| want != sha256_hex) {
        bail!("{file} 校验失败（SHA-256 不符）");
    }
    Ok(())
}

async fn fetch(
    app: &AppHandle,
    client: &reqwest::Client,
    url: &str,
    target: &Path,
    id: &str,
    spec: &FileSpec,
    cancel: &AtomicBool,
) -> Result<()> {
    use sha2::Digest;
    let file = spec.path;
    let resp = watch(client.get(url).send(), cancel).await??.error_for_status()?;
    let total = resp.content_length().unwrap_or(0);
    let part = target.with_extension("part");
    let mut out = std::fs::File::create(&part)?;
    let mut stream = resp.bytes_stream();
    let mut got = 0u64;
    let mut last_emit = Instant::now();
    let mut hash = sha2::Sha256::new();
    let res: Result<()> = async {
        while let Some(chunk) = watch(stream.next(), cancel).await? {
            let chunk = chunk?;
            if chunk.len() as u64 > spec.size.saturating_sub(got) {
                bail!("{file} 下载数据超过预期大小，已停止并丢弃");
            }
            std::io::Write::write_all(&mut out, &chunk)?;
            hash.update(&chunk);
            got += chunk.len() as u64;
            if last_emit.elapsed().as_millis() > 200 {
                last_emit = Instant::now();
                let _ = app.emit("model-progress", Progress { id, file, downloaded: got, total, done: false, error: None });
            }
        }
        verify(spec, got, &format!("{:x}", std::mem::take(&mut hash).finalize()))
    }
    .await;
    drop(out);
    if let Err(e) = res {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, target)?;
    Ok(())
}

pub fn cancel(app: &AppHandle, id: &str) {
    if let Some(c) = app.state::<AppState>().downloads.lock_safe().get(id) {
        c.store(true, Ordering::Relaxed);
    }
}

pub fn delete(app: &AppHandle, id: &str) -> Result<()> {
    let s = spec(id).ok_or_else(|| anyhow!("unknown model"))?;
    let st = app.state::<AppState>();
    let downloads = st.downloads.lock_safe();
    if downloads.contains_key(id) {
        bail!("模型正在下载，请先取消下载后再删除");
    }
    let d = user_models_dir(app).join(s.dir);
    st.change_model_files(|| {
        if d.exists() {
            std::fs::remove_dir_all(d)?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap()
    }

    #[test]
    fn cancel_stops_a_stalled_download_at_once() {
        let cancel = AtomicBool::new(false);
        let t = Instant::now();
        let r = rt().block_on(async {
            let stalled = std::future::pending::<()>();
            let flip = async {
                tokio::time::sleep(Duration::from_millis(300)).await;
                cancel.store(true, Ordering::Relaxed);
                std::future::pending::<()>().await
            };
            futures_util::future::select(Box::pin(watch(stalled, &cancel)), Box::pin(flip)).await
        });
        let futures_util::future::Either::Left((res, _)) = r else { panic!("watch never returned") };
        assert_eq!(res.unwrap_err().to_string(), "已取消");
        assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
    }

    #[test]
    fn finished_futures_pass_through() {
        let cancel = AtomicBool::new(false);
        assert_eq!(rt().block_on(watch(async { 7 }, &cancel)).unwrap(), 7);
    }

    #[test]
    fn verify_checks_size_and_hash() {
        let f = FileSpec { url: "", path: "x.bin", size: 3, sha256: Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad") };
        assert!(verify(&f, 3, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad").is_ok());
        assert!(verify(&f, 4, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad").is_err());
        assert!(verify(&f, 3, &"0".repeat(64)).is_err());
        let small = FileSpec { url: "", path: "config.json", size: 215, sha256: None };
        assert!(verify(&small, 215, "anything").is_ok());
    }

    fn temporary_directory(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("ocrtrans-{label}-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn resumed_files_are_verified_and_cancellable() {
        let root = temporary_directory("resume");
        let path = root.join("model.bin");
        let f = FileSpec { url: "", path: "model.bin", size: 3, sha256: Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad") };
        let cancel = AtomicBool::new(false);
        std::fs::write(&path, b"abc").unwrap();
        assert!(verify_existing(&f, &path, &cancel).is_ok());
        std::fs::write(&path, b"abd").unwrap();
        assert!(verify_existing(&f, &path, &cancel).is_err());
        std::fs::write(&path, b"ab").unwrap();
        assert!(verify_existing(&f, &path, &cancel).is_err());
        std::fs::write(&path, b"abc").unwrap();
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(verify_existing(&f, &path, &cancel).unwrap_err().to_string(), "已取消");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn replacement_preserves_old_model_and_recovers_interrupted_swap() {
        let root = temporary_directory("replace");
        let dest = root.join("model");
        let tmp = root.join("model.partial");
        std::fs::create_dir(&dest).unwrap();
        std::fs::write(dest.join("model.bin"), b"old").unwrap();
        assert!(replace_directory(&tmp, &dest).is_err());
        assert_eq!(std::fs::read(dest.join("model.bin")).unwrap(), b"old");
        std::fs::rename(&dest, dest.with_extension("previous")).unwrap();
        std::fs::create_dir(&tmp).unwrap();
        std::fs::write(tmp.join("model.bin"), b"new").unwrap();
        replace_directory(&tmp, &dest).unwrap();
        assert_eq!(std::fs::read(dest.join("model.bin")).unwrap(), b"new");
        assert!(!dest.with_extension("previous").exists());
        assert!(!tmp.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn incomplete_ocr_directory_does_not_hide_complete_model() {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("ocrtrans-model-test-{}-{nonce}", std::process::id()));
        let custom = root.join("custom");
        let bundled = root.join("bundled");
        for path in [&custom, &bundled] {
            std::fs::create_dir_all(path.join("ocr")).unwrap();
            std::fs::write(path.join("ocr/rec.onnx"), [1]).unwrap();
        }
        std::fs::write(custom.join("ocr/det.onnx"), []).unwrap();
        std::fs::write(bundled.join("ocr/det.onnx"), [1]).unwrap();
        let found = locate_in(spec("ocr").unwrap(), [custom, bundled.clone()]);
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(found, Some(bundled.join("ocr")));
    }

    #[test]
    fn every_url_names_a_fixed_revision() {
        for m in MODELS {
            for f in m.files {
                let u = file_url(f, "https://huggingface.co");
                assert!(!u.contains("/resolve/main/") && !u.contains("/resolve/master/"), "{u}");
                assert!(f.size > 0, "{u}");
            }
        }
    }
}
