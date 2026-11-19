//! PP-OCRv5 text detection (DBNet) + recognition (SVTR/CTC) on ONNX Runtime.
//!
//! Pre/post-processing follows RapidOCR / PaddleOCR defaults:
//! det: BGR, ImageNet mean/std, side multiple of 32, DB threshold 0.3, box thresh 0.6, unclip 1.5
//! rec: BGR, (x/255-0.5)/0.5, height 48, CTC greedy decode with ppocrv5_dict + space.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{ensure, Context, Result};

use image::{imageops, GrayImage, Luma, Rgb, RgbImage};
use imageproc::geometric_transformations::{warp_into, Border, Interpolation, Projection};
use imageproc::point::Point;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::environment::Environment;
use ort::value::Tensor;
use serde::Serialize;

/// Quadrilateral in source-image pixel coords: TL, TR, BR, BL.
pub type Quad = [[f32; 2]; 4];

#[derive(Debug, Clone, Serialize)]
pub struct TextLine {
    pub quad: Quad,
    pub text: String,
    pub score: f32,
}

type IndexedRecognition = (usize, (String, f32));

pub struct OcrOptions {
    /// Longest side of the resized image before detector padding (larger retains more small text).
    pub det_max_side: u32,
    pub det_thresh: f32,
    pub box_thresh: f32,
    pub unclip_ratio: f32,
    pub min_rec_score: f32,
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self { det_max_side: 2240, det_thresh: 0.3, box_thresh: 0.5, unclip_ratio: 1.6, min_rec_score: 0.5 }
    }
}

pub struct Ocr {
    _env: Environment,
    det: Session,
    rec: Vec<Session>,
    dict: Vec<String>,
    pub opts: OcrOptions,
}

/// Platform file name of the ONNX Runtime shared library.
pub const ORT_LIB: &str = if cfg!(target_os = "windows") {
    "onnxruntime.dll"
} else if cfg!(target_os = "macos") {
    "libonnxruntime.dylib"
} else {
    "libonnxruntime.so"
};

/// Locate the ONNX Runtime shared library: $ORT_DYLIB_PATH, next to the executable,
/// the macOS bundle's Resources/Frameworks, or ./runtime (walking up from cwd and exe).
pub fn find_ort_lib() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ORT_DYLIB_PATH") {
        return Some(PathBuf::from(p));
    }
    let mut roots = vec![];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            roots.push(d.to_path_buf());
            roots.push(d.join("../Resources"));
            roots.push(d.join("../Resources/runtime"));
            roots.push(d.join("../Frameworks"));
            roots.extend(d.ancestors().map(|a| a.join("runtime")));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.extend(cwd.ancestors().map(|a| a.join("runtime")));
    }
    #[cfg(target_os = "macos")]
    let name = if objc2::available!(macos = 14.0) { ORT_LIB } else { "libonnxruntime-ventura.dylib" };
    #[cfg(not(target_os = "macos"))]
    let name = ORT_LIB;
    roots.into_iter().map(|r| r.join(name)).find(|p| p.exists())
}

static ORT_ENV: OnceLock<std::result::Result<Environment, String>> = OnceLock::new();

fn ort_env() -> Result<Environment> {
    ORT_ENV
        .get_or_init(|| {
            let lib = find_ort_lib().ok_or_else(|| format!("{ORT_LIB} not found (set ORT_DYLIB_PATH or run scripts/fetch_models.sh)"))?;
            ort::init_from(&lib)
                .map_err(|e| {
                    // the official onnxruntime.dll links the Visual C++ runtime dynamically (MSVCP140 / VCRUNTIME140)
                    let hint = if cfg!(windows) {
                        " — 可能缺少 Microsoft Visual C++ 2015–2022 运行库 (x64)：https://aka.ms/vs/17/release/vc_redist.x64.exe"
                    } else {
                        ""
                    };
                    format!("loading {}: {e}{hint}", lib.display())
                })?
                .with_name("ocrtrans")
                .build()
                .map_err(|e| e.to_string())
        })
        .clone()
        .map_err(|e| anyhow::anyhow!(e))
}

