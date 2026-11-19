//! Group OCR lines into paragraphs and estimate their colours for in-place rendering.

use image::RgbImage;
use serde::Serialize;

use crate::ocr::{quad_size, Quad, TextLine};

#[derive(Debug, Clone, Serialize)]
pub struct Paragraph {
    /// Axis-aligned bounding box [x, y, w, h] in source pixels.
    pub bbox: [f32; 4],
    pub lines: Vec<TextLine>,
    pub text: String,
    /// Average line height in source pixels (used to pick a font size).
    pub line_height: f32,
    pub lang: Lang,
    /// Estimated background / foreground colours as #rrggbb.
    pub bg: String,
    pub fg: String,
    pub translation: Option<String>,
    /// How the renderer should cover the original text: "inpaint" (background already cleaned) or
    /// "translucent" (busy background: draw a soft panel). None = flat fill with `bg`.
    #[serde(default)]
    pub fill: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    En,
    Ja,
    Zh,
    Other,
}

pub fn detect_lang(s: &str) -> Lang {
    let (mut latin, mut kana, mut han) = (0, 0, 0);
    for c in s.chars() {
        match c as u32 {
            0x41..=0x5A | 0x61..=0x7A => latin += 1,
            0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9F => kana += 1,
            0x4E00..=0x9FFF | 0x3400..=0x4DBF => han += 1,
            _ => {}
        }
    }
    if kana > 0 && kana * 5 >= (han + kana) / 2 {
        Lang::Ja
    } else if han > 0 && han >= latin / 2 {
        Lang::Zh
    } else if latin > 0 {
        Lang::En
    } else {
        Lang::Other
    }
}

fn bbox(q: &Quad) -> [f32; 4] {
    let x0 = q.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
    let y0 = q.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
    let x1 = q.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
    let y1 = q.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
    [x0, y0, x1 - x0, y1 - y0]
}

fn mid_y(b: [f32; 4]) -> f32 {
    b[1] + b[3] / 2.0
}

fn order_rows<T>(items: &mut [T], bounds: impl Fn(&T) -> [f32; 4]) -> Vec<std::ops::Range<usize>> {
    items.sort_by(|a, b| mid_y(bounds(a)).total_cmp(&mid_y(bounds(b))));
    let mut rows = Vec::new();
    let mut start = 0;
    while start < items.len() {
        let first = bounds(&items[start]);
        let mut end = start + 1;
        while end < items.len() {
            let next = bounds(&items[end]);
            if mid_y(next) - mid_y(first) > first[3].min(next[3]) * 0.35 {
                break;
            }
            end += 1;
        }
        items[start..end].sort_by(|a, b| bounds(a)[0].total_cmp(&bounds(b)[0]));
        rows.push(start..end);
        start = end;
    }
    rows
}

/// Keep dense borderless tables separate when adjacent rows repeat the same column alignment.
fn dense_table_cells(lines: &mut [TextLine]) -> Vec<bool> {
    let rows = order_rows(lines, |line| bbox(&line.quad));
    let mut cells = vec![false; lines.len()];
    for pair in rows.windows(2) {
        let (upper, lower) = (&lines[pair[0].clone()], &lines[pair[1].clone()]);
        if upper.len() < 2 || upper.len() != lower.len() {
            continue;
        }
        let aligned = upper.iter().zip(lower).all(|(a, b)| {
            let (a, b) = (bbox(&a.quad), bbox(&b.quad));
            let h = a[3].min(b[3]);
            let gap = mid_y(b) - mid_y(a);
            gap > h * 0.65 && gap < h * 2.5 && [0.0, 0.5, 1.0].into_iter().any(|anchor| {
                ((a[0] + a[2] * anchor) - (b[0] + b[2] * anchor)).abs() < h * 0.45
            })
        });
        let mut gaps = upper.windows(2).chain(lower.windows(2)).map(|pair| {
            let (a, b) = (bbox(&pair[0].quad), bbox(&pair[1].quad));
            (b[0] - (a[0] + a[2])) / a[3].min(b[3]).max(1.0)
        });
        if aligned && gaps.clone().all(|gap| gap > -0.45) && gaps.any(|gap| gap < 1.2) {
            cells[pair[0].start..pair[1].end].fill(true);
        }
    }
    cells
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3000..=0x9FFF | 0xFF00..=0xFFEF)
}

