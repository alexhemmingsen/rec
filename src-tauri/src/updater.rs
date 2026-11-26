//! Update check against GitHub Releases. Sends a plain download request: no
//! recordings, no account, no identifier. It can be turned off in Settings.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::recorder::AppState;

/// How often a running Rec checks for a new version (it also checks at launch).
pub const CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
}

pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => {
            let info = UpdateInfo {
                version: update.version.clone(),
                notes: update.body.clone(),
            };
            *app.state::<AppState>().pending_update.lock().unwrap() = Some(update);
            let _ = app.emit("update-available", &info);
            Ok(Some(info))
        }
        Ok(None) => Ok(None),
        Err(e) => {
            log::info!("update check failed: {e}");
            Err(e.to_string())
        }
    }
}

/// Download and install the pending update, then restart Rec.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let update = app
        .state::<AppState>()
        .pending_update
        .lock()
        .unwrap()
        .take();
    let Some(update) = update else {
        return Err("No update is waiting.".into());
    };
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart();
}