fn session(env: &Environment, path: &Path, threads: usize, shared: Option<&ort::session::builder::PrepackedWeights>) -> Result<Session> {
    let e = |e: ort::Error<_>| anyhow::anyhow!("{e}");
    let mut b = Session::builder(env)
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .with_optimization_level(GraphOptimizationLevel::All)
        .map_err(e)?
        .with_intra_threads(threads)
        .map_err(e)?
        // worker threads block instead of spinning while they wait for work: OCR runs once per capture, and a spinning
        // pool keeps cores busy after every run while the app sits idle
        .with_intra_op_spinning(false)
        .map_err(e)?
        .with_inter_op_spinning(false)
        .map_err(e)?;
    // Inputs change shape on every call (line widths, image sizes). With ORT's default arena allocator and
    // memory-pattern cache, every new shape leaves buffers behind and resident memory keeps growing over a
    // session's lifetime; the plain device allocator returns memory after each run.
    use ort::memory::{AllocationDevice, AllocatorType, MemoryInfo, MemoryType};
    let plain = MemoryInfo::new(AllocationDevice::CPU, 0, AllocatorType::Device, MemoryType::Default).map_err(|e| anyhow::anyhow!("{e}"))?;
    b = b.with_allocator(plain).map_err(e)?.with_memory_pattern(false).map_err(e)?;
    if let Some(w) = shared {
        // the parallel recogniser sessions share one copy of the prepacked weights
        b = b.with_prepacked_weights(w).map_err(e)?;
    }
    b.commit_from_file(path).map_err(|e| anyhow::anyhow!("loading {}: {e}", path.display()))
}

impl Ocr {
    /// `dir` must contain det.onnx, rec.onnx, dict.txt
    pub fn load(dir: &Path) -> Result<Self> {
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let env = ort_env()?;
        let det = session(&env, &dir.join("det.onnx"), cores.min(8), None)?;
        // several small recogniser sessions run lines in parallel
        // measured on an M-series 12-core Mac: one line per run on cores/2 sessions x 2 threads is ~2.2x faster
        // than one 8-thread session with batches of 8 (no padding, better core utilisation)
        // keep a couple of cores free so the UI stays smooth while OCR runs
        let mut workers = (cores.saturating_sub(2) / 2).clamp(1, 5);
        let mut rec_threads = 2;
        if let Ok(v) = std::env::var("OCRTRANS_REC_WORKERS") {
            // "workers,threads" — tuning knob
            let v: Vec<usize> = v.split(',').filter_map(|x| x.parse().ok()).collect();
            if v.len() == 2 {
                (workers, rec_threads) = (v[0].max(1), v[1].max(1));
            }
        }
        let shared = ort::session::builder::PrepackedWeights::new();
        let rec = (0..workers).map(|_| session(&env, &dir.join("rec.onnx"), rec_threads, Some(&shared))).collect::<Result<Vec<_>>>()?;
        // dictionary: dict.txt if present, else the `character` metadata RapidOCR embeds in rec.onnx
        let raw = match std::fs::read_to_string(dir.join("dict.txt")) {
            Ok(s) => s,
            Err(_) => rec[0]
                .metadata()
                .ok()
                .and_then(|m| m.custom("character"))
                .context("no dict.txt and no `character` metadata in rec.onnx")?,
        };
        // index 0 = CTC blank, then the dictionary, then the space char
        let mut dict = vec![String::new()];
        dict.extend(raw.lines().map(|l| l.trim_end_matches('\r').to_string()));
        dict.push(" ".into());
        Ok(Self { _env: env, det, rec, dict, opts: OcrOptions::default() })
    }

