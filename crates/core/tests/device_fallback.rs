//! Exercise a persisted GPU failure against the real model loader (Metal and Vulkan must both honor it).
#![cfg(feature = "llm")]

#[test]
fn remembered_gpu_failure_uses_the_cpu() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models/qwen3-1.7b/model.gguf");
    if !path.exists() {
        eprintln!("Qwen3 model missing — skipping device fallback integration test");
        return;
    }
    ocrtrans_core::llm::set_cpu_only(false);
    ocrtrans_core::llm::set_gpu_blocked(Some("previous GPU failure".into()));
    let llm = ocrtrans_core::llm::LlmTranslator::load(&path).expect("CPU fallback loads");
    assert!(!llm.on_gpu());
    assert!(ocrtrans_core::llm::running_on().is_some_and(|device| device.starts_with("CPU")));
    // Check an actual token generation as well as the status label.
    let prompt = ocrtrans_core::llm::build_prompt(&[("en", "Save")], &[("en", "Save")], 0, &[]);
    let answer = llm.generate(&prompt, 32).expect("CPU fallback generates");
    assert!(!answer.trim().is_empty());
    drop(llm);
    ocrtrans_core::llm::set_gpu_blocked(None);
}
