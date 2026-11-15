//! ocrtrans-core: screenshot -> OCR (PP-OCRv5) -> paragraphs -> offline translation (CTranslate2).

pub mod capture;
pub mod cpu;
pub mod inpaint;
pub mod layout;
#[cfg(feature = "llm")]
pub mod llm;
pub mod ocr;
pub mod translate;

use std::path::Path;

use anyhow::Result;
use image::RgbImage;
use serde::Serialize;

pub use layout::{Lang, Paragraph};

/// Leave the process immediately (`_exit`): no C/C++ static destructors run. Call it only after the models
/// (ONNX sessions, CTranslate2, llama.cpp/Metal) have been dropped and files flushed — some native libraries
/// abort in their static teardown when the process exits while their resources are still referenced.
pub fn exit_now(code: i32) -> ! {
    extern "C" {
        fn _exit(code: i32) -> !;
    }
    unsafe { _exit(code) }
}
static LOGGER: std::sync::OnceLock<fn(&str)> = std::sync::OnceLock::new();

/// Where the core's diagnostic lines go (the app passes its log file writer); stderr until set.
pub fn set_logger(f: fn(&str)) {
    let _ = LOGGER.set(f);
}

/// Hardware choice for the quality model: `cpu_only` from 设置; `gpu_failed` = a GPU failure remembered from an
/// earlier run (that GPU is not tried again). No-op without the `llm` feature.
pub fn set_llm_device(cpu_only: bool, gpu_failed: Option<String>) {
    #[cfg(feature = "llm")]
    {
        llm::set_cpu_only(cpu_only);
        llm::set_gpu_blocked(gpu_failed);
    }
    #[cfg(not(feature = "llm"))]
    let _ = (cpu_only, gpu_failed);
}

/// (what the quality model ran on last, why the GPU is not used) for 设置.
pub fn llm_device_state() -> (Option<String>, Option<String>) {
    #[cfg(feature = "llm")]
    return (llm::running_on(), llm::gpu_blocked());
    #[cfg(not(feature = "llm"))]
    (None, None)
}

pub(crate) fn log(msg: &str) {
    match LOGGER.get() {
        Some(f) => f(msg),
        None => eprintln!("{msg}"),
    }
}

/// Stop long-running work (LLM generation) at the next step, so whoever holds the pipeline releases it quickly.
/// For process exit: generations stay refused afterwards.
pub fn is_exiting() -> bool {
    #[cfg(feature = "llm")]
    return llm::aborted();
    #[cfg(not(feature = "llm"))]
    false
}

pub fn abort_work() {
    #[cfg(feature = "llm")]
    llm::abort();
}

