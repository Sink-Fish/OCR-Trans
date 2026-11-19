//! Offline translation with CTranslate2 (via ct2rs).
//!
//! * `Marian`  – Helsinki-NLP/opus-mt-en-zh: source.spm / target.spm, needs `>>cmn_Hans<<` target token.
//! * `M2M100`  – facebook/m2m100_418M: one sentencepiece.bpe.model, `__ja__` source token and
//!   `__zh__` forced as target prefix.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

#[cfg(feature = "nmt")]
pub use backend::MtModel;
#[cfg(not(feature = "nmt"))]
pub use stub::MtModel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelKind {
    Marian,
    M2M100,
}

/// Files required by the supported CTranslate2 models, including their tokenizer and vocabulary.
/// A partial custom/download directory must not hide a complete installation elsewhere.
pub fn model_complete(dir: &Path, kind: ModelKind) -> bool {
    let file = |name: &str| dir.join(name).metadata().is_ok_and(|m| m.is_file() && m.len() > 0);
    let vocabulary = |name: &str| file(&format!("{name}.json")) || file(&format!("{name}.txt"));
    file("model.bin")
        && file("config.json")
        && match kind {
            ModelKind::Marian => file("source.spm") && file("target.spm"),
            ModelKind::M2M100 => file("sentencepiece.bpe.model"),
        }
        && (vocabulary("shared_vocabulary")
            || (vocabulary("target_vocabulary") && (vocabulary("source_vocabulary") || vocabulary("source_1_vocabulary"))))
}

/// Builds without the native CTranslate2/SentencePiece backend (e.g. `cargo check` for Windows from macOS)
/// keep the same API; loading a model just fails.
#[cfg(not(feature = "nmt"))]
mod stub {
    use super::*;

    pub struct MtModel;

    impl MtModel {
        pub fn load_marian(_: &Path, _: Option<&str>) -> Result<Self> {
            Err(anyhow!("built without the `nmt` feature"))
        }
        pub fn load_m2m(_: &Path, _: &str, _: &str) -> Result<Self> {
            Err(anyhow!("built without the `nmt` feature"))
        }
        pub fn kind(&self) -> ModelKind {
            ModelKind::Marian
        }
        pub fn translate(&self, _: &[String]) -> Result<Vec<String>> {
            Err(anyhow!("built without the `nmt` feature"))
        }
    }
}

#[cfg(feature = "nmt")]
mod backend {
use super::*;
use anyhow::{ensure, Context};
use ct2rs::{ComputeType, Config, Tokenizer, TranslationOptions};
use ct2rs::sys::Translator;
use sentencepiece::SentencePieceProcessor;

struct SpmTokenizer {
    enc: SentencePieceProcessor,
    dec: SentencePieceProcessor,
    src_prefix: Vec<String>,
}

impl ct2rs::Tokenizer for SpmTokenizer {
    fn encode(&self, input: &str) -> Result<Vec<String>> {
        let mut v = self.src_prefix.clone();
        v.extend(self.enc.encode(input)?.into_iter().map(|p| p.piece.to_string()));
        v.push("</s>".into());
        Ok(v)
    }
    fn decode(&self, tokens: Vec<String>) -> Result<String> {
        Ok(self.dec.decode_pieces(&tokens)?)
    }
}

pub struct MtModel {
    translator: Translator,
    tokenizer: SpmTokenizer,
    kind: ModelKind,
    target_prefix: Option<String>,
}

fn config() -> Config {
    let threads = std::thread::available_parallelism().map(|n| n.get().min(8)).unwrap_or(4);
    Config { compute_type: ComputeType::INT8, num_threads_per_replica: threads, ..Default::default() }
}

impl MtModel {
    /// Load an opus-mt style model directory (model.bin + source.spm + target.spm).
    pub fn load_marian(dir: &Path, target_lang_token: Option<&str>) -> Result<Self> {
        let tok = SpmTokenizer {
            enc: SentencePieceProcessor::open(dir.join("source.spm")).context("source.spm")?,
            dec: SentencePieceProcessor::open(dir.join("target.spm")).context("target.spm")?,
            src_prefix: target_lang_token.map(|t| vec![t.to_string()]).unwrap_or_default(),
        };
        Ok(Self {
            translator: Translator::new(dir, &config())?,
            tokenizer: tok,
            kind: ModelKind::Marian,
            target_prefix: None,
        })
    }

