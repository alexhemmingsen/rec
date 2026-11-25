//! Linux backend: GStreamer.
//!
//! - X11 sessions capture the region with `ximagesrc`.
//! - Wayland sessions go through the xdg-desktop-portal ScreenCast API and
//!   PipeWire. The system's "share your screen" dialog appears the first
//!   time; Rec stores the portal's restore token so later recordings skip it.
//!
//! Encoding uses the first H.264 encoder GStreamer has (x264 from
//! gstreamer1.0-plugins-ugly, OpenH264, or VA-API).

use std::os::fd::{AsRawFd, OwnedFd};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use crate::gst_pipeline::{self, PixelRect};
use crate::{
    CaptureError, DisplayInfo, Frame, Permission, RecordRequest, RecordingSummary, Rect, Result,
    output_dimensions, target_bitrate,
};

const THUMBNAIL_MAX_WIDTH: u32 = 640;

pub fn permission_status() -> Permission {
    // On Wayland the portal asks when recording starts.
    Permission::NotRequired
}

pub fn request_permission() -> bool {
    true
}

pub fn is_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland")
}

/// GDK's integer scale (Rec's windows are GTK); 1 unless GDK_SCALE is set.
fn gdk_scale() -> f64 {
    std::env::var("GDK_SCALE")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|s| *s >= 1.0)
        .unwrap_or(1.0)
}

/// Monitors from X11 RandR (works on X11 and, through XWayland, on Wayland).
pub fn displays() -> Vec<DisplayInfo> {
    use x11rb::connection::Connection;
    use x11rb::protocol::randr::ConnectionExt as _;

    let Ok((conn, screen)) = x11rb::connect(None) else {
        return Vec::new();
    };
    let root = conn.setup().roots[screen].root;
    let Ok(cookie) = conn.randr_get_monitors(root, true) else {
        return Vec::new();
    };
    let Ok(reply) = cookie.reply() else {
        return Vec::new();
    };
    let scale = gdk_scale();
    let mut list: Vec<DisplayInfo> = reply
        .monitors
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let px = Rect::new(m.x as f64, m.y as f64, m.width as f64, m.height as f64);
            DisplayInfo {
                id: i as u32 + 1,
                frame: Rect::new(
                    px.x / scale,
                    px.y / scale,
                    px.width / scale,
                    px.height / scale,
                ),
                pixel_frame: px,
                scale_factor: scale,
                is_main: m.primary,
            }
        })
        .collect();
    if !list.is_empty() && !list.iter().any(|d| d.is_main) {
        list[0].is_main = true;
    }
    list
}

fn gst_err(e: impl std::fmt::Display) -> CaptureError {
    CaptureError::Platform(e.to_string())
}

