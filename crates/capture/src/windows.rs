//! Windows backend: Windows Graphics Capture for frames, Media Foundation's
//! H.264 encoder (through windows-capture's `VideoEncoder`) for the MP4.
//!
//! Rec's own windows are kept out of the video by the app, which marks them
//! with `WDA_EXCLUDEFROMCAPTURE` (Tauri's content protection).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::core::BOOL;
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::encoder::{
    AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder,
    VideoSettingsSubType,
};
use windows_capture::frame::Frame as CaptureFrame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

use crate::{
    CaptureError, DisplayInfo, Frame, Permission, RecordRequest, RecordingSummary, Rect, Result,
    bgra_to_rgba_thumbnail, output_dimensions, target_bitrate,
};

const THUMBNAIL_MAX_WIDTH: usize = 640;
/// `MONITORINFOF_PRIMARY` from WinUser.h.
const MONITORINFOF_PRIMARY: u32 = 1;
/// Windows timestamps are in 100 ns ticks.
const TICKS_PER_SECOND: i64 = 10_000_000;

pub fn permission_status() -> Permission {
    Permission::NotRequired
}

pub fn request_permission() -> bool {
    true
}

struct RawMonitor {
    handle: HMONITOR,
    rect: RECT,
    primary: bool,
    dpi: u32,
}

fn monitors() -> Vec<RawMonitor> {
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        let list = unsafe { &mut *(data.0 as *mut Vec<HMONITOR>) };
        list.push(monitor);
        BOOL(1)
    }
    let mut handles: Vec<HMONITOR> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut handles as *mut _ as isize),
        );
    }
    handles
        .into_iter()
        .filter_map(|handle| unsafe {
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(handle, &mut info).as_bool() {
                return None;
            }
            let (mut dx, mut dy) = (96u32, 96u32);
            if GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_err() {
                dx = 96;
            }
            Some(RawMonitor {
                handle,
                rect: info.rcMonitor,
                primary: info.dwFlags & MONITORINFOF_PRIMARY != 0,
                dpi: dx.max(1),
            })
        })
        .collect()
}

/// Display ids are 1-based positions in the system's monitor order.
pub fn displays() -> Vec<DisplayInfo> {
    monitors()
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let scale = m.dpi as f64 / 96.0;
            let px = Rect::new(
                m.rect.left as f64,
                m.rect.top as f64,
                (m.rect.right - m.rect.left) as f64,
                (m.rect.bottom - m.rect.top) as f64,
            );
            DisplayInfo {
                id: i as u32 + 1,
                frame: Rect::new(
                    px.x / scale,
                    px.y / scale,
                    px.width / scale,
                    px.height / scale,
                ),
                pixel_frame: px,
                scale_factor: scale,
                is_main: m.primary,
            }
        })
        .collect()
}

/// Now, on the same clock as `Direct3D11CaptureFrame::SystemRelativeTime`.
fn now_ticks() -> i64 {
    let (mut counter, mut freq) = (0i64, 0i64);
    unsafe {
        let _ = QueryPerformanceCounter(&mut counter);
        let _ = QueryPerformanceFrequency(&mut freq);
    }
    if freq <= 0 {
        return 0;
    }
    ((counter as i128 * TICKS_PER_SECOND as i128) / freq as i128) as i64
}

/// Where to crop in the monitor's pixels, and the encoded size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Geometry {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub out_width: u32,
    pub out_height: u32,
}

/// Map a region in logical units to a crop in the monitor's pixels.
pub(crate) fn geometry(
    region: Rect,
    scale: f64,
    monitor_width: u32,
    monitor_height: u32,
) -> Option<Geometry> {
    let x = (region.x * scale).round().max(0.0) as u32;
    let y = (region.y * scale).round().max(0.0) as u32;
    let mut width = ((region.width * scale).round() as u32).min(monitor_width.saturating_sub(x));
    let mut height = ((region.height * scale).round() as u32).min(monitor_height.saturating_sub(y));
    width &= !1;
    height &= !1;
    if width < 2 || height < 2 {
        return None;
    }
    let (out_width, out_height) = output_dimensions(width as f64, height as f64, 1.0);
    Some(Geometry {
        x,
        y,
        width,
        height,
        out_width,
        out_height,
    })
}