    /// Load an M2M100 model directory (model.bin + sentencepiece.bpe.model).
    pub fn load_m2m(dir: &Path, src: &str, tgt: &str) -> Result<Self> {
        let spm = dir.join("sentencepiece.bpe.model");
        let tok = SpmTokenizer {
            enc: SentencePieceProcessor::open(&spm).context("sentencepiece.bpe.model")?,
            dec: SentencePieceProcessor::open(&spm)?,
            src_prefix: vec![format!("__{src}__")],
        };
        Ok(Self {
            translator: Translator::new(dir, &config())?,
            tokenizer: tok,
            kind: ModelKind::M2M100,
            target_prefix: Some(format!("__{tgt}__")),
        })
    }

    pub fn kind(&self) -> ModelKind {
        self.kind
    }

    /// Translate a batch of segments.
    pub fn translate(&self, texts: &[String]) -> Result<Vec<String>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }
        let mut sources = vec![];
        let mut owners = vec![];
        for (i, text) in texts.iter().enumerate() {
            for part in split_for_model(&self.tokenizer.enc, text)? {
                sources.push(self.tokenizer.encode(part)?);
                owners.push(i);
            }
        }
        const MAX_OUTPUT: usize = 512;
        let opts: TranslationOptions<String, String> = TranslationOptions {
            beam_size: 2,
            // Source segments have a checked token budget. Disable CT2's implicit 1024-token
            // truncation so text can never disappear before inference.
            max_input_length: 0,
            max_decoding_length: MAX_OUTPUT,
            max_batch_size: 8,
            repetition_penalty: 1.1,
            no_repeat_ngram_size: 4,
            ..Default::default()
        };
        let res = match &self.target_prefix {
            None => self.translator.translate_batch(&sources, &opts, None)?,
            Some(p) => {
                let prefixes = vec![vec![p.clone()]; sources.len()];
                self.translator.translate_batch_with_target_prefix(&sources, &prefixes, &opts, None)?
            }
        };
        ensure!(res.len() == owners.len(), "快速翻译未返回全部片段，已保留原文");
        let mut out = vec![String::new(); texts.len()];
        for (owner, result) in owners.into_iter().zip(res) {
            let tokens = result.hypotheses.into_iter().next().context("快速翻译未返回结果")?;
            ensure!(tokens.len() < MAX_OUTPUT, "快速翻译达到输出长度上限，已保留原文以免丢失内容");
            let decoded = self.tokenizer.decode(tokens)?;
            let text = self.target_prefix.as_ref()
                .and_then(|prefix| decoded.trim_start().strip_prefix(prefix.as_str()))
                .unwrap_or(&decoded);
            out[owner].push_str(text);
        }
        Ok(out.into_iter().map(|t| tidy_zh(&t)).collect())
    }
}

