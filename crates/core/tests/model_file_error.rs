//! A tiny valid GGUF container reaches native file opening without downloading model weights.
//! Its missing model metadata must return an error, never terminate the process. In particular,
//! Windows llama/ggml DLLs must share the CRT that owns the FILE*/file descriptor.
#![cfg(feature = "llm")]

#[test]
fn invalid_model_metadata_returns_an_error_without_crashing() {
    let path = std::env::temp_dir().join(format!(
        "ocrtrans-empty-model-{}-{}.gguf",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    // GGUF v3: valid container header, zero tensors and zero metadata key/value pairs.
    let mut bytes = b"GGUF".to_vec();
    bytes.extend_from_slice(&3_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u64.to_le_bytes());
    bytes.extend_from_slice(&0_u64.to_le_bytes());
    std::fs::write(&path, bytes).unwrap();
    ocrtrans_core::llm::set_cpu_only(true);
    let result = ocrtrans_core::llm::LlmTranslator::load(&path);
    std::fs::remove_file(path).unwrap();
    assert!(result.is_err(), "a GGUF without architecture or tensors is not a model");
}
