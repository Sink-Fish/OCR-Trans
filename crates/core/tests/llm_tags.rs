//! High-quality tier regression (0.5.5): Qwen3 echoed the prompt's per-paragraph "(英语)" tag in the target
//! language, so translations came out as "（中文）检查". Runs the real model on short UI / terminal style paragraphs
//! (the kind that showed the prefix on the user's screen) and checks both the raw answer (prompt) and the parsed
//! translations (parser); skipped when models/qwen3-1.7b is not installed.
#![cfg(feature = "llm")]

use std::path::PathBuf;

use ocrtrans_core::llm::{build_prompt, has_lang_tag, parse_answer, LlmTranslator};

const PARAS: [(&str, &str); 12] = [
    ("en", "Docs"),
    ("en", "Check"),
    ("en", "Short progress entry."),
    ("en", "run_product_check.py check: passed. Testing on a real device: NOT_RUN."),
    ("en", "Nothing committed. I have not bumped the version, packaged or installed anything."),
    ("en", "UI layout change is settled; the regression tests and the product spec now match."),
    ("en", "product-experience-spec.md"),
    ("en", "Settings"),
    ("en", "Save"),
    ("ja", "設定を保存"),
    ("ja", "キャンセル"),
    ("en", "Error: The file could not be saved because the disk is full."),
];

#[test]
fn qwen_answers_carry_no_language_tag() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models/qwen3-1.7b/model.gguf");
    if !path.exists() {
        eprintln!("models/qwen3-1.7b missing — skipping (run scripts/fetch_models.sh --with-llm)");
        return;
    }
    let llm = LlmTranslator::load(&path).expect("model loads");
    let mut bad = vec![];
    // the whole screen in one request, and a chunk with the rest of the screen as context
    for (chunk, offset) in [(&PARAS[..], 0), (&PARAS[6..], 6)] {
        let raw = llm.generate(&build_prompt(chunk, &PARAS, offset, &[]), 1024).expect("generate");
        eprintln!("raw answer:\n{raw}");
        for line in raw.lines().filter(|l| l.trim_start().starts_with('[')) {
            let text = &line[line.find(']').unwrap() + 1..];
            if has_lang_tag(text) {
                bad.push(format!("raw: {line}"));
            }
        }
        for ((_, src), t) in chunk.iter().zip(parse_answer(&raw, chunk)) {
            match t {
                Some(t) if has_lang_tag(&t) => bad.push(format!("parsed: {src:?} -> {t:?}")),
                None => bad.push(format!("parsed: {src:?} -> no answer")),
                _ => {}
            }
        }
    }
    assert!(bad.is_empty(), "language tags / missing answers: {bad:#?}");
}