/// Nearest-neighbour scale of a tightly packed BGRA image. Only used when a
/// region is too large for H.264.
fn scale_bgra(src: &[u8], w: u32, h: u32, out_w: u32, out_h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (out_w * out_h * 4) as usize];
    for oy in 0..out_h {
        let sy = (oy as u64 * h as u64 / out_h as u64) as u32;
        for ox in 0..out_w {
            let sx = (ox as u64 * w as u64 / out_w as u64) as u32;
            let s = ((sy * w + sx) * 4) as usize;
            let d = ((oy * out_w + ox) * 4) as usize;
            out[d..d + 4].copy_from_slice(&src[s..s + 4]);
        }
    }
    out
}

/// Media Foundation wants bottom-up rows for raw buffers.
pub(crate) fn flip_rows(src: &[u8], width: u32, height: u32) -> Vec<u8> {
    let row = (width * 4) as usize;
    let mut out = Vec::with_capacity(src.len());
    for y in (0..height as usize).rev() {
        out.extend_from_slice(&src[y * row..(y + 1) * row]);
    }
    out
}

/// An MP4 writer for BGRA frames with 100 ns timestamps.
///
/// Media Foundation's transcoder assumes a constant frame rate, so frames
/// land on exact 1/fps steps from the first one, and gaps (Windows Graphics
/// Capture only delivers frames when something changes) are filled by
/// repeating the last frame.
pub struct Mp4Writer {
    encoder: Option<VideoEncoder>,
    width: u32,
    height: u32,
    interval: i64,
    first: Option<i64>,
    last_index: Option<i64>,
    last_frame: Option<Vec<u8>>,
    frames: u64,
}

impl Mp4Writer {
    pub fn new(path: &std::path::Path, width: u32, height: u32, fps: u32) -> Result<Self> {
        if width < 2 || height < 2 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
            return Err(CaptureError::Platform(format!(
                "invalid video size {width}x{height}"
            )));
        }
        if path.exists() {
            std::fs::remove_file(path).map_err(|e| CaptureError::Platform(e.to_string()))?;
        }
        let fps = fps.max(1);
        let encoder = VideoEncoder::new(
            VideoSettingsBuilder::new(width, height)
                .sub_type(VideoSettingsSubType::H264)
                .frame_rate(fps)
                .bitrate(target_bitrate(width, height, fps)),
            AudioSettingsBuilder::new().disabled(true),
            ContainerSettingsBuilder::new(),
            path,
        )
        .map_err(|e| CaptureError::Platform(format!("cannot start the video encoder: {e}")))?;
        Ok(Mp4Writer {
            encoder: Some(encoder),
            width,
            height,
            interval: TICKS_PER_SECOND / fps as i64,
            first: None,
            last_index: None,
            last_frame: None,
            frames: 0,
        })
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    fn index(&self, ticks: i64) -> i64 {
        let first = self.first.unwrap_or(ticks);
        ((ticks - first) as f64 / self.interval as f64).round() as i64
    }

    fn send(&mut self, bottom_up: &[u8], index: i64) -> Result<()> {
        let first = self.first.unwrap_or(0);
        let encoder = self.encoder.as_mut().ok_or(CaptureError::NoFrames)?;
        encoder
            .send_frame_buffer(bottom_up, first + index * self.interval)
            .map_err(|e| CaptureError::Platform(format!("cannot write frame: {e}")))?;
        self.frames += 1;
        Ok(())
    }

    /// Repeat the last frame for every slot up to and including the one at
    /// `ticks`.
    pub fn fill_to(&mut self, ticks: i64) -> Result<()> {
        let (Some(last), Some(frame)) = (self.last_index, self.last_frame.take()) else {
            return Ok(());
        };
        let target = self.index(ticks);
        let mut result = Ok(());
        for i in last + 1..=target {
            if let Err(e) = self.send(&frame, i) {
                result = Err(e);
                break;
            }
            self.last_index = Some(i);
        }
        self.last_frame = Some(frame);
        result
    }

    /// Append a top-down BGRA frame (`width * height * 4` bytes). Returns
    /// false when its slot is already taken.
    pub fn append(&mut self, bgra: &[u8], ticks: i64) -> Result<bool> {
        self.first.get_or_insert(ticks);
        let index = self.index(ticks);
        if self.last_index.is_some_and(|last| index <= last) {
            return Ok(false);
        }
        self.fill_to(ticks - self.interval)?;
        let bottom_up = flip_rows(bgra, self.width, self.height);
        self.send(&bottom_up, index)?;
        self.last_index = Some(index);
        self.last_frame = Some(bottom_up);
        Ok(true)
    }

