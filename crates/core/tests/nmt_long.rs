//! A single sentence longer than CT2's former 1024-token input limit must retain its final glossary term.
#![cfg(feature = "nmt")]

#[test]
fn long_sentence_keeps_its_final_term() {
    let models = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models");
    let mut translators = ocrtrans_core::Translators::new(&models);
    if !translators.has("en") {
        eprintln!("English NMT model missing — skipping long sentence integration test");
        return;
    }
    let mut source: String = (0..100)
        .map(|i| format!("The application saves document number {i} and keeps all folders available, "))
        .collect();
    source.push_str("FinishMarker");
    let out = translators.translate_paragraphs("en", &[source], &[("FinishMarker".into(), "结束标记".into())]).unwrap();
    assert_eq!(out.len(), 1);
    assert!(out[0].contains("结束标记"), "final source term was lost: {}", out[0]);
}