    pub fn detect(&mut self, img: &RgbImage) -> Result<Vec<Quad>> {
        let (w, h) = img.dimensions();
        if w == 0 || h == 0 {
            return Ok(vec![]);
        }
        let max_side = std::env::var("OCRTRANS_DET_MAX")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|&v| v > 0)
            .unwrap_or(self.opts.det_max_side.max(1));
        // A stride-sized resize would distort a narrow selection. Give its glyphs some
        // space at the edges, and pad instead of stretching the text to a multiple of 32.
        const STRIDE: u32 = 32;
        let margin = if w.min(h) < 64 { 16 } else { 0 };
        let preferred_scale = if w.min(h) < STRIDE { 2.0 } else { 1.0 };
        let scale = (max_side as f32 / w.max(h) as f32).min(preferred_scale);
        let resized_side = |side: u32| {
            let scaled = side as f32 * scale;
            if margin == 0 {
                ((scaled / STRIDE as f32).round() as u32).max(1) * STRIDE
            } else {
                (scaled.round() as u32).max(1)
            }
        };
        let (sw, sh) = (resized_side(w), resized_side(h));
        let rw = (sw + 2 * margin).div_ceil(STRIDE) * STRIDE;
        let rh = (sh + 2 * margin).div_ceil(STRIDE) * STRIDE;
        let resized = if (sw, sh) == (w, h) {
            Cow::Borrowed(img)
        } else {
            Cow::Owned(imageops::resize(img, sw, sh, imageops::FilterType::Triangle))
        };
        let (rw_, rh_) = (rw as usize, rh as usize);
        let mean = [0.485f32, 0.456, 0.406];
        let std = [0.229f32, 0.224, 0.225];
        let mut data = vec![0f32; 3 * rw_ * rh_];
        for y in 0..rh {
            let sy = y.saturating_sub(margin).min(sh - 1);
            for x in 0..rw {
                let sx = x.saturating_sub(margin).min(sw - 1);
                let p = resized.get_pixel(sx, sy);
                let i = y as usize * rw_ + x as usize;
                // Replicate edge colors in the margin, including in dark screenshots.
                for c in 0..3 {
                    let v = p[2 - c] as f32 / 255.0;
                    data[c * rw_ * rh_ + i] = (v - mean[c]) / std[c];
                }
            }
        }
        let input = Tensor::from_array(([1usize, 3, rh_, rw_], data))?;
        let outputs = self.det.run(ort::inputs![input])?;
        ensure!(outputs.len() > 0, "OCR detection model returned no output");
        let (shape, prob) = outputs[0].try_extract_tensor::<f32>()?;
        ensure!(shape.as_ref() == [1, 1, rh as i64, rw as i64] && prob.len() == rw_ * rh_,
            "OCR detection model has incompatible output shape: {shape:?}");
        let prob: Vec<f32> = prob.to_vec();
        drop(outputs);