    /// Finish the file; with `end`, the last frame is held until then.
    pub fn finish(mut self, end: Option<i64>) -> Result<Duration> {
        if self.last_index.is_none() {
            return Err(CaptureError::NoFrames);
        }
        if let Some(end) = end {
            self.fill_to(end)?;
        }
        let last = self.last_index.unwrap_or(0);
        let encoder = self.encoder.take().ok_or(CaptureError::NoFrames)?;
        encoder
            .finish()
            .map_err(|e| CaptureError::Platform(format!("cannot finish the video file: {e}")))?;
        Ok(Duration::from_nanos(
            (last * self.interval).max(0) as u64 * 100,
        ))
    }
}

struct Shared {
    writer: Option<Mp4Writer>,
    geometry: Geometry,
    min_interval: i64,
    last_sent: Option<i64>,
    thumbnail: Option<Frame>,
    error: Option<String>,
    scratch: Vec<u8>,
}

struct Handler {
    shared: Arc<Mutex<Shared>>,
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = Arc<Mutex<Shared>>;
    type Error = String;

    fn new(ctx: Context<Self::Flags>) -> std::result::Result<Self, Self::Error> {
        Ok(Handler { shared: ctx.flags })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut CaptureFrame,
        _control: InternalCaptureControl,
    ) -> std::result::Result<(), Self::Error> {
        let ticks = frame.timestamp().map_err(|e| e.to_string())?.Duration;
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| "capture state poisoned".to_string())?;
        if let Some(last) = shared.last_sent {
            // 30 fps: skip frames that arrive sooner than the next slot.
            if ticks - last < shared.min_interval * 9 / 10 {
                return Ok(());
            }
        }
        let g = shared.geometry;
        let x1 = (g.x + g.width).min(frame.width());
        let y1 = (g.y + g.height).min(frame.height());
        if x1 <= g.x || y1 <= g.y {
            return Ok(());
        }
        let buffer = match frame.buffer_crop(g.x, g.y, x1, y1) {
            Ok(b) => b,
            Err(e) => {
                shared.error.get_or_insert_with(|| e.to_string());
                return Ok(());
            }
        };
        let (w, h) = (x1 - g.x, y1 - g.y);
        let mut scratch = std::mem::take(&mut shared.scratch);
        let pixels = buffer.as_nopadding_buffer(&mut scratch).to_vec();
        shared.scratch = scratch;
        if (w, h) != (g.width, g.height) {
            return Ok(()); // the monitor changed size mid-recording
        }
        if shared.thumbnail.is_none() {
            shared.thumbnail = bgra_to_rgba_thumbnail(
                &pixels,
                w as usize,
                h as usize,
                (w * 4) as usize,
                THUMBNAIL_MAX_WIDTH,
            );
        }
        let pixels = if (g.out_width, g.out_height) != (w, h) {
            scale_bgra(&pixels, w, h, g.out_width, g.out_height)
        } else {
            pixels
        };
        let result = match shared.writer.as_mut() {
            Some(writer) => writer.append(&pixels, ticks),
            None => return Ok(()),
        };
        match result {
            Ok(true) => shared.last_sent = Some(ticks),
            Ok(false) => {}
            Err(e) => {
                shared.error.get_or_insert_with(|| e.to_string());
            }
        }
        Ok(())
    }
}

pub struct Recording {
    control: Option<CaptureControl<Handler, String>>,
    shared: Arc<Mutex<Shared>>,
    path: std::path::PathBuf,
    width: u32,
    height: u32,
}