/// A ScreenCast portal session, kept open on its own thread until dropped.
struct PortalSession {
    fd: OwnedFd,
    node: u32,
    stop: Option<mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for PortalSession {
    fn drop(&mut self) {
        self.stop.take();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn restore_token_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(base.join("rec").join("screencast-restore-token"))
}

fn open_portal() -> Result<PortalSession> {
    use ashpd::desktop::PersistMode;
    use ashpd::desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType};

    let (tx, rx) = mpsc::channel::<std::result::Result<(OwnedFd, u32), String>>();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let thread = std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
                return;
            }
        };
        runtime.block_on(async move {
            let result: std::result::Result<_, ashpd::Error> = async {
                let proxy = Screencast::new().await?;
                let session = proxy.create_session(Default::default()).await?;
                let token_path = restore_token_path();
                let token = token_path
                    .as_ref()
                    .and_then(|p| std::fs::read_to_string(p).ok());
                proxy
                    .select_sources(
                        &session,
                        SelectSourcesOptions::default()
                            .set_cursor_mode(CursorMode::EMBEDDED)
                            .set_sources(SourceType::MONITOR)
                            .set_multiple(false)
                            .set_persist_mode(PersistMode::ExplicitlyRevoked)
                            .set_restore_token(token.as_deref().map(str::trim)),
                    )
                    .await?;
                let streams = proxy
                    .start(&session, None, Default::default())
                    .await?
                    .response()?;
                if let (Some(t), Some(p)) = (streams.restore_token(), token_path.as_ref()) {
                    if let Some(dir) = p.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = std::fs::write(p, t);
                }
                let node = streams
                    .streams()
                    .first()
                    .map(|s| s.pipe_wire_node_id())
                    .ok_or(ashpd::Error::NoResponse)?;
                let fd = proxy
                    .open_pipe_wire_remote(&session, Default::default())
                    .await?;
                Ok((session, fd, node))
            }
            .await;
            match result {
                Ok((session, fd, node)) => {
                    let _ = tx.send(Ok((fd, node)));
                    // Keep the session open until the recording ends.
                    let _ = tokio::task::spawn_blocking(move || stop_rx.recv()).await;
                    let _ = session.close().await;
                }
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                }
            }
        });
    });
    // The user may need time to answer the system dialog.
    match rx.recv_timeout(Duration::from_secs(120)) {
        Ok(Ok((fd, node))) => Ok(PortalSession {
            fd,
            node,
            stop: Some(stop_tx),
            thread: Some(thread),
        }),
        Ok(Err(e)) => Err(CaptureError::Platform(format!(
            "screen sharing was not started: {e}"
        ))),
        Err(_) => Err(CaptureError::Timeout("the screen sharing dialog")),
    }
}

pub struct Recording {
    pipeline: gst::Pipeline,
    path: PathBuf,
    started: Instant,
    width: u32,
    height: u32,
    thumbnail: Option<Frame>,
    _portal: Option<PortalSession>,
}

// SAFETY: GStreamer objects are thread-safe; the portal session only holds a
// file descriptor and a thread handle.
unsafe impl Send for Recording {}

fn pull_thumbnail(sink: &gst_app::AppSink, timeout: Duration) -> Option<Frame> {
    let sample = sink.try_pull_sample(gst::ClockTime::from_nseconds(timeout.as_nanos() as u64))?;
    let caps = sample.caps()?;
    let s = caps.structure(0)?;
    let width = s.get::<i32>("width").ok()? as u32;
    let height = s.get::<i32>("height").ok()? as u32;
    let buffer = sample.buffer()?;
    let map = buffer.map_readable().ok()?;
    let data = map.as_slice();
    let stride = data.len() / height.max(1) as usize;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height as usize {
        rgba.extend_from_slice(&data[y * stride..y * stride + width as usize * 4]);
    }
    Some(Frame {
        width,
        height,
        rgba,
    })
}

impl Recording {
    pub fn start(request: RecordRequest) -> Result<Recording> {
        gst::init().map_err(gst_err)?;
        let display = displays()
            .into_iter()
            .find(|d| d.id == request.display_id)
            .ok_or(CaptureError::DisplayNotFound(request.display_id))?;
        let scale = display.scale_factor;
        let (mw, mh) = (
            display.pixel_frame.width as u32,
            display.pixel_frame.height as u32,
        );
        let r = &request.region;
        let x = ((r.x * scale).round().max(0.0) as u32).min(mw.saturating_sub(2));
        let y = ((r.y * scale).round().max(0.0) as u32).min(mh.saturating_sub(2));
        let w = (((r.width * scale).round() as u32).min(mw - x)) & !1;
        let h = (((r.height * scale).round() as u32).min(mh - y)) & !1;
        if w < 2 || h < 2 {
            return Err(CaptureError::EmptyRegion);
        }
        let (out_w, out_h) = output_dimensions(w as f64, h as f64, 1.0);
        let encoder = gst_pipeline::encoder_description(
            |name| gst::ElementFactory::find(name).is_some(),
            target_bitrate(out_w, out_h, request.fps),
        )
        .ok_or_else(|| {
            CaptureError::Platform(
                "no H.264 encoder found: install gstreamer1.0-plugins-ugly (x264) or OpenH264"
                    .into(),
            )
        })?;

        let (source, portal) = if is_wayland() {
            let portal = open_portal()?;
            let src = gst_pipeline::pipewire_source(
                portal.fd.as_raw_fd(),
                portal.node,
                mw,
                mh,
                PixelRect {
                    x,
                    y,
                    width: w,
                    height: h,
                },
            );
            (src, Some(portal))
        } else {
            let name = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into());
            let global = PixelRect {
                x: display.pixel_frame.x as u32 + x,
                y: display.pixel_frame.y as u32 + y,
                width: w,
                height: h,
            };
            (
                gst_pipeline::x11_source(&name, global, request.show_cursor),
                None,
            )
        };

