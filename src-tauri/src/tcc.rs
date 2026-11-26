//! Whether Rec is in a list in System Settings → Privacy & Security, from
//! TCC, macOS's permission store.
//!
//! "Grant access" does one thing: if Rec isn't in the list, macOS's own
//! prompt, which adds it; if it is, System Settings at the list. macOS has no
//! public call to tell those apart (`CGPreflightScreenCaptureAccess` and
//! `AXIsProcessTrusted` only say yes or no), so this asks TCC.framework's
//! `TCCAccessPreflight`, a private call that's been stable for years.

use std::ffi::{c_char, c_void};

use objc2::rc::Retained;
use objc2_foundation::NSString;

/// A list in Privacy & Security.
#[derive(Clone, Copy, Debug)]
pub enum List {
    ScreenRecording,
    Accessibility,
}

impl List {
    /// The list's name in TCC.
    fn service(self) -> &'static str {
        match self {
            List::ScreenRecording => "kTCCServiceScreenCapture",
            List::Accessibility => "kTCCServiceAccessibility",
        }
    }
}

/// Where Rec stands in a list.
#[derive(Debug, PartialEq)]
pub enum Listing {
    /// In the list, switched on (Screen Recording may still need a restart).
    On,
    /// In the list, switched off.
    Off,
    /// Not in the list: macOS hasn't asked, or has forgotten Rec (its
    /// signature changed, or someone removed it with "−").
    Missing,
}

/// Rec's place in `list`; None if macOS won't say (the private call is
/// gone in some macOS version).
pub fn listing(list: List) -> Option<Listing> {
    unsafe extern "C" {
        fn dlopen(path: *const c_char, mode: i32) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }
    type Preflight = unsafe extern "C" fn(*const c_void, *const c_void) -> i32;
    const RTLD_LAZY: i32 = 1;
    // SAFETY: plain C calls with NUL-terminated strings; null results mean
    // "not available" and are checked.
    let symbol = unsafe {
        let handle = dlopen(
            c"/System/Library/PrivateFrameworks/TCC.framework/TCC".as_ptr(),
            RTLD_LAZY,
        );
        if handle.is_null() {
            return None;
        }
        dlsym(handle, c"TCCAccessPreflight".as_ptr())
    };
    if symbol.is_null() {
        return None;
    }
    // SAFETY: TCCAccessPreflight(CFStringRef service, CFDictionaryRef options)
    // returns an int: 0 allowed, 1 denied, 2 not decided. NSString is
    // toll-free bridged to CFString.
    let preflight: Preflight = unsafe { std::mem::transmute(symbol) };
    let service = NSString::from_str(list.service());
    let result = unsafe { preflight(Retained::as_ptr(&service).cast(), std::ptr::null()) };
    match result {
        0 => Some(Listing::On),
        1 => Some(Listing::Off),
        2 => Some(Listing::Missing),
        _ => None,
    }
}
