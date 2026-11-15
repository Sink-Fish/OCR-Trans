//! Remove the original text under translated paragraphs so the translation can be drawn on a clean background.
//!
//! For each translated paragraph we build a text-pixel mask (pixels inside the line boxes that differ from the
//! estimated background colour, dilated a little) and fill it from the outside in ("onion peel", the same
//! ordering idea as Telea's fast-marching inpainting): every masked pixel that touches known pixels takes the
//! distance-weighted mean of its known 8-neighbours, then becomes known itself. Backgrounds that are too busy
//! for this to look right (photos, textures) are marked `translucent` so the renderer draws a soft panel instead.

use image::RgbImage;

use crate::layout::Paragraph;
use crate::ocr::Quad;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    Inpaint,
    Translucent,
}

impl Fill {
    pub fn as_str(self) -> &'static str {
        match self {
            Fill::Inpaint => "inpaint",
            Fill::Translucent => "translucent",
        }
    }
}

fn parse_hex(s: &str) -> [u8; 3] {
    let h = s.trim_start_matches('#');
    let v = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    [v(0), v(2), v(4)]
}

fn dist(a: [u8; 3], b: [u8; 3]) -> f32 {
    let d: i32 = (0..3).map(|c| (a[c] as i32 - b[c] as i32).pow(2)).sum();
    (d as f32).sqrt()
}

fn quad_bbox(q: &Quad, w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    if w == 0 || h == 0 || q.iter().flatten().any(|v| !v.is_finite()) {
        return None;
    }
    let x0 = q.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
    let y0 = q.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
    let x1 = q.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
    let y1 = q.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
    if x1 < 0.0 || y1 < 0.0 || x0 >= w as f32 || y0 >= h as f32 {
        return None;
    }
    Some((
        x0.max(0.0) as u32,
        y0.max(0.0) as u32,
        (x1.ceil() as u32).min(w.saturating_sub(1)),
        (y1.ceil() as u32).min(h.saturating_sub(1)),
    ))
}

