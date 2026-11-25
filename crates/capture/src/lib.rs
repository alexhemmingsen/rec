//! Screen region recording for Rec.
//!
//! The crate records one rectangular region of one display to an H.264 MP4
//! file, using each platform's native capture API:
//!
//! - macOS: ScreenCaptureKit for frames, AVAssetWriter (VideoToolbox) for
//!   hardware H.264 encoding.
//! - Windows: Windows Graphics Capture for frames, Media Foundation for
//!   H.264.
//! - Linux: GStreamer, with `ximagesrc` on X11 and the xdg-desktop-portal
//!   ScreenCast API (PipeWire) on Wayland.
//!
//! The platform-independent parts (geometry, output sizing) live here so they
//! can be unit tested everywhere.

use std::path::PathBuf;
use std::time::Duration;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;

#[cfg(any(target_os = "linux", test))]
mod gst_pipeline;

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
mod unsupported;
#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
use unsupported as platform;

#[cfg(target_os = "macos")]
pub use macos::writer::Mp4Writer;
#[cfg(windows)]
pub use windows::Mp4Writer;

/// Largest frame (in pixels) H.264 level 5.2 can encode: 36 864 macroblocks
/// of 16x16, i.e. 4096x2304. Bigger regions are scaled down to fit.
pub const MAX_ENCODED_PIXELS: u64 = 4096 * 2304;

/// A rectangle in logical points (macOS) or logical pixels (other platforms).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Clamp the rectangle so it lies inside `bounds`, which has its origin at
    /// (0, 0). Returns `None` if nothing of it is left.
    pub fn clamp_to(&self, bounds_width: f64, bounds_height: f64) -> Option<Rect> {
        let x0 = self.x.max(0.0);
        let y0 = self.y.max(0.0);
        let x1 = (self.x + self.width).min(bounds_width);
        let y1 = (self.y + self.height).min(bounds_height);
        if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
            return None;
        }
        Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
    }
}

/// One display that can be recorded.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayInfo {
    /// Platform display id (`CGDirectDisplayID` on macOS).
    pub id: u32,
    /// Frame in global logical coordinates, origin at the top-left of the
    /// main display, y growing downwards.
    pub frame: Rect,
    /// The same frame in physical pixels (global). Windows and Linux place
    /// windows with these so mixed-DPI setups line up exactly.
    pub pixel_frame: Rect,
    /// Physical pixels per logical point (2.0 on Retina).
    pub scale_factor: f64,
    pub is_main: bool,
}

/// What to record and where to save it.
#[derive(Clone, Debug)]
pub struct RecordRequest {
    pub display_id: u32,
    /// Region relative to the display's top-left corner, in logical points.
    pub region: Rect,
    pub output: PathBuf,
    pub fps: u32,
    pub show_cursor: bool,
    /// A rectangle inside the region (region-relative, logical points) that
    /// must not appear in the video: where Rec's Stop pill sits when the
    /// region fills the screen. macOS and Windows exclude Rec's windows
    /// from the capture themselves; Linux can't, so it covers this area with
    /// what was there when recording started.
    pub exclude: Option<Rect>,
}

/// A small RGBA image, used for the History thumbnail.
#[derive(Clone, Debug)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// Tightly packed RGBA, `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

