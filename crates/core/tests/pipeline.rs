//! End-to-end tests on the images in testdata/. They need the models fetched by scripts/fetch_models.sh
//! (and runtime/libonnxruntime.*); when those are missing the tests print a note and pass.

use std::path::PathBuf;

use ocrtrans_core::{Lang, Pipeline, TranslateOptions};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn pipeline() -> Option<Pipeline> {
    let models = root().join("models");
    if !models.join("ocr/rec.onnx").exists() || ocrtrans_core::ocr::find_ort_lib().is_none() {
        eprintln!("models/ or ONNX Runtime missing — skipping (run scripts/fetch_models.sh)");
        return None;
    }
    Some(Pipeline::load(&models).expect("pipeline loads"))
}

fn image(name: &str) -> image::RgbImage {
    image::open(root().join("testdata").join(name)).expect("test image").to_rgb8()
}

fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}

#[test]
fn english_ui_ocr_and_translation() {
    let Some(mut p) = pipeline() else { return };
    let mut r = p.recognize(&image("en_dark_ui.png")).unwrap();
    let texts: Vec<&str> = r.paragraphs.iter().map(|p| p.text.as_str()).collect();
    assert_eq!(texts.len(), 5, "{texts:?}");
    assert_eq!(texts[0], "Settings");
    assert!(texts[4].starts_with("Error: The file could not be saved"));
    assert!(r.paragraphs.iter().all(|p| p.lang == Lang::En));
    assert_eq!(r.paragraphs[0].bg, "#1e1f22");
    if root().join("models/opus-mt-en-zh/model.bin").exists() {
        p.translate(&mut r).unwrap();
        assert_eq!(r.engine, "nmt");
        assert!(r.paragraphs.iter().all(|p| p.translation.as_deref().is_some_and(has_cjk)));
        assert!(r.paragraphs[4].translation.as_ref().unwrap().contains("磁盘"));
    }
}

#[test]
fn paragraphs_and_reading_order() {
    let Some(mut p) = pipeline() else { return };
    let r = p.recognize(&image("en_article.png")).unwrap();
    assert_eq!(r.paragraphs.len(), 4);
    assert!(r.paragraphs[1].lines.len() >= 2, "multi-line paragraph kept together");
    assert!(r.paragraphs.windows(2).all(|w| w[0].bbox[1] < w[1].bbox[1]), "top-to-bottom order");
    // quads are inside the image
    for para in &r.paragraphs {
        for l in &para.lines {
            for [x, y] in l.quad {
                assert!(x >= 0.0 && y >= 0.0 && x < r.width as f32 && y < r.height as f32);
            }
        }
    }
}

#[test]
fn japanese_detection_and_translation() {
    let Some(mut p) = pipeline() else { return };
    let mut r = p.recognize(&image("ja_ui.png")).unwrap();
    assert_eq!(r.paragraphs.len(), 4);
    assert!(r.paragraphs.iter().all(|p| p.lang == Lang::Ja));
    assert!(r.paragraphs[3].text.contains("ダウンロード"));
    if root().join("models/m2m100_418m/model.bin").exists() {
        p.translate(&mut r).unwrap();
        assert!(r.paragraphs[3].translation.as_ref().unwrap().contains("下载"));
    } else {
        p.translate(&mut r).unwrap();
        assert!(r.warnings.iter().any(|w| w.contains("日")), "missing-model warning: {:?}", r.warnings);
    }
}

#[test]
fn glossary_respected_by_nmt() {
    let Some(mut p) = pipeline() else { return };
    if !root().join("models/opus-mt-en-zh/model.bin").exists() {
        return;
    }
    let mut r = p.recognize(&image("en_dark_ui.png")).unwrap();
    let opts = TranslateOptions { quality: false, glossary: vec![("subscription".into(), "会员".into()), ("Settings".into(), "偏好设置".into())] };
    p.translate_with(&mut r, &opts).unwrap();
    assert_eq!(r.paragraphs[0].translation.as_deref(), Some("偏好设置"));
    assert!(r.paragraphs[3].translation.as_ref().unwrap().contains("会员"));
}

