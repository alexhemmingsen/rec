//! macOS backend: ScreenCaptureKit frames into an AVAssetWriter.

pub mod writer;

use std::ptr::NonNull;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{AnyThread, DefinedClass, define_class, msg_send};
use objc2_core_foundation::{CFArray, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGDisplayBounds, CGDisplayCopyDisplayMode, CGDisplayMode, CGGetActiveDisplayList,
    CGGetOnlineDisplayList, CGMainDisplayID, CGPreflightScreenCaptureAccess,
    CGRequestScreenCaptureAccess,
};
use objc2_core_media::{CMClock, CMSampleBuffer, CMTime};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetHeight, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress, kCVPixelFormatType_32BGRA,
};
use objc2_foundation::{
    NSArray, NSDictionary, NSError, NSNumber, NSObject, NSObjectProtocol, NSString,
};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCFrameStatus, SCRunningApplication, SCShareableContent, SCStream,
    SCStreamConfiguration, SCStreamFrameInfoStatus, SCStreamOutput, SCStreamOutputType, SCWindow,
};

use self::writer::Mp4Writer;
use crate::{
    CaptureError, DisplayInfo, Frame, Permission, RecordRequest, RecordingSummary, Rect, Result,
    bgra_to_rgba_thumbnail, output_dimensions,
};

const THUMBNAIL_MAX_WIDTH: usize = 640;

pub fn permission_status() -> Permission {
    if CGPreflightScreenCaptureAccess() {
        Permission::Granted
    } else {
        Permission::NotGranted
    }
}

pub fn request_permission() -> bool {
    CGRequestScreenCaptureAccess()
}

pub fn displays() -> Vec<DisplayInfo> {
    let mut ids = [0u32; 32];
    let mut count = 0u32;
    // SAFETY: both pointers are valid for the given capacity.
    let err = unsafe { CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) };
    if err.0 != 0 || count == 0 {
        // Sleeping displays are not "active"; list them anyway so callers
        // always see the layout (pressing the shortcut wakes them up).
        // SAFETY: as above.
        let err = unsafe { CGGetOnlineDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) };
        if err.0 != 0 {
            log::warn!("cannot list displays: {}", err.0);
            return Vec::new();
        }
    }
    let main = CGMainDisplayID();
    ids[..count as usize]
        .iter()
        .map(|&id| {
            let b = CGDisplayBounds(id);
            let mode = CGDisplayCopyDisplayMode(id);
            let pixel_width = CGDisplayMode::pixel_width(mode.as_deref()) as f64;
            let scale = if b.size.width > 0.0 && pixel_width > 0.0 {
                pixel_width / b.size.width
            } else {
                1.0
            };
            DisplayInfo {
                id,
                frame: Rect::new(b.origin.x, b.origin.y, b.size.width, b.size.height),
                pixel_frame: Rect::new(
                    b.origin.x * scale,
                    b.origin.y * scale,
                    b.size.width * scale,
                    b.size.height * scale,
                ),
                scale_factor: scale,
                is_main: id == main,
            }
        })
        .collect()
}

/// Shared between the capture callback (on the capture queue) and the thread
/// that stops the recording.
struct Shared {
    writer: Option<Mp4Writer>,
    thumbnail: Option<Frame>,
    error: Option<String>,
}

struct OutputIvars {
    shared: Arc<Mutex<Shared>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and this class does
    // not implement Drop.
    #[unsafe(super(NSObject))]
    #[name = "RecStreamOutput"]
    #[ivars = OutputIvars]
    struct StreamOutput;

    unsafe impl NSObjectProtocol for StreamOutput {}

    unsafe impl SCStreamOutput for StreamOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn stream_did_output(
            &self,
            _stream: &SCStream,
            sample: &CMSampleBuffer,
            kind: SCStreamOutputType,
        ) {
            if kind != SCStreamOutputType::Screen {
                return;
            }
            self.handle_frame(sample);
        }
    }
);

impl StreamOutput {
    fn new(shared: Arc<Mutex<Shared>>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(OutputIvars { shared });
        unsafe { msg_send![super(this), init] }
    }

    fn handle_frame(&self, sample: &CMSampleBuffer) {
        unsafe {
            if !sample.is_valid() || frame_status(sample) != Some(SCFrameStatus::Complete) {
                return;
            }
            let Some(image) = sample.image_buffer() else {
                return;
            };
            let pts = sample.presentation_time_stamp();
            let mut shared = match self.ivars().shared.lock() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            if shared.thumbnail.is_none() {
                shared.thumbnail = copy_thumbnail(&image);
            }
            let Some(writer) = shared.writer.as_mut() else {
                return;
            };
            if let Err(e) = writer.append(&image, pts) {
                log::error!("capture: {e}");
                shared.error.get_or_insert_with(|| e.to_string());
            }
        }
    }
}

