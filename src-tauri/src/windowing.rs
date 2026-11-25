//! The three kinds of window: the main window, one selection overlay per
//! display, and the floating Stop pill.

use std::sync::atomic::{AtomicU64, Ordering};

use rec_capture::{DisplayInfo, Rect};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
#[cfg(not(target_os = "macos"))]
use tauri::{PhysicalPosition, PhysicalSize};

use crate::layout;

pub const MAIN: &str = "main";
pub const PILL: &str = "pill";
const OVERLAY_PREFIX: &str = "overlay-";

pub fn overlay_label(display_id: u32) -> String {
    format!("{OVERLAY_PREFIX}{display_id}")
}

pub fn is_overlay(label: &str) -> bool {
    label.starts_with(OVERLAY_PREFIX)
}

/// Bring the main window up, e.g. after a Dock click or an error.
pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn floating_builder<'a>(
    app: &'a AppHandle,
    label: &'a str,
    title: &str,
) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title(title)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        // GTK ignores size requests on non-resizable windows; borderless
        // windows have nothing to resize them with anyway.
        .resizable(cfg!(target_os = "linux"))
        .shadow(false)
        .visible(false)
        .focused(false)
        .visible_on_all_workspaces(true);
    // Keeps the window out of screen captures on Windows
    // (WDA_EXCLUDEFROMCAPTURE). macOS excludes Rec's windows by app instead.
    #[cfg(windows)]
    let builder = builder.content_protected(true);
    #[cfg(target_os = "macos")]
    let builder = builder.accept_first_mouse(true);
    builder
}

/// Place a floating window exactly at `frame` (global logical coordinates,
/// top-left origin), which is `pixels` in physical pixels.
fn place(app: &AppHandle, window: &WebviewWindow, frame: Rect, pixels: Rect, level: isize) {
    #[cfg(target_os = "macos")]
    {
        let w = window.clone();
        let _ = app.run_on_main_thread(move || {
            if let Ok(ns) = w.ns_window() {
                crate::macos::configure_floating(ns, frame, level);
            }
        });
    }
    #[cfg(target_os = "macos")]
    let _ = pixels;
    #[cfg(not(target_os = "macos"))]
    {
        // Physical pixels: logical positions are ambiguous across monitors
        // with different scale factors.
        let _ = (app, level, frame);
        let _ = window.set_position(PhysicalPosition::new(
            pixels.x.round() as i32,
            pixels.y.round() as i32,
        ));
        let _ = window.set_size(PhysicalSize::new(
            pixels.width.round() as u32,
            pixels.height.round() as u32,
        ));
    }
}

/// Create (hidden) overlays for every display and close those for displays
/// that are gone. Creating ahead of time makes the shortcut feel instant.
pub fn prepare_overlays(app: &AppHandle, displays: &[DisplayInfo]) -> tauri::Result<()> {
    let wanted: Vec<String> = displays.iter().map(|d| overlay_label(d.id)).collect();
    for (label, window) in app.webview_windows() {
        if is_overlay(&label) && !wanted.contains(&label) {
            let _ = window.destroy();
        }
    }
    for d in displays {
        let label = overlay_label(d.id);
        log::info!(
            "overlay for display {} at {:?} (scale {})",
            d.id,
            d.frame,
            d.scale_factor
        );
        let window = match app.get_webview_window(&label) {
            Some(w) => w,
            None => floating_builder(app, &label, "Rec selection")
                .inner_size(d.frame.width, d.frame.height)
                .position(d.frame.x, d.frame.y)
                .build()?,
        };
        place(app, &window, d.frame, d.pixel_frame, overlay_level());
    }
    Ok(())
}

fn overlay_level() -> isize {
    #[cfg(target_os = "macos")]
    return crate::macos::OVERLAY_LEVEL;
    #[cfg(not(target_os = "macos"))]
    0
}

fn pill_level() -> isize {
    #[cfg(target_os = "macos")]
    return crate::macos::PILL_LEVEL;
    #[cfg(not(target_os = "macos"))]
    0
}