/// Split oversized sentences at source-token boundaries, preferring nearby whitespace/punctuation.
/// Slices cover every original byte and keep glossary placeholders intact; output room is more than twice
/// the source budget. The original 1024-input/256-output defaults silently lost long sentence tails.
fn split_for_model<'a>(encoder: &SentencePieceProcessor, text: &'a str) -> Result<Vec<&'a str>> {
    const MAX_SOURCE: usize = 192;
    let mut rest = text;
    let mut out = vec![];
    while !rest.is_empty() {
        let pieces = encoder.encode(rest)?;
        if pieces.len() <= MAX_SOURCE {
            out.push(rest);
            break;
        }
        let mut cut = pieces[MAX_SOURCE - 1].span.1 as usize;
        ensure!(cut > 0 && cut < rest.len() && rest.is_char_boundary(cut), "无法安全拆分过长的翻译段落");
        if let Some((i, c)) = rest[..cut].char_indices().rev()
            .find(|&(i, c)| i >= cut / 2 && (c.is_whitespace() || matches!(c, ',' | ';' | ':' | '，' | '；' | '：' | '、'))) {
            cut = i + c.len_utf8();
        }
        loop {
            // A cut inside `{123}` would prevent restoration of the glossary term.
            if let Some(open) = rest[..cut].rfind('{') {
                if let Some(close) = rest[open..].find('}') {
                    let end = open + close + 1;
                    if end > cut && rest[open + 1..end - 1].trim().chars().all(|c| c.is_ascii_digit()) {
                        cut = open;
                    }
                }
            }
            ensure!(cut > 0, "术语占位符超过翻译长度上限，已保留原文");
            if encoder.encode(&rest[..cut])?.len() <= MAX_SOURCE {
                break;
            }
            // SentencePiece normalization can change at a new end-of-input. Recheck rather
            // than relying on offsets alone, and retreat by a complete Unicode character.
            cut = rest[..cut].char_indices().next_back().map_or(0, |(i, _)| i);
        }
        out.push(&rest[..cut]);
        rest = &rest[cut..];
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_splits_preserve_every_byte_and_glossary_marker() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models/opus-mt-en-zh/source.spm");
        if !path.exists() {
            eprintln!("SentencePiece model missing — skipping token splitting integration test");
            return;
        }
        let encoder = SentencePieceProcessor::open(path).unwrap();
        for text in [
            "The file includes {12345} and needs to preserve its ending, ".repeat(100),
            "这是混合文本 {12} と日本語のテストです，".repeat(100),
            "longUnbrokenWord0123456789".repeat(100),
        ] {
            let parts = split_for_model(&encoder, &text).unwrap();
            assert!(parts.len() > 1);
            assert_eq!(parts.concat(), text);
            assert!(parts.iter().all(|s| encoder.encode(s).unwrap().len() <= 192));
            for part in parts {
                assert_eq!(part.matches('{').count(), part.matches('}').count(), "split placeholder: {part}");
            }
        }
    }
}
}

fn is_cjk_or_fullwidth(c: char) -> bool {
    matches!(c as u32, 0x3000..=0x303F | 0x3400..=0x9FFF | 0xFF00..=0xFFEF | 0x2018..=0x201D)
}

/// Clean NMT output for Chinese: drop unknown-token marks, spaces next to CJK characters,
/// and use full-width punctuation between CJK text.
pub fn tidy_zh(s: &str) -> String {
    let s = s.replace(" ⁇ ", "").replace('⁇', "");
    let chars: Vec<char> = s.trim().chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let prev = out.chars().last();
        let next = chars[i + 1..].iter().copied().find(|c| *c != ' ');
        if c == ' ' {
            let p = prev.is_some_and(is_cjk_or_fullwidth);
            let n = next.is_some_and(is_cjk_or_fullwidth) || matches!(next, Some(',' | '.' | ';' | ':' | '!' | '?'));
            if p || n {
                continue;
            }
        }
        let cjk_before = prev.is_some_and(is_cjk_or_fullwidth);
        let mapped = match c {
            ',' if cjk_before => '，',
            ';' if cjk_before => '；',
            ':' if cjk_before => '：',
            '!' if cjk_before => '！',
            '?' if cjk_before => '？',
            '(' if next.is_some_and(is_cjk_or_fullwidth) => '（',
            ')' if cjk_before => '）',
            _ => c,
        };
        out.push(mapped);
    }
    out
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// Find glossary terms in `text` (case-insensitive; Latin terms only as whole words, longest match first).
/// Returns (char start, char end, glossary index) spans, non-overlapping, in order.
fn find_terms(text: &str, glossary: &[(String, String)]) -> Vec<(usize, usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let lower: Vec<char> = text.chars().map(|c| c.to_lowercase().next().unwrap_or(c)).collect();
    let mut order: Vec<usize> = (0..glossary.len()).filter(|&i| !glossary[i].0.trim().is_empty()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(glossary[i].0.chars().count()));
    let terms: Vec<(Vec<char>, bool)> = glossary
        .iter()
        .map(|(s, _)| (s.chars().map(|c| c.to_lowercase().next().unwrap_or(c)).collect(), s.is_ascii()))
        .collect();
    let mut spans = vec![];
    let mut i = 0;
    'scan: while i < chars.len() {
        for &g in &order {
            let (t, latin) = &terms[g];
            let n = t.len();
            if i + n <= chars.len()
                && lower[i..i + n] == t[..]
                && (!latin || ((i == 0 || !is_word_char(chars[i - 1])) && (i + n == chars.len() || !is_word_char(chars[i + n]))))
            {
                spans.push((i, i + n, g));
                i += n;
                continue 'scan;
            }
        }
        i += 1;
    }
    spans
}