pub use ocr::{Ocr, TextLine};
pub use translate::Translators;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Timings {
    pub load_ms: f64,
    pub det_ms: f64,
    pub rec_ms: f64,
    pub layout_ms: f64,
    pub translate_ms: f64,
    pub total_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineResult {
    pub width: u32,
    pub height: u32,
    pub paragraphs: Vec<Paragraph>,
    pub timings: Timings,
    /// Non-fatal problems (e.g. missing translation model for a language).
    pub warnings: Vec<String>,
    /// Engine that produced the translations: "nmt", "llm" or "llm+nmt".
    #[serde(default)]
    pub engine: String,
}

#[derive(Debug, Clone, Default, serde::Deserialize, Serialize)]
pub struct TranslateOptions {
    /// Use the LLM tier (falls back to NMT per paragraph when unavailable or unparsable).
    pub quality: bool,
    /// User glossary: source term -> Chinese.
    pub glossary: Vec<(String, String)>,
}

impl PipelineResult {
    pub fn plain_text(&self) -> String {
        self.paragraphs.iter().map(|p| p.text.as_str()).collect::<Vec<_>>().join("\n")
    }
}

pub struct Pipeline {
    pub ocr: Ocr,
    pub translators: Translators,
    #[cfg(feature = "llm")]
    llm_path: Option<std::path::PathBuf>,
    #[cfg(feature = "llm")]
    llm: Option<llm::LlmTranslator>,
    #[cfg(feature = "llm")]
    llm_last_used: Option<std::time::Instant>,
}

impl Pipeline {
    pub fn load(models_dir: &Path) -> Result<Self> {
        let mut p = Self::with_parts(&models_dir.join("ocr"), Translators::new(models_dir))?;
        p.set_llm_path(Some(models_dir.join("qwen3-1.7b").join("model.gguf")));
        Ok(p)
    }

    pub fn with_parts(ocr_dir: &Path, translators: Translators) -> Result<Self> {
        Ok(Self {
            ocr: Ocr::load(ocr_dir)?,
            translators,
            #[cfg(feature = "llm")]
            llm_path: None,
            #[cfg(feature = "llm")]
            llm: None,
            #[cfg(feature = "llm")]
            llm_last_used: None,
        })
    }

    /// Path of the GGUF model for the quality tier (ignored without the `llm` feature).
    pub fn set_llm_path(&mut self, path: Option<std::path::PathBuf>) {
        #[cfg(feature = "llm")]
        {
            self.llm_path = path.filter(|p| p.exists());
            self.llm = None;
        }
        #[cfg(not(feature = "llm"))]
        let _ = path;
    }

    /// Free the LLM (≈1.2 GB) if it has not been used for `idle`; returns true if it was unloaded.
    pub fn unload_llm_if_idle(&mut self, idle: std::time::Duration) -> bool {
        #[cfg(feature = "llm")]
        if self.llm.is_some() && self.llm_last_used.is_some_and(|t| t.elapsed() >= idle) {
            self.llm = None;
            return true;
        }
        let _ = idle;
        false
    }

    /// Whether the quality model is in memory (else the next quality translation loads it first).
    pub fn llm_loaded(&self) -> bool {
        #[cfg(feature = "llm")]
        return self.llm.is_some();
        #[cfg(not(feature = "llm"))]
        false
    }

    /// Load the quality model now if it is installed and not loaded (so the caller can say "loading" meanwhile).
    /// A failure is left for the translation to report.
    pub fn prepare_llm(&mut self) {
        #[cfg(feature = "llm")]
        if self.llm.is_none() {
            if let Some(path) = self.llm_path.clone() {
                match llm::LlmTranslator::load(&path) {
                    Ok(m) => {
                        self.llm = Some(m);
                        self.llm_last_used = Some(std::time::Instant::now());
                    }
                    Err(e) => crate::log(&format!("llm: loading failed: {e:#}")),
                }
            }
        }
    }

    /// Drop the quality model (a device setting changed): the next use loads it again.
    pub fn unload_llm(&mut self) {
        #[cfg(feature = "llm")]
        {
            self.llm = None;
        }
    }

    pub fn has_llm(&self) -> bool {
        #[cfg(feature = "llm")]
        return self.llm_path.is_some();
        #[cfg(not(feature = "llm"))]
        false
    }

    /// OCR only.
    pub fn recognize(&mut self, img: &RgbImage) -> Result<PipelineResult> {
        let t0 = std::time::Instant::now();
        let (lines, det_ms, rec_ms) = self.ocr.run(img)?;
        let t1 = std::time::Instant::now();
        let paragraphs = layout::build_paragraphs(img, lines);
        let layout_ms = ocr::ms(t1.elapsed());
        Ok(PipelineResult {
            width: img.width(),
            height: img.height(),
            paragraphs,
            timings: Timings { det_ms, rec_ms, layout_ms, total_ms: ocr::ms(t0.elapsed()), ..Default::default() },
            warnings: vec![],
            engine: String::new(),
        })
    }

    /// Translate paragraphs of `res` in place (en/ja -> zh) with the fast NMT tier.
    pub fn translate(&mut self, res: &mut PipelineResult) -> Result<()> {
        self.translate_with(res, &TranslateOptions::default())
    }

    /// Translate paragraphs of `res` in place (en/ja -> zh). Chinese / unknown text is left alone.
    pub fn translate_with(&mut self, res: &mut PipelineResult, opts: &TranslateOptions) -> Result<()> {
        let t0 = std::time::Instant::now();
        let todo: Vec<usize> = (0..res.paragraphs.len())
            .filter(|&i| matches!(res.paragraphs[i].lang, Lang::En | Lang::Ja))
            .collect();
        let mut used_llm = false;
        let mut llm_count = 0;
        if opts.quality && !todo.is_empty() {
            match self.translate_llm(res, &todo, &opts.glossary) {
                Ok(n) => {
                    used_llm = n > 0;
                    llm_count = n;
                }
                Err(e) => {
                    crate::log(&format!("llm: unavailable: {e:#}"));
                    res.warnings.push(format!("高质量翻译不可用，已改用快速翻译：{e:#}"));
                }
            }
            // paragraphs the LLM did not answer (cut off by the time limit, not translated, a failed chunk) are
            // translated by the fast engine below — never shown in the source language
            if used_llm && llm_count < todo.len() {
                res.warnings.push(format!("{} 段已用快速翻译（高质量翻译超时或没能翻译这些段落）", todo.len() - llm_count));
            } else if !used_llm && res.warnings.is_empty() {
                res.warnings.push("高质量翻译没能完成，已全部改用快速翻译".into());
            }
        }
        Self::finish_translation(&mut self.translators, res, &opts.glossary, used_llm, t0)
    }

    /// Complete remaining paragraphs with the fast model, retaining completed quality results.
    pub fn finish_translation(translators: &mut Translators, res: &mut PipelineResult, glossary: &[(String, String)], used_llm: bool, t0: std::time::Instant) -> Result<()> {
        let mut used_nmt = false;
        for (lang, code) in [(Lang::En, "en"), (Lang::Ja, "ja")] {
            let idx: Vec<usize> = res.paragraphs.iter().enumerate()
                .filter(|(_, p)| p.lang == lang && p.translation.is_none()).map(|(i, _)| i).collect();
            if idx.is_empty() {
                continue;
            }
            if !translators.has(code) {
                res.warnings.push(format!("未安装{}→中翻译模型", if code == "en" { "英" } else { "日" }));
                continue;
            }
            let texts: Vec<String> = idx.iter().map(|&i| res.paragraphs[i].text.clone()).collect();
            // a failing fast engine must not throw away what the LLM already translated
            let out = match translators.translate_paragraphs(code, &texts, glossary) {
                Ok(out) => out,
                Err(e) if used_llm => {
                    crate::log(&format!("fast translation failed: {e:#}"));
                    res.warnings.push(format!("快速翻译失败：{e:#}"));
                    continue;
                }
                Err(e) => return Err(e),
            };
            let mut empty = 0;
            for (i, t) in idx.into_iter().zip(out) {
                if t.trim().is_empty() {
                    empty += 1;
                } else {
                    res.paragraphs[i].translation = Some(t);
                    used_nmt = true;
                }
            }
            if empty > 0 {
                res.warnings.push(format!("{empty} 段未返回译文，已保留原文"));
            }
        }
        res.engine = match (used_llm, used_nmt) {
            (true, true) => "llm+nmt",
            (true, false) => "llm",
            (false, true) => "nmt",
            _ => "",
        }
        .into();
        let ms = ocr::ms(t0.elapsed());
        res.timings.total_ms += ms - res.timings.translate_ms;
        res.timings.translate_ms = ms;
        Ok(())
    }

    #[cfg(feature = "llm")]
    fn translate_llm(&mut self, res: &mut PipelineResult, todo: &[usize], glossary: &[(String, String)]) -> Result<usize> {
        let path = self.llm_path.clone().ok_or_else(|| anyhow::anyhow!("未安装 Qwen3 模型"))?;
        if self.llm.is_none() {
            self.llm = Some(llm::LlmTranslator::load(&path)?);
        }
        self.llm_last_used = Some(std::time::Instant::now());
        let llm = self.llm.as_ref().unwrap();
        let on_gpu = llm.on_gpu();
        let paras: Vec<(&str, &str)> = todo
            .iter()
            .map(|&i| (if res.paragraphs[i].lang == Lang::Ja { "ja" } else { "en" }, res.paragraphs[i].text.as_str()))
            .collect();
        let out = match llm.translate(&paras, glossary) {
            // a GPU that fails is not used again: load the model on the CPU and redo the screen there
            Err(e) if on_gpu && !llm::aborted() => {
                llm::set_gpu_blocked(Some(format!("{e:#}")));
                crate::log(&format!("llm: the GPU failed ({e:#}); redoing the screen on the CPU"));
                // Release the failed GPU allocation before loading another 1.2 GB copy.
                self.llm = None;
                self.llm = Some(llm::LlmTranslator::load(&path)?);
                self.llm.as_ref().unwrap().translate(&paras, glossary)?
            }
            r => r?,
        };
        let mut n = 0;
        let llm::LlmOutcome { translations, notes } = out;
        if !notes.is_empty() {
            crate::log(&format!("llm: left to the fast engine — {}", notes.join("; ")));
        }
        for (&i, t) in todo.iter().zip(translations) {
            if let Some(t) = t {
                let t = translate::tidy_zh(&translate::replace_terms(&t, glossary));
                if !t.is_empty() {
                    res.paragraphs[i].translation = Some(t);
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    #[cfg(not(feature = "llm"))]
    fn translate_llm(&mut self, _: &mut PipelineResult, _: &[usize], _: &[(String, String)]) -> Result<usize> {
        Err(anyhow::anyhow!("this build has no LLM support"))
    }

    /// Erase the original text under translated paragraphs; sets each paragraph's `fill`.
    pub fn clean_background(img: &RgbImage, res: &mut PipelineResult) -> RgbImage {
        let (out, fills) = inpaint::clean(img, &res.paragraphs);
        for (p, f) in res.paragraphs.iter_mut().zip(fills) {
            p.fill = f.map(|f| f.as_str().to_string());
        }
        out
    }

    pub fn run(&mut self, img: &RgbImage, translate: bool) -> Result<PipelineResult> {
        let mut r = self.recognize(img)?;
        if translate {
            self.translate(&mut r)?;
        }
        Ok(r)
    }
}