/// Returns the cleaned image and the fill mode per paragraph (None for untranslated paragraphs).
pub fn clean(img: &RgbImage, paras: &[Paragraph]) -> (RgbImage, Vec<Option<Fill>>) {
    let (w, h) = img.dimensions();
    let mut out = img.clone();
    let mut fills = vec![None; paras.len()];
    if w == 0 || h == 0 {
        return (out, fills);
    }
    for (pi, p) in paras.iter().enumerate() {
        if p.translation.is_none() || p.lines.is_empty() {
            continue;
        }
        let bg = parse_hex(&p.bg);
        let fg = parse_hex(&p.fg);
        let thr = (dist(fg, bg) * 0.3).max(24.0);
        let grow = (p.line_height * 0.08).clamp(1.0, 4.0) as i32;

        // Keep the mask inside the line boxes, which are already padded by the detector
        // and may sit next to a table divider.
        let boxes: Vec<_> = p.lines.iter().filter_map(|l| quad_bbox(&l.quad, w, h)).collect();
        if boxes.is_empty() {
            continue;
        }
        let rx0 = boxes.iter().map(|b| b.0).min().unwrap();
        let ry0 = boxes.iter().map(|b| b.1).min().unwrap();
        let rx1 = boxes.iter().map(|b| b.2).max().unwrap();
        let ry1 = boxes.iter().map(|b| b.3).max().unwrap();
        let (rw, rh) = ((rx1 - rx0 + 1) as usize, (ry1 - ry0 + 1) as usize);

        // 1. raw text mask inside line boxes
        let mut mask = vec![false; rw * rh];
        let mut inside = vec![false; rw * rh];
        for &(x0, y0, x1, y1) in &boxes {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let i = (y - ry0) as usize * rw + (x - rx0) as usize;
                    inside[i] = true;
                    if dist(img.get_pixel(x, y).0, bg) > thr {
                        mask[i] = true;
                    }
                }
            }
        }
        // 2. dilate (square kernel)
        let mut dil = mask.clone();
        for y in 0..rh as i32 {
            for x in 0..rw as i32 {
                if !mask[y as usize * rw + x as usize] {
                    continue;
                }
                for dy in -grow..=grow {
                    for dx in -grow..=grow {
                        let (nx, ny) = (x + dx, y + dy);
                        if nx >= 0 && ny >= 0 && (nx as usize) < rw && (ny as usize) < rh && inside[ny as usize * rw + nx as usize] {
                            dil[ny as usize * rw + nx as usize] = true;
                        }
                    }
                }
            }
        }
        // 3. how busy is the background? Use local texture (mean absolute difference to the pixel 2 px right/below)
        //    in a ring just outside the text region: smooth gradients score low, photos/noise high.
        let ring = (p.line_height * 0.3).max(3.0) as u32;
        let (ox0, oy0) = (rx0.saturating_sub(ring), ry0.saturating_sub(ring));
        let (ox1, oy1) = (rx1.saturating_add(ring).min(w.saturating_sub(3)), ry1.saturating_add(ring).min(h.saturating_sub(3)));
        let (mut sum, mut n) = (0f32, 0u32);
        let tex = |x: u32, y: u32| {
            let a = img.get_pixel(x, y).0;
            (dist(a, img.get_pixel(x + 2, y).0) + dist(a, img.get_pixel(x, y + 2).0)) / 2.0
        };
        if w >= 3 && h >= 3 {
            for y in oy0..=oy1 {
                for x in ox0..=ox1 {
                    if x >= rx0 && x <= rx1 && y >= ry0 && y <= ry1 {
                        continue;
                    }
                    sum += tex(x, y);
                    n += 1;
                }
            }
        }
        let busy = if n == 0 { 0.0 } else { sum / n as f32 };
        let fill = if busy > 14.0 { Fill::Translucent } else { Fill::Inpaint };
        // 4. onion-peel fill of the dilated mask
        let mut known: Vec<bool> = dil.iter().map(|m| !m).collect();
        let mut buf: Vec<[f32; 3]> = (0..rw * rh)
            .map(|i| {
                let p = img.get_pixel(rx0 + (i % rw) as u32, ry0 + (i / rw) as u32).0;
                [p[0] as f32, p[1] as f32, p[2] as f32]
            })
            .collect();
        let mut remaining = dil.iter().filter(|m| **m).count();
        while remaining > 0 {
            let mut front = vec![];
            for y in 0..rh {
                for x in 0..rw {
                    let i = y * rw + x;
                    if known[i] {
                        continue;
                    }
                    let (mut acc, mut wsum) = ([0f32; 3], 0f32);
                    for dy in -1i32..=1 {
                        for dx in -1i32..=1 {
                            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                            if (dx, dy) == (0, 0) || nx < 0 || ny < 0 || nx as usize >= rw || ny as usize >= rh {
                                continue;
                            }
                            let j = ny as usize * rw + nx as usize;
                            if known[j] {
                                let wgt = if dx == 0 || dy == 0 { 1.0 } else { 0.7 };
                                for c in 0..3 {
                                    acc[c] += buf[j][c] * wgt;
                                }
                                wsum += wgt;
                            }
                        }
                    }
                    if wsum > 0.0 {
                        front.push((i, [acc[0] / wsum, acc[1] / wsum, acc[2] / wsum]));
                    }
                }
            }
            if front.is_empty() {
                // mask covers the whole region: fall back to the background colour
                for i in 0..rw * rh {
                    if !known[i] {
                        buf[i] = [bg[0] as f32, bg[1] as f32, bg[2] as f32];
                        known[i] = true;
                    }
                }
                break;
            }
            remaining -= front.len();
            for (i, v) in front {
                buf[i] = v;
                known[i] = true;
            }
        }
        // 5. one 3x3 smoothing pass over the filled pixels to hide the peel rings
        let snapshot = buf.clone();
        for y in 0..rh {
            for x in 0..rw {
                let i = y * rw + x;
                if !dil[i] {
                    continue;
                }
                let (mut acc, mut k) = ([0f32; 3], 0f32);
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if nx >= 0 && ny >= 0 && (nx as usize) < rw && (ny as usize) < rh {
                            let j = ny as usize * rw + nx as usize;
                            for c in 0..3 {
                                acc[c] += snapshot[j][c];
                            }
                            k += 1.0;
                        }
                    }
                }
                buf[i] = [acc[0] / k, acc[1] / k, acc[2] / k];
            }
        }
        for y in 0..rh {
            for x in 0..rw {
                let i = y * rw + x;
                if dil[i] {
                    let v = buf[i];
                    out.put_pixel(rx0 + x as u32, ry0 + y as u32, image::Rgb([v[0] as u8, v[1] as u8, v[2] as u8]));
                }
            }
        }
        fills[pi] = Some(fill);
    }
    (out, fills)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::TextLine;

    fn para(quad: Quad, bg: &str, fg: &str) -> Paragraph {
        Paragraph {
            bbox: [quad[0][0], quad[0][1], quad[1][0] - quad[0][0], quad[2][1] - quad[0][1]],
            lines: vec![TextLine { quad, text: "x".into(), score: 1.0 }],
            text: "x".into(),
            line_height: quad[2][1] - quad[0][1],
            lang: crate::layout::Lang::En,
            bg: bg.into(),
            fg: fg.into(),
            translation: Some("译".into()),
            fill: None,
        }
    }

    #[test]
    fn removes_text_on_gradient() {
        // horizontal gradient background with a black "glyph" bar in the middle
        let mut img = RgbImage::from_fn(200, 60, |x, _| image::Rgb([100 + (x / 4) as u8, 120, 140]));
        for y in 25..35 {
            for x in 60..140 {
                img.put_pixel(x, y, image::Rgb([0, 0, 0]));
            }
        }
        let p = para([[50.0, 18.0], [150.0, 18.0], [150.0, 42.0], [50.0, 42.0]], "#7d788c", "#000000");
        let (out, fills) = clean(&img, &[p]);
        assert_eq!(fills[0], Some(Fill::Inpaint));
        // no dark pixels left, and the gradient is roughly preserved
        for y in 25..35 {
            for x in 60..140 {
                let v = out.get_pixel(x, y).0;
                assert!(v[1] > 100, "pixel {x},{y} still dark: {v:?}");
                let expect = 100 + (x / 4) as i32;
                assert!((v[0] as i32 - expect).abs() < 20, "gradient lost at {x}: {} vs {expect}", v[0]);
            }
        }
        // pixels outside the paragraph untouched
        assert_eq!(out.get_pixel(5, 5), img.get_pixel(5, 5));
    }

    #[test]
    fn busy_background_is_translucent() {
        let mut img = RgbImage::from_fn(120, 40, |x, y| if (x / 2 + y / 2) % 2 == 0 { image::Rgb([255, 255, 255]) } else { image::Rgb([90, 30, 10]) });
        img.put_pixel(0, 0, image::Rgb([0, 0, 0]));
        let p = para([[10.0, 5.0], [110.0, 5.0], [110.0, 35.0], [10.0, 35.0]], "#ffffff", "#000000");
        let (_, fills) = clean(&img, &[p]);
        assert_eq!(fills[0], Some(Fill::Translucent));
    }

    #[test]
    fn empty_tiny_and_outside_images_are_safe() {
        let p = para([[1.0, 1.0], [8.0, 1.0], [8.0, 8.0], [1.0, 8.0]], "#ffffff", "#000000");
        for (w, h) in [(0, 0), (0, 10), (10, 0), (1, 1), (2, 2), (2, 10), (10, 2)] {
            let img = RgbImage::from_pixel(w, h, image::Rgb([255; 3]));
            let (out, _) = clean(&img, std::slice::from_ref(&p));
            assert_eq!(out, img);
        }
        let img = RgbImage::from_pixel(10, 10, image::Rgb([255; 3]));
        for q in [
            [[20.0, 20.0], [30.0, 20.0], [30.0, 30.0], [20.0, 30.0]],
            [[-20.0, -20.0], [-10.0, -20.0], [-10.0, -10.0], [-20.0, -10.0]],
            [[f32::NAN, 0.0]; 4],
        ] {
            let (out, fills) = clean(&img, &[para(q, "#ffffff", "#000000")]);
            assert_eq!(out, img);
            assert_eq!(fills, [None]);
        }
    }
}