fn substitute(text: &str, spans: &[(usize, usize, usize)], mut with: impl FnMut(usize, usize) -> String) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut pos = 0;
    for (k, &(a, b, g)) in spans.iter().enumerate() {
        out.extend(&chars[pos..a]);
        out.push_str(&with(k, g));
        pos = b;
    }
    out.extend(&chars[pos..]);
    out
}

/// Replace glossary terms with their translations.
pub fn replace_terms(text: &str, glossary: &[(String, String)]) -> String {
    let spans = find_terms(text, glossary);
    substitute(text, &spans, |_, g| glossary[g].1.clone())
}

/// A numeric placeholder including optional whitespace, with its exclusive character end and number.
fn numbered_marker(chars: &[char], start: usize) -> Option<(usize, usize)> {
    if chars.get(start) != Some(&'{') {
        return None;
    }
    let mut i = start + 1;
    while chars.get(i).is_some_and(|c| c.is_whitespace()) {
        i += 1;
    }
    let digits = i;
    let mut n = 0usize;
    while let Some(d) = chars.get(i).and_then(|c| c.to_digit(10)) {
        n = n.checked_mul(10)?.checked_add(d as usize)?;
        i += 1;
    }
    if i == digits {
        return None;
    }
    while chars.get(i).is_some_and(|c| c.is_whitespace()) {
        i += 1;
    }
    (chars.get(i) == Some(&'}')).then_some((i + 1, n))
}

