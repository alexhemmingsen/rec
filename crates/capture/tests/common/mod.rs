//! Helpers shared by the platform tests.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

pub fn temp_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rec-capture-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// The file is an MP4; when ffprobe is installed, also check codec, size and
/// duration (within 0.15 s).
pub fn assert_mp4(path: &Path, width: u32, height: u32, duration: f64) {
    let bytes = std::fs::read(path).unwrap();
    assert!(bytes.len() > 1000, "file too small: {}", bytes.len());
    assert_eq!(&bytes[4..8], b"ftyp", "not an MP4 file");
    let Ok(out) = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name,width,height:format=duration",
            "-of",
            "default=nw=1",
        ])
        .arg(path)
        .output()
    else {
        eprintln!("ffprobe not found; skipped the codec cross-check");
        return;
    };
    let info = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(info.contains("codec_name=h264"), "{info}");
    assert!(info.contains(&format!("width={width}")), "{info}");
    assert!(info.contains(&format!("height={height}")), "{info}");
    let d: f64 = info
        .lines()
        .find_map(|l| l.strip_prefix("duration="))
        .and_then(|d| d.trim().parse().ok())
        .unwrap();
    assert!(
        (d - duration).abs() < 0.15,
        "ffprobe duration {d}, expected {duration}"
    );
}
