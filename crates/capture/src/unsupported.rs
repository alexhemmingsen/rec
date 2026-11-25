//! Placeholder backend for platforms Rec does not record on yet.
//!
//! Rec releases macOS first, then Windows (Windows Graphics Capture + Media
//! Foundation), then Linux (PipeWire via the ScreenCast portal). Until then the
//! rest of the app builds and runs here, and recording reports
//! [`CaptureError::Unsupported`].

use crate::{CaptureError, DisplayInfo, Permission, RecordRequest, RecordingSummary, Result};

pub fn permission_status() -> Permission {
    Permission::NotRequired
}

pub fn request_permission() -> bool {
    true
}

pub fn displays() -> Vec<DisplayInfo> {
    Vec::new()
}

pub struct Recording;

impl Recording {
    pub fn start(_request: RecordRequest) -> Result<Recording> {
        Err(CaptureError::Unsupported)
    }

    pub fn stop(self) -> Result<RecordingSummary> {
        Err(CaptureError::Unsupported)
    }

    pub fn cancel(self) {}
}
