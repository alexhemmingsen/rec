//! Commands the web UI calls.

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use tauri_plugin_opener::OpenerExt;

use crate::ffmpeg::{self, FfmpegStatus};
use crate::history::Item;
use crate::recorder::{self, AppState};
use crate::updater::{self, UpdateInfo};
use crate::{naming, shortcut};

pub fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(windows) {
        "windows"
    } else {
        "linux"
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub platform: &'static str,
    pub version: String,
    pub shortcut: String,
    pub save_dir: String,
    pub save_dir_is_default: bool,
    pub check_for_updates: bool,
    pub onboarding_complete: bool,
    pub open_at_login: bool,
    pub permission: &'static str,
    pub recording_supported: bool,
    pub phase: &'static str,
}

fn permission_name() -> &'static str {
    match rec_capture::permission_status() {
        rec_capture::Permission::Granted => "granted",
        rec_capture::Permission::NotGranted => "notGranted",
        rec_capture::Permission::NotRequired => "notRequired",
    }
}

fn info(app: &AppHandle) -> AppInfo {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().unwrap().clone();
    AppInfo {
        platform: platform(),
        version: app.package_info().version.to_string(),
        save_dir: recorder::save_dir(app, &settings)
            .to_string_lossy()
            .into_owned(),
        save_dir_is_default: settings.save_dir.is_none(),
        shortcut: settings.shortcut,
        check_for_updates: settings.check_for_updates,
        onboarding_complete: settings.onboarding_complete,
        open_at_login: app.autolaunch().is_enabled().unwrap_or(false),
        permission: permission_name(),
        recording_supported: cfg!(target_os = "macos"),
        phase: state.phase.lock().unwrap().name(),
    }
}

fn update_settings(
    app: &AppHandle,
    f: impl FnOnce(&mut crate::settings::Settings),
) -> Result<AppInfo, String> {
    let state = app.state::<AppState>();
    {
        let mut settings = state.settings.lock().unwrap();
        f(&mut settings);
        settings
            .save(&state.settings_path)
            .map_err(|e| format!("Cannot save settings: {e}"))?;
    }
    Ok(info(app))
}

#[tauri::command]
pub fn get_app_info(app: AppHandle) -> AppInfo {
    info(&app)
}

#[tauri::command]
pub fn set_shortcut(app: AppHandle, accelerator: String) -> Result<AppInfo, String> {
    let new = shortcut::parse(&accelerator)?;
    let state = app.state::<AppState>();
    let old = state.settings.lock().unwrap().shortcut.clone();
    if old == accelerator {
        return Ok(info(&app));
    }
    let gs = app.global_shortcut();
    let _ = gs.unregister(old.as_str());
    if let Err(e) = gs.register(new) {
        // Put the old one back so the app never ends up without a shortcut.
        let _ = gs.register(old.as_str());
        return Err(format!(
            "That shortcut is taken by another app or the system ({e}). Try another."
        ));
    }
    update_settings(&app, |s| s.shortcut = accelerator)
}

/// Pause the global shortcut while the user records a new one in Settings.
#[tauri::command]
pub fn set_shortcut_suspended(app: AppHandle, suspended: bool) {
    let current = app
        .state::<AppState>()
        .settings
        .lock()
        .unwrap()
        .shortcut
        .clone();
    let gs = app.global_shortcut();
    if suspended {
        let _ = gs.unregister(current.as_str());
    } else if !gs.is_registered(current.as_str())
        && let Err(e) = gs.register(current.as_str())
    {
        log::error!("cannot re-register {current}: {e}");
    }
}

#[tauri::command]
pub async fn choose_save_dir(app: AppHandle) -> Result<AppInfo, String> {
    let current = {
        let state = app.state::<AppState>();
        let settings = state.settings.lock().unwrap();
        recorder::save_dir(&app, &settings)
    };
    let picked = app
        .dialog()
        .file()
        .set_directory(current)
        .blocking_pick_folder();
    match picked.and_then(|p| p.into_path().ok()) {
        Some(path) => update_settings(&app, |s| s.save_dir = Some(path)),
        None => Ok(info(&app)),
    }
}

#[tauri::command]
pub fn reset_save_dir(app: AppHandle) -> Result<AppInfo, String> {
    update_settings(&app, |s| s.save_dir = None)
}