/// The `SCStreamFrameInfoStatus` attachment of a ScreenCaptureKit frame.
/// Idle frames (nothing changed on screen) carry no pixels.
unsafe fn frame_status(sample: &CMSampleBuffer) -> Option<SCFrameStatus> {
    unsafe {
        let attachments = sample.sample_attachments_array(false)?;
        // CFArray of CFDictionary is toll-free bridged to NSArray of NSDictionary.
        let array: &NSArray<NSDictionary<NSString, AnyObject>> = &*((&*attachments)
            as *const CFArray
            as *const NSArray<NSDictionary<NSString, AnyObject>>);
        let info = array.firstObject()?;
        let value = info.objectForKey(SCStreamFrameInfoStatus)?;
        let number = value.downcast::<NSNumber>().ok()?;
        Some(SCFrameStatus(number.integerValue()))
    }
}

/// Copy a downscaled RGBA version of a BGRA pixel buffer.
unsafe fn copy_thumbnail(buffer: &CVPixelBuffer) -> Option<Frame> {
    unsafe {
        if CVPixelBufferLockBaseAddress(buffer, CVPixelBufferLockFlags::ReadOnly) != 0 {
            return None;
        }
        let base = CVPixelBufferGetBaseAddress(buffer) as *const u8;
        let width = CVPixelBufferGetWidth(buffer);
        let height = CVPixelBufferGetHeight(buffer);
        let stride = CVPixelBufferGetBytesPerRow(buffer);
        let frame = if base.is_null() || height == 0 {
            None
        } else {
            let bytes = std::slice::from_raw_parts(base, stride * height);
            bgra_to_rgba_thumbnail(bytes, width, height, stride, THUMBNAIL_MAX_WIDTH)
        };
        CVPixelBufferUnlockBaseAddress(buffer, CVPixelBufferLockFlags::ReadOnly);
        frame
    }
}

/// Wraps a non-Send Objective-C value so it can cross a channel. Only used to
/// hand completion-handler results back to the waiting thread.
struct SendBox<T>(T);
// SAFETY: the wrapped objects are immutable snapshots (SCShareableContent) or
// errors, read by exactly one thread after the handler ran.
unsafe impl<T> Send for SendBox<T> {}

fn wait<T>(rx: mpsc::Receiver<T>, what: &'static str, secs: u64) -> Result<T> {
    rx.recv_timeout(Duration::from_secs(secs))
        .map_err(|_| CaptureError::Timeout(what))
}

fn error_text(err: *mut NSError) -> Option<String> {
    NonNull::new(err).map(|e| unsafe { e.as_ref().localizedDescription().to_string() })
}

fn shareable_content() -> Result<Retained<SCShareableContent>> {
    let (tx, rx) =
        mpsc::channel::<SendBox<std::result::Result<Retained<SCShareableContent>, String>>>();
    let handler = RcBlock::new(move |content: *mut SCShareableContent, err: *mut NSError| {
        let result = match unsafe { Retained::retain(content) } {
            Some(content) => Ok(content),
            None => Err(error_text(err).unwrap_or_else(|| "no shareable content".into())),
        };
        let _ = tx.send(SendBox(result));
    });
    unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&handler) };
    let SendBox(result) = wait(rx, "the list of displays", 10)?;
    result.map_err(|e| {
        if CGPreflightScreenCaptureAccess() {
            CaptureError::Platform(e)
        } else {
            CaptureError::PermissionDenied
        }
    })
}

/// Start or stop a stream and wait for the completion handler.
fn run_stream_call(
    what: &'static str,
    call: impl FnOnce(&block2::DynBlock<dyn Fn(*mut NSError)>),
) -> Result<()> {
    let (tx, rx) = mpsc::channel::<Option<String>>();
    let handler = RcBlock::new(move |err: *mut NSError| {
        let _ = tx.send(error_text(err));
    });
    call(&handler);
    match wait(rx, what, 10)? {
        None => Ok(()),
        Some(e) => Err(CaptureError::Platform(e)),
    }
}

pub struct Recording {
    stream: Retained<SCStream>,
    // Kept alive for as long as the stream may call it.
    _output: Retained<StreamOutput>,
    _queue: DispatchRetained<DispatchQueue>,
    shared: Arc<Mutex<Shared>>,
    width: u32,
    height: u32,
}

// SAFETY: SCStream may be stopped from any thread; the shared state is behind
// a Mutex.
unsafe impl Send for Recording {}

