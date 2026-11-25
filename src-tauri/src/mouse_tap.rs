//! macOS: while you choose a region, clicks, scrolling and typing go to your
//! apps as usual; only a left-button drag becomes the selection. An event tap
//! holds each left press back until it's clear whether it's a click (then it
//! is sent on to the app) or a drag (then Rec keeps it, so the app below
//! doesn't select text or move a window).
//!
//! The tap needs Accessibility permission. Without it the overlay catches the
//! mouse instead, and you can't use your apps until you drag or press Esc.

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::rc::Retained;
use objc2_foundation::{NSDictionary, NSNumber, NSString};

use crate::macos::MouseKind;

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

type CGEventRef = *mut c_void;
type CFMachPortRef = *mut c_void;
type TapCallback = extern "C" fn(*mut c_void, u32, CGEventRef, *mut c_void) -> CGEventRef;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: TapCallback,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    fn CGEventCreateCopy(event: CGEventRef) -> CGEventRef;
    fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    fn CGEventPost(tap: u32, event: CGEventRef);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFMachPortCreateRunLoopSource(
        allocator: *const c_void,
        port: CFMachPortRef,
        order: isize,
    ) -> *mut c_void;
    fn CFRunLoopGetMain() -> *mut c_void;
    fn CFRunLoopAddSource(run_loop: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRelease(cf: *const c_void);
    static kCFRunLoopCommonModes: *const c_void;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
}

const SESSION_TAP: u32 = 1;
const HEAD_INSERT: u32 = 0;
const ACTIVE_FILTER: u32 = 0;
const LEFT_DOWN: u32 = 1;
const LEFT_UP: u32 = 2;
const LEFT_DRAGGED: u32 = 6;
const DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;
/// kCGEventSourceUserData: marks the clicks Rec sends on, so the tap lets
/// them through.
const USER_DATA: u32 = 42;
const REPLAYED: i64 = 0x5245_4321;
/// How far (points) the mouse must move with the button down to be a drag.
/// A little more than the overlay's click tolerance.
const DRAG_THRESHOLD: f64 = 6.0;

/// Set while choosing a region. Read by the tap on every event, set from any
/// thread.
static ACTIVE: AtomicBool = AtomicBool::new(false);

type OnEvent = Rc<dyn Fn(MouseKind, f64, f64)>;

struct Tap {
    port: Option<CFMachPortRef>,
    on_event: OnEvent,
    press: Press<OwnedEvent>,
}

thread_local! {
    // The tap runs on the main run loop, so its state lives on the main thread.
    static TAP: RefCell<Option<Tap>> = const { RefCell::new(None) };
}

/// A held-back mouse-down, released when dropped.
struct OwnedEvent(CGEventRef);

impl Drop for OwnedEvent {
    fn drop(&mut self) {
        // SAFETY: created by CGEventCreateCopy, owned here.
        unsafe { CFRelease(self.0) };
    }
}

pub fn accessibility_granted() -> bool {
    // SAFETY: no arguments; safe to call from any thread.
    unsafe { AXIsProcessTrusted() }
}

/// Ask for Accessibility: macOS shows its prompt and lists Rec in Settings.
pub fn request_accessibility() -> bool {
    let key = NSString::from_str("AXTrustedCheckOptionPrompt");
    let value = NSNumber::new_bool(true);
    let options = NSDictionary::from_slices(&[&*key], &[&*value]);
    // SAFETY: NSDictionary is toll-free bridged to CFDictionary.
    unsafe { AXIsProcessTrustedWithOptions(Retained::as_ptr(&options).cast()) }
}

/// Remember what to call for drags. Main thread, once at startup. The tap
/// itself is created the first time it's needed (permission may come later).
pub fn install(on_event: impl Fn(MouseKind, f64, f64) + 'static) {
    TAP.with(|t| {
        *t.borrow_mut() = Some(Tap {
            port: None,
            on_event: Rc::new(on_event),
            press: Press::Idle,
        })
    });
}