#[test]
fn inpainting_cleans_gradient() {
    let Some(mut p) = pipeline() else { return };
    let img = image("en_gradient.png");
    let mut r = p.recognize(&img).unwrap();
    for para in &mut r.paragraphs {
        para.translation = Some("测试".into());
    }
    let clean = Pipeline::clean_background(&img, &mut r);
    assert!(r.paragraphs.iter().all(|p| p.fill.as_deref() == Some("inpaint")), "{:?}", r.paragraphs.iter().map(|p| &p.fill).collect::<Vec<_>>());
    // the white title text is gone: no near-white pixels left inside its box
    let [x, y, w, h] = r.paragraphs[0].bbox;
    let white = (y as u32..(y + h) as u32)
        .flat_map(|yy| (x as u32..(x + w) as u32).map(move |xx| (xx, yy)))
        .filter(|&(xx, yy)| clean.get_pixel(xx, yy).0.iter().all(|&c| c > 230))
        .count();
    assert_eq!(white, 0);
}

#[test]
fn empty_and_tiny_images_do_not_panic() {
    let Some(mut p) = pipeline() else { return };
    for (w, h) in [(0, 0), (1, 1), (3, 200), (200, 3), (40, 40)] {
        let r = p.recognize(&image::RgbImage::from_pixel(w, h, image::Rgb([255, 255, 255]))).unwrap();
        assert!(r.paragraphs.is_empty());
    }
}

#[test]
fn tightly_cropped_text_is_detected_at_source_coordinates() {
    let Some(mut p) = pipeline() else { return };
    let source = image("en_dark_ui.png");
    // The title with one pixel of margin around its visible strokes.
    let crop = image::imageops::crop_imm(&source, 60, 40, 80, 23).to_image();
    let r = p.recognize(&crop).unwrap();
    assert_eq!(r.plain_text(), "Settings");
    let line = &r.paragraphs[0].lines[0];
    for [x, y] in line.quad {
        assert!(x >= 0.0 && x < r.width as f32 && y >= 0.0 && y < r.height as f32);
    }
}

#[test]
fn small_text_is_detected_in_a_4k_capture() {
    let Some(mut p) = pipeline() else { return };
    let source = image("en_small.png");
    let mut canvas = image::RgbImage::from_pixel(3840, 2160, image::Rgb([255, 255, 255]));
    image::imageops::replace(&mut canvas, &source, 0, 0);
    let r = p.recognize(&canvas).unwrap();
    let text = r.plain_text();
    assert!(text.contains("By clicking Continue"), "{text:?}");
    assert!(text.contains("Free shipping on orders"), "{text:?}");
}

#[cfg(feature = "nmt")]
#[test]
fn empty_automatic_translation_keeps_original_pixels() {
    let Some(mut p) = pipeline() else { return };
    if !p.translators.has("en") {
        return;
    }
    let source = image("en_dark_ui.png");
    let crop = image::imageops::crop_imm(&source, 60, 40, 80, 23).to_image();
    let mut r = p.recognize(&crop).unwrap();
    assert_eq!(r.plain_text(), "Settings");
    // Use a glossary hit to reproduce an empty backend result without relying on model sampling.
    let opts = TranslateOptions { quality: false, glossary: vec![("Settings".into(), String::new())] };
    p.translate_with(&mut r, &opts).unwrap();
    assert!(r.paragraphs[0].translation.is_none());
    assert!(!r.warnings.is_empty());
    assert_eq!(Pipeline::clean_background(&crop, &mut r), crop);
}

#[cfg(feature = "llm")]
#[test]
fn quality_tier_if_installed() {
    let Some(mut p) = pipeline() else { return };
    if !p.has_llm() {
        return;
    }
    let mut r = p.recognize(&image("en_small.png")).unwrap();
    p.translate_with(&mut r, &TranslateOptions { quality: true, glossary: vec![] }).unwrap();
    assert!(r.engine.starts_with("llm"), "engine {}", r.engine);
    assert!(r.paragraphs.iter().all(|p| p.translation.as_deref().is_some_and(has_cjk)));
}
