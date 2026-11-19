//! Pure coordinate math (selection → crop, monitor matching), kept separate so it can be unit-tested.

/// A selection mapped onto the captured monitor image.
#[derive(Debug, Clone, PartialEq)]
pub struct Crop {
    /// physical pixel rect inside the captured image
    pub px: u32,
    pub py: u32,
    pub pw: u32,
    pub ph: u32,
    /// global logical rect (where the pin window goes)
    pub rect: [f64; 4],
    /// physical pixels per logical point
    pub scale: f64,
}

/// `sel` = [x, y, w, h] in overlay-local logical coordinates (w/h may be negative if dragged up/left).
/// `origin` = monitor's global logical origin, `mon_w` = its logical width, `img` = captured image size in pixels.
pub fn selection_to_crop(sel: [f64; 4], origin: (f64, f64), mon_w: f64, img: (u32, u32)) -> Option<Crop> {
    let (iw, ih) = img;
    if iw == 0 || ih == 0 || !mon_w.is_finite() || mon_w <= 0.0 || !origin.0.is_finite() || !origin.1.is_finite() || !sel.iter().all(|v| v.is_finite()) {
        return None;
    }
    let scale = iw as f64 / mon_w;
    let (mut x, mut y, mut w, mut h) = (sel[0], sel[1], sel[2], sel[3]);
    if w < 0.0 {
        x += w;
        w = -w;
    }
    if h < 0.0 {
        y += h;
        h = -h;
    }
    // clip to the monitor
    let (x0, y0) = (x.max(0.0), y.max(0.0));
    let (x1, y1) = ((x + w).min(iw as f64 / scale), (y + h).min(ih as f64 / scale));
    if x1 - x0 < 1.0 / scale || y1 - y0 < 1.0 / scale {
        return None;
    }
    let px = ((x0 * scale).round() as u32).min(iw - 1);
    let py = ((y0 * scale).round() as u32).min(ih - 1);
    let pw = (((x1 - x0) * scale).round() as u32).clamp(1, iw - px);
    let ph = (((y1 - y0) * scale).round() as u32).clamp(1, ih - py);
    Some(Crop { px, py, pw, ph, rect: [origin.0 + x0, origin.1 + y0, pw as f64 / scale, ph as f64 / scale], scale })
}

/// Windows saved rectangles use the capturing display's scale, including in older histories.
pub fn physical_anchor(rect: [f64; 4], scale: f32) -> [i32; 2] {
    [rect[0], rect[1]].map(|v| (v * f64::from(scale)).round() as i32)
}

/// Index of the monitor rect ([x, y, w, h]) containing the point.
pub fn monitor_at(rects: &[[f64; 4]], x: f64, y: f64) -> Option<usize> {
    rects.iter().position(|r| x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3])
}

/// Half-open native display bounds, including monitors left of or above the main display.
pub fn display_contains(rect: [i32; 4], (x, y): (f64, f64)) -> bool {
    let [left, top, width, height] = rect.map(f64::from);
    x >= left && x < left + width && y >= top && y < top + height
}

