fn main() {
    #[cfg(target_os = "macos")]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rerun-if-changed=src/macos_capture.m");
        cc::Build::new().file("src/macos_capture.m").flag("-fobjc-arc").compile("macos_capture");
        println!("cargo:rustc-link-lib=framework=ScreenCaptureKit");
    }
    // Windows: our manifest (Tauri's default + the UTF-8 active code page, see windows/app.manifest)
    let windows = tauri_build::WindowsAttributes::new().app_manifest(include_str!("windows/app.manifest"));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows)).expect("failed to run tauri-build");
}
