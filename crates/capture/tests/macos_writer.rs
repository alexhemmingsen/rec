//! Encodes synthetic frames with the real AVAssetWriter / VideoToolbox path
//! and checks the resulting MP4. Needs no Screen Recording permission.
#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr::NonNull;

use objc2_core_foundation::CFRetained;
use objc2_core_media::CMTime;
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferCreate, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
    kCVPixelFormatType_32BGRA,
};
use rec_capture::{CaptureError, Mp4Writer};

fn temp_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rec-capture-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// A BGRA frame with a white square that moves with `t`.
fn make_frame(width: usize, height: usize, t: usize) -> CFRetained<CVPixelBuffer> {
    unsafe {
        let mut out: *mut CVPixelBuffer = std::ptr::null_mut();
        let status = CVPixelBufferCreate(
            None,
            width,
            height,
            kCVPixelFormatType_32BGRA,
            None,
            NonNull::from(&mut out),
        );
        assert_eq!(status, 0, "CVPixelBufferCreate failed");
        let buffer = CFRetained::from_raw(NonNull::new(out).unwrap());
        assert_eq!(
            CVPixelBufferLockBaseAddress(&buffer, CVPixelBufferLockFlags(0)),
            0
        );
        let base = CVPixelBufferGetBaseAddress(&buffer) as *mut u8;
        let stride = CVPixelBufferGetBytesPerRow(&buffer);
        let pixels = std::slice::from_raw_parts_mut(base, stride * height);
        let sq = 40;
        let sx = (t * 5) % (width - sq);
        for y in 0..height {
            for x in 0..width {
                let i = y * stride + x * 4;
                let inside = x >= sx && x < sx + sq && y >= 50 && y < 50 + sq;
                let px: [u8; 4] = if inside {
                    [255, 255, 255, 255]
                } else {
                    [120, 40, 20, 255]
                };
                pixels[i..i + 4].copy_from_slice(&px);
            }
        }
        CVPixelBufferUnlockBaseAddress(&buffer, CVPixelBufferLockFlags(0));
        buffer
    }
}

fn append_blocking(writer: &mut Mp4Writer, frame: &CVPixelBuffer, pts: CMTime) {
    for _ in 0..500 {
        if (writer.frames() == 0 || writer.is_ready()) && writer.append(frame, pts).unwrap() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("encoder never became ready");
}

fn ffprobe(path: &Path, entries: &str) -> Option<String> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            entries,
            "-of",
            "default=nw=1",
        ])
        .arg(path)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

#[test]
fn writes_a_valid_h264_mp4() {
    let path = temp_path("synthetic.mp4");
    let (w, h) = (320usize, 240usize);
    let mut writer = Mp4Writer::new(&path, w as u32, h as u32, 30, false).unwrap();
    // Timestamps start at an arbitrary clock value, as host time does.
    let base = 1_000_000i64;
    for i in 0..45 {
        let frame = make_frame(w, h, i);
        append_blocking(&mut writer, &frame, unsafe {
            CMTime::new(base + i as i64 * 20, 600)
        });
    }
    // The screen "stays still" until 3 s after the first frame.
    let end = unsafe { CMTime::new(base + 3 * 600, 600) };
    let summary = writer.finish(Some(end)).unwrap();

    assert!(
        (summary.duration.as_secs_f64() - 3.0).abs() < 0.01,
        "{:?}",
        summary.duration
    );
    assert_eq!(summary.frames, 46, "45 frames plus the repeated last frame");

    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.len() > 1000, "file too small: {}", bytes.len());
    assert_eq!(&bytes[4..8], b"ftyp", "not an MP4 file");

    // Cross-check with ffprobe when it is installed (it is on CI and dev).
    if let Some(info) = ffprobe(&path, "stream=codec_name,width,height:format=duration") {
        assert!(info.contains("codec_name=h264"), "{info}");
        assert!(info.contains("width=320"), "{info}");
        assert!(info.contains("height=240"), "{info}");
        let duration: f64 = info
            .lines()
            .find_map(|l| l.strip_prefix("duration="))
            .and_then(|d| d.trim().parse().ok())
            .unwrap();
        assert!((duration - 3.0).abs() < 0.1, "ffprobe duration {duration}");
    } else {
        eprintln!("ffprobe not found; skipped the codec cross-check");
    }
    std::fs::remove_file(&path).ok();
}

#[test]
fn finishing_without_frames_reports_no_frames_and_leaves_no_file() {
    let path = temp_path("empty.mp4");
    let writer = Mp4Writer::new(&path, 64, 64, 30, true).unwrap();
    let err = writer.finish(None).unwrap_err();
    assert!(matches!(err, CaptureError::NoFrames), "{err}");
    assert!(!path.exists());
}

#[test]
fn rejects_odd_sizes() {
    let path = temp_path("odd.mp4");
    assert!(Mp4Writer::new(&path, 101, 64, 30, true).is_err());
}

#[test]
fn out_of_order_frames_are_skipped() {
    let path = temp_path("order.mp4");
    let mut writer = Mp4Writer::new(&path, 64, 64, 30, false).unwrap();
    let f = make_frame(64, 64, 0);
    assert!(writer.append(&f, unsafe { CMTime::new(100, 600) }).unwrap());
    assert!(!writer.append(&f, unsafe { CMTime::new(100, 600) }).unwrap());
    assert!(!writer.append(&f, unsafe { CMTime::new(50, 600) }).unwrap());
    assert_eq!(writer.frames(), 1);
    writer.cancel();
    assert!(!path.exists());
}