/// The result of a finished recording.
#[derive(Clone, Debug)]
pub struct RecordingSummary {
    pub path: PathBuf,
    pub duration: Duration,
    pub width: u32,
    pub height: u32,
    pub frames: u64,
    /// A downscaled copy of the first frame, for a thumbnail.
    pub thumbnail: Option<Frame>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Permission {
    Granted,
    /// Not granted yet, or denied. macOS does not distinguish the two.
    NotGranted,
    /// The platform needs no permission to record.
    NotRequired,
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("Screen Recording permission has not been granted")]
    PermissionDenied,
    #[error("display {0} is not available")]
    DisplayNotFound(u32),
    #[error("the selected region is empty")]
    EmptyRegion,
    #[error("no frames were captured")]
    NoFrames,
    #[error("screen recording is not supported on this platform yet")]
    Unsupported,
    #[error("timed out waiting for {0}")]
    Timeout(&'static str),
    #[error("{0}")]
    Platform(String),
}

pub type Result<T> = std::result::Result<T, CaptureError>;

/// Pixel size of the encoded video for a region of `width` x `height` logical
/// points on a display with `scale` pixels per point.
///
/// Full sharpness (Retina) by default. Dimensions are rounded down to even
/// numbers, which H.264 with 4:2:0 chroma requires, and scaled down uniformly
/// when the frame would exceed what H.264 can encode.
pub fn output_dimensions(width: f64, height: f64, scale: f64) -> (u32, u32) {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let mut w = (width * scale).max(2.0);
    let mut h = (height * scale).max(2.0);
    let pixels = w * h;
    if pixels > MAX_ENCODED_PIXELS as f64 {
        let factor = (MAX_ENCODED_PIXELS as f64 / pixels).sqrt();
        w *= factor;
        h *= factor;
    }
    (even_floor(w), even_floor(h))
}

fn even_floor(v: f64) -> u32 {
    let v = v.floor() as u32;
    (v & !1).max(2)
}

/// Target average bitrate for screen content: about 0.15 bits per pixel per
/// frame, kept between 2 and 24 Mbit/s. Text stays crisp; files stay small.
pub fn target_bitrate(width: u32, height: u32, fps: u32) -> u32 {
    let bits = width as f64 * height as f64 * fps.max(1) as f64 * 0.15;
    bits.clamp(2_000_000.0, 24_000_000.0) as u32
}

/// Downscale a BGRA buffer with row stride `stride` to at most `max_width`
/// pixels wide, returning RGBA. Nearest-neighbour sampling: it is only used
/// for thumbnails.
pub fn bgra_to_rgba_thumbnail(
    bgra: &[u8],
    width: usize,
    height: usize,
    stride: usize,
    max_width: usize,
) -> Option<Frame> {
    if width == 0
        || height == 0
        || stride < width * 4
        || bgra.len() < stride * (height - 1) + width * 4
    {
        return None;
    }
    let step = (width as f64 / max_width.max(1) as f64).max(1.0);
    let out_w = ((width as f64 / step).floor() as usize).max(1);
    let out_h = ((height as f64 / step).floor() as usize).max(1);
    let mut rgba = Vec::with_capacity(out_w * out_h * 4);
    for oy in 0..out_h {
        let sy = ((oy as f64 * step) as usize).min(height - 1);
        let row = &bgra[sy * stride..];
        for ox in 0..out_w {
            let sx = ((ox as f64 * step) as usize).min(width - 1);
            let p = &row[sx * 4..sx * 4 + 4];
            rgba.extend_from_slice(&[p[2], p[1], p[0], 255]);
        }
    }
    Some(Frame {
        width: out_w as u32,
        height: out_h as u32,
        rgba,
    })
}

/// Whether this process may record the screen.
pub fn permission_status() -> Permission {
    platform::permission_status()
}

/// Ask the OS for Screen Recording permission. On macOS this shows the system
/// prompt the first time; afterwards it only returns the current state.
pub fn request_permission() -> bool {
    platform::request_permission()
}

/// Displays that can be recorded.
pub fn displays() -> Vec<DisplayInfo> {
    platform::displays()
}

/// A recording in progress. Call [`Recording::stop`] to finish the file.
pub struct Recording {
    inner: platform::Recording,
}

impl Recording {
    /// Start recording. Returns once frames are flowing (or capture failed).
    pub fn start(request: RecordRequest) -> Result<Recording> {
        if request.region.width < 1.0 || request.region.height < 1.0 {
            return Err(CaptureError::EmptyRegion);
        }
        Ok(Recording {
            inner: platform::Recording::start(request)?,
        })
    }

    /// Stop capturing and finish writing the MP4.
    pub fn stop(self) -> Result<RecordingSummary> {
        self.inner.stop()
    }