/// Start letting clicks through and catching drags. Main thread. False when
/// it can't (no Accessibility permission): the caller falls back to the
/// overlay catching the mouse.
pub fn activate() -> bool {
    if !accessibility_granted() {
        return false;
    }
    TAP.with(|t| {
        let mut t = t.borrow_mut();
        let Some(tap) = t.as_mut() else {
            return false;
        };
        if tap.port.is_none() {
            tap.port = create_tap();
        }
        let Some(port) = tap.port else {
            return false;
        };
        tap.press = Press::Idle;
        ACTIVE.store(true, Ordering::SeqCst);
        // SAFETY: a live tap created by create_tap.
        unsafe { CGEventTapEnable(port, true) };
        true
    })
}

/// Stop catching drags. Any thread; the tap switches itself off on the next
/// mouse event.
pub fn deactivate() {
    ACTIVE.store(false, Ordering::SeqCst);
}

fn create_tap() -> Option<CFMachPortRef> {
    let mask = (1u64 << LEFT_DOWN) | (1u64 << LEFT_UP) | (1u64 << LEFT_DRAGGED);
    // SAFETY: plain CoreGraphics/CoreFoundation calls on the main thread; the
    // run loop source keeps the port alive for the life of the app.
    unsafe {
        let port = CGEventTapCreate(
            SESSION_TAP,
            HEAD_INSERT,
            ACTIVE_FILTER,
            mask,
            on_tap,
            std::ptr::null_mut(),
        );
        if port.is_null() {
            log::warn!("cannot create the mouse event tap");
            return None;
        }
        let source = CFMachPortCreateRunLoopSource(std::ptr::null(), port, 0);
        CFRunLoopAddSource(CFRunLoopGetMain(), source, kCFRunLoopCommonModes);
        CFRelease(source);
        Some(port)
    }
}

extern "C" fn on_tap(
    _proxy: *mut c_void,
    ty: u32,
    event: CGEventRef,
    _info: *mut c_void,
) -> CGEventRef {
    if ty == DISABLED_BY_TIMEOUT || ty == DISABLED_BY_USER_INPUT {
        if ACTIVE.load(Ordering::SeqCst) {
            TAP.with(|t| {
                if let Some(port) = t.borrow().as_ref().and_then(|t| t.port) {
                    // SAFETY: a live tap.
                    unsafe { CGEventTapEnable(port, true) };
                }
            });
        }
        return event;
    }
    // SAFETY: the event is valid for the duration of the callback.
    if unsafe { CGEventGetIntegerValueField(event, USER_DATA) } == REPLAYED {
        return event;
    }

    let (step, on_event) = TAP.with(|t| {
        let mut t = t.borrow_mut();
        let Some(tap) = t.as_mut() else {
            return (Step::Pass, None);
        };
        if !ACTIVE.load(Ordering::SeqCst) {
            tap.press = Press::Idle;
            if let Some(port) = tap.port {
                // SAFETY: a live tap.
                unsafe { CGEventTapEnable(port, false) };
            }
            return (Step::Pass, None);
        }
        // SAFETY: the event is valid for the duration of the callback.
        let p = unsafe { CGEventGetLocation(event) };
        let step = tap.press.next(ty, p.x, p.y, || {
            // SAFETY: copies the event; OwnedEvent releases the copy.
            OwnedEvent(unsafe { CGEventCreateCopy(event) })
        });
        (step, Some(tap.on_event.clone()))
    });

    match step {
        Step::Pass => event,
        Step::Swallow => std::ptr::null_mut(),
        Step::Emit(events) => {
            if let Some(on_event) = on_event {
                for (kind, x, y) in events {
                    on_event(kind, x, y);
                }
            }
            std::ptr::null_mut()
        }
        Step::Replay(down) => {
            // It was a click: send the press and this release on to the app.
            // SAFETY: both events are owned copies, marked so the tap lets
            // them through, posted in order.
            unsafe {
                let up = OwnedEvent(CGEventCreateCopy(event));
                CGEventSetIntegerValueField(down.0, USER_DATA, REPLAYED);
                CGEventSetIntegerValueField(up.0, USER_DATA, REPLAYED);
                CGEventPost(SESSION_TAP, down.0);
                CGEventPost(SESSION_TAP, up.0);
            }
            std::ptr::null_mut()
        }
    }
}