#[tauri::command]
pub fn set_open_at_login(app: AppHandle, enabled: bool) -> Result<AppInfo, String> {
    let launcher = app.autolaunch();
    let result = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    result.map_err(|e| format!("Cannot change Open at login: {e}"))?;
    Ok(info(&app))
}

#[tauri::command]
pub fn set_check_for_updates(app: AppHandle, enabled: bool) -> Result<AppInfo, String> {
    update_settings(&app, |s| s.check_for_updates = enabled)
}

#[tauri::command]
pub fn complete_onboarding(app: AppHandle) -> Result<AppInfo, String> {
    update_settings(&app, |s| s.onboarding_complete = true)
}

#[tauri::command]
pub fn permission_status() -> &'static str {
    permission_name()
}

/// Ask macOS for Screen Recording access and open the matching Settings pane.
#[tauri::command]
pub fn request_screen_permission(app: AppHandle) -> &'static str {
    if rec_capture::permission_status() == rec_capture::Permission::NotGranted {
        rec_capture::request_permission();
        open_permission_settings(app);
    }
    permission_name()
}

#[tauri::command]
pub fn open_permission_settings(app: AppHandle) {
    if cfg!(target_os = "macos") {
        let url = "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";
        if let Err(e) = app.opener().open_url(url, None::<&str>) {
            log::error!("cannot open System Settings: {e}");
        }
    }
}

#[tauri::command]
pub fn relaunch(app: AppHandle) {
    app.restart();
}

#[tauri::command]
pub async fn check_ffmpeg() -> FfmpegStatus {
    tauri::async_runtime::spawn_blocking(|| ffmpeg::detect(platform()))
        .await
        .unwrap_or_else(|_| FfmpegStatus {
            found: false,
            path: None,
            install_command: ffmpeg::install_command(platform()).into(),
        })
}

#[tauri::command]
pub fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_history(state: State<'_, AppState>) -> Vec<Item> {
    state.history.lock().unwrap().items()
}

fn entry_path(state: &AppState, id: &str) -> Result<std::path::PathBuf, String> {
    state
        .history
        .lock()
        .unwrap()
        .get(id)
        .map(|e| e.path.clone())
        .ok_or_else(|| "That recording is no longer in History.".to_string())
}

#[tauri::command]
pub fn copy_recording_path(app: AppHandle, id: String) -> Result<(), String> {
    let path = entry_path(&app.state::<AppState>(), &id)?;
    app.clipboard()
        .write_text(naming::clipboard_text(&path))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_recording(app: AppHandle, id: String) -> Result<(), String> {
    let path = entry_path(&app.state::<AppState>(), &id)?;
    if !path.exists() {
        return Err("The file was moved or deleted.".into());
    }
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reveal_recording(app: AppHandle, id: String) -> Result<(), String> {
    let path = entry_path(&app.state::<AppState>(), &id)?;
    if path.exists() {
        app.opener()
            .reveal_item_in_dir(&path)
            .map_err(|e| e.to_string())
    } else if let Some(dir) = path.parent().filter(|d| d.exists()) {
        app.opener()
            .open_path(dir.to_string_lossy(), None::<&str>)
            .map_err(|e| e.to_string())
    } else {
        Err("The folder no longer exists.".into())
    }
}

/// Delete a recording: the file goes to the Trash (recoverable), its
/// thumbnail is removed and it leaves History.
#[tauri::command]
pub fn delete_recording(app: AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let entry = state.history.lock().unwrap().get(&id).cloned();
    let Some(entry) = entry else { return Ok(()) };
    if entry.path.exists() {
        trash::delete(&entry.path).map_err(|e| format!("Cannot delete the file: {e}"))?;
    }
    if let Some(thumb) = &entry.thumbnail {
        let _ = std::fs::remove_file(thumb);
    }
    state
        .history
        .lock()
        .unwrap()
        .remove(&id)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    display_id: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        recorder::start_recording(
            &app,
            display_id,
            rec_capture::Rect::new(x, y, width, height),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn cancel_selection(app: AppHandle) {
    recorder::cancel_selection(&app);
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle) -> Result<Item, String> {
    tauri::async_runtime::spawn_blocking(move || recorder::stop_recording(&app))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn pill_done(app: AppHandle) {
    recorder::pill_done(&app);
}

#[tauri::command]
pub fn recording_started_at(app: AppHandle) -> Option<i64> {
    recorder::recording_started_at(&app)
}

#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    updater::check(&app).await
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    updater::install(&app).await
}