    /// Stop capturing and delete the partial file.
    pub fn cancel(self) {
        self.inner.cancel()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retina_region_doubles_and_stays_even() {
        assert_eq!(output_dimensions(400.0, 300.0, 2.0), (800, 600));
        assert_eq!(output_dimensions(401.5, 301.5, 2.0), (802, 602));
        assert_eq!(output_dimensions(333.0, 201.0, 1.0), (332, 200));
    }

    #[test]
    fn tiny_regions_never_go_below_two_pixels() {
        assert_eq!(output_dimensions(0.2, 0.2, 1.0), (2, 2));
    }

    #[test]
    fn oversized_regions_are_scaled_to_fit_h264() {
        // A 5K display at full sharpness: 5120x2880.
        let (w, h) = output_dimensions(2560.0, 1440.0, 2.0);
        assert!((w as u64) * (h as u64) <= MAX_ENCODED_PIXELS, "{w}x{h}");
        assert_eq!(w % 2, 0);
        assert_eq!(h % 2, 0);
        // Aspect ratio is kept (16:9, within rounding).
        let ratio = w as f64 / h as f64;
        assert!((ratio - 16.0 / 9.0).abs() < 0.01, "{ratio}");
    }

    #[test]
    fn invalid_scale_falls_back_to_one() {
        assert_eq!(output_dimensions(100.0, 100.0, 0.0), (100, 100));
        assert_eq!(output_dimensions(100.0, 100.0, f64::NAN), (100, 100));
    }

    #[test]
    fn bitrate_is_bounded() {
        assert_eq!(target_bitrate(10, 10, 30), 2_000_000);
        assert_eq!(target_bitrate(4096, 2304, 30), 24_000_000);
        let mid = target_bitrate(1600, 1000, 30);
        assert_eq!(mid, 7_200_000);
    }

    #[test]
    fn clamp_keeps_rect_inside_display() {
        let r = Rect::new(-10.0, 20.0, 100.0, 1000.0)
            .clamp_to(1440.0, 900.0)
            .unwrap();
        assert_eq!(r, Rect::new(0.0, 20.0, 90.0, 880.0));
        assert!(
            Rect::new(2000.0, 0.0, 10.0, 10.0)
                .clamp_to(1440.0, 900.0)
                .is_none()
        );
    }

    #[test]
    fn thumbnail_converts_bgra_and_downscales() {
        // 4x2 image, stride 20 (4 bytes padding per row). Pixel (x, y) has
        // blue = x, green = y, red = 200.
        let (w, h, stride) = (4usize, 2usize, 20usize);
        let mut bgra = vec![0u8; stride * h];
        for y in 0..h {
            for x in 0..w {
                let i = y * stride + x * 4;
                bgra[i..i + 4].copy_from_slice(&[x as u8, y as u8, 200, 255]);
            }
        }
        let full = bgra_to_rgba_thumbnail(&bgra, w, h, stride, 100).unwrap();
        assert_eq!((full.width, full.height), (4, 2));
        assert_eq!(&full.rgba[0..4], &[200, 0, 0, 255]);
        let last = full.rgba.len() - 4;
        assert_eq!(&full.rgba[last..], &[200, 1, 3, 255]);

        let half = bgra_to_rgba_thumbnail(&bgra, w, h, stride, 2).unwrap();
        assert_eq!((half.width, half.height), (2, 1));
        assert_eq!(half.rgba.len(), 2 * 4);
    }

    #[test]
    fn thumbnail_rejects_short_buffers() {
        assert!(bgra_to_rgba_thumbnail(&[0; 10], 4, 2, 16, 10).is_none());
        assert!(bgra_to_rgba_thumbnail(&[], 0, 0, 0, 10).is_none());
    }

    #[test]
    fn empty_region_is_rejected_before_touching_the_platform() {
        let err = Recording::start(RecordRequest {
            display_id: 0,
            region: Rect::new(0.0, 0.0, 0.0, 10.0),
            output: PathBuf::from("/tmp/never.mp4"),
            fps: 30,
            show_cursor: true,
            exclude: None,
        })
        .err()
        .unwrap();
        assert!(matches!(err, CaptureError::EmptyRegion));
    }
}