/// Show the selection overlays. With `pass_through` they let the mouse
/// through (the macOS event tap catches drags) and Rec stays in the
/// background, so you can keep using your apps. Otherwise they catch the
/// mouse and Rec comes to the front.
pub fn show_overlays(
    app: &AppHandle,
    displays: &[DisplayInfo],
    pass_through: bool,
) -> tauri::Result<()> {
    SHOWN.fetch_add(1, Ordering::SeqCst);
    prepare_overlays(app, displays)?;
    for (label, window) in app.webview_windows() {
        if is_overlay(&label) {
            let _ = window.set_ignore_cursor_events(pass_through);
        }
    }
    let _ = app.emit("overlay-reset", ());
    let focus_id = display_under_mouse(app, displays);
    for d in displays {
        if let Some(w) = app.get_webview_window(&overlay_label(d.id)) {
            w.show()?;
            log::info!(
                "overlay {} shown: visible={:?} position={:?} size={:?}",
                d.id,
                w.is_visible().ok(),
                w.outer_position().ok(),
                w.outer_size().ok()
            );
        }
    }
    if pass_through {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    let _ = app.run_on_main_thread(crate::macos::activate_self);
    if let Some(w) = focus_id.and_then(|id| app.get_webview_window(&overlay_label(id))) {
        let _ = w.set_focus();
    }
    Ok(())
}

fn display_under_mouse(app: &AppHandle, displays: &[DisplayInfo]) -> Option<u32> {
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = app.run_on_main_thread(move || {
            let _ = tx.send(crate::macos::mouse_location());
        });
        if let Ok(Some((x, y))) = rx.recv_timeout(std::time::Duration::from_millis(500))
            && let Some(d) = displays.iter().find(|d| {
                x >= d.frame.x
                    && x < d.frame.x + d.frame.width
                    && y >= d.frame.y
                    && y < d.frame.y + d.frame.height
            })
        {
            return Some(d.id);
        }
    }
    #[cfg(not(target_os = "macos"))]
    if let Ok(p) = app.cursor_position()
        && let Some(d) = displays.iter().find(|d| {
            let f = d.pixel_frame;
            p.x >= f.x && p.x < f.x + f.width && p.y >= f.y && p.y < f.y + f.height
        })
    {
        return Some(d.id);
    }
    let _ = app;
    displays
        .iter()
        .find(|d| d.is_main)
        .or(displays.first())
        .map(|d| d.id)
}

/// The recorded region, relative to its display, for the overlay page.
#[derive(Clone, Serialize)]
struct FrameRegion {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// While recording, the overlay on the recorded display stays up as a frame:
/// the region clear and the rest tinted, so you can see what's recorded. It
/// lets clicks through and sits under the Stop pill. Rec's windows are left
/// out of the video (by app on macOS, WDA_EXCLUDEFROMCAPTURE on Windows). On
/// Linux nothing keeps them out of the capture, so the overlays go away.
pub fn frame_overlay(app: &AppHandle, display_id: u32, region: Rect) {
    if cfg!(target_os = "linux") {
        hide_overlays(app);
        return;
    }
    let target = overlay_label(display_id);
    for (label, window) in app.webview_windows() {
        if !is_overlay(&label) {
            continue;
        }
        if label != target {
            let _ = window.hide();
            continue;
        }
        let _ = window.set_ignore_cursor_events(true);
        #[cfg(target_os = "macos")]
        {
            let w = window.clone();
            let _ = app.run_on_main_thread(move || {
                if let Ok(ns) = w.ns_window() {
                    crate::macos::set_level(ns, crate::macos::PILL_LEVEL - 1);
                }
            });
        }
        let payload = FrameRegion {
            x: region.x,
            y: region.y,
            width: region.width,
            height: region.height,
        };
        let _ = app.emit_to(label.as_str(), "overlay-recording", payload);
    }
}

/// Counts `show_overlays` calls, so a delayed hide never hides overlays that
/// were shown again in the meantime.
static SHOWN: AtomicU64 = AtomicU64::new(0);

/// Clear the overlays, then hide them. A hidden window keeps its last picture
/// (often the dark tint), which would flash when it's shown again; clearing
/// first and hiding a few frames later leaves a blank picture instead.
pub fn hide_overlays(app: &AppHandle) {
    let _ = app.emit("overlay-clear", ());
    let shown = SHOWN.load(Ordering::SeqCst);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(80));
        if SHOWN.load(Ordering::SeqCst) != shown {
            return;
        }
        for (label, window) in app.webview_windows() {
            if is_overlay(&label) {
                let _ = window.hide();
            }
        }
    });
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PillStarted {
    pub started_at_ms: i64,
}

pub fn ensure_pill(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    match app.get_webview_window(PILL) {
        Some(w) => Ok(w),
        None => floating_builder(app, PILL, "Rec")
            .inner_size(layout::PILL_WIDTH, layout::PILL_HEIGHT)
            .build(),
    }
}

/// Show the Stop pill at the top centre of the display, where the "Drag to
/// record" hint was.
pub fn show_pill(app: &AppHandle, display: &DisplayInfo, started_at_ms: i64) -> tauri::Result<()> {
    let pill = ensure_pill(app)?;
    let frame = layout::pill_frame(display.frame);
    place(
        app,
        &pill,
        frame,
        layout::to_pixels(frame, display),
        pill_level(),
    );
    let _ = app.emit_to(PILL, "pill-started", PillStarted { started_at_ms });
    pill.show()
}

pub fn hide_pill(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(PILL) {
        let _ = w.hide();
    }
}
