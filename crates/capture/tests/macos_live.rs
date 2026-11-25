//! Records the real screen. Needs Screen Recording permission for the process
//! running the tests (the terminal), so it is ignored by default:
//!
//!     cargo test -p rec-capture --test macos_live -- --ignored
#![cfg(target_os = "macos")]

use std::time::Duration;

use rec_capture::{Permission, RecordRequest, Recording, Rect, displays, permission_status};

#[test]
#[ignore = "needs Screen Recording permission"]
fn records_a_region_of_the_main_display() {
    assert_eq!(
        permission_status(),
        Permission::Granted,
        "grant Screen Recording to your terminal first"
    );
    let main = displays()
        .into_iter()
        .find(|d| d.is_main)
        .expect("no main display");
    let out = std::env::temp_dir().join("rec-live-test.mp4");
    let recording = Recording::start(RecordRequest {
        display_id: main.id,
        region: Rect::new(10.0, 10.0, 400.0, 300.0),
        output: out.clone(),
        fps: 30,
        show_cursor: true,
    })
    .unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    let summary = recording.stop().unwrap();
    assert!(summary.frames >= 1);
    assert!(
        summary.duration.as_secs_f64() > 1.2,
        "{:?}",
        summary.duration
    );
    let expected = rec_capture::output_dimensions(400.0, 300.0, main.scale_factor);
    assert_eq!((summary.width, summary.height), expected);
    assert!(summary.thumbnail.is_some());
    assert!(out.exists());
    std::fs::remove_file(out).ok();
}

#[test]
fn lists_at_least_one_display() {
    let all = displays();
    assert!(!all.is_empty());
    assert_eq!(all.iter().filter(|d| d.is_main).count(), 1);
    for d in all {
        assert!(d.frame.width > 0.0 && d.frame.height > 0.0);
        assert!(d.scale_factor >= 1.0);
    }
}
