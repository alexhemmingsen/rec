//! The recording flow: shortcut → selection → recording → Stop → clipboard.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{Local, Utc};
use rec_capture::{CaptureError, DisplayInfo, Permission, RecordRequest, Recording, Rect};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::history::{Entry, History, Item};
use crate::settings::Settings;
use crate::{layout, naming, windowing};

pub const FPS: u32 = 30;
/// A click without dragging (or a drag smaller than this) cancels.
pub const MIN_REGION: f64 = 4.0;
pub const ESCAPE: &str = "Escape";

pub enum Phase {
    Idle,
    Selecting {
        displays: Vec<DisplayInfo>,
    },
    Recording {
        recording: Recording,
        path: PathBuf,
        started_at_ms: i64,
    },
    Finishing,
}

impl Phase {
    pub fn name(&self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Selecting { .. } => "selecting",
            Phase::Recording { .. } => "recording",
            Phase::Finishing => "finishing",
        }
    }
}

/// What to put back when a recording ends.
#[derive(Default)]
pub struct Restore {
    /// The main window was in front when the shortcut was pressed (e.g. the
    /// onboarding's practice step): show it again afterwards.
    pub main_was_focused: bool,
    /// The app in front before Rec took focus for the overlay.
    pub previous_app: Option<i32>,
}

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
    pub history: Mutex<History>,
    pub thumbnails_dir: PathBuf,
    pub phase: Mutex<Phase>,
    pub restore: Mutex<Restore>,
    pub pending_update: Mutex<Option<tauri_plugin_updater::Update>>,
    /// Where a native (AppKit) drag started, global top-left coordinates.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub native_drag: Mutex<Option<(f64, f64)>>,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Serialize)]
pub struct DragRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingError {
    pub message: String,
    pub permission: bool,
}

fn emit_error(app: &AppHandle, err: &CaptureError) {
    let permission = matches!(err, CaptureError::PermissionDenied);
    let message = match err {
        CaptureError::PermissionDenied => {
            "Rec needs Screen Recording permission. Open Settings → Permissions to grant it."
                .to_string()
        }
        CaptureError::Unsupported => {
            "Recording isn't available on this platform yet. Rec ships on macOS first.".to_string()
        }
        other => format!("Recording failed: {other}"),
    };
    log::error!("{message}");
    let _ = app.emit(
        "recording-error",
        RecordingError {
            message,
            permission,
        },
    );
}

fn set_escape(app: &AppHandle, on: bool) {
    let gs = app.global_shortcut();
    if on {
        match gs.register(ESCAPE) {
            Ok(()) => log::info!("Esc registered while selecting"),
            Err(e) => log::warn!("could not register Esc: {e}"),
        }
    } else {
        let _ = gs.unregister(ESCAPE);
    }
}

pub fn is_escape(shortcut: &Shortcut) -> bool {
    ESCAPE
        .parse::<Shortcut>()
        .map(|esc| &esc == shortcut)
        .unwrap_or(false)
}

/// The global shortcut was pressed.
pub fn on_shortcut(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut phase = state.phase.lock().unwrap();
    log::info!("shortcut pressed while {}", phase.name());
    // Pressing the shortcut while selecting or recording does nothing.
    if !matches!(*phase, Phase::Idle) {
        return;
    }
    if rec_capture::permission_status() == Permission::NotGranted {
        drop(phase);
        rec_capture::request_permission();
        windowing::show_main(app);
        emit_error(app, &CaptureError::PermissionDenied);
        return;
    }
    let displays = rec_capture::displays();
    if displays.is_empty() {
        drop(phase);
        windowing::show_main(app);
        emit_error(app, &CaptureError::Unsupported);
        return;
    }

    // Remember what to give focus back to, and get the main window out of the
    // way so it doesn't cover what you want to record.
    {
        let mut restore = state.restore.lock().unwrap();
        let main = app.get_webview_window(windowing::MAIN);
        let main_visible = main
            .as_ref()
            .and_then(|w| w.is_visible().ok())
            .unwrap_or(false);
        let main_focused = main
            .as_ref()
            .and_then(|w| w.is_focused().ok())
            .unwrap_or(false);
        restore.main_was_focused = main_visible && main_focused;
        restore.previous_app = frontmost_app(app);
        if main_visible && let Some(w) = main {
            let _ = w.hide();
        }
    }

    if let Err(e) = windowing::show_overlays(app, &displays) {
        log::error!("cannot show the selection overlay: {e}");
        drop(phase);
        restore_after(app);
        return;
    }
    set_escape(app, true);
    *phase = Phase::Selecting { displays };
    drop(phase);
    let _ = app.emit("phase-changed", "selecting");
}

