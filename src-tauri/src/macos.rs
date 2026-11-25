//! AppKit details Tauri does not expose: window levels above the menu bar,
//! frames in top-left global coordinates, and handing focus back to the app
//! the user was using.

use std::ffi::c_void;

use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSEvent, NSRunningApplication, NSScreen,
    NSWindow, NSWindowCollectionBehavior, NSWorkspace,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use rec_capture::Rect;

/// NSScreenSaverWindowLevel: the selection overlay covers the menu bar and
/// Dock, like the system screenshot tool.
pub const OVERLAY_LEVEL: isize = 1000;
/// NSStatusWindowLevel: the Stop pill floats above normal windows.
pub const PILL_LEVEL: isize = 25;

/// Height of the primary screen (the one with the menu bar), which anchors
/// Cocoa's bottom-left coordinate system.
fn primary_height(mtm: MainThreadMarker) -> f64 {
    NSScreen::screens(mtm)
        .firstObject()
        .map(|s| s.frame().size.height)
        .unwrap_or(0.0)
}

/// Top-left global rect to Cocoa's bottom-left global rect.
pub fn to_cocoa(rect: Rect, primary_height: f64) -> NSRect {
    NSRect::new(
        NSPoint::new(rect.x, primary_height - rect.y - rect.height),
        NSSize::new(rect.width, rect.height),
    )
}

/// Make a Tauri window float at `level` on every Space (including over
/// full-screen apps) and put it exactly at `frame`. Must run on the main
/// thread.
pub fn configure_floating(ns_window: *mut c_void, frame: Rect, level: isize) {
    let Some(mtm) = MainThreadMarker::new() else {
        log::error!("configure_floating called off the main thread");
        return;
    };
    if ns_window.is_null() {
        return;
    }
    // SAFETY: Tauri hands out a valid NSWindow pointer, used on the main thread.
    let window: &NSWindow = unsafe { &*(ns_window as *const NSWindow) };
    window.setLevel(level);
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    window.setHasShadow(false);
    window.setOpaque(false);
    window.setFrame_display(to_cocoa(frame, primary_height(mtm)), true);
}

/// The mouse position in top-left global coordinates.
pub fn mouse_location() -> Option<(f64, f64)> {
    let mtm = MainThreadMarker::new()?;
    let p = NSEvent::mouseLocation();
    Some((p.x, primary_height(mtm) - p.y))
}

/// Process id of the app in front right now.
pub fn frontmost_app_pid() -> Option<i32> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    Some(app.processIdentifier())
}

/// Give focus back to the app with `pid` (the one the user was recording).
pub fn activate_app(pid: i32) {
    if pid == std::process::id() as i32 {
        return;
    }
    if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
        #[allow(deprecated)]
        app.activateWithOptions(NSApplicationActivationOptions::empty());
    }
}

/// Bring Rec to the front (so the overlay gets the keyboard for Esc).
pub fn activate_self() {
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).activate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_top_left_to_cocoa_coordinates() {
        // A 100x50 rect 10pt from the top of a 900pt-tall primary screen.
        let r = to_cocoa(Rect::new(20.0, 10.0, 100.0, 50.0), 900.0);
        assert_eq!((r.origin.x, r.origin.y), (20.0, 840.0));
        assert_eq!((r.size.width, r.size.height), (100.0, 50.0));
        // A display above the primary one has negative top-left y.
        let above = to_cocoa(Rect::new(0.0, -1080.0, 1920.0, 1080.0), 900.0);
        assert_eq!(above.origin.y, 900.0);
    }
}
