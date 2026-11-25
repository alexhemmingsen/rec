//! Encodes synthetic frames with Media Foundation and checks the MP4.
#![cfg(windows)]

use rec_capture::{CaptureError, Mp4Writer};

mod common;

fn frame(w: u32, h: u32, t: u32) -> Vec<u8> {
    let mut px = vec![0u8; (w * h * 4) as usize];
    let sq = 40;
    let sx = (t * 5) % (w - sq);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let inside = x >= sx && x < sx + sq && (50..50 + sq).contains(&y);
            px[i..i + 4].copy_from_slice(if inside {
                &[255, 255, 255, 255]
            } else {
                &[120, 40, 20, 255]
            });
        }
    }
    px
}

#[test]
fn writes_a_valid_h264_mp4() {
    let path = common::temp_path("synthetic.mp4");
    let mut writer = Mp4Writer::new(&path, 320, 240, 30).unwrap();
    let base = 50_000_000i64;
    for i in 0..45u32 {
        assert!(
            writer
                .append(&frame(320, 240, i), base + i as i64 * 333_333)
                .unwrap()
        );
    }
    // The screen "stays still" until 3 s after the first frame.
    let duration = writer.finish(Some(base + 30_000_000)).unwrap();
    assert!((duration.as_secs_f64() - 3.0).abs() < 0.01, "{duration:?}");
    // 91 frames at 30 fps: the last one lasts until 3.033 s.
    common::assert_mp4(&path, 320, 240, 3.033);
}

#[test]
fn fills_gaps_at_a_constant_frame_rate() {
    let path = common::temp_path("gaps.mp4");
    let mut writer = Mp4Writer::new(&path, 64, 64, 30).unwrap();
    let f = frame(64, 64, 0);
    assert!(writer.append(&f, 0).unwrap());
    // Nothing changes for a second, then one frame.
    assert!(writer.append(&f, 10_000_000).unwrap());
    assert_eq!(writer.frames(), 31, "30 slots filled plus the new frame");
    // A frame for a slot that is already written is skipped.
    assert!(!writer.append(&f, 10_000_000 + 100).unwrap());
    let duration = writer.finish(Some(20_000_000)).unwrap();
    assert!((duration.as_secs_f64() - 2.0).abs() < 0.01, "{duration:?}");
    common::assert_mp4(&path, 64, 64, 2.033);
}

#[test]
fn finishing_without_frames_reports_no_frames() {
    let path = common::temp_path("empty.mp4");
    let writer = Mp4Writer::new(&path, 64, 64, 30).unwrap();
    assert!(matches!(writer.finish(None), Err(CaptureError::NoFrames)));
}

#[test]
fn rejects_odd_sizes() {
    assert!(Mp4Writer::new(&common::temp_path("odd.mp4"), 101, 64, 30).is_err());
}
