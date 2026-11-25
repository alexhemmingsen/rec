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
    bgra_to_rgba_thumbnail, output_dimensions, target_bitrate,
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
    patch: Option<PathBuf>,
    _portal: Option<PortalSession>,
}

// SAFETY: GStreamer objects are thread-safe; the portal session only holds a
// file descriptor and a thread handle.
unsafe impl Send for Recording {}

/// The first full-size frame: tightly packed BGRA.
struct FirstFrame {
    width: u32,
    height: u32,
    bgra: Vec<u8>,
}

fn pull_first_frame(sink: &gst_app::AppSink, timeout: Duration) -> Option<FirstFrame> {
    let sample = sink.try_pull_sample(gst::ClockTime::from_nseconds(timeout.as_nanos() as u64))?;
    let caps = sample.caps()?;
    let s = caps.structure(0)?;
    let width = s.get::<i32>("width").ok()? as u32;
    let height = s.get::<i32>("height").ok()? as u32;
    let buffer = sample.buffer()?;
    let map = buffer.map_readable().ok()?;
    let data = map.as_slice();
    let stride = data.len() / height.max(1) as usize;
    let row = width as usize * 4;
    let mut bgra = Vec::with_capacity(row * height as usize);
    for y in 0..height as usize {
        bgra.extend_from_slice(data.get(y * stride..y * stride + row)?);
    }
    Some(FirstFrame {
        width,
        height,
        bgra,
    })
}

/// Write `rect` of a BGRA frame as an RGBA PNG.
fn write_patch(frame: &FirstFrame, rect: PixelRect, path: &std::path::Path) -> std::io::Result<()> {
    let mut rgba = Vec::with_capacity((rect.width * rect.height * 4) as usize);
    for y in rect.y..rect.y + rect.height {
        for x in rect.x..rect.x + rect.width {
            let i = ((y * frame.width + x) * 4) as usize;
            let p = &frame.bgra[i..i + 4];
            rgba.extend_from_slice(&[p[2], p[1], p[0], 255]);
        }
    }
    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), rect.width, rect.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
    writer
        .write_image_data(&rgba)
        .map_err(std::io::Error::other)
}

impl Recording {
    pub fn start(request: RecordRequest) -> Result<Recording> {
        log::info!(
            "linux capture: starting ({})",
            if is_wayland() { "wayland" } else { "x11" }
        );
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

        let parser =
            gst_pipeline::parser(gst::ElementFactory::find("h264parse").is_some(), &encoder)
                .ok_or_else(|| {
                    CaptureError::Platform(
                        "install gstreamer1.0-plugins-bad (h264parse) to record".into(),
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
        // Linux can't leave Rec's own Stop pill out of a capture, so when
        // the pill sits inside the region, cover it with what was there
        // before it appeared.
        let cover = request.exclude.and_then(|e| {
            gst_pipeline::exclude_pixels(
                (e.x, e.y, e.width, e.height),
                scale,
                (w, h),
                (out_w, out_h),
            )
        });
        let can_cover = gst::ElementFactory::find("gdkpixbufoverlay").is_some();
        if cover.is_some() && !can_cover {
            log::warn!(
                "gdkpixbufoverlay missing: the Stop pill may show in a full-screen recording"
            );
        }
        let description = gst_pipeline::pipeline(
            &source,
            request.fps,
            out_w,
            out_h,
            cover.is_some() && can_cover,
            &encoder,
            parser,
            &request.output.to_string_lossy(),
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

        log::info!("linux capture: pipeline built, starting");
        pipeline.set_state(gst::State::Playing).map_err(gst_err)?;
        let started = Instant::now();
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
        log::info!("linux capture: playing, waiting for the first frame");
        let first = pull_first_frame(&sink, Duration::from_secs(3));
        // Only the first frame is needed; stop converting the rest.
        if let Some(valve) = pipeline.by_name("firstframe") {
            valve.set_property("drop", true);
        }
        let Some(first) = first else {
            log::info!("linux capture: no first frame");
            let _ = pipeline.set_state(gst::State::Null);
            let _ = std::fs::remove_file(&request.output);
            return Err(CaptureError::NoFrames);
        };
        log::info!(
            "linux capture: first frame {}x{}",
            first.width,
            first.height
        );
        let thumbnail = bgra_to_rgba_thumbnail(
            &first.bgra,
            first.width as usize,
            first.height as usize,
            first.width as usize * 4,
            THUMBNAIL_MAX_WIDTH as usize,
        );

        let mut patch = None;
        if let (Some(rect), Some(mask)) = (cover, pipeline.by_name("mask")) {
            let rect = PixelRect {
                width: rect.width.min(first.width.saturating_sub(rect.x)),
                height: rect.height.min(first.height.saturating_sub(rect.y)),
                ..rect
            };
            let path = std::env::temp_dir().join(format!("rec-cover-{}.png", std::process::id()));
            match write_patch(&first, rect, &path) {
                Ok(()) => {
                    mask.set_property("offset-x", rect.x as i32);
                    mask.set_property("offset-y", rect.y as i32);
                    mask.set_property("location", path.to_string_lossy().to_string());
                    log::info!("linux capture: covering the Stop pill at {rect:?}");
                    patch = Some(path);
                }
                Err(e) => log::warn!("cannot cover the Stop pill: {e}"),
            }
        }

        Ok(Recording {
            pipeline,
            path: request.output,
            started,
            width: out_w,
            height: out_h,
            thumbnail,
            patch,
            _portal: portal,
        })
    }

    pub fn stop(self) -> Result<RecordingSummary> {
        let duration = self.started.elapsed();
        log::info!("linux capture: stopping after {duration:?}");
        // A closed valve drops events too, so the thumbnail branch would never
        // see EOS and the pipeline would never report it: open it first.
        if let Some(valve) = self.pipeline.by_name("firstframe") {
            valve.set_property("drop", false);
        }
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
        if let Some(p) = &self.patch {
            let _ = std::fs::remove_file(p);
        }
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
        if let Some(p) = &self.patch {
            let _ = std::fs::remove_file(p);
        }
    }
}