impl Recording {
    pub fn start(request: RecordRequest) -> Result<Recording> {
        if !CGPreflightScreenCaptureAccess() {
            return Err(CaptureError::PermissionDenied);
        }
        let content = shareable_content()?;
        unsafe {
            let display: Retained<SCDisplay> = content
                .displays()
                .iter()
                .find(|d| d.displayID() == request.display_id)
                .ok_or(CaptureError::DisplayNotFound(request.display_id))?;

            // Leave Rec's own windows (the selection overlay and the Stop
            // pill) out of the video.
            let pid = std::process::id() as i32;
            let own: Vec<Retained<SCRunningApplication>> = content
                .applications()
                .iter()
                .filter(|a| a.processID() == pid)
                .collect();
            let own_refs: Vec<&SCRunningApplication> = own.iter().map(|a| &**a).collect();
            if own.is_empty() {
                log::warn!(
                    "Rec's own app is not in the shareable content; its windows may be captured"
                );
            }
            let excluded = NSArray::from_slice(&own_refs);
            let no_windows: Retained<NSArray<SCWindow>> = NSArray::new();
            let filter = SCContentFilter::initWithDisplay_excludingApplications_exceptingWindows(
                SCContentFilter::alloc(),
                &display,
                &excluded,
                &no_windows,
            );

            let display_frame = display.frame();
            let region = request
                .region
                .clamp_to(display_frame.size.width, display_frame.size.height)
                .ok_or(CaptureError::EmptyRegion)?;
            let scale = filter.pointPixelScale() as f64;
            let (width, height) = output_dimensions(region.width, region.height, scale);

            let config = SCStreamConfiguration::new();
            config.setSourceRect(CGRect::new(
                CGPoint::new(region.x, region.y),
                CGSize::new(region.width, region.height),
            ));
            config.setWidth(width as usize);
            config.setHeight(height as usize);
            config.setScalesToFit(false);
            config.setShowsCursor(request.show_cursor);
            config.setPixelFormat(kCVPixelFormatType_32BGRA);
            config.setMinimumFrameInterval(CMTime::new(1, request.fps.max(1) as i32));
            config.setQueueDepth(6);
            config.setCapturesAudio(false);

            let writer = Mp4Writer::new(&request.output, width, height, request.fps, true)?;
            let shared = Arc::new(Mutex::new(Shared {
                writer: Some(writer),
                thumbnail: None,
                error: None,
            }));
            let output = StreamOutput::new(shared.clone());
            let queue = DispatchQueue::new("com.alexhem.rec.capture", None);

            let stream = SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                &filter,
                &config,
                None,
            );
            let output_proto: &ProtocolObject<dyn SCStreamOutput> =
                ProtocolObject::from_ref(&*output);
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(
                    output_proto,
                    SCStreamOutputType::Screen,
                    Some(&queue),
                )
                .map_err(|e| CaptureError::Platform(e.localizedDescription().to_string()))?;

            if let Err(e) = run_stream_call("screen capture to start", |h| {
                stream.startCaptureWithCompletionHandler(Some(h))
            }) {
                if let Some(w) = shared.lock().ok().and_then(|mut s| s.writer.take()) {
                    w.cancel();
                }
                return Err(if CGPreflightScreenCaptureAccess() {
                    e
                } else {
                    CaptureError::PermissionDenied
                });
            }

            Ok(Recording {
                stream,
                _output: output,
                _queue: queue,
                shared,
                width,
                height,
            })
        }
    }

    fn stop_stream(&self) {
        if let Err(e) = run_stream_call("screen capture to stop", |h| unsafe {
            self.stream.stopCaptureWithCompletionHandler(Some(h))
        }) {
            log::warn!("stopping capture: {e}");
        }
    }

    pub fn stop(self) -> Result<RecordingSummary> {
        // The video lasts until the moment Stop was pressed, even if the
        // screen did not change at the end.
        let end = unsafe { CMClock::host_time_clock().time() };
        self.stop_stream();
        let (writer, thumbnail, error) = {
            let mut shared = self
                .shared
                .lock()
                .map_err(|_| CaptureError::Platform("capture state poisoned".into()))?;
            (
                shared.writer.take(),
                shared.thumbnail.take(),
                shared.error.take(),
            )
        };
        let writer = writer.ok_or(CaptureError::NoFrames)?;
        if let Some(e) = error {
            writer.cancel();
            return Err(CaptureError::Platform(e));
        }
        let summary = writer.finish(Some(end))?;
        Ok(RecordingSummary {
            path: summary.path,
            duration: summary.duration,
            width: self.width,
            height: self.height,
            frames: summary.frames,
            thumbnail,
        })
    }

    pub fn cancel(self) {
        self.stop_stream();
        if let Some(w) = self.shared.lock().ok().and_then(|mut s| s.writer.take()) {
            w.cancel();
        }
    }
}
