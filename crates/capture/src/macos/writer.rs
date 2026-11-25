//! H.264 MP4 writing with AVAssetWriter (hardware encoding via VideoToolbox).

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_av_foundation::{
    AVAssetWriter, AVAssetWriterInput, AVAssetWriterInputPixelBufferAdaptor, AVAssetWriterStatus,
    AVFileTypeMPEG4, AVMediaTypeVideo, AVVideoAllowFrameReorderingKey, AVVideoAverageBitRateKey,
    AVVideoCodecKey, AVVideoCodecTypeH264, AVVideoCompressionPropertiesKey,
    AVVideoExpectedSourceFrameRateKey, AVVideoHeightKey, AVVideoMaxKeyFrameIntervalKey,
    AVVideoProfileLevelH264HighAutoLevel, AVVideoProfileLevelKey, AVVideoWidthKey,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::CMTime;
use objc2_core_video::CVPixelBuffer;
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};

use crate::{CaptureError, Result, target_bitrate};

/// What [`Mp4Writer::finish`] reports.
#[derive(Clone, Debug)]
pub struct WriterSummary {
    pub path: PathBuf,
    pub duration: Duration,
    pub frames: u64,
}

/// Writes BGRA pixel buffers to an H.264 MP4 file.
///
/// The session starts at the first appended frame's timestamp, so timestamps
/// can be any monotonic clock (ScreenCaptureKit uses the host time clock).
pub struct Mp4Writer {
    writer: Retained<AVAssetWriter>,
    input: Retained<AVAssetWriterInput>,
    adaptor: Retained<AVAssetWriterInputPixelBufferAdaptor>,
    path: PathBuf,
    width: u32,
    height: u32,
    first_pts: Option<CMTime>,
    last_pts: Option<CMTime>,
    last_buffer: Option<CFRetained<CVPixelBuffer>>,
    frames: u64,
}

// SAFETY: AVAssetWriter and its inputs may be used from any thread as long as
// calls are not concurrent. Every Mp4Writer is owned by one Mutex or one
// thread at a time.
unsafe impl Send for Mp4Writer {}