/// What the tap does with an event.
#[derive(Debug, PartialEq)]
enum Step<D> {
    /// Let it through to the app.
    Pass,
    /// Hold it back.
    Swallow,
    /// Hold it back and report these to the selection.
    Emit(Vec<(MouseKind, f64, f64)>),
    /// It was a click: send this held press, then the release, to the app.
    Replay(D),
}

/// The left button, from press to release. `D` is the held-back press.
enum Press<D> {
    Idle,
    Held { down: D, x: f64, y: f64 },
    Dragging,
}

impl<D> Press<D> {
    fn next(&mut self, ty: u32, x: f64, y: f64, hold: impl FnOnce() -> D) -> Step<D> {
        match (ty, std::mem::replace(self, Press::Idle)) {
            (LEFT_DOWN, _) => {
                *self = Press::Held { down: hold(), x, y };
                Step::Swallow
            }
            (LEFT_DRAGGED, Press::Held { down, x: sx, y: sy }) => {
                if (x - sx).hypot(y - sy) >= DRAG_THRESHOLD {
                    *self = Press::Dragging;
                    Step::Emit(vec![(MouseKind::Down, sx, sy), (MouseKind::Dragged, x, y)])
                } else {
                    *self = Press::Held { down, x: sx, y: sy };
                    Step::Swallow
                }
            }
            (LEFT_DRAGGED, Press::Dragging) => {
                *self = Press::Dragging;
                Step::Emit(vec![(MouseKind::Dragged, x, y)])
            }
            (LEFT_UP, Press::Held { down, .. }) => Step::Replay(down),
            (LEFT_UP, Press::Dragging) => Step::Emit(vec![(MouseKind::Up, x, y)]),
            (_, state) => {
                // A drag or release whose press came before the tap: not ours.
                *self = state;
                Step::Pass
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_is_sent_on_to_the_app() {
        let mut p = Press::Idle;
        assert_eq!(p.next(LEFT_DOWN, 10.0, 10.0, || "down"), Step::Swallow);
        assert_eq!(p.next(LEFT_DRAGGED, 12.0, 11.0, || "x"), Step::Swallow);
        assert_eq!(p.next(LEFT_UP, 12.0, 11.0, || "x"), Step::Replay("down"));
        assert!(matches!(p, Press::Idle));
    }

    #[test]
    fn a_drag_becomes_the_selection() {
        let mut p = Press::Idle;
        assert_eq!(p.next(LEFT_DOWN, 10.0, 10.0, || ()), Step::Swallow);
        assert_eq!(
            p.next(LEFT_DRAGGED, 30.0, 40.0, || ()),
            Step::Emit(vec![
                (MouseKind::Down, 10.0, 10.0),
                (MouseKind::Dragged, 30.0, 40.0)
            ])
        );
        assert_eq!(
            p.next(LEFT_DRAGGED, 50.0, 60.0, || ()),
            Step::Emit(vec![(MouseKind::Dragged, 50.0, 60.0)])
        );
        assert_eq!(
            p.next(LEFT_UP, 50.0, 60.0, || ()),
            Step::Emit(vec![(MouseKind::Up, 50.0, 60.0)])
        );
        assert!(matches!(p, Press::Idle));
    }

    #[test]
    fn events_from_a_press_before_the_tap_pass_through() {
        let mut p: Press<()> = Press::Idle;
        assert_eq!(p.next(LEFT_DRAGGED, 1.0, 1.0, || ()), Step::Pass);
        assert_eq!(p.next(LEFT_UP, 1.0, 1.0, || ()), Step::Pass);
    }
}