/// Size and place the text window (识字) for a selection `sel` ([x, y, w, h], logical) inside the monitor's work
/// area `work`. Wide captures put the image on top and the text below, others side by side (TextWindow.tsx uses
/// the same 1.4 aspect split). The window goes below the selection, else above, else as low as fits; always on screen.
pub fn text_window_frame(sel: [f64; 4], work: [f64; 4]) -> [f64; 4] {
    const PAD: f64 = 32.0; // page padding, both sides
    const BAR: f64 = 64.0; // header + action bar
    let (lw, lh) = (sel[2].max(1.0), sel[3].max(1.0));
    let (w, h) = if lw / lh >= 1.4 {
        let w = (lw + PAD).clamp(480.0, 960.0);
        let img_h = (lh * ((w - PAD) / lw).min(1.0)).clamp(80.0, 380.0);
        (w, img_h + PAD + 16.0 + 170.0 + BAR)
    } else {
        let h = (lh + PAD + 16.0).clamp(360.0, 680.0);
        let img_w = (lw * ((h - PAD) / lh).min(1.0)).clamp(160.0, 560.0);
        (img_w + PAD + 16.0 + 340.0, h)
    };
    let (w, h) = (w.min(work[2]), h.min(work[3]));
    let x = sel[0].clamp(work[0], work[0] + work[2] - w);
    let below = sel[1] + sel[3] + 8.0;
    let above = sel[1] - 8.0 - h;
    let y = if below + h <= work[1] + work[3] {
        below
    } else if above >= work[1] {
        above
    } else {
        work[1] + work[3] - h
    };
    [x, y, w, h]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_logical_windows_rects_resolve_to_distinct_physical_monitors() {
        let screens = [[0., 0., 1920., 1080.], [1920., 0., 3840., 2160.]];
        for (scale, expected) in [(1., 0), (2., 1)] {
            let [x,y] = physical_anchor([1100.,100.,40.,40.], scale);
            assert_eq!(monitor_at(&screens, f64::from(x), f64::from(y)), Some(expected));
        }
    }

    #[test]
    fn retina_2x() {
        // 1728x1080 pt monitor captured at 3456x2160
        let c = selection_to_crop([216.0, 112.0, 900.0, 356.0], (0.0, 0.0), 1728.0, (3456, 2160)).unwrap();
        assert_eq!((c.px, c.py, c.pw, c.ph), (432, 224, 1800, 712));
        assert_eq!(c.rect, [216.0, 112.0, 900.0, 356.0]);
        assert_eq!(c.scale, 2.0);
    }

    #[test]
    fn fractional_scale_and_offset_monitor() {
        // Windows 150 %: 2560 px wide monitor = 1706.67 logical, placed left of the primary (negative origin)
        let c = selection_to_crop([100.0, 50.0, 200.0, 100.0], (-1706.67, 0.0), 2560.0 / 1.5, (2560, 1440)).unwrap();
        assert_eq!((c.px, c.py, c.pw, c.ph), (150, 75, 300, 150));
        assert!((c.rect[0] - (-1606.67)).abs() < 0.01);
        assert!((c.scale - 1.5).abs() < 1e-9);
    }

    #[test]
    fn reversed_drag_and_clamping() {
        // dragged from bottom-right to top-left, partly outside the monitor
        let c = selection_to_crop([120.0, 80.0, -150.0, -100.0], (0.0, 0.0), 100.0, (200, 200)).unwrap();
        assert_eq!((c.px, c.py), (0, 0));
        assert_eq!((c.pw, c.ph), (200, 160));
        assert!(selection_to_crop([10.0, 10.0, 0.2, 50.0], (0.0, 0.0), 100.0, (200, 200)).is_none());
        assert!(selection_to_crop([10.0, 10.0, 5.0, 5.0], (0.0, 0.0), 100.0, (0, 0)).is_none());
        assert!(selection_to_crop([f64::NAN, 0.0, 5.0, 5.0], (0.0, 0.0), 100.0, (10, 10)).is_none());
        assert!(selection_to_crop([0.0, 0.0, 5.0, 5.0], (0.0, 0.0), f64::NAN, (10, 10)).is_none());
        assert!(selection_to_crop([0.0, 0.0, 5.0, 5.0], (f64::INFINITY, 0.0), 10.0, (10, 10)).is_none());
    }

    #[test]
    fn monitor_under_point() {
        // primary 1728x1117 at (0,0), external to the right, another above-left with negative origin
        let r = [[0.0, 0.0, 1728.0, 1117.0], [1728.0, 0.0, 1920.0, 1080.0], [-1920.0, -1080.0, 1920.0, 1080.0]];
        assert_eq!(monitor_at(&r, 10.0, 10.0), Some(0));
        assert_eq!(monitor_at(&r, 1728.0, 500.0), Some(1));
        assert_eq!(monitor_at(&r, -5.0, -5.0), Some(2));
        assert_eq!(monitor_at(&r, 5000.0, 5.0), None);
    }

    #[test]
    fn text_window_stays_on_screen() {
        let work = [0.0, 25.0, 1728.0, 1092.0];
        let inside = |f: [f64; 4]| f[0] >= work[0] && f[1] >= work[1] && f[0] + f[2] <= work[0] + work[2] + 1e-9 && f[1] + f[3] <= work[1] + work[3] + 1e-9;
        // wide line of text mid-screen: below it, image on top
        let f = text_window_frame([200.0, 200.0, 600.0, 60.0], work);
        assert_eq!((f[0], f[1]), (200.0, 268.0));
        assert!(f[2] >= 480.0 && f[3] < 500.0);
        // near the bottom: above the selection
        let f = text_window_frame([200.0, 900.0, 600.0, 150.0], work);
        assert!(f[1] + f[3] <= 892.0 && inside(f));
        // bottom-right corner, tall selection: pulled left and on screen
        let f = text_window_frame([1500.0, 300.0, 220.0, 800.0], work);
        assert!(inside(f) && f[0] < 1500.0);
        // full-screen selection: fits inside the work area
        assert!(inside(text_window_frame([0.0, 0.0, 1728.0, 1117.0], work)));
        // tiny selection still gets a usable window
        let f = text_window_frame([10.0, 30.0, 4.0, 4.0], work);
        assert!(f[2] >= 400.0 && f[3] >= 360.0 && inside(f));
        // secondary monitor with a negative origin
        let w2 = [-1920.0, 0.0, 1920.0, 1050.0];
        let f = text_window_frame([-300.0, 800.0, 290.0, 200.0], w2);
        assert!(f[0] >= -1920.0 && f[0] + f[2] <= 0.0 && f[1] + f[3] <= 1050.0);
    }
}
