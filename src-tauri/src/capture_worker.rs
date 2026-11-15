//! OS capture calls may hang after sleep or a display change. Run them in a disposable process: a timeout
//! kills and reaps that process, so the next capture starts cleanly and no late frame can change the overlay.
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use image::RgbaImage;
use ocrtrans_core::capture::{self, MonitorInfo};
use serde::{Deserialize, Serialize};

const TIMEOUT: Duration = Duration::from_secs(8);
pub(crate) static STALL_NEXT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Serialize, Deserialize)]
struct Header {
    monitor: MonitorInfo,
    raw: [i32; 4],
    displays: Vec<[i32; 4]>,
    width: u32,
    height: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct Frame {
    pub monitor: MonitorInfo,
    pub raw: [i32; 4],
    pub displays: Vec<[i32; 4]>,
    /// Share pixels with the window's image protocol and a possible return to this display.
    pub image: Arc<RgbaImage>,
}

/// Internal entry point, before Tauri starts. Stdout is a length-prefixed header followed by raw RGBA pixels.
pub fn run() -> Result<()> {
    let mut args = std::env::args().skip(2);
    let cursor = args.next().and_then(|x| Some((x.parse().ok()?, args.next()?.parse().ok()?)));
    if args.any(|arg| arg == "--stall-test") {
        std::thread::sleep(Duration::from_secs(30));
    }
    let frame = capture_screen(cursor);
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match frame {
        Ok(frame) => write_frame(&mut out, &frame)?,
        Err(e) => write_header(&mut out, &serde_json::json!({ "error": format!("{e:#}") }))?,
    }
    out.flush()?;
    Ok(())
}

fn capture_screen(cursor: Option<(f64, f64)>) -> Result<Frame> {
    let (monitor, raw, displays) = crate::windows::monitor_at_point(cursor)?;
    let image = match std::env::var_os("OCRTRANS_TEST_SHOT") {
        Some(p) => {
            let (w, h) = ((monitor.width as f32 * monitor.scale_factor) as u32, (monitor.height as f32 * monitor.scale_factor) as u32);
            image::imageops::resize(&image::open(p)?.to_rgba8(), w, h, image::imageops::FilterType::Triangle)
        }
        #[cfg(target_os = "macos")]
        None => capture_macos(&monitor)?,
        #[cfg(not(target_os = "macos"))]
        None => capture::capture_monitor(monitor.id)?,
    };
    Ok(Frame { monitor, raw, displays, image: Arc::new(image) })
}

#[cfg(target_os = "macos")]
fn capture_macos(monitor: &MonitorInfo) -> Result<RgbaImage> {
    use std::ffi::{c_char, CStr};
    extern "C" {
        fn ocr_capture_display(id: u32, width: u32, height: u32, rgba: *mut u8, error: *mut c_char, capacity: usize) -> i32;
    }
    let width = (monitor.width as f64 * f64::from(monitor.scale_factor)).round() as u32;
    let height = (monitor.height as f64 * f64::from(monitor.scale_factor)).round() as u32;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 128 * 1024 * 1024 {
        bail!("截图尺寸无效");
    }
    let mut image = RgbaImage::new(width, height);
    let mut error = [0; 512];
    let status = unsafe { ocr_capture_display(monitor.id, width, height, image.as_mut_ptr(), error.as_mut_ptr(), error.len()) };
    match status {
        0 => Ok(image),
        1 if crate::windows::has_screen_access() => capture::capture_monitor(monitor.id),
        1 | 2 => bail!("macOS 未允许此次截图。请在系统设置 → 隐私与安全性 → 屏幕录制中检查 OCR Trans 的授权，然后重试。"),
        _ => bail!("ScreenCaptureKit：{}", unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy()),
    }
}

fn write_header(out: &mut impl Write, header: &impl Serialize) -> Result<()> {
    let header = serde_json::to_vec(header)?;
    out.write_all(&(header.len() as u32).to_le_bytes())?;
    out.write_all(&header)?;
    Ok(())
}

fn write_frame(out: &mut impl Write, frame: &Frame) -> Result<()> {
    write_header(out, &Header {
        monitor: frame.monitor.clone(),
        raw: frame.raw,
        displays: frame.displays.clone(),
        width: frame.image.width(),
        height: frame.image.height(),
    })?;
    out.write_all(frame.image.as_raw())?;
    Ok(())
}

fn read_frame(mut input: impl Read) -> Result<Frame> {
    let mut size = [0; 4];
    input.read_exact(&mut size).context("截图辅助进程未返回图像")?;
    let size = u32::from_le_bytes(size) as usize;
    if size > 8192 {
        bail!("截图数据头无效");
    }
    let mut bytes = vec![0; size];
    input.read_exact(&mut bytes)?;
    let header: serde_json::Value = serde_json::from_slice(&bytes)?;
    if let Some(error) = header.get("error").and_then(serde_json::Value::as_str) {
        bail!("{error}");
    }
    let h: Header = serde_json::from_value(header)?;
    let len = (u64::from(h.width) * u64::from(h.height)).saturating_mul(4);
    if h.width == 0 || h.height == 0 || len > 512 * 1024 * 1024 {
        bail!("截图尺寸无效");
    }
    let mut pixels = vec![0; len as usize];
    input.read_exact(&mut pixels)?;
    let image = RgbaImage::from_raw(h.width, h.height, pixels).ok_or_else(|| anyhow!("截图数据无效"))?;
    Ok(Frame { monitor: h.monitor, raw: h.raw, displays: h.displays, image: Arc::new(image) })
}

pub(crate) fn capture(cursor: Option<(f64, f64)>) -> Result<Frame> {
    let mut cmd = Command::new(std::env::current_exe()?);
    cmd.arg("--capture-worker");
    if let Some((x, y)) = cursor {
        cmd.arg(x.to_string()).arg(y.to_string());
    } else {
        cmd.args(["NaN", "NaN"]); // no pointer reading; use the primary monitor
    }
    if STALL_NEXT.swap(false, std::sync::atomic::Ordering::SeqCst) {
        cmd.arg("--stall-test");
    }
    capture_with(&mut cmd, TIMEOUT)
}

fn capture_with(cmd: &mut Command, timeout: Duration) -> Result<Frame> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().context("无法启动截图辅助进程")?;
    let stdout = child.stdout.take().expect("piped stdout");
    let (tx, rx) = std::sync::mpsc::channel();
    // Read concurrently: a full pipe must not hold the child up while its parent waits.
    let reader = std::thread::spawn(move || { let _ = tx.send(read_frame(stdout)); });
    let result = rx.recv_timeout(timeout).map_err(|_| anyhow!("截屏超时，已自动重置截图服务，请重试"));
    // Always reap, including parse errors and timeouts. The helper owns no UI or shared application state.
    let _ = child.kill();
    let _ = child.wait();
    let _ = reader.join();
    result?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_round_trip_and_truncation() {
        let m = MonitorInfo { id: 7, name: "显示器".into(), x: -100, y: 0, width: 2, height: 1, scale_factor: 2.0, is_primary: false };
        let image = RgbaImage::from_raw(2, 1, vec![1, 2, 3, 255, 4, 5, 6, 255]).unwrap();
        let raw = [-200, 0, 4, 2];
        let frame = Frame { monitor: m, raw, displays: vec![raw, [0, 0, 100, 100]], image: Arc::new(image) };
        let mut bytes = vec![];
        write_frame(&mut bytes, &frame).unwrap();
        let actual = read_frame(bytes.as_slice()).unwrap();
        assert_eq!(actual.monitor.id, 7);
        assert_eq!(actual.monitor.name, "显示器");
        assert_eq!(actual.raw, frame.raw);
        assert_eq!(actual.displays, frame.displays);
        assert_eq!(actual.image, frame.image);
        assert!(read_frame(&bytes[..bytes.len() - 1]).is_err());
        assert!(read_frame(u32::MAX.to_le_bytes().as_slice()).is_err());
        let mut bytes = vec![];
        write_header(&mut bytes, &serde_json::json!({ "error": "monitor disconnected" })).unwrap();
        assert_eq!(read_frame(bytes.as_slice()).unwrap_err().to_string(), "monitor disconnected");
    }

    #[cfg(unix)]
    #[test]
    fn hung_helper_is_reaped_and_next_capture_can_run() {
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "exec sleep 30"]);
        let started = std::time::Instant::now();
        let error = capture_with(&mut cmd, Duration::from_millis(100)).unwrap_err();
        assert!(error.to_string().contains("超时"));
        assert!(started.elapsed() < Duration::from_secs(3));
        let mut next = Command::new("/usr/bin/true");
        let error = capture_with(&mut next, Duration::from_secs(1)).unwrap_err();
        assert!(error.to_string().contains("未返回图像"));
    }
}
