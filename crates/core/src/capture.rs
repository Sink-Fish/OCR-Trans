//! Screen capture via xcap (macOS: ScreenCaptureKit/CoreGraphics, Windows: DXGI/GDI).

use anyhow::{anyhow, Result};
use image::RgbaImage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub id: u32,
    pub name: String,
    /// Logical position/size (points on macOS, DIP on Windows)
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub is_primary: bool,
}

pub fn monitors() -> Result<Vec<MonitorInfo>> {
    xcap::Monitor::all()?
        .into_iter()
        .map(|m| {
            Ok(MonitorInfo {
                id: m.id()?,
                name: m.name().unwrap_or_default(),
                x: m.x()?,
                y: m.y()?,
                width: m.width()?,
                height: m.height()?,
                scale_factor: m.scale_factor().unwrap_or(1.0),
                is_primary: m.is_primary().unwrap_or(false),
            })
        })
        .collect()
}

/// Capture a whole monitor at native (physical pixel) resolution.
pub fn capture_monitor(id: u32) -> Result<RgbaImage> {
    let m = xcap::Monitor::all()?
        .into_iter()
        .find(|m| m.id().ok() == Some(id))
        .ok_or_else(|| anyhow!("monitor {id} not found"))?;
    Ok(m.capture_image()?)
}

/// Capture the monitor containing the logical point (x, y).
pub fn capture_at(x: i32, y: i32) -> Result<(MonitorInfo, RgbaImage)> {
    let m = xcap::Monitor::from_point(x, y)?;
    let id = m.id()?;
    let info = monitors()?.into_iter().find(|i| i.id == id).ok_or_else(|| anyhow!("monitor vanished"))?;
    Ok((info, m.capture_image()?))
}

/// Map a global logical rect onto a monitor capture of `img` pixels: returns the pixel rect (x, y, w, h).
pub fn region_in_image(info: &MonitorInfo, img: (u32, u32), x: i32, y: i32, w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    if info.width == 0 || info.height == 0 || img.0 == 0 || img.1 == 0 || w == 0 || h == 0 {
        return None;
    }
    // Intersect both edges before scaling. Clamping only the origin would capture unrelated
    // pixels for a selection crossing the monitor's top/left, or entirely outside it.
    let axis = |start: i32, size: u32, origin: i32, extent: u32, pixels: u32| {
        let start = i64::from(start) - i64::from(origin);
        let lo = start.max(0);
        let hi = (start + i64::from(size)).min(i64::from(extent));
        if hi <= lo {
            return None;
        }
        let scale = f64::from(pixels) / f64::from(extent);
        let a = (lo as f64 * scale).round() as u32;
        let b = ((hi as f64 * scale).round() as u32).min(pixels);
        (b > a).then_some((a, b - a))
    };
    let (rx, rw) = axis(x, w, info.x, info.width, img.0)?;
    let (ry, rh) = axis(y, h, info.y, info.height, img.1)?;
    (rw > 0 && rh > 0).then_some((rx, ry, rw, rh))
}

/// Capture a region given in global logical coordinates.
pub fn capture_region(x: i32, y: i32, w: u32, h: u32) -> Result<RgbaImage> {
    if w == 0 || h == 0 {
        return Err(anyhow!("empty region"));
    }
    let cx = i32::try_from(i64::from(x) + i64::from(w) / 2).map_err(|_| anyhow!("region outside supported screen coordinates"))?;
    let cy = i32::try_from(i64::from(y) + i64::from(h) / 2).map_err(|_| anyhow!("region outside supported screen coordinates"))?;
    let (info, img) = capture_at(cx, cy)?;
    let (rx, ry, rw, rh) = region_in_image(&info, img.dimensions(), x, y, w, h).ok_or_else(|| anyhow!("empty region"))?;
    Ok(image::imageops::crop_imm(&img, rx, ry, rw, rh).to_image())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(x: i32, y: i32, w: u32, h: u32, s: f32) -> MonitorInfo {
        MonitorInfo { id: 1, name: "t".into(), x, y, width: w, height: h, scale_factor: s, is_primary: true }
    }

    #[test]
    fn retina_region() {
        let m = mon(0, 0, 1728, 1117, 2.0);
        assert_eq!(region_in_image(&m, (3456, 2234), 0, 0, 900, 400), Some((0, 0, 1800, 800)));
    }

    #[test]
    fn secondary_monitor_with_offset_and_clamp() {
        // external 1x monitor to the left of the primary, region running past its right edge
        let m = mon(-1920, 0, 1920, 1080, 1.0);
        assert_eq!(region_in_image(&m, (1920, 1080), -100, 1000, 300, 300), Some((1820, 1000, 100, 80)));
        assert_eq!(region_in_image(&m, (0, 0), 0, 0, 10, 10), None);
    }

    #[test]
    fn region_intersects_each_monitor_edge() {
        let m = mon(100, 200, 800, 600, 2.0);
        assert_eq!(region_in_image(&m, (1600, 1200), 90, 180, 40, 50), Some((0, 0, 60, 60)));
        for (x, y, w, h) in [(0, 0, 50, 50), (900, 200, 10, 10), (100, 800, 10, 10), (100, 200, 0, 10)] {
            assert_eq!(region_in_image(&m, (1600, 1200), x, y, w, h), None);
        }
        // Very distant displays cannot overflow signed coordinate subtraction.
        assert_eq!(region_in_image(&mon(i32::MIN, 0, 100, 100, 1.0), (100, 100), i32::MAX, 0, 10, 10), None);
    }

    #[test]
    fn region_uses_both_capture_dimensions() {
        assert_eq!(region_in_image(&mon(0, 0, 100, 100, 1.0), (200, 300), 10, 10, 20, 20), Some((20, 30, 40, 60)));
        assert_eq!(region_in_image(&mon(0, 0, 100, 0, 1.0), (100, 100), 0, 0, 10, 10), None);
    }
}