        let bitmap = GrayImage::from_fn(rw, rh, |x, y| {
            Luma([if prob[y as usize * rw_ + x as usize] > self.opts.det_thresh { 255 } else { 0 }])
        });
        if let Ok(dbg) = std::env::var("OCRTRANS_DEBUG_DET") {
            let _ = GrayImage::from_fn(rw, rh, |x, y| Luma([(prob[y as usize * rw_ + x as usize] * 255.0) as u8])).save(dbg);
        }
        let sx = w as f32 / sw as f32;
        let sy = h as f32 / sh as f32;
        let mut quads = Vec::new();
        let dbg = std::env::var("OCRTRANS_DEBUG_DET").is_ok();
        for c in imageproc::contours::find_contours::<i32>(&bitmap) {
            if c.border_type != imageproc::contours::BorderType::Outer || c.points.len() < 4 {
                continue;
            }
            let rect = imageproc::geometry::min_area_rect(&c.points);
            if dbg {
                eprintln!("contour n={} rect={:?} score={:.2}", c.points.len(), rect, box_score(&prob, rw_, rh_, &c.points));
            }
            let q = order_quad(rect.map(|p| [p.x as f32, p.y as f32]));
            let (qw, qh) = quad_size(&q);
            if qw.min(qh) < 3.0 {
                continue;
            }
            if box_score(&prob, rw_, rh_, &c.points) < self.opts.box_thresh {
                continue;
            }
            // unclip: offset distance = area * ratio / perimeter, applied to the rectangle
            let d = qw * qh * self.opts.unclip_ratio / (2.0 * (qw + qh));
            let q = expand_quad(&q, d);
            let (qw, qh) = quad_size(&q);
            if qw.min(qh) < 5.0 {
                continue;
            }
            let q = q.map(|[x, y]| [
                ((x - margin as f32) * sx).clamp(0.0, w as f32 - 1.0),
                ((y - margin as f32) * sy).clamp(0.0, h as f32 - 1.0),
            ]);
            let (qw, qh) = quad_size(&q);
            if qw >= 1.0 && qh >= 1.0 {
                quads.push(q);
            }
        }
        Ok(reading_order(quads))
    }

    pub fn recognize(&mut self, img: &RgbImage, quads: &[Quad]) -> Result<Vec<(String, f32)>> {
        let batch_size: usize = std::env::var("OCRTRANS_REC_BATCH").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
        if quads.is_empty() {
            return Ok(vec![]);
        }
        // 1. every line warped straight to the recogniser height (no full-size intermediate crop), in parallel
        let groups = par_map(quads, |q| recognition_images(img, q));
        let mut lines = Vec::new();
        let mut owners = Vec::new();
        for (id, parts) in groups.into_iter().enumerate() {
            for (part, space_before) in parts {
                lines.push(part);
                owners.push((id, space_before));
            }
        }
        // 2. batches of similar width so little padding is computed
        let mut order: Vec<usize> = (0..lines.len()).collect();
        // widest first, so the long lines don't end up last on one worker
        order.sort_by_key(|&i| std::cmp::Reverse(lines[i].width()));
        let batches: Vec<&[usize]> = order.chunks(batch_size.max(1)).collect();
        // 3. batches spread over the recogniser sessions, one thread per session
        let next = std::sync::atomic::AtomicUsize::new(0);
        let dict = &self.dict;
        let lines = &lines;
        let batches = &batches;
        let results: Vec<Result<Vec<IndexedRecognition>>> = std::thread::scope(|sc| {
            let handles: Vec<_> = self
                .rec
                .iter_mut()
                .map(|sess| {
                    let next = &next;
                    sc.spawn(move || -> Result<Vec<IndexedRecognition>> {
                        let mut out = vec![];
                        loop {
                            let b = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            let Some(batch) = batches.get(b) else { break };
                            out.extend(run_rec_batch(sess, lines, batch, dict)?);
                        }
                        Ok(out)
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap_or_else(|_| Err(anyhow::anyhow!("recogniser thread panicked")))).collect()
        });
        let mut parts = vec![(String::new(), 0f32); lines.len()];
        for r in results {
            for (i, t) in r? {
                parts[i] = t;
            }
        }
        let mut texts = vec![(String::new(), 0f32); quads.len()];
        let mut lengths = vec![0usize; quads.len()];
        for ((part, score), (id, space)) in parts.into_iter().zip(owners) {
            let (text, confidence) = &mut texts[id];
            if space && text.ends_with(|c: char| c.is_ascii_graphic()) && part.starts_with(|c: char| c.is_ascii_alphanumeric()) {
                text.push(' ');
            }
            let len = part.chars().count();
            *confidence += len as f32 * score;
            lengths[id] += len;
            text.push_str(&part);
        }
        for ((_, score), len) in texts.iter_mut().zip(lengths) { *score /= len.max(1) as f32; }
        Ok(texts)
    }

    pub fn run(&mut self, img: &RgbImage) -> Result<(Vec<TextLine>, f64, f64)> {
        let t0 = std::time::Instant::now();
        let quads = self.detect(img)?;
        let t1 = std::time::Instant::now();
        let texts = self.recognize(img, &quads)?;
        let t2 = std::time::Instant::now();
        let lines = quads
            .into_iter()
            .zip(texts)
            .filter(|(_, (t, s))| !t.trim().is_empty() && *s >= self.opts.min_rec_score)
            .map(|(quad, (text, score))| TextLine { quad, text: text.trim().to_string(), score })
            .collect();
        Ok((lines, ms(t1 - t0), ms(t2 - t1)))
    }
}

/// Reading order: rows top to bottom, boxes left to right within a row. A row starts at its topmost box and takes
/// every following box whose top is less than `ROW_TOL` px below that start.
///
/// Rows are formed first and then sorted with plain keys: a single comparator "same row if |dy| < 10 → by x, else
/// by y" is not transitive (0.5.4), and Rust's sort (1.81+) panics on such comparators.
pub fn reading_order(mut quads: Vec<Quad>) -> Vec<Quad> {
    const ROW_TOL: f32 = 10.0;
    quads.sort_by(|a, b| a[0][1].total_cmp(&b[0][1]));
    let mut out = Vec::with_capacity(quads.len());
    let mut row: Vec<Quad> = Vec::new();
    for q in quads {
        if row.first().is_some_and(|r| q[0][1] - r[0][1] >= ROW_TOL) {
            row.sort_by(|a, b| a[0][0].total_cmp(&b[0][0]));
            out.append(&mut row);
        }
        row.push(q);
    }
    row.sort_by(|a, b| a[0][0].total_cmp(&b[0][0]));
    out.append(&mut row);
    out
}

/// Recogniser input height (PP-OCRv5: 3x48xW).
const REC_H: u32 = 48;
const REC_MAX_W: u32 = 3200;

/// Map `f` over `items` on up to N threads, keeping order.
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(items.len().max(1));
    if n <= 1 || items.len() < 4 {
        return items.iter().map(&f).collect();
    }
    let chunk = items.len().div_ceil(n);
    std::thread::scope(|sc| {
        let hs: Vec<_> = items.chunks(chunk).map(|c| sc.spawn(|| c.iter().map(&f).collect::<Vec<R>>())).collect();
        hs.into_iter().flat_map(|h| h.join().expect("par_map worker panicked")).collect()
    })
}

/// Split exceptionally wide lines before resampling. Cut inside whitespace when possible.
fn recognition_images(img: &RgbImage, q: &Quad) -> Vec<(RgbImage, bool)> {
    let (w, h) = quad_size(q);
    let ratio = if h >= w * 1.5 { h / w.max(1.0) } else { w / h.max(1.0) };
    if ratio * REC_H as f32 <= REC_MAX_W as f32 { return vec![(line_image(img, q), false)]; }
    let crop = crop_quad(img, q);
    let (width, height) = crop.dimensions();
    let target = (height * 26).max(1); // 1248 pixels at recognizer resolution
    let mut start = 0;
    let mut space_before = false;
    let mut parts = Vec::new();
    while start < width {
        let (end, space_after) = if width - start <= target + target / 2 {
            (width, false)
        } else {
            let center = start + target;
            let mut best = (0, u32::MAX, center);
            let mut run = 0;
            for x in center - target / 4..=center + target / 4 {
                let mut lo = [255; 3];
                let mut hi = [0; 3];
                for y in 0..height {
                    for (i, c) in crop.get_pixel(x, y).0.into_iter().enumerate() {
                        lo[i] = lo[i].min(c);
                        hi[i] = hi[i].max(c);
                    }
                }
                if (0..3).all(|i| hi[i] - lo[i] < 42) {
                    run += 1;
                    let mid = x - run / 2;
                    if run > best.0 || (run == best.0 && mid.abs_diff(center) < best.1) { best = (run, mid.abs_diff(center), mid); }
                } else { run = 0; }
            }
            (best.2, best.0 >= (height / 7).max(2))
        };
        let piece = imageops::crop_imm(&crop, start, 0, end - start, height);
        let tw = ((end - start) as u64 * u64::from(REC_H)).div_ceil(u64::from(height)) as u32;
        parts.push((imageops::resize(&*piece, tw.max(1), REC_H, imageops::FilterType::Triangle), space_before));
        start = end;
        space_before = space_after;
    }
    parts
}

/// One text line as a REC_H-high image: horizontal lines are perspective-warped directly to the target size;
/// tall (vertical) lines are cropped, rotated, then resized.
fn line_image(img: &RgbImage, q: &Quad) -> RgbImage {
    let (w, h) = quad_size(q);
    if h >= w * 1.5 {
        let c = crop_quad(img, q);
        let tw = ((REC_H as f32 * c.width() as f32 / c.height().max(1) as f32).ceil() as u32).clamp(1, REC_MAX_W);
        return imageops::resize(&c, tw, REC_H, imageops::FilterType::Triangle);
    }
    let tw = ((REC_H as f32 * w / h.max(1.0)).ceil() as u32).clamp(1, REC_MAX_W);
    // when shrinking a lot, warp to an intermediate 2x height first to avoid aliasing thin strokes
    let k = if h > REC_H as f32 * 2.5 { 2 } else { 1 };
    let (ow, oh) = (tw * k, REC_H * k);
    let dst = [(0.0, 0.0), (ow as f32, 0.0), (ow as f32, oh as f32), (0.0, oh as f32)];
    let src = [(q[0][0], q[0][1]), (q[1][0], q[1][1]), (q[2][0], q[2][1]), (q[3][0], q[3][1])];
    let mut out = RgbImage::new(ow, oh);
    match Projection::from_control_points(src, dst) {
        Some(p) => warp_into(img, p, Interpolation::Bilinear, Border::Constant(Rgb([255, 255, 255])), &mut out),
        None => return imageops::resize(&crop_quad(img, q), tw, REC_H, imageops::FilterType::Triangle),
    }
    if k > 1 {
        imageops::resize(&out, tw, REC_H, imageops::FilterType::Triangle)
    } else {
        out
    }
}

/// Run one padded batch through a recogniser session; returns (line index, (text, score)).
fn run_rec_batch(sess: &mut Session, lines: &[RgbImage], batch: &[usize], dict: &[String]) -> Result<Vec<IndexedRecognition>> {
    let hh = REC_H as usize;
    let bw = batch.iter().map(|&i| lines[i].width() as usize).max().unwrap_or(1).max(320);
    let n = batch.len();
    let mut data = vec![0f32; n * 3 * hh * bw];
    for (bi, &ci) in batch.iter().enumerate() {
        for (x, y, p) in lines[ci].enumerate_pixels() {
            for ch in 0..3 {
                // BGR, (x/255 - 0.5) / 0.5
                data[((bi * 3 + ch) * hh + y as usize) * bw + x as usize] = p[2 - ch] as f32 / 127.5 - 1.0;
            }
        }
    }
    let input = Tensor::from_array(([n, 3, hh, bw], data))?;
    let outputs = sess.run(ort::inputs![input])?;
    ensure!(outputs.len() > 0, "OCR recognition model returned no output");
    let (shape, out) = outputs[0].try_extract_tensor::<f32>()?;
    ensure!(shape.len() == 3 && shape[0] == n as i64 && shape[1] > 0 && shape[2] == dict.len() as i64,
        "OCR recognition model/dictionary mismatch: output {shape:?}, dictionary has {} entries", dict.len());
    let (t, classes) = (shape[1] as usize, shape[2] as usize);
    Ok(batch
        .iter()
        .enumerate()
        .map(|(bi, &ci)| (ci, ctc_decode(&out[bi * t * classes..(bi + 1) * t * classes], t, classes, dict)))
        .collect())
}

pub(crate) fn ms(d: std::time::Duration) -> f64 {
    (d.as_secs_f64() * 1000.0 * 10.0).round() / 10.0
}

fn ctc_decode(seq: &[f32], t: usize, classes: usize, dict: &[String]) -> (String, f32) {
    let mut text = String::new();
    let mut scores = Vec::new();
    let mut prev = 0usize;
    for step in 0..t {
        let row = &seq[step * classes..(step + 1) * classes];
        let (idx, &p) = row.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap();
        if idx != 0 && idx != prev {
            if let Some(ch) = dict.get(idx) {
                text.push_str(ch);
                scores.push(p);
            }
        }
        prev = idx;
    }
    let score = if scores.is_empty() { 0.0 } else { scores.iter().sum::<f32>() / scores.len() as f32 };
    (text, score)
}

/// Mean probability inside the contour's bounding box (fast approximation of PaddleOCR's polygon mean).
fn box_score(prob: &[f32], w: usize, h: usize, pts: &[Point<i32>]) -> f32 {
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for p in pts {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    let (x0, y0) = (x0.max(0) as usize, y0.max(0) as usize);
    let (x1, y1) = ((x1 as usize).min(w - 1), (y1 as usize).min(h - 1));
    let mut s = 0f32;
    let mut n = 0usize;
    for y in y0..=y1 {
        for x in x0..=x1 {
            s += prob[y * w + x];
            n += 1;
        }
    }
    if n == 0 { 0.0 } else { s / n as f32 }
}

/// Order 4 points as TL, TR, BR, BL.
pub fn order_quad(p: [[f32; 2]; 4]) -> Quad {
    let mut pts = p.to_vec();
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let (mut l, mut r) = (vec![pts[0], pts[1]], vec![pts[2], pts[3]]);
    l.sort_by(|a, b| a[1].total_cmp(&b[1]));
    r.sort_by(|a, b| a[1].total_cmp(&b[1]));
    [l[0], r[0], r[1], l[1]]
}

pub fn quad_size(q: &Quad) -> (f32, f32) {
    let d = |a: [f32; 2], b: [f32; 2]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    (d(q[0], q[1]).max(d(q[3], q[2])), d(q[0], q[3]).max(d(q[1], q[2])))
}

fn expand_quad(q: &Quad, d: f32) -> Quad {
    let cx = q.iter().map(|p| p[0]).sum::<f32>() / 4.0;
    let cy = q.iter().map(|p| p[1]).sum::<f32>() / 4.0;
    // unit vectors along width and height edges
    let ux = [q[1][0] - q[0][0], q[1][1] - q[0][1]];
    let uy = [q[3][0] - q[0][0], q[3][1] - q[0][1]];
    let nx = (ux[0].hypot(ux[1])).max(1e-6);
    let ny = (uy[0].hypot(uy[1])).max(1e-6);
    let ux = [ux[0] / nx, ux[1] / nx];
    let uy = [uy[0] / ny, uy[1] / ny];
    let (hw, hh) = (nx / 2.0 + d, ny / 2.0 + d);
    let p = |sx: f32, sy: f32| [cx + sx * hw * ux[0] + sy * hh * uy[0], cy + sx * hw * ux[1] + sy * hh * uy[1]];
    [p(-1.0, -1.0), p(1.0, -1.0), p(1.0, 1.0), p(-1.0, 1.0)]
}

/// Perspective-crop a quad into an upright rectangle; rotate tall crops (vertical text) by 90°.
fn crop_quad(img: &RgbImage, q: &Quad) -> RgbImage {
    let (w, h) = quad_size(q);
    let (w, h) = (w.round().max(1.0) as u32, h.round().max(1.0) as u32);
    let dst = [(0.0, 0.0), (w as f32, 0.0), (w as f32, h as f32), (0.0, h as f32)];
    let src = [(q[0][0], q[0][1]), (q[1][0], q[1][1]), (q[2][0], q[2][1]), (q[3][0], q[3][1])];
    let mut out = RgbImage::new(w, h);
    match Projection::from_control_points(src, dst) {
        Some(proj) => warp_into(img, proj, Interpolation::Bilinear, Border::Constant(Rgb([255, 255, 255])), &mut out),
        None => {
            let x = q[0][0] as u32;
            let y = q[0][1] as u32;
            out = imageops::crop_imm(img, x, y, w.min(img.width() - x), h.min(img.height() - y)).to_image();
        }
    }
    if h as f32 >= w as f32 * 1.5 {
        out = imageops::rotate270(&out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f32, y: f32) -> Quad {
        [[x, y], [x + 50.0, y], [x + 50.0, y + 20.0], [x, y + 20.0]]
    }

    #[test]
    fn reading_order_rows_then_columns() {
        // the 0.5.4 comparator cycled on these three (a < b < c < a)
        let q = reading_order(vec![at(20.0, 0.0), at(10.0, 9.0), at(0.0, 12.0)]);
        assert_eq!(q.iter().map(|q| (q[0][0], q[0][1])).collect::<Vec<_>>(), vec![(10.0, 9.0), (20.0, 0.0), (0.0, 12.0)]);
        let q = reading_order(vec![at(300.0, 52.0), at(0.0, 50.0), at(0.0, 0.0), at(300.0, 3.0)]);
        assert_eq!(q.iter().map(|q| (q[0][0], q[0][1])).collect::<Vec<_>>(), vec![(0.0, 0.0), (300.0, 3.0), (0.0, 50.0), (300.0, 52.0)]);
    }

    #[test]
    fn reading_order_never_panics() {
        // dense random layouts: the old comparator panicked in ~13% of these on Rust 1.81+
        let mut seed = 12345u64;
        let mut rnd = |m: f32| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 10_000) as f32 / 10_000.0 * m
        };
        for n in [3usize, 8, 20, 40, 100, 300] {
            for _ in 0..300 {
                let v: Vec<Quad> = (0..n).map(|_| at(rnd(1000.0), rnd(n as f32 * 12.0))).collect();
                let out = reading_order(v.clone());
                assert_eq!(out.len(), v.len());
            }
        }
    }
}
