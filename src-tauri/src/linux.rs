//! Linux specifics.
//!
//! Rec's windows run through XWayland on Wayland sessions: Wayland doesn't
//! let apps place windows at exact positions or keep them on top, which the
//! selection overlay and the Stop pill need. Recording itself uses the
//! ScreenCast portal (see rec-capture).
//!
//! Global shortcuts: X11 key grabs only see keystrokes while an X11 window
//! has focus, so on Wayland Rec also binds its shortcut through the
//! GlobalShortcuts portal (GNOME 48+, KDE Plasma 5.27+). How well that works
//! depends on the desktop.

/// Call before Tauri starts GTK.
pub fn prefer_xwayland() {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    if wayland && std::env::var_os("GDK_BACKEND").is_none() {
        // SAFETY: called at the very start of main, before any threads exist.
        unsafe { std::env::set_var("GDK_BACKEND", "x11") };
    }
}

/// Tauri accelerator ("Control+Shift+5") to the XDG shortcuts format the
/// portal expects ("CTRL+SHIFT+5").
pub fn portal_trigger(accelerator: &str) -> String {
    accelerator
        .split('+')
        .filter(|t| !t.is_empty())
        .map(|t| match t.to_ascii_lowercase().as_str() {
            "control" | "ctrl" | "cmdorctrl" | "commandorcontrol" => "CTRL".to_string(),
            "alt" | "option" => "ALT".to_string(),
            "shift" => "SHIFT".to_string(),
            "super" | "meta" | "cmd" | "command" => "LOGO".to_string(),
            _ => {
                let key = t
                    .strip_prefix("Digit")
                    .or_else(|| t.strip_prefix("Key"))
                    .unwrap_or(t);
                if key.len() == 1 {
                    key.to_ascii_lowercase()
                } else {
                    key.to_string()
                }
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Bind the shortcut through the GlobalShortcuts portal on Wayland and call
/// `on_press` whenever it fires. Runs on its own thread; failures (no portal,
/// user declined) are logged and the X11 binding keeps working where it can.
pub fn bind_portal_shortcut(accelerator: &str, on_press: impl Fn() + Send + 'static) {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return;
    }
    let trigger = portal_trigger(accelerator);
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(r) => r,
            Err(e) => return log::warn!("global shortcut portal: {e}"),
        };
        runtime.block_on(async move {
            use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
            use futures_util::StreamExt;

            let result: Result<(), ashpd::Error> = async {
                let portal = GlobalShortcuts::new().await?;
                let session = portal.create_session(Default::default()).await?;
                let shortcut = NewShortcut::new("record", "Start a Rec recording")
                    .preferred_trigger(trigger.as_str());
                portal
                    .bind_shortcuts(&session, &[shortcut], None, Default::default())
                    .await?
                    .response()?;
                log::info!("bound {trigger} through the GlobalShortcuts portal");
                let mut activated = portal.receive_activated().await?;
                while let Some(event) = activated.next().await {
                    if event.shortcut_id() == "record" {
                        on_press();
                    }
                }
                Ok(())
            }
            .await;
            if let Err(e) = result {
                log::warn!("global shortcut portal unavailable: {e}");
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_accelerators_to_portal_triggers() {
        assert_eq!(portal_trigger("Control+Shift+5"), "CTRL+SHIFT+5");
        assert_eq!(portal_trigger("Alt+Super+R"), "ALT+LOGO+r");
        assert_eq!(portal_trigger("Control+Shift+Digit5"), "CTRL+SHIFT+5");
        assert_eq!(portal_trigger("Control+F5"), "CTRL+F5");
    }
}