impl Recording {
    pub fn start(request: RecordRequest) -> Result<Recording> {
        let all = monitors();
        let index = request
            .display_id
            .checked_sub(1)
            .ok_or(CaptureError::DisplayNotFound(request.display_id))? as usize;
        let raw = all
            .get(index)
            .ok_or(CaptureError::DisplayNotFound(request.display_id))?;
        let scale = raw.dpi as f64 / 96.0;
        let (mw, mh) = (
            (raw.rect.right - raw.rect.left) as u32,
            (raw.rect.bottom - raw.rect.top) as u32,
        );
        let geometry = geometry(request.region, scale, mw, mh).ok_or(CaptureError::EmptyRegion)?;
        let writer = Mp4Writer::new(
            &request.output,
            geometry.out_width,
            geometry.out_height,
            request.fps,
        )?;
        let shared = Arc::new(Mutex::new(Shared {
            writer: Some(writer),
            geometry,
            min_interval: TICKS_PER_SECOND / request.fps.max(1) as i64,
            last_sent: None,
            thumbnail: None,
            error: None,
            scratch: Vec::new(),
        }));
        let monitor = Monitor::from_raw_hmonitor(raw.handle.0);
        let cursor = if request.show_cursor {
            CursorCaptureSettings::WithCursor
        } else {
            CursorCaptureSettings::WithoutCursor
        };

        // Ask for no yellow capture border; older Windows builds can't, so
        // fall back to the default.
        let mut control = None;
        let mut last_error = String::new();
        for border in [
            DrawBorderSettings::WithoutBorder,
            DrawBorderSettings::Default,
        ] {
            let settings = Settings::new(
                monitor,
                cursor,
                border,
                SecondaryWindowSettings::Default,
                MinimumUpdateIntervalSettings::Default,
                DirtyRegionSettings::Default,
                ColorFormat::Bgra8,
                shared.clone(),
            );
            match Handler::start_free_threaded(settings) {
                Ok(c) => {
                    control = Some(c);
                    break;
                }
                Err(e) => last_error = e.to_string(),
            }
        }
        let control = control.ok_or_else(|| {
            CaptureError::Platform(format!("cannot start screen capture: {last_error}"))
        })?;

        // Report a dead capture now rather than at Stop.
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let (frames, error) = {
                let s = shared
                    .lock()
                    .map_err(|_| CaptureError::Platform("capture state poisoned".into()))?;
                (
                    s.writer.as_ref().map(|w| w.frames()).unwrap_or(0),
                    s.error.clone(),
                )
            };
            if let Some(e) = error {
                let _ = control.stop();
                return Err(CaptureError::Platform(e));
            }
            if frames > 0 || control.is_finished() || Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(15));
        }

        // While the screen is still, keep the constant-rate stream going in
        // real time instead of filling the whole gap at Stop.
        let ticker_shared = Arc::downgrade(&shared);
        let lag = 2 * TICKS_PER_SECOND / request.fps.max(1) as i64;
        std::thread::spawn(move || {
            while let Some(shared) = ticker_shared.upgrade() {
                if let Ok(mut s) = shared.lock() {
                    let Some(writer) = s.writer.as_mut() else {
                        break;
                    };
                    if let Err(e) = writer.fill_to(now_ticks() - lag) {
                        s.error.get_or_insert_with(|| e.to_string());
                        break;
                    }
                }
                drop(shared);
                std::thread::sleep(Duration::from_millis(100));
            }
        });

        Ok(Recording {
            control: Some(control),
            shared,
            path: request.output,
            width: geometry.out_width,
            height: geometry.out_height,
        })
    }

    fn stop_capture(&mut self) {
        if let Some(control) = self.control.take()
            && let Err(e) = control.stop()
        {
            log::warn!("stopping capture: {e}");
        }
    }

    pub fn stop(mut self) -> Result<RecordingSummary> {
        let end = now_ticks();
        self.stop_capture();
        let (writer, thumbnail, error) = {
            let mut s = self
                .shared
                .lock()
                .map_err(|_| CaptureError::Platform("capture state poisoned".into()))?;
            (s.writer.take(), s.thumbnail.take(), s.error.take())
        };
        let writer = writer.ok_or(CaptureError::NoFrames)?;
        if let Some(e) = error {
            return Err(CaptureError::Platform(e));
        }
        let frames = writer.frames();
        let duration = writer.finish(Some(end))?;
        Ok(RecordingSummary {
            path: self.path.clone(),
            duration,
            width: self.width,
            height: self.height,
            frames,
            thumbnail,
        })
    }

    pub fn cancel(mut self) {
        self.stop_capture();
        if let Ok(mut s) = self.shared.lock()
            && let Some(w) = s.writer.take()
        {
            let _ = w.finish(None);
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_scales_and_keeps_even_sizes() {
        let g = geometry(Rect::new(10.0, 20.0, 401.0, 301.0), 1.5, 3840, 2160).unwrap();
        assert_eq!((g.x, g.y), (15, 30));
        assert_eq!((g.width, g.height), (602, 452));
        assert_eq!((g.out_width, g.out_height), (602, 452));
    }

    #[test]
    fn geometry_clamps_to_the_monitor() {
        let g = geometry(Rect::new(1900.0, 1000.0, 400.0, 400.0), 1.0, 1920, 1080).unwrap();
        assert_eq!((g.width, g.height), (20, 80));
        assert!(geometry(Rect::new(1919.0, 0.0, 10.0, 10.0), 1.0, 1920, 1080).is_none());
    }

    #[test]
    fn flip_rows_reverses_row_order() {
        let src: Vec<u8> = (0..2 * 3 * 4).map(|i| i as u8).collect();
        let out = flip_rows(&src, 2, 3);
        assert_eq!(&out[0..8], &src[16..24]);
        assert_eq!(&out[16..24], &src[0..8]);
    }
}