/// Sample a straight contrast edge without mistaking scattered letter strokes for a divider.
fn is_divider(img: &RgbImage, axis: usize, edge: u32, lo: f32, hi: f32) -> bool {
    let along_size = if axis == 0 { img.height() } else { img.width() };
    let lo = lo.max(0.0);
    let hi = hi.min(along_size.saturating_sub(1) as f32);
    if hi - lo < 7.0 {
        return false;
    }
    let mut lighter = 0;
    let mut darker = 0;
    for i in 0..9 {
        let p = (lo + (hi - lo) * (i as f32 + 0.5) / 9.0).round() as u32;
        let (before, after) = if axis == 0 {
            (img.get_pixel(edge, p), img.get_pixel(edge + 1, p))
        } else {
            (img.get_pixel(p, edge), img.get_pixel(p, edge + 1))
        };
        let delta: i16 = before.0.iter().zip(after.0).map(|(&a, b)| b as i16 - a as i16).sum();
        lighter += u32::from(delta >= 66);
        darker += u32::from(delta <= -66);
    }
    lighter.max(darker) >= 8
}

/// A divider between two OCR boxes is evidence that they are in different cells.
fn has_divider(img: &RgbImage, a: [f32; 4], b: [f32; 4], axis: usize) -> bool {
    let along = 1 - axis;
    let size = if axis == 0 { img.width() } else { img.height() };
    if size < 2 {
        return false;
    }
    let lo = a[along].max(b[along]);
    let hi = (a[along] + a[along + 2]).min(b[along] + b[along + 2]);
    // A row rule can fall inside the padding of both detector boxes, away from their edges.
    let end_of_a = a[axis] + a[axis + 2];
    let padding = a[3].min(b[3]) * 0.25;
    let first = (end_of_a.min(b[axis]) - padding).floor() as u32;
    let last = ((end_of_a.max(b[axis]) + padding).ceil() as u32).min(size - 2);
    (first..=last).any(|edge| is_divider(img, axis, edge, lo, hi))
}

/// Pull detector padding back from any rules it crosses so erasing and drawing stop inside a cell.
fn trim_at_dividers(img: &RgbImage, q: &mut Quad) {
    let b = bbox(q);
    let reach = (b[2].min(b[3]) * 0.35).ceil() as u32;
    for (axis, size) in [img.width(), img.height()].into_iter().enumerate() {
        if size < 2 {
            continue;
        }
        let along = 1 - axis;
        let lo = b[along];
        let hi = lo + b[along + 2];
        let start = b[axis];
        let end = start + b[axis + 2];
        let first = start.floor() as u32;
        let last = (end.ceil() as u32).min(size - 1);
        let start = (first..=first.saturating_add(reach).min(size - 2))
            .rev()
            .find(|&edge| is_divider(img, axis, edge, lo, hi))
            .map_or(start, |edge| (edge + 1) as f32);
        let end = (last.saturating_sub(reach)..last)
            .find(|&edge| is_divider(img, axis, edge, lo, hi))
            .map_or(end, |edge| edge as f32);
        if end - start >= b[axis + 2] * 0.5 {
            for point in q.iter_mut() {
                point[axis] = point[axis].clamp(start, end);
            }
        }
    }
}

/// Join lines of one paragraph: spaces for Latin text, nothing between CJK, de-hyphenate.
fn join_lines(lines: &[TextLine]) -> String {
    let mut out = String::new();
    for l in lines {
        let t = l.text.trim();
        if out.is_empty() {
            out.push_str(t);
            continue;
        }
        let last = out.chars().last().unwrap();
        let first = t.chars().next().unwrap_or(' ');
        if last == '-' && first.is_lowercase() {
            out.pop();
        } else if !(is_cjk(last) && is_cjk(first)) {
            out.push(' ');
        }
        out.push_str(t);
    }
    out
}

