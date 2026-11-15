//! Regression test for the 0.5.1 quit crash: exiting while the llama.cpp (Metal) model is still referenced aborts
//! in ggml's static teardown (`GGML_ASSERT([rsets->data count] == 0)`). The app now drops the pipeline and leaves
//! with `exit_now` (`_exit`). This test runs the CLI in a child process with the same exit path.

use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(mode: &str, quality: bool) -> Option<std::process::ExitStatus> {
    let models = root().join("models");
    if !models.join("ocr/rec.onnx").exists() || (quality && !models.join("qwen3-1.7b/model.gguf").exists()) {
        eprintln!("models missing — skipping");
        return None;
    }
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ocrtrans-cli"));
    cmd.current_dir(root()).arg("--models").arg(&models).args(["--exit-mode", mode]);
    if quality {
        cmd.arg("--quality");
    }
    cmd.arg(root().join("testdata/en_small.png"));
    Some(cmd.output().expect("run cli").status)
}

#[test]
fn app_exit_path_is_clean_with_llm_loaded() {
    if let Some(st) = run("app", true) {
        assert!(st.success(), "exit status {st:?}");
    }
}

#[test]
fn app_exit_path_is_clean_with_fast_engine() {
    if let Some(st) = run("app", false) {
        assert!(st.success(), "exit status {st:?}");
    }
}

/// Documents the hazard the app avoids (not a requirement — passes either way, prints what happened).
#[test]
fn leaking_the_llm_at_exit_is_what_crashed() {
    if let Some(st) = run("leak", true) {
        eprintln!("leak-at-exit status: {st:?} (SIGABRT/134 on llama.cpp Metal builds)");
    }
}
