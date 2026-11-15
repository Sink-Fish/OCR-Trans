//! CLI for the P0 pipeline benchmark.
//!
//!   ocrtrans-cli [--models DIR] [--no-translate] [--quality] [--term SRC=DST]... [--repeat N] <image>...
//!   ocrtrans-cli [--models DIR] --capture X,Y,W,H        (global logical coords)
//!   ocrtrans-cli [--models DIR] --capture-screen

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use ocrtrans_core::{capture, Pipeline};

fn main() -> Result<()> {
    let mut models = PathBuf::from(std::env::var("OCRTRANS_MODELS").unwrap_or_else(|_| "models".into()));
    let mut translate = true;
    let mut repeat = 1usize;
    let mut inputs: Vec<String> = vec![];
    let mut region: Option<Vec<i32>> = None;
    let mut full = false;
    let mut save: Option<PathBuf> = None;
    let mut opts = ocrtrans_core::TranslateOptions::default();
    // exit-path experiments (regression tests): "leak" = keep the pipeline alive at exit like the app's global
    // state does; "app" = what the app does since 0.5.2 (free the pipeline, then leave without C++ static teardown)
    let mut exit_mode = String::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--models" => models = args.next().context("--models DIR")?.into(),
            "--no-translate" => translate = false,
            "--repeat" => repeat = args.next().context("--repeat N")?.parse()?,
            "--capture" => {
                region = Some(args.next().context("--capture X,Y,W,H")?.split(',').map(|v| v.trim().parse()).collect::<Result<_, _>>()?)
            }
            "--capture-screen" => full = true,
            "--quality" => opts.quality = true,
            "--term" => {
                let t = args.next().context("--term SRC=DST")?;
                let (a, b) = t.split_once('=').context("--term SRC=DST")?;
                opts.glossary.push((a.into(), b.into()));
            }
            "--exit-mode" => exit_mode = args.next().context("--exit-mode leak|app")?,
            "--save" => save = Some(args.next().context("--save PATH")?.into()),
            "-h" | "--help" => {
                eprintln!("usage: ocrtrans-cli [--models DIR] [--no-translate] [--quality] [--term SRC=DST]... [--repeat N] [--save PNG] (<image>... | --capture X,Y,W,H | --capture-screen)");
                return Ok(());
            }
            _ if a.starts_with('-') => bail!("unknown option {a}; see --help"),
            _ => inputs.push(a),
        }
    }

    if repeat == 0 {
        bail!("--repeat must be at least 1");
    }
    if let Some(r) = &region {
        if r.len() != 4 || r[2] <= 0 || r[3] <= 0 {
            bail!("--capture needs X,Y,W,H with positive width and height");
        }
    }
    if !matches!(exit_mode.as_str(), "" | "leak" | "app") {
        bail!("--exit-mode must be leak or app");
    }
    if region.is_none() && !full && inputs.is_empty() {
        bail!("no input; see --help");
    }

    let t = Instant::now();
    let mut p = Pipeline::load(&models)?;
    let load_ms = t.elapsed().as_secs_f64() * 1000.0;
    eprintln!("models loaded in {load_ms:.0} ms");

    let mut images = vec![];
    if let Some(r) = region {
        let t = Instant::now();
        let img = capture::capture_region(r[0], r[1], r[2] as u32, r[3] as u32)?;
        eprintln!("capture: {:.0} ms ({}x{})", t.elapsed().as_secs_f64() * 1000.0, img.width(), img.height());
        images.push(("capture".to_string(), image::DynamicImage::ImageRgba8(img).to_rgb8()));
    }
    if full {
        let t = Instant::now();
        let (_, img) = capture::capture_at(0, 0)?;
        eprintln!("capture: {:.0} ms ({}x{})", t.elapsed().as_secs_f64() * 1000.0, img.width(), img.height());
        images.push(("screen".to_string(), image::DynamicImage::ImageRgba8(img).to_rgb8()));
    }
    if let (Some(path), Some((_, img))) = (&save, images.first()) {
        img.save(path)?;
    }
    for i in &inputs {
        images.push((i.clone(), image::open(i).with_context(|| format!("open {i}"))?.to_rgb8()));
    }
    if images.is_empty() {
        bail!("no input; see --help");
    }

    let mut all = vec![];
    for (name, img) in images {
        let mut res = None;
        for k in 0..repeat {
            let mut r = p.recognize(&img)?;
            if translate {
                p.translate_with(&mut r, &opts)?;
            }
            if k == 0 {
                r.timings.load_ms = (load_ms * 10.0).round() / 10.0;
            }
            eprintln!(
                "{name} [{}x{}] run {k}: det {:.0} ms, rec {:.0} ms, layout {:.1} ms, translate {:.0} ms ({}), total {:.0} ms",
                r.width, r.height, r.timings.det_ms, r.timings.rec_ms, r.timings.layout_ms, r.timings.translate_ms, r.engine, r.timings.total_ms
            );
            res = Some(r);
        }
        all.push(serde_json::json!({ "input": name, "result": res }));
    }
    println!("{}", serde_json::to_string_pretty(&all)?);
    match exit_mode.as_str() {
        "leak" => {
            std::mem::forget(p);
            std::process::exit(0);
        }
        "app" => {
            drop(p);
            ocrtrans_core::exit_now(0);
        }
        _ => {}
    }
    Ok(())
}