/// The detector sometimes splits one text line at a wide space ("Welcome | back!"). Join boxes that sit on the
/// same row, have similar height and a horizontal gap below ~1.2 line heights.
fn merge_row_fragments(img: &RgbImage, mut lines: Vec<TextLine>) -> Vec<TextLine> {
    lines.sort_by(|a, b| bbox(&a.quad)[0].total_cmp(&bbox(&b.quad)[0]));
    let mut out: Vec<TextLine> = Vec::new();
    for l in lines {
        let b = bbox(&l.quad);
        let h = quad_size(&l.quad).1;
        let target = out.iter_mut().rev().find(|o| {
            let ob = bbox(&o.quad);
            let oh = quad_size(&o.quad).1;
            let cy = (b[1] + b[3] / 2.0, ob[1] + ob[3] / 2.0);
            let gap = b[0] - (ob[0] + ob[2]);
            (cy.0 - cy.1).abs() < h.min(oh) * 0.35
                && h.max(oh) / h.min(oh).max(1.0) < 1.3
                && gap > -h.max(oh) * 1.5
                && gap < h.max(oh) * 1.2
                && !has_divider(img, ob, b, 0)
        });
        match target {
            Some(o) => {
                let ob = bbox(&o.quad);
                let (x0, y0) = (ob[0].min(b[0]), ob[1].min(b[1]));
                let (x1, y1) = ((ob[0] + ob[2]).max(b[0] + b[2]), (ob[1] + ob[3]).max(b[1] + b[3]));
                let mut right = l.text.trim_start().to_string();
                if b[0] < ob[0] + ob[2] {
                    // overlapping boxes: the overlap was recognised twice ("Welcome" + "e back!") — drop the
                    // duplicated prefix of the right fragment
                    let left: Vec<char> = o.text.trim_end().chars().collect();
                    let r: Vec<char> = right.chars().collect();
                    for k in (1..=4.min(r.len()).min(left.len())).rev() {
                        if left[left.len() - k..] == r[..k] && r.get(k).is_some_and(|c| !c.is_alphanumeric()) {
                            right = r[k..].iter().collect::<String>().trim_start().to_string();
                            break;
                        }
                    }
                }
                let sep = if is_cjk(o.text.chars().last().unwrap_or(' ')) && is_cjk(right.chars().next().unwrap_or(' ')) { "" } else { " " };
                o.text = format!("{}{}{}", o.text.trim_end(), sep, right);
                o.score = o.score.min(l.score);
                o.quad = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
            }
            None => out.push(l),
        }
    }
    out
}

/// Lines are mergeable into a paragraph when vertically adjacent, of similar height and
/// horizontally overlapping (or left-aligned).
fn group_paragraphs(img: &RgbImage, mut lines: Vec<TextLine>) -> Vec<Vec<TextLine>> {
    for line in &mut lines {
        trim_at_dividers(img, &mut line.quad);
    }
    let cells = dense_table_cells(&mut lines);
    let mut table = Vec::new();
    let prose = lines.into_iter().zip(cells).filter_map(|(line, cell)| {
        if cell {
            table.push(vec![line]);
            None
        } else {
            Some(line)
        }
    }).collect();
    let mut lines = merge_row_fragments(img, prose);
    lines.sort_by(|a, b| bbox(&a.quad)[1].total_cmp(&bbox(&b.quad)[1]));
    let mut groups: Vec<Vec<TextLine>> = Vec::new();
    'outer: for line in lines {
        let b = bbox(&line.quad);
        let h = quad_size(&line.quad).1;
        for g in groups.iter_mut().rev() {
            let last = g.last().unwrap();
            let lb = bbox(&last.quad);
            let lh = quad_size(&last.quad).1;
            let gap = b[1] - (lb[1] + lb[3]);
            let similar_h = h.max(lh) / h.min(lh).max(1.0) < 1.35;
            let overlap = (b[0] + b[2]).min(lb[0] + lb[2]) - b[0].max(lb[0]);
            let left_aligned = (b[0] - lb[0]).abs() < lh * 1.5;
            // a line that is on the same row (e.g. columns) never merges
            if gap > -lh * 0.3
                && gap < lh.max(h) * 0.5
                && similar_h
                && (overlap > 0.0 && left_aligned || overlap > b[2].min(lb[2]) * 0.6)
                && !has_divider(img, lb, b, 1)
            {
                g.push(line);
                continue 'outer;
            }
            // only look a few groups back to keep it local
            if lb[1] + lb[3] < b[1] - 3.0 * h {
                break;
            }
        }
        groups.push(vec![line]);
    }
    groups.extend(table);
    order_rows(&mut groups, |group| bbox(&group[0].quad));
    groups
}