/// Replace glossary terms with `{n}` placeholders (which opus-mt copies verbatim); returns the targets.
/// Existing numeric braces are protected too, so source text such as "{1}: Pin" cannot collide with a term.
pub fn protect_terms(text: &str, glossary: &[(String, String)]) -> (String, Vec<String>) {
    let spans = find_terms(text, glossary);
    if spans.is_empty() {
        return (text.to_string(), vec![]);
    }
    let chars: Vec<char> = text.chars().collect();
    let mut terms = spans.iter().peekable();
    let mut targets = vec![];
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if let Some(&&(a, b, g)) = terms.peek() {
            if a == i {
                targets.push(glossary[g].1.clone());
                out.push_str(&format!("{{{}}}", targets.len()));
                i = b;
                terms.next();
                continue;
            }
        }
        if let Some((end, _)) = numbered_marker(&chars, i) {
            if terms.peek().is_none_or(|&&(start, _, _)| start >= end) {
                targets.push(chars[i..end].iter().collect());
                out.push_str(&format!("{{{}}}", targets.len()));
                i = end;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    (out, targets)
}

/// Put the protected terms back (tolerates spaces the model inserted, e.g. "{ 1 }").
pub fn restore_terms(text: &str, targets: &[String]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if let Some((end, n)) = numbered_marker(&chars, i) {
            if let Some(t) = n.checked_sub(1).and_then(|n| targets.get(n)) {
                out.push_str(t);
                i = end;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Glossary entries that occur in `text`.
pub fn terms_in<'a>(text: &str, glossary: &'a [(String, String)]) -> Vec<&'a (String, String)> {
    let mut v: Vec<&(String, String)> = find_terms(text, glossary).into_iter().map(|(_, _, g)| &glossary[g]).collect();
    v.dedup();
    v
}

/// Split a paragraph into sentences so long paragraphs don't degrade the NMT output.
pub fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        cur.push(c);
        let end = match c {
            '。' | '！' | '？' => true,
            '.' | '!' | '?' => {
                let next = chars.get(i + 1).copied();
                let next2 = chars.get(i + 2).copied();
                // ". X" with X uppercase/quote, and the token before isn't a 1-letter abbreviation
                let prev_word: String = cur.trim_end_matches(['.', '!', '?']).rsplit(' ').next().unwrap_or("").into();
                next == Some(' ')
                    && next2.is_some_and(|n| n.is_uppercase() || n == '"' || n == '“' || n.is_ascii_digit())
                    && !(prev_word.len() <= 2 && prev_word.chars().all(|c| c.is_uppercase()))
                    && !matches!(prev_word.as_str(), "Mr" | "Mrs" | "Ms" | "Dr" | "St" | "vs" | "e.g" | "i.e" | "etc")
            }
            _ => false,
        };
        if end && cur.trim().chars().count() > 1 {
            out.push(cur.trim().to_string());
            cur.clear();
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// All models the app knows how to use, keyed by source language.
pub struct Translators {
    en_zh_dir: Option<PathBuf>,
    m2m_dir: Option<PathBuf>,
    en_zh: Option<MtModel>,
    ja_zh: Option<MtModel>,
}

impl Translators {
    /// Look for `opus-mt-en-zh/` and `m2m100_418m/` inside `models_dir`.
    pub fn new(models_dir: &Path) -> Self {
        Self::with_dirs(Some(models_dir.join("opus-mt-en-zh")), Some(models_dir.join("m2m100_418m")))
    }

    pub fn with_dirs(en_zh_dir: Option<PathBuf>, m2m_dir: Option<PathBuf>) -> Self {
        Self {
            en_zh_dir: en_zh_dir.filter(|d| model_complete(d, ModelKind::Marian)),
            m2m_dir: m2m_dir.filter(|d| model_complete(d, ModelKind::M2M100)),
            en_zh: None,
            ja_zh: None,
        }
    }

    pub fn has(&self, lang: &str) -> bool {
        match lang {
            "en" => self.en_zh_dir.is_some() || self.m2m_dir.is_some(),
            "ja" => self.m2m_dir.is_some(),
            _ => false,
        }
    }

    /// Lazily load and return the model for `lang` -> zh.
    pub fn get(&mut self, lang: &str) -> Result<&MtModel> {
        match lang {
            "en" => {
                if self.en_zh.is_none() {
                    self.en_zh = Some(match (&self.en_zh_dir, &self.m2m_dir) {
                        (Some(d), _) => MtModel::load_marian(d, Some(">>cmn_Hans<<"))?,
                        (None, Some(d)) => MtModel::load_m2m(d, "en", "zh")?,
                        _ => return Err(anyhow!("no en->zh model installed")),
                    });
                }
                Ok(self.en_zh.as_ref().unwrap())
            }
            "ja" => {
                if self.ja_zh.is_none() {
                    let d = self.m2m_dir.as_ref().ok_or_else(|| anyhow!("no ja->zh model installed"))?;
                    self.ja_zh = Some(MtModel::load_m2m(d, "ja", "zh")?);
                }
                Ok(self.ja_zh.as_ref().unwrap())
            }
            other => Err(anyhow!("no translation model for {other}->zh")),
        }
    }


    /// Translate paragraphs (each split into sentences, all batched together).
    /// Glossary terms are substituted in the source (NMT models copy the Chinese through) and again in the
    /// output for any term the model left untranslated.
    pub fn translate_paragraphs(&mut self, lang: &str, paras: &[String], glossary: &[(String, String)]) -> Result<Vec<String>> {
        let model = self.get(lang)?;
        let mut flat = Vec::new();
        let mut jobs = Vec::new(); // (sentence count, placeholder targets, whole-paragraph glossary hit)
        for p in paras {
            if let Some((_, t)) = glossary.iter().find(|(s, _)| s.trim().eq_ignore_ascii_case(p.trim())) {
                jobs.push((0, vec![], Some(t.clone())));
                continue;
            }
            let (protected, targets) = protect_terms(p, glossary);
            let s = split_sentences(&protected);
            jobs.push((s.len(), targets, None));
            flat.extend(s);
        }
        let tr = model.translate(&flat)?;
        let mut it = tr.into_iter();
        Ok(jobs
            .into_iter()
            .map(|(n, targets, whole)| match whole {
                Some(t) => t,
                None => {
                    let joined = it.by_ref().take(n).collect::<Vec<_>>().join("");
                    // Replace any unprotected source words first. A glossary target may itself
                    // contain the source term ("API" -> "API 接口") and must not be translated twice.
                    tidy_zh(&restore_terms(&replace_terms(&joined, glossary), &targets))
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tidy() {
        assert_eq!(tidy_zh("错误: 由于磁盘已满, 文件无法保存 。"), "错误：由于磁盘已满，文件无法保存。");
        assert_eq!(tidy_zh("辛克菲什·普罗夫  ⁇"), "辛克菲什·普罗夫");
        assert_eq!(tidy_zh("版本 5.5 发布"), "版本5.5发布");
    }

    #[test]
    fn glossary() {
        let g = vec![("pin".to_string(), "钉图".to_string()), ("Pin window".to_string(), "钉图窗口".to_string())];
        assert_eq!(replace_terms("Close the pin window. Pinned? PIN", &g), "Close the 钉图窗口. Pinned? 钉图");
        let (p, t) = protect_terms("Close the pin window. Pinned? PIN", &g);
        assert_eq!(p, "Close the {1}. Pinned? {2}");
        assert_eq!(restore_terms("关闭{ 1 }。固定？{2}", &t), "关闭钉图窗口。固定？钉图");
        assert_eq!(terms_in("a pin and a PIN", &g).len(), 1);
        let j = vec![("設定".to_string(), "设置".to_string())];
        assert_eq!(replace_terms("設定画面", &j), "设置画面");
    }

    #[test]
    fn sentences() {
        let s = split_sentences("Hello there. This is Mr. Smith. A.B. test! Ok");
        assert_eq!(s, vec!["Hello there.", "This is Mr. Smith.", "A.B. test!", "Ok"]);
    }

    #[test]
    fn glossary_preserves_literal_placeholders_and_never_recursively_replaces_targets() {
        let g = vec![("Pin".into(), "钉图 {2}".into()), ("API".into(), "API 接口".into())];
        let (protected, targets) = protect_terms("{1}: Pin and API, then { 2 }.", &g);
        assert_eq!(protected, "{1}: {2} and {3}, then {4}.");
        assert_eq!(restore_terms(&protected, &targets), "{1}: 钉图 {2} and API 接口, then { 2 }.");
        assert_eq!(restore_terms("{  1\t} {2}", &["{2}".into(), "原文".into()]), "{2} 原文");
        let (unchanged, targets) = protect_terms("No term: {1}", &g);
        assert!(targets.is_empty());
        assert_eq!(restore_terms(&unchanged, &targets), unchanged);
    }

    #[test]
    fn incomplete_translation_models_are_not_advertised_as_installed() {
        let dir = std::env::temp_dir().join(format!("ocrtrans-model-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&dir).unwrap();
        for name in ["model.bin", "source.spm", "target.spm", "sentencepiece.bpe.model"] {
            std::fs::write(dir.join(name), "data").unwrap();
        }
        assert!(!model_complete(&dir, ModelKind::Marian));
        std::fs::write(dir.join("config.json"), "{}").unwrap();
        assert!(!model_complete(&dir, ModelKind::M2M100));
        std::fs::write(dir.join("shared_vocabulary.txt"), "").unwrap();
        assert!(!model_complete(&dir, ModelKind::Marian));
        std::fs::write(dir.join("shared_vocabulary.txt"), "tokens").unwrap();
        assert!(model_complete(&dir, ModelKind::Marian));
        assert!(model_complete(&dir, ModelKind::M2M100));
        std::fs::remove_file(dir.join("shared_vocabulary.txt")).unwrap();
        std::fs::write(dir.join("source_vocabulary.json"), "[]").unwrap();
        std::fs::write(dir.join("target_vocabulary.txt"), "tokens").unwrap();
        assert!(model_complete(&dir, ModelKind::Marian));
        std::fs::remove_file(dir.join("target.spm")).unwrap();
        assert!(!Translators::with_dirs(Some(dir.clone()), None).has("en"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
