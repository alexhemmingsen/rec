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
use crate::{layout, naming, sound, windowing};

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

/// Choosing a region is over (recording, or cancelled): Esc and the mouse
/// go back to normal.
fn end_selection_input(app: &AppHandle) {
    set_escape(app, false);
    #[cfg(target_os = "macos")]
    crate::mouse_tap::deactivate();
}

/// macOS with Accessibility: clicks, scrolling and typing keep going to your
/// apps while you choose; only a drag starts the selection. False means the
/// overlay has to catch the mouse instead.
fn pass_clicks_through(app: &AppHandle) -> bool {
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = app.run_on_main_thread(move || {
            let _ = tx.send(crate::mouse_tap::activate());
        });
        rx.recv_timeout(std::time::Duration::from_millis(500))
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        false
    }
}

/// The stop shortcut, when it differs from the start shortcut (which is
/// always registered and stops a recording itself when they're the same).
fn separate_stop_shortcut(app: &AppHandle) -> Option<String> {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().unwrap();
    let stop = settings.effective_stop_shortcut();
    (stop != settings.shortcut).then(|| stop.to_string())
}

/// The stop shortcut is only claimed while recording, so it doesn't block
/// those keys (⌘↩ by default) in other apps the rest of the time.
fn set_stop_shortcut_active(app: &AppHandle, on: bool) {
    let Some(stop) = separate_stop_shortcut(app) else {
        return;
    };
    let gs = app.global_shortcut();
    if on {
        if let Err(e) = gs.register(stop.as_str()) {
            log::warn!("could not register the stop shortcut {stop}: {e}");
        }
    } else {
        let _ = gs.unregister(stop.as_str());
    }
}

/// Esc: cancels choosing a region, or abandons a recording (stops it and
/// throws the video away: no file, no History entry, nothing copied).
pub fn on_escape(app: &AppHandle) {
    let recording = matches!(
        *app.state::<AppState>().phase.lock().unwrap(),
        Phase::Recording { .. }
    );
    if recording {
        abandon_recording(app);
    } else {
        cancel_selection(app);
    }
}

pub fn abandon_recording(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (recording, path) = {
        let mut phase = state.phase.lock().unwrap();
        match std::mem::replace(&mut *phase, Phase::Finishing) {
            Phase::Recording {
                recording, path, ..
            } => (recording, path),
            other => {
                *phase = other;
                return;
            }
        }
    };
    log::info!("recording abandoned with Esc");
    windowing::hide_overlays(app);
    windowing::hide_pill(app);
    set_escape(app, false);
    set_stop_shortcut_active(app, false);
    if let Err(e) = recording.stop() {
        log::warn!("stopping the abandoned recording: {e}");
    }
    if let Err(e) = std::fs::remove_file(&path) {
        log::warn!("cannot delete {}: {e}", path.display());
    }
    *state.phase.lock().unwrap() = Phase::Idle;
    let _ = app.emit("phase-changed", "idle");
    restore_after(app);
}

/// The pressed keys are a stop shortcut different from the start shortcut.
pub fn is_custom_stop_shortcut(app: &AppHandle, shortcut: &Shortcut) -> bool {
    separate_stop_shortcut(app)
        .and_then(|s| s.parse::<Shortcut>().ok())
        .is_some_and(|s| &s == shortcut)
}

/// Stop the recording from the keyboard. The Stop pill does the stopping, so
/// it shows "Saving…" and "Path copied" just as when it's clicked.
pub fn on_stop_shortcut(app: &AppHandle) {
    let recording = matches!(
        *app.state::<AppState>().phase.lock().unwrap(),
        Phase::Recording { .. }
    );
    log::info!("stop shortcut pressed; recording: {recording}");
    if recording {
        let _ = app.emit_to(windowing::PILL, "pill-stop", ());
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
    // While recording, the start shortcut stops it only when it's also set
    // as the stop shortcut.
    if matches!(*phase, Phase::Recording { .. }) {
        drop(phase);
        if separate_stop_shortcut(app).is_none() {
            on_stop_shortcut(app);
        }
        return;
    }
    // Pressing the shortcut while choosing a region does nothing.
    if !matches!(*phase, Phase::Idle) {
        return;
    }
    if rec_capture::permission_status() == Permission::NotGranted {
        drop(phase);
        if crate::commands::first_ask(app, |s| &mut s.asked_screen_recording) {
            rec_capture::request_permission();
        }
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

    sound::play_shortcut(app);

    // Remember what to give focus back to. Windows on screen stay as they
    // are: you arrange the desktop, Rec records it.
    state.restore.lock().unwrap().previous_app = frontmost_app(app);

    let pass_through = pass_clicks_through(app);
    log::info!("selecting; clicks pass through: {pass_through}");
    if let Err(e) = windowing::show_overlays(app, &displays, pass_through) {
        log::error!("cannot show the selection overlay: {e}");
        drop(phase);
        end_selection_input(app);
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
    reactivate_previous(app);
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
    end_selection_input(app);
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
    end_selection_input(app);

    let region = match region.clamp_to(display.frame.width, display.frame.height) {
        Some(r) if r.width >= MIN_REGION && r.height >= MIN_REGION => r,
        _ => {
            *phase = Phase::Idle;
            drop(phase);
            windowing::hide_overlays(app);
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
        windowing::hide_overlays(app);
        restore_after(app);
        windowing::show_main(app);
        emit_error(app, &err);
        return Err(err.to_string());
    }
    let path = naming::unique_path(&dir, Local::now());
    // Where the Stop pill will go (top centre of the display), relative to
    // the region, in case the region covers it.
    let global = layout::to_global(region, display.frame);
    let frame = layout::pill_frame(display.frame);
    let pill = Rect::new(
        frame.x - global.x,
        frame.y - global.y,
        frame.width,
        frame.height,
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
            set_stop_shortcut_active(app, true);
            // Esc abandons the recording.
            set_escape(app, true);
            // Keep the box you drew on screen while recording.
            windowing::frame_overlay(app, display_id, region);
            // Give the keyboard back to the app being recorded.
            reactivate_previous(app);
            match windowing::show_pill(app, &display, started_at_ms) {
                Ok(()) => log::info!("Stop pill shown"),
                Err(e) => log::error!("cannot show the Stop pill: {e}"),
            }
            let _ = app.emit("phase-changed", "recording");
            Ok(())
        }
        Err(err) => {
            *phase = Phase::Idle;
            drop(phase);
            windowing::hide_overlays(app);
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
            if region.width < MIN_REGION || region.height < MIN_REGION {
                // A click or a box too small to record: keep choosing. Only
                // Esc cancels.
                let none = DragRect {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                };
                let _ = app.emit_to(windowing::overlay_label(d.id), "overlay-drag", none);
                return;
            }
            let (app, id) = (app.clone(), d.id);
            std::thread::spawn(move || {
                if let Err(e) = start_recording(&app, id, region) {
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
    windowing::hide_overlays(app);
    set_escape(app, false);
    set_stop_shortcut_active(app, false);

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