pub fn build_paragraphs(img: &RgbImage, lines: Vec<TextLine>) -> Vec<Paragraph> {
    let mut paragraphs: Vec<_> = group_paragraphs(img, lines)
        .into_iter()
        .map(|ls| {
            let bbs: Vec<[f32; 4]> = ls.iter().map(|l| bbox(&l.quad)).collect();
            let x0 = bbs.iter().map(|b| b[0]).fold(f32::MAX, f32::min);
            let y0 = bbs.iter().map(|b| b[1]).fold(f32::MAX, f32::min);
            let x1 = bbs.iter().map(|b| b[0] + b[2]).fold(f32::MIN, f32::max);
            let y1 = bbs.iter().map(|b| b[1] + b[3]).fold(f32::MIN, f32::max);
            let line_height = ls.iter().map(|l| quad_size(&l.quad).1).sum::<f32>() / ls.len() as f32;
            let text = join_lines(&ls);
            let (bg, fg) = estimate_colors(img, &ls);
            Paragraph {
                bbox: [x0, y0, x1 - x0, y1 - y0],
                lang: detect_lang(&text),
                text,
                lines: ls,
                line_height,
                bg: hex(bg),
                fg: hex(fg),
                translation: None,
                fill: None,
            }
        })
        .collect();
    // Short kanji labels on a Japanese menu lack kana of their own. Use a nearby
    // Japanese heading in the same column or row; leave standalone Chinese as detected.
    let japanese: Vec<_> = paragraphs.iter().filter(|p| p.lang == Lang::Ja).map(|p| p.bbox).collect();
    for p in &mut paragraphs {
        if p.lang != Lang::Zh || p.text.chars().count() > 16
            || !p.text.chars().all(|c| matches!(c, '\u{3400}'..='\u{9fff}') || c.is_whitespace()) { continue; }
        let [x,y,w,h] = p.bbox;
        let nearby = japanese.iter().any(|&[jx,jy,jw,jh]| {
            let dx = (x - jx - jw).max(jx - x - w).max(0.0);
            let dy = (y - jy - jh).max(jy - y - h).max(0.0);
            let unit = h.max(jh);
            (dx < unit * 2.0 && dy < unit * 18.0) || (dy < unit && dx < unit * 12.0)
        });
        if nearby { p.lang = Lang::Ja; }
    }
    paragraphs
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Background = median of pixels on each line box border; foreground = the in-box pixel cluster
/// farthest from the background.
fn estimate_colors(img: &RgbImage, lines: &[TextLine]) -> ([u8; 3], [u8; 3]) {
    let (w, h) = img.dimensions();
    let mut border = Vec::new();
    let mut inner = Vec::new();
    for l in lines {
        let b = bbox(&l.quad);
        let (x0, y0) = (b[0].max(0.0) as u32, b[1].max(0.0) as u32);
        let (x1, y1) = (((b[0] + b[2]) as u32).min(w - 1), ((b[1] + b[3]) as u32).min(h - 1));
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        for x in (x0..=x1).step_by(2) {
            border.push(img.get_pixel(x, y0).0);
            border.push(img.get_pixel(x, y1).0);
        }
        for y in (y0..=y1).step_by(2) {
            border.push(img.get_pixel(x0, y).0);
            border.push(img.get_pixel(x1, y).0);
        }
        let step = (((x1 - x0) * (y1 - y0) / 4000) as f32).sqrt().max(1.0) as usize;
        for y in (y0..=y1).step_by(step) {
            for x in (x0..=x1).step_by(step) {
                inner.push(img.get_pixel(x, y).0);
            }
        }
    }
    if border.is_empty() {
        return ([255, 255, 255], [0, 0, 0]);
    }
    let med = |v: &mut Vec<[u8; 3]>, c: usize| {
        v.sort_by_key(|p| p[c]);
        v[v.len() / 2][c]
    };
    let bg = [med(&mut border, 0), med(&mut border, 1), med(&mut border, 2)];
    let dist = |p: &[u8; 3]| (0..3).map(|c| (p[c] as i32 - bg[c] as i32).pow(2)).sum::<i32>();
    inner.sort_by_key(|p| -dist(p));
    let top = &inner[..(inner.len() / 10).max(1).min(inner.len())];
    let fg = if top.is_empty() || dist(&top[0]) < 40 * 40 {
        // low contrast: pick black or white, whichever contrasts with bg
        if luminance(bg) > 140.0 { [0, 0, 0] } else { [255, 255, 255] }
    } else {
        let n = top.len() as u32;
        let s = top.iter().fold([0u32; 3], |a, p| [a[0] + p[0] as u32, a[1] + p[1] as u32, a[2] + p[2] as u32]);
        [(s[0] / n) as u8, (s[1] / n) as u8, (s[2] / n) as u8]
    };
    (bg, fg)
}

fn luminance(c: [u8; 3]) -> f32 {
    0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32
}


#[cfg(test)]
mod tests {
    use super::*;

    fn line(x: f32, y: f32, w: f32, h: f32, t: &str) -> TextLine {
        TextLine { quad: [[x, y], [x + w, y], [x + w, y + h], [x, y + h]], text: t.into(), score: 0.9 }
    }

    #[test]
    fn lang_detection() {
        assert_eq!(detect_lang("Hello world"), Lang::En);
        assert_eq!(detect_lang("ダウンロードが完了しました。"), Lang::Ja);
        assert_eq!(detect_lang("設定画面"), Lang::Zh); // kanji-only is ambiguous; treated as Chinese
        assert_eq!(detect_lang("下载完成 OK"), Lang::Zh);
        assert_eq!(detect_lang("12:30 ✓"), Lang::Other);
    }

    #[test]
    fn row_fragments_merge() {
        let img = RgbImage::new(700, 100);
        let v = merge_row_fragments(&img, vec![line(185.0, 10.0, 300.0, 30.0, "back! Your files"), line(10.0, 12.0, 150.0, 28.0, "Welcome")]);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].text, "Welcome back! Your files");
        // far apart columns stay separate
        let v = merge_row_fragments(&img, vec![line(59.0, 47.0, 150.0, 34.0, "Welcome"), line(191.0, 45.0, 436.0, 42.0, "e back! Your files")]);
        assert_eq!(v[0].text, "Welcome back! Your files");
        let v = merge_row_fragments(&img, vec![line(10.0, 10.0, 100.0, 30.0, "Name"), line(400.0, 10.0, 100.0, 30.0, "Size")]);
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn paragraphs_split_on_gap() {
        let img = RgbImage::new(500, 200);
        let g = group_paragraphs(&img, vec![
            line(10.0, 10.0, 400.0, 30.0, "First line of a paragraph that"),
            line(10.0, 44.0, 380.0, 30.0, "continues here."),
            line(10.0, 120.0, 300.0, 30.0, "Second paragraph."),
        ]);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].len(), 2);
        assert_eq!(join_lines(&g[0]), "First line of a paragraph that continues here.");
    }

    #[test]
    fn ruled_table_keeps_close_cells_separate() {
        let img = RgbImage::from_fn(100, 65, |x, y| {
            image::Rgb(if x == 44 || y == 32 || (15..30).contains(&x) && (17..23).contains(&y) { [60; 3] } else { [255; 3] })
        });
        // The detector's padding crosses both dividers in these boxes.
        let mut cells = build_paragraphs(&img, vec![
            line(10.0, 10.0, 37.0, 24.0, "Name"),
            line(42.0, 10.0, 42.0, 24.0, "Status"),
            line(24.0, 30.0, 23.0, 24.0, "File"),
            line(42.0, 30.0, 42.0, 24.0, "Ready"),
        ]);
        assert_eq!(cells.len(), 4, "{:?}", cells.iter().map(|p| &p.text).collect::<Vec<_>>());
        assert!(cells.iter().all(|p| p.lines.len() == 1));
        for cell in &mut cells {
            cell.translation = Some("译文".into());
        }
        let (cleaned, _) = crate::inpaint::clean(&img, &cells);
        assert!(cleaned.get_pixel(20, 20).0.iter().all(|&channel| channel > 245));
        assert!((0..65).all(|y| cleaned.get_pixel(44, y) == img.get_pixel(44, y)));
        assert!((0..100).all(|x| cleaned.get_pixel(x, 32) == img.get_pixel(x, 32)));
    }

    #[test]
    fn repeated_columns_preserve_dense_borderless_table_cells_in_reading_order() {
        let img = RgbImage::new(150, 65);
        let cells = build_paragraphs(&img, vec![
            line(10.0, 11.0, 30.0, 20.0, "Name"),
            line(45.0, 8.0, 40.0, 20.0, "Status"),
            line(12.0, 35.0, 20.0, 20.0, "File"),
            line(64.0, 34.0, 21.0, 20.0, "Ready"),
        ]);
        assert_eq!(cells.iter().map(|p| p.text.as_str()).collect::<Vec<_>>(), ["Name", "Status", "File", "Ready"]);
    }

    #[test]
    fn dehyphenate_and_cjk_join() {
        assert_eq!(join_lines(&[line(0.0, 0.0, 1.0, 1.0, "transla-"), line(0.0, 2.0, 1.0, 1.0, "tion works")]), "translation works");
        assert_eq!(join_lines(&[line(0.0, 0.0, 1.0, 1.0, "这是第一行"), line(0.0, 2.0, 1.0, 1.0, "第二行")]), "这是第一行第二行");
    }

    #[test]
    fn colors_on_dark_ui() {
        let mut img = RgbImage::from_pixel(200, 60, image::Rgb([30, 31, 34]));
        for y in 20..40 {
            for x in 20..180 {
                if (x / 3) % 2 == 0 {
                    img.put_pixel(x, y, image::Rgb([230, 230, 230]));
                }
            }
        }
        let p = build_paragraphs(&img, vec![line(15.0, 15.0, 170.0, 30.0, "text")]);
        assert_eq!(p[0].bg, "#1e1f22");
        assert!(p[0].fg.starts_with("#e"), "fg {}", p[0].fg);
    }
}