fn frontmost_app(app: &AppHandle) -> Option<i32> {
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = app.run_on_main_thread(move || {
            let _ = tx.send(crate::macos::frontmost_app_pid());
        });
        rx.recv_timeout(std::time::Duration::from_millis(500))
            .ok()
            .flatten()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        None
    }
}

fn reactivate_previous(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let pid = app.state::<AppState>().restore.lock().unwrap().previous_app;
        if let Some(pid) = pid {
            let _ = app.run_on_main_thread(move || crate::macos::activate_app(pid));
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

/// Put focus back where it was before the shortcut.
fn restore_after(app: &AppHandle) {
    let state = app.state::<AppState>();
    let main_was_focused = std::mem::take(&mut state.restore.lock().unwrap().main_was_focused);
    if main_was_focused {
        windowing::show_main(app);
    } else {
        reactivate_previous(app);
    }
}

/// Esc, or a click without a drag, on the overlay.
pub fn cancel_selection(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut phase = state.phase.lock().unwrap();
    log::info!("cancel while {}", phase.name());
    if !matches!(*phase, Phase::Selecting { .. }) {
        return;
    }
    *phase = Phase::Idle;
    drop(phase);
    set_escape(app, false);
    windowing::hide_overlays(app);
    restore_after(app);
    let _ = app.emit("phase-changed", "idle");
}

pub fn save_dir(app: &AppHandle, settings: &Settings) -> PathBuf {
    let downloads = app
        .path()
        .download_dir()
        .or_else(|_| app.path().home_dir().map(|h| h.join("Downloads")))
        .unwrap_or_else(|_| PathBuf::from("."));
    settings.resolved_save_dir(&downloads)
}

/// The user let go of the mouse: start recording `region` (relative to the
/// display) right away, with no countdown.
pub fn start_recording(app: &AppHandle, display_id: u32, region: Rect) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut phase = state.phase.lock().unwrap();
    let Phase::Selecting { displays } = &*phase else {
        return Err("not selecting".into());
    };
    let Some(display) = displays.iter().find(|d| d.id == display_id).cloned() else {
        return Err(format!("unknown display {display_id}"));
    };
    set_escape(app, false);
    windowing::hide_overlays(app);

    let region = match region.clamp_to(display.frame.width, display.frame.height) {
        Some(r) if r.width >= MIN_REGION && r.height >= MIN_REGION => r,
        _ => {
            *phase = Phase::Idle;
            drop(phase);
            restore_after(app);
            let _ = app.emit("phase-changed", "idle");
            return Ok(());
        }
    };

    let dir = save_dir(app, &state.settings.lock().unwrap());
    if let Err(e) = std::fs::create_dir_all(&dir) {
        *phase = Phase::Idle;
        drop(phase);
        let err = CaptureError::Platform(format!("cannot create {}: {e}", dir.display()));
        restore_after(app);
        windowing::show_main(app);
        emit_error(app, &err);
        return Err(err.to_string());
    }
    let path = naming::unique_path(&dir, Local::now());
    // Where the Stop pill will go, relative to the region, if it lands
    // inside it (the region fills the screen).
    let global = layout::to_global(region, display.frame);
    let (px, py) = layout::pill_origin(
        global,
        display.frame,
        layout::PILL_WIDTH,
        layout::PILL_HEIGHT,
    );
    let pill = Rect::new(
        px - global.x,
        py - global.y,
        layout::PILL_WIDTH,
        layout::PILL_HEIGHT,
    );
    let exclude = (pill.x < region.width
        && pill.y < region.height
        && pill.x + pill.width > 0.0
        && pill.y + pill.height > 0.0)
        .then_some(pill);
    let request = RecordRequest {
        display_id,
        region,
        output: path.clone(),
        fps: FPS,
        show_cursor: true,
        exclude,
    };

    log::info!(
        "recording display {display_id} region {region:?} to {}",
        path.display()
    );
    match Recording::start(request) {
        Ok(recording) => {
            log::info!("recording started");
            let started_at_ms = Utc::now().timestamp_millis();
            *phase = Phase::Recording {
                recording,
                path,
                started_at_ms,
            };
            drop(phase);
            // Give the keyboard back to the app being recorded.
            reactivate_previous(app);
            match windowing::show_pill(app, global, &display, started_at_ms) {
                Ok(()) => log::info!("Stop pill shown"),
                Err(e) => log::error!("cannot show the Stop pill: {e}"),
            }
            let _ = app.emit("phase-changed", "recording");
            Ok(())
        }
        Err(err) => {
            *phase = Phase::Idle;
            drop(phase);
            restore_after(app);
            windowing::show_main(app);
            emit_error(app, &err);
            let _ = app.emit("phase-changed", "idle");
            Err(err.to_string())
        }
    }
}

/// macOS: AppKit mouse events drive the selection (see `macos::watch_mouse`).
/// Runs on the main thread, so the slow work goes to another thread.
#[cfg(target_os = "macos")]
pub fn on_native_mouse(app: &AppHandle, kind: crate::macos::MouseKind, x: f64, y: f64) {
    use crate::macos::MouseKind;
    let state = app.state::<AppState>();
    let displays = match &*state.phase.lock().unwrap() {
        Phase::Selecting { displays } => displays.clone(),
        _ => return,
    };
    let contains = |d: &&DisplayInfo, px: f64, py: f64| {
        let f = d.frame;
        px >= f.x && px < f.x + f.width && py >= f.y && py < f.y + f.height
    };
    let mut drag = state.native_drag.lock().unwrap();
    match kind {
        MouseKind::Down => {
            log::info!("native mousedown at {x},{y}");
            *drag = Some((x, y));
        }
        MouseKind::Dragged => {
            let Some((sx, sy)) = *drag else { return };
            let Some(d) = displays.iter().find(|d| contains(d, sx, sy)) else {
                return;
            };
            let (ex, ey) = (
                x.clamp(d.frame.x, d.frame.x + d.frame.width),
                y.clamp(d.frame.y, d.frame.y + d.frame.height),
            );
            let rect = DragRect {
                x: sx.min(ex) - d.frame.x,
                y: sy.min(ey) - d.frame.y,
                width: (sx - ex).abs(),
                height: (sy - ey).abs(),
            };
            let _ = app.emit_to(windowing::overlay_label(d.id), "overlay-drag", rect);
        }
        MouseKind::Up => {
            let Some((sx, sy)) = drag.take() else { return };
            log::info!("native mouseup at {x},{y}");
            let Some(d) = displays.iter().find(|d| contains(d, sx, sy)) else {
                return;
            };
            let (ex, ey) = (
                x.clamp(d.frame.x, d.frame.x + d.frame.width),
                y.clamp(d.frame.y, d.frame.y + d.frame.height),
            );
            let region = Rect::new(
                sx.min(ex) - d.frame.x,
                sy.min(ey) - d.frame.y,
                (sx - ex).abs(),
                (sy - ey).abs(),
            );
            let (app, id) = (app.clone(), d.id);
            std::thread::spawn(move || {
                if region.width < MIN_REGION || region.height < MIN_REGION {
                    cancel_selection(&app);
                } else if let Err(e) = start_recording(&app, id, region) {
                    log::info!("start_recording: {e}");
                }
            });
        }
    }
}

/// When the current recording started, if one is running.
pub fn recording_started_at(app: &AppHandle) -> Option<i64> {
    match &*app.state::<AppState>().phase.lock().unwrap() {
        Phase::Recording { started_at_ms, .. } => Some(*started_at_ms),
        _ => None,
    }
}

/// Stop was clicked: finish the file, add it to History and put its path on
/// the clipboard.
pub fn stop_recording(app: &AppHandle) -> Result<Item, String> {
    let state = app.state::<AppState>();
    let (recording, path) = {
        let mut phase = state.phase.lock().unwrap();
        match std::mem::replace(&mut *phase, Phase::Finishing) {
            Phase::Recording {
                recording, path, ..
            } => (recording, path),
            other => {
                *phase = other;
                return Err("not recording".into());
            }
        }
    };

    let result = recording.stop();
    log::info!(
        "recording stopped: {:?}",
        result
            .as_ref()
            .map(|s| (s.duration, s.frames))
            .map_err(|e| e.to_string())
    );
    *state.phase.lock().unwrap() = Phase::Idle;
    let _ = app.emit("phase-changed", "idle");

    let summary = match result {
        Ok(s) => s,
        Err(err) => {
            let _ = std::fs::remove_file(&path);
            windowing::show_main(app);
            emit_error(app, &err);
            return Err(err.to_string());
        }
    };

    let id = Utc::now().format("%Y%m%d%H%M%S%3f").to_string();
    let thumbnail = summary.thumbnail.as_ref().and_then(|frame| {
        let out = state.thumbnails_dir.join(format!("{id}.jpg"));
        match save_thumbnail(frame, &out) {
            Ok(()) => Some(out),
            Err(e) => {
                log::warn!("thumbnail: {e}");
                None
            }
        }
    });
    let entry = Entry {
        id,
        path: summary.path.clone(),
        created_at: Utc::now(),
        duration_secs: summary.duration.as_secs_f64(),
        width: summary.width,
        height: summary.height,
        thumbnail,
    };
    let item = {
        let mut history = state.history.lock().unwrap();
        if let Err(e) = history.add(entry.clone()) {
            log::error!("cannot save history: {e}");
        }
        history
            .items()
            .into_iter()
            .find(|i| i.entry.id == entry.id)
            .unwrap_or(Item {
                entry,
                missing: false,
                size_bytes: None,
            })
    };

    if let Err(e) = app
        .clipboard()
        .write_text(naming::clipboard_text(&summary.path))
    {
        log::error!("cannot write to the clipboard: {e}");
    }
    let _ = app.emit("recording-finished", &item);
    Ok(item)
}

/// The pill finished showing "Path copied".
pub fn pill_done(app: &AppHandle) {
    windowing::hide_pill(app);
    restore_after(app);
}

pub fn save_thumbnail(frame: &rec_capture::Frame, out: &Path) -> Result<(), String> {
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let img = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone())
        .ok_or("thumbnail buffer has the wrong size")?;
    let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
    let mut file = std::fs::File::create(out).map_err(|e| e.to_string())?;
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, 82)
        .encode_image(&rgb)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnails_are_written_as_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("thumbs/a.jpg");
        let frame = rec_capture::Frame {
            width: 4,
            height: 2,
            rgba: vec![200; 4 * 2 * 4],
        };
        save_thumbnail(&frame, &out).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(&bytes[..2], &[0xFF, 0xD8], "JPEG magic");
    }

    #[test]
    fn mismatched_thumbnail_buffers_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let frame = rec_capture::Frame {
            width: 4,
            height: 4,
            rgba: vec![0; 3],
        };
        assert!(save_thumbnail(&frame, &dir.path().join("x.jpg")).is_err());
    }

    #[test]
    fn escape_shortcut_is_recognised() {
        assert!(is_escape(&"Escape".parse::<Shortcut>().unwrap()));
        assert!(!is_escape(&"Control+Shift+5".parse::<Shortcut>().unwrap()));
    }
}