        if request.output.exists() {
            std::fs::remove_file(&request.output).map_err(gst_err)?;
        }
        let (tw, th) = gst_pipeline::thumb_size(out_w, out_h, THUMBNAIL_MAX_WIDTH);
        let description = gst_pipeline::pipeline(
            &source,
            request.fps,
            out_w,
            out_h,
            &encoder,
            &request.output.to_string_lossy(),
            tw,
            th,
        );
        log::info!("pipeline: {description}");
        let pipeline = gst::parse::launch(&description)
            .map_err(gst_err)?
            .downcast::<gst::Pipeline>()
            .map_err(|_| CaptureError::Platform("not a pipeline".into()))?;
        let sink = pipeline
            .by_name("thumb")
            .and_then(|e| e.downcast::<gst_app::AppSink>().ok())
            .ok_or_else(|| CaptureError::Platform("thumbnail sink missing".into()))?;

        pipeline.set_state(gst::State::Playing).map_err(gst_err)?;
        let bus = pipeline
            .bus()
            .ok_or_else(|| CaptureError::Platform("no bus".into()))?;
        // Fail fast if the source can't start (e.g. no X display).
        if let Some(msg) = bus.timed_pop_filtered(
            gst::ClockTime::from_mseconds(300),
            &[gst::MessageType::Error],
        ) {
            let _ = pipeline.set_state(gst::State::Null);
            if let gst::MessageView::Error(e) = msg.view() {
                return Err(CaptureError::Platform(e.error().to_string()));
            }
        }
        let started = Instant::now();
        let thumbnail = pull_thumbnail(&sink, Duration::from_secs(3));
        if thumbnail.is_none() {
            let _ = pipeline.set_state(gst::State::Null);
            let _ = std::fs::remove_file(&request.output);
            return Err(CaptureError::NoFrames);
        }

        Ok(Recording {
            pipeline,
            path: request.output,
            started,
            width: out_w,
            height: out_h,
            thumbnail,
            _portal: portal,
        })
    }

    pub fn stop(self) -> Result<RecordingSummary> {
        let duration = self.started.elapsed();
        self.pipeline.send_event(gst::event::Eos::new());
        let bus = self
            .pipeline
            .bus()
            .ok_or_else(|| CaptureError::Platform("no bus".into()))?;
        let msg = bus.timed_pop_filtered(
            gst::ClockTime::from_seconds(15),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        );
        let _ = self.pipeline.set_state(gst::State::Null);
        match msg.as_ref().map(|m| m.view()) {
            Some(gst::MessageView::Eos(_)) => {}
            Some(gst::MessageView::Error(e)) => {
                return Err(CaptureError::Platform(e.error().to_string()));
            }
            _ => return Err(CaptureError::Timeout("the video file to finish writing")),
        }
        let size = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        if size == 0 {
            return Err(CaptureError::NoFrames);
        }
        Ok(RecordingSummary {
            path: self.path.clone(),
            duration,
            width: self.width,
            height: self.height,
            frames: (duration.as_secs_f64() * 30.0) as u64,
            thumbnail: self.thumbnail.clone(),
        })
    }

    pub fn cancel(self) {
        let _ = self.pipeline.set_state(gst::State::Null);
        let _ = std::fs::remove_file(&self.path);
    }
}
