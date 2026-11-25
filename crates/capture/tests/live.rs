//! Records the real screen on any platform. Needs a desktop session (and
//! Screen Recording permission on macOS), so it is ignored by default:
//!
//!     cargo test -p rec-capture --test live -- --ignored

use std::time::Duration;

use rec_capture::{
    Permission, RecordRequest, Recording, Rect, displays, output_dimensions, permission_status,
};

mod common;

#[test]
#[ignore = "needs a desktop session and, on macOS, Screen Recording permission"]
fn records_a_region_of_the_main_display() {
    assert_ne!(
        permission_status(),
        Permission::NotGranted,
        "grant Screen Recording first"
    );
    let main = displays()
        .into_iter()
        .find(|d| d.is_main)
        .expect("no main display");
    eprintln!("main display: {main:?}");
    let out = common::temp_path("live.mp4");
    let recording = Recording::start(RecordRequest {
        display_id: main.id,
        region: Rect::new(10.0, 10.0, 400.0, 300.0),
        output: out.clone(),
        fps: 30,
        show_cursor: true,
    })
    .unwrap();
    std::thread::sleep(Duration::from_millis(2000));
    let summary = recording.stop().unwrap();
    eprintln!(
        "summary: {:?} {}x{} {} frames",
        summary.duration, summary.width, summary.height, summary.frames
    );
    assert_eq!(summary.path, out);
    assert!(summary.frames >= 1);
    // Capture can take a few hundred ms to deliver its first frame on a
    // slow (virtual) machine.
    assert!(
        summary.duration.as_secs_f64() > 1.2,
        "{:?}",
        summary.duration
    );
    let expected = output_dimensions(400.0, 300.0, main.scale_factor);
    assert_eq!((summary.width, summary.height), expected);
    assert!(summary.thumbnail.is_some());
    common::assert_mp4(&out, expected.0, expected.1, summary.duration.as_secs_f64());
}

#[test]
fn lists_displays_with_one_main() {
    let all = displays();
    if cfg!(any(target_os = "macos", windows)) {
        assert!(!all.is_empty());
    }
    if !all.is_empty() {
        assert_eq!(all.iter().filter(|d| d.is_main).count(), 1, "{all:?}");
    }
    for d in all {
        assert!(d.frame.width > 0.0 && d.frame.height > 0.0);
        assert!(d.pixel_frame.width >= d.frame.width);
        assert!(d.scale_factor >= 1.0);
    }
}
