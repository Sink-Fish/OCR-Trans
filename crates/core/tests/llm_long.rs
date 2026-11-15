//! High-quality tier on long text (0.6.1): a text-heavy screen must come back translated paragraph by paragraph —
//! answers that copy the English are not accepted (they used to be shown as the "translation"), and the speed is
//! printed for comparison. Runs the real model; skipped when models/qwen3-1.7b is not installed.
#![cfg(feature = "llm")]

use std::path::PathBuf;

use ocrtrans_core::llm::{untranslated, LlmTranslator};

const ARTICLE: [&str; 9] = [
    "Release notes for the spring update",
    "This update focuses on reliability. The background sync engine was rewritten so that a lost network connection no longer stops the queue: files are retried with an increasing delay, and the status bar now shows how many items are still waiting.",
    "Large folders open faster. Instead of reading every file when a folder is opened, the app now reads the first page of results and loads the rest while you scroll, which makes folders with tens of thousands of files usable on older laptops.",
    "Search understands more. You can filter by file type, by the person who last changed a file, and by date ranges such as last week or the past three months. Saved searches appear in the sidebar and update automatically.",
    "Sharing links can now expire. When you create a link, choose whether it stops working after a day, a week or a month, and whether people who open it may download the file or only view it in the browser.",
    "Accessibility improvements include full keyboard navigation in every dialog, better contrast in the dark theme, and descriptions for all icons so that screen readers announce what each button does.",
    "Known issues: on some systems the first sync after the update takes longer than usual because the local index is rebuilt. This happens only once. If a sync seems stuck for more than an hour, restart the app.",
    "Thank you to everyone who sent feedback during the beta. Many of the changes above started as suggestions in the community forum, and we read every post even when we cannot reply to all of them.",
    "Settings",
];

#[test]
fn long_screen_is_translated_paragraph_by_paragraph() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models/qwen3-1.7b/model.gguf");
    if !path.exists() {
        eprintln!("models/qwen3-1.7b missing — skipping (run scripts/fetch_models.sh --with-llm)");
        return;
    }
    // OCRTRANS_TEST_CPU_ONLY=1: the CPU path (no GPU device in the model's device list), as with 设置 → 仅 CPU
    ocrtrans_core::llm::set_cpu_only(std::env::var_os("OCRTRANS_TEST_CPU_ONLY").is_some());
    let llm = LlmTranslator::load(&path).expect("model loads");
    eprintln!("running on {:?}", ocrtrans_core::llm::running_on());
    let paras: Vec<(&str, &str)> = ARTICLE.iter().map(|t| ("en", *t)).collect();
    let t0 = std::time::Instant::now();
    let out = llm.translate(&paras, &[]).expect("translate");
    let secs = t0.elapsed().as_secs_f64();
    let chars: usize = ARTICLE.iter().map(|t| t.len()).sum();
    eprintln!("{} paragraphs, {chars} characters in {secs:.1} s; notes: {:?}", ARTICLE.len(), out.notes);
    let mut missing = vec![];
    for ((_, src), t) in paras.iter().zip(&out.translations) {
        eprintln!("  {src:.40}… → {:?}", t.as_deref().map(|t| t.chars().take(30).collect::<String>()));
        match t {
            Some(t) => assert!(!untranslated(src, t), "copied, not translated: {t}"),
            None => missing.push(*src),
        }
    }
    // on a normal machine the whole screen fits the time limits; at most the one-word heading may be dropped
    assert!(missing.iter().all(|s| s.split_whitespace().count() <= 1), "left to the fast engine: {missing:?}");
}
