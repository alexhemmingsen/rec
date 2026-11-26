//! Rec: press a shortcut, drag a box, the recording starts; click Stop and the
//! video's path is on your clipboard, ready to paste into an AI agent.

mod commands;
mod ffmpeg;
mod history;
mod layout;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod mouse_tap;
mod naming;
mod recorder;
mod settings;
mod shortcut;
mod sound;
#[cfg(target_os = "macos")]
mod tcc;
mod updater;
mod windowing;

use std::sync::Mutex;

#[cfg(target_os = "macos")]
use tauri::RunEvent;
use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::history::History;
use crate::recorder::{AppState, Phase, Restore};
use crate::settings::{DEFAULT_SHORTCUT, Settings};

#[cfg(target_os = "linux")]
pub use linux::prefer_xwayland;

/// Passed by "Open at login": start in the background without the window.
pub const HIDDEN_ARG: &str = "--hidden";

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windowing::show_main(app)
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                // REC_DEBUG=1 adds per-event tracing (used by the e2e tests).
                .level_for(
                    "rec_lib::macos",
                    if std::env::var_os("REC_DEBUG").is_some() {
                        log::LevelFilter::Debug
                    } else {
                        log::LevelFilter::Info
                    },
                )
                .build(),
        )
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    // Off the hotkey thread: the flow (un)registers Esc, which
                    // on Linux waits for that very thread.
                    let app = app.clone();
                    let escape = recorder::is_escape(shortcut);
                    let stop = recorder::is_custom_stop_shortcut(&app, shortcut);
                    let pen = recorder::is_pen_shortcut(&app, shortcut);
                    std::thread::spawn(move || {
                        if escape {
                            recorder::on_escape(&app);
                        } else if stop {
                            recorder::on_stop_shortcut(&app);
                        } else if pen {
                            recorder::on_pen_shortcut(&app);
                        } else {
                            recorder::on_shortcut(&app);
                        }
                    });
                })
                .build(),
        )
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN_ARG]),
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let data_dir = app.path().app_data_dir()?;
            let mut settings = Settings::load(&settings_path);

            if let Err(e) = app.global_shortcut().register(settings.shortcut.as_str()) {
                log::error!("cannot register shortcut {}: {e}", settings.shortcut);
                if settings.shortcut != DEFAULT_SHORTCUT
                    && app.global_shortcut().register(DEFAULT_SHORTCUT).is_ok()
                {
                    settings.shortcut = DEFAULT_SHORTCUT.to_string();
                    let _ = settings.save(&settings_path);
                }
            }

            #[cfg(target_os = "linux")]
            {
                let h = handle.clone();
                linux::bind_portal_shortcut(&settings.shortcut, move || recorder::on_shortcut(&h));
            }

            let show_window =
                !std::env::args().any(|a| a == HIDDEN_ARG) || !settings.onboarding_complete;

            app.manage(AppState {
                settings: Mutex::new(settings),
                settings_path,
                history: Mutex::new(History::load(&data_dir.join("history.json"))),
                thumbnails_dir: data_dir.join("thumbnails"),
                phase: Mutex::new(Phase::Idle),
                restore: Mutex::new(Restore::default()),
                pending_update: Mutex::new(None),
                native_drag: Mutex::new(None),
            });

            #[cfg(target_os = "macos")]
            {
                let h = handle.clone();
                macos::watch_mouse(move |kind, x, y| recorder::on_native_mouse(&h, kind, x, y));
                let h = handle.clone();
                mouse_tap::install(move |kind, x, y| recorder::on_native_mouse(&h, kind, x, y));
            }

            // Create the overlays and the pill now so the shortcut is instant.
            if let Err(e) = windowing::prepare_overlays(&handle, &rec_capture::displays()) {
                log::error!("cannot prepare overlays: {e}");
            }
            if let Err(e) = windowing::ensure_pill(&handle) {
                log::error!("cannot prepare the Stop pill: {e}");
            }
            if let Err(e) = windowing::ensure_canvas(&handle) {
                log::error!("cannot prepare the drawing window: {e}");
            }

            if show_window {
                windowing::show_main(&handle);
            }
            // At launch, then every few hours: Rec usually stays running for
            // days (it opens at login, and closing the window doesn't quit
            // it). Each time it follows the current "check for updates".
            let h = handle.clone();
            std::thread::spawn(move || {
                loop {
                    let enabled = h
                        .state::<AppState>()
                        .settings
                        .lock()
                        .unwrap()
                        .check_for_updates;
                    if enabled {
                        let _ = tauri::async_runtime::block_on(updater::check(&h));
                    }
                    std::thread::sleep(updater::CHECK_EVERY);
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps Rec running so the shortcut still works.
            if window.label() == windowing::MAIN
                && let WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                if cfg!(target_os = "macos") {
                    let _ = window.hide();
                } else {
                    let _ = window.minimize();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::set_shortcut,
            commands::set_shortcut_suspended,
            commands::set_stop_shortcut,
            commands::set_pen,
            commands::choose_save_dir,
            commands::reset_save_dir,
            commands::set_open_at_login,
            commands::set_check_for_updates,
            commands::complete_onboarding,
            commands::permission_status,
            commands::request_screen_permission,
            commands::warm_up_capture,
            commands::open_permission_settings,
            commands::accessibility_status,
            commands::request_accessibility,
            commands::open_accessibility_settings,
            commands::relaunch,
            commands::check_ffmpeg,
            commands::install_ffmpeg,
            commands::copy_text,
            commands::list_history,
            commands::copy_recording_path,
            commands::open_recording,
            commands::reveal_recording,
            commands::delete_recording,
            commands::ui_log,
            commands::start_recording,
            commands::cancel_selection,
            commands::stop_recording,
            commands::pill_done,
            commands::recording_started_at,
            commands::check_for_update,
            commands::install_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Rec")
        .run(|app, event| {
            // Clicking the Dock icon brings the window back.
            #[cfg(target_os = "macos")]
            if let RunEvent::Reopen { .. } = event {
                windowing::show_main(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}
