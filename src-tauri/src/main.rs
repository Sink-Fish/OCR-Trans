// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--capture-worker")) {
        std::process::exit(if ocr_trans_lib::capture_worker::run().is_ok() { 0 } else { 1 });
    }
    // llama.cpp's Metal backend keeps the model's memory wired with "residency sets" and, for that, runs a
    // background thread that wakes every 5 ms for as long as the process lives (after the first high-quality
    // translation, even when the model has been released). Without residency sets generation measured the same
    // (≈85 tok/s on an M3 Pro) and the app is idle when it is idle. Set before any thread starts.
    if cfg!(target_os = "macos") && std::env::var_os("GGML_METAL_NO_RESIDENCY").is_none() {
        std::env::set_var("GGML_METAL_NO_RESIDENCY", "1");
    }
    #[cfg(feature = "llm")]
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--quality-worker")) {
        let status = ocr_trans_lib::quality_worker::run();
        ocrtrans_core::exit_now(if status.is_ok() { 0 } else { 1 });
    }
    ocr_trans_lib::run()
}