fn key(k: Option<&'static NSString>, name: &str) -> Result<&'static NSString> {
    k.ok_or_else(|| CaptureError::Platform(format!("AVFoundation constant {name} is missing")))
}

fn ns_error(err: &objc2_foundation::NSError) -> String {
    err.localizedDescription().to_string()
}

impl Mp4Writer {
    /// Create a writer for a `width` x `height` video at `fps`. Set `realtime`
    /// for live capture; leave it off when appending pre-made frames.
    pub fn new(path: &Path, width: u32, height: u32, fps: u32, realtime: bool) -> Result<Self> {
        if width < 2 || height < 2 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
            return Err(CaptureError::Platform(format!(
                "invalid video size {width}x{height}"
            )));
        }
        if path.exists() {
            std::fs::remove_file(path).map_err(|e| {
                CaptureError::Platform(format!("cannot replace {}: {e}", path.display()))
            })?;
        }
        let url = NSURL::from_file_path(path)
            .ok_or_else(|| CaptureError::Platform(format!("invalid path {}", path.display())))?;

        unsafe {
            let file_type = key(AVFileTypeMPEG4, "AVFileTypeMPEG4")?;
            let writer =
                AVAssetWriter::initWithURL_fileType_error(AVAssetWriter::alloc(), &url, file_type)
                    .map_err(|e| {
                        CaptureError::Platform(format!(
                            "cannot create video file: {}",
                            ns_error(&e)
                        ))
                    })?;

            let bitrate = NSNumber::new_u32(target_bitrate(width, height, fps));
            let fps_n = NSNumber::new_u32(fps);
            let keyframe_interval = NSNumber::new_u32(fps * 2);
            let no = NSNumber::new_bool(false);
            let profile = key(
                AVVideoProfileLevelH264HighAutoLevel,
                "AVVideoProfileLevelH264HighAutoLevel",
            )?;
            let compression: Retained<NSDictionary<NSString, AnyObject>> =
                NSDictionary::from_slices(
                    &[
                        key(AVVideoAverageBitRateKey, "AVVideoAverageBitRateKey")?,
                        key(
                            AVVideoExpectedSourceFrameRateKey,
                            "AVVideoExpectedSourceFrameRateKey",
                        )?,
                        key(
                            AVVideoMaxKeyFrameIntervalKey,
                            "AVVideoMaxKeyFrameIntervalKey",
                        )?,
                        key(
                            AVVideoAllowFrameReorderingKey,
                            "AVVideoAllowFrameReorderingKey",
                        )?,
                        key(AVVideoProfileLevelKey, "AVVideoProfileLevelKey")?,
                    ],
                    &[
                        bitrate.as_ref(),
                        fps_n.as_ref(),
                        keyframe_interval.as_ref(),
                        no.as_ref(),
                        profile.as_ref(),
                    ],
                );

            let codec = key(AVVideoCodecTypeH264, "AVVideoCodecTypeH264")?;
            let w = NSNumber::new_u32(width);
            let h = NSNumber::new_u32(height);
            let settings: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
                &[
                    key(AVVideoCodecKey, "AVVideoCodecKey")?,
                    key(AVVideoWidthKey, "AVVideoWidthKey")?,
                    key(AVVideoHeightKey, "AVVideoHeightKey")?,
                    key(
                        AVVideoCompressionPropertiesKey,
                        "AVVideoCompressionPropertiesKey",
                    )?,
                ],
                &[codec.as_ref(), w.as_ref(), h.as_ref(), compression.as_ref()],
            );

            let media_type = key(AVMediaTypeVideo, "AVMediaTypeVideo")?;
            let input = AVAssetWriterInput::initWithMediaType_outputSettings(
                AVAssetWriterInput::alloc(),
                media_type,
                Some(&settings),
            );
            input.setExpectsMediaDataInRealTime(realtime);
            if !writer.canAddInput(&input) {
                return Err(CaptureError::Platform(
                    "the video encoder rejected the settings".into(),
                ));
            }
            writer.addInput(&input);

            let adaptor = AVAssetWriterInputPixelBufferAdaptor::initWithAssetWriterInput_sourcePixelBufferAttributes(
                AVAssetWriterInputPixelBufferAdaptor::alloc(),
                &input,
                None,
            );

            Ok(Mp4Writer {
                writer,
                input,
                adaptor,
                path: path.to_path_buf(),
                width,
                height,
                first_pts: None,
                last_pts: None,
                last_buffer: None,
                frames: 0,
            })
        }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Whether the encoder can take another frame right now. Live capture
    /// drops a frame when this is false rather than blocking.
    pub fn is_ready(&self) -> bool {
        unsafe { self.input.isReadyForMoreMediaData() }
    }

    fn writer_error(&self, what: &str) -> CaptureError {
        let detail = unsafe { self.writer.error() }
            .map(|e| ns_error(&e))
            .unwrap_or_else(|| "unknown error".into());
        CaptureError::Platform(format!("{what}: {detail}"))
    }

    /// Append one frame. Frames with a timestamp not after the previous one
    /// are skipped. Returns whether the frame was written.
    pub fn append(&mut self, buffer: &CVPixelBuffer, pts: CMTime) -> Result<bool> {
        unsafe {
            if self.first_pts.is_none() {
                if !self.writer.startWriting() {
                    return Err(self.writer_error("cannot start the video encoder"));
                }
                self.writer.startSessionAtSourceTime(pts);
                self.first_pts = Some(pts);
            } else if let Some(last) = self.last_pts
                && pts.seconds() <= last.seconds()
            {
                return Ok(false);
            }
            if self.writer.status() != AVAssetWriterStatus::Writing {
                return Err(self.writer_error("video encoder stopped"));
            }
            if !self.input.isReadyForMoreMediaData() {
                return Ok(false);
            }
            if !self
                .adaptor
                .appendPixelBuffer_withPresentationTime(buffer, pts)
            {
                return Err(self.writer_error("cannot write frame"));
            }
        }
        self.last_pts = Some(pts);
        self.last_buffer = Some(retain_buffer(buffer));
        self.frames += 1;
        Ok(true)
    }

    /// Finish the file. With `end`, the video lasts until that timestamp: the
    /// last frame is repeated there, so a still screen at the end is kept.
    pub fn finish(mut self, end: Option<CMTime>) -> Result<WriterSummary> {
        let (Some(first), Some(last)) = (self.first_pts, self.last_pts) else {
            unsafe { self.writer.cancelWriting() };
            let _ = std::fs::remove_file(&self.path);
            return Err(CaptureError::NoFrames);
        };
        let mut end_pts = last;
        unsafe {
            if let Some(end) = end
                && end.seconds() > last.seconds() + 0.001
                && let Some(buffer) = self.last_buffer.take()
            {
                // Wait briefly for the encoder; if it stays busy the
                // file just ends at the last real frame.
                for _ in 0..50 {
                    if self.input.isReadyForMoreMediaData() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                if self.input.isReadyForMoreMediaData()
                    && self
                        .adaptor
                        .appendPixelBuffer_withPresentationTime(&buffer, end)
                {
                    self.frames += 1;
                    end_pts = end;
                }
            }
            self.input.markAsFinished();
            self.writer.endSessionAtSourceTime(end_pts);

            let (tx, rx) = mpsc::channel::<()>();
            let block = RcBlock::new(move || {
                let _ = tx.send(());
            });
            self.writer.finishWritingWithCompletionHandler(&block);
            rx.recv_timeout(Duration::from_secs(30))
                .map_err(|_| CaptureError::Timeout("the video file to finish writing"))?;

            if self.writer.status() != AVAssetWriterStatus::Completed {
                return Err(self.writer_error("cannot finish the video file"));
            }
        }
        let secs = unsafe { end_pts.seconds() - first.seconds() }.max(0.0);
        Ok(WriterSummary {
            path: self.path.clone(),
            duration: Duration::from_secs_f64(secs),
            frames: self.frames,
        })
    }

    /// Abandon the file.
    pub fn cancel(self) {
        unsafe { self.writer.cancelWriting() };
        let _ = std::fs::remove_file(&self.path);
    }
}

fn retain_buffer(buffer: &CVPixelBuffer) -> CFRetained<CVPixelBuffer> {
    // SAFETY: `buffer` is a valid CVPixelBuffer reference; retaining it keeps
    // it alive after the capture callback returns.
    unsafe { CFRetained::retain(std::ptr::NonNull::from(buffer)) }
}
