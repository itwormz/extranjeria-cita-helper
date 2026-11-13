//! Native attention presentation, independent from browser ownership and actions.
use serde::Deserialize;
#[derive(Default, Deserialize)]
pub struct StatusFacts {
    #[serde(default)]
    pub failed: bool,
    #[serde(default)]
    pub connection_lost: bool,
    #[serde(default)]
    pub booking_handoff: bool,
    #[serde(default)]
    pub browser_retained: bool,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub mode: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconStatus {
    Idle,
    Running,
    Paused,
    Attention,
    Error,
}
pub fn icon_status(state: &StatusFacts, active: bool, worker_error: bool) -> IconStatus {
    if worker_error || state.failed || state.connection_lost {
        IconStatus::Error
    } else if state.booking_handoff || state.browser_retained || state.mode == "manual" {
        IconStatus::Attention
    } else if state.paused || state.mode == "paused" {
        if active {
            IconStatus::Paused
        } else {
            IconStatus::Idle
        }
    } else if active && state.mode != "finished" {
        IconStatus::Running
    } else {
        IconStatus::Idle
    }
}
/// Original dot geometry/palette and double neutral/white outline, composited over the app icon.
pub fn status_pixels(
    base: &[u8],
    width: u32,
    height: u32,
    status: IconStatus,
) -> Result<Vec<u8>, &'static str> {
    if width == 0
        || height == 0
        || width > 1024
        || height > 1024
        || base.len() != width as usize * height as usize * 4
    {
        return Err("icon_invalid");
    }
    let mut output = base.to_vec();
    let color = match status {
        IconStatus::Idle => return Ok(output),
        IconStatus::Running => [55, 187, 120],
        IconStatus::Paused => [139, 142, 153],
        IconStatus::Attention => [237, 181, 72],
        IconStatus::Error => [229, 101, 104],
    };
    let size = f64::from(width.min(height));
    let cx = f64::from(width) * 0.8475;
    let cy = f64::from(height) * 0.8475;
    let radius = size * 0.1125;
    let outer = (size * 0.035).max(1.0) / 2.0;
    let white = (size * 0.02).max(0.75) / 2.0;
    for y in 0..height {
        for x in 0..width {
            if (f64::from(x) - cx).abs() > radius + outer + 1.0
                || (f64::from(y) - cy).abs() > radius + outer + 1.0
            {
                continue;
            }
            let index = (y as usize * width as usize + x as usize) * 4;
            let mut sums = [0.0; 4];
            for sy in 0..4 {
                for sx in 0..4 {
                    let d = ((f64::from(x) + (f64::from(sx) + 0.5) / 4.0 - cx).powi(2)
                        + (f64::from(y) + (f64::from(sy) + 0.5) / 4.0 - cy).powi(2))
                    .sqrt();
                    let pixel = if d < radius - outer {
                        [color[0], color[1], color[2], 255]
                    } else if (d - radius).abs() <= white {
                        [255; 4]
                    } else if (d - radius).abs() <= outer {
                        [147, 149, 158, 255]
                    } else {
                        [
                            base[index],
                            base[index + 1],
                            base[index + 2],
                            base[index + 3],
                        ]
                    };
                    let alpha = f64::from(pixel[3]) / 255.0;
                    for channel in 0..3 {
                        sums[channel] += f64::from(pixel[channel]) * alpha;
                    }
                    sums[3] += alpha;
                }
            }
            if sums[3] > 0.0 {
                for channel in 0..3 {
                    output[index + channel] = (sums[channel] / sums[3]).round() as u8;
                }
                output[index + 3] = (sums[3] / 16.0 * 255.0).round() as u8;
            }
        }
    }
    Ok(output)
}
/// Shrink an RGBA icon by an integer `factor` with an alpha-weighted box average. Windows scales a 512 px window
/// icon down to 16-32 px roughly; handing it a 64 px version keeps the small icons sharp.
pub fn shrink_icon(rgba: &[u8], width: u32, height: u32, factor: u32) -> Result<(Vec<u8>, u32, u32), &'static str> {
    if factor == 0 || width == 0 || height == 0 || width % factor != 0 || height % factor != 0 {
        return Err("icon_size_invalid");
    }
    if rgba.len() != (width as usize) * (height as usize) * 4 {
        return Err("icon_size_invalid");
    }
    let (out_w, out_h) = (width / factor, height / factor);
    let mut out = vec![0u8; (out_w as usize) * (out_h as usize) * 4];
    let area = f64::from(factor * factor);
    for y in 0..out_h {
        for x in 0..out_w {
            let (mut sums, mut alpha_sum) = ([0.0f64; 3], 0.0f64);
            for dy in 0..factor {
                for dx in 0..factor {
                    let index = (((y * factor + dy) * width + x * factor + dx) as usize) * 4;
                    let alpha = f64::from(rgba[index + 3]) / 255.0;
                    for channel in 0..3 {
                        sums[channel] += f64::from(rgba[index + channel]) * alpha;
                    }
                    alpha_sum += alpha;
                }
            }
            let target = ((y * out_w + x) as usize) * 4;
            if alpha_sum > 0.0 {
                for channel in 0..3 {
                    out[target + channel] = (sums[channel] / alpha_sum).round() as u8;
                }
            }
            out[target + 3] = (alpha_sum / area * 255.0).round() as u8;
        }
    }
    Ok((out, out_w, out_h))
}
/// Monotonic cadence: delayed polls produce one chime, never a burst of missed repeats.
pub struct AttentionTimer {
    interval: u64,
    next: Option<u64>,
    repeat: bool,
}
impl AttentionTimer {
    pub fn new(interval: u64) -> Self {
        Self {
            interval: interval.clamp(100, 86_400_000),
            next: None,
            repeat: false,
        }
    }
    pub fn set_interval(&mut self, interval: u64) {
        let interval = interval.clamp(100, 86_400_000);
        if let Some(next) = self.next {
            // Anchor to the last scheduled chime, do not restart the alarm or play a new one now.
            self.next = Some(next.saturating_sub(self.interval).saturating_add(interval));
        }
        self.interval = interval;
    }
    pub fn start(&mut self, now: u64, repeat: bool) {
        self.next = Some(now);
        self.repeat = repeat;
    }
    pub fn stop(&mut self) {
        self.next = None;
    }
    pub fn due(&mut self, now: u64) -> bool {
        if !self.next.is_some_and(|at| now >= at) {
            return false;
        }
        self.next = self.repeat.then(|| now.saturating_add(self.interval));
        true
    }
}
#[cfg(feature = "shell")]
pub fn apply_icon(window: &tauri::WebviewWindow, status: IconStatus) -> Result<(), &'static str> {
    use tauri::Manager;
    let image = window
        .app_handle()
        .default_window_icon()
        .ok_or("icon_unavailable")?;
    #[cfg(not(target_os = "macos"))]
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        // The executable carries a multi-size icon that Windows picks from by itself (sharp at 16-256 px), so
        // the idle state leaves it alone. Only a status badge, or going back from one, sets a window icon, and
        // then a 64 px version: a 512 px picture shrunk by the system looks soft.
        static OVERRIDDEN: AtomicBool = AtomicBool::new(false);
        if status == IconStatus::Idle && !OVERRIDDEN.load(Ordering::Relaxed) {
            return Ok(());
        }
        let factor = (image.width() / 64).max(1);
        let (small, width, height) = shrink_icon(image.rgba(), image.width(), image.height(), factor)
            .unwrap_or_else(|_| (image.rgba().to_vec(), image.width(), image.height()));
        let pixels = status_pixels(&small, width, height, status)?;
        OVERRIDDEN.store(true, Ordering::Relaxed);
        return window
            .set_icon(tauri::image::Image::new_owned(pixels, width, height))
            .map_err(|_| "icon_unavailable");
    }
    #[cfg(target_os = "macos")]
    {
        let pixels = status_pixels(image.rgba(), image.width(), image.height(), status)?;
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, image.width(), image.height());
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .map_err(|_| "icon_unavailable")?
                .write_image_data(&pixels)
                .map_err(|_| "icon_unavailable")?;
        }
        let set = move || -> Result<(), &'static str> {
            use objc2::{AllocAnyThread, MainThreadMarker};
            use objc2_app_kit::{NSApplication, NSImage};
            use objc2_foundation::NSData;
            let mtm = MainThreadMarker::new().ok_or("icon_unavailable")?;
            let data = NSData::with_bytes(&bytes);
            let image = NSImage::initWithData(NSImage::alloc(), &data).ok_or("icon_unavailable")?;
            // Main-thread AppKit call; NSImage is retained for the call and copied/retained by NSApplication.
            unsafe {
                NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image));
            }
            Ok(())
        };
        if objc2::MainThreadMarker::new().is_some() {
            return set();
        }
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        window
            .run_on_main_thread(move || {
                let _ = tx.send(set());
            })
            .map_err(|_| "icon_unavailable")?;
        rx.recv_timeout(std::time::Duration::from_secs(2))
            .map_err(|_| "icon_unavailable")?
    }
}
