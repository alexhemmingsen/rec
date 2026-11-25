//! User settings, stored as JSON in the app's config folder.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The shortcut Rec starts with: ⌃⇧5 on macOS, Ctrl+Shift+5 elsewhere.
pub const DEFAULT_SHORTCUT: &str = "Control+Shift+5";

/// The keys that stop a recording unless changed: ⌘↩ on macOS, Ctrl+Enter
/// elsewhere.
pub const DEFAULT_STOP_SHORTCUT: &str = if cfg!(target_os = "macos") {
    "Super+Enter"
} else {
    "Control+Enter"
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Global shortcut in Tauri accelerator syntax, e.g. "Control+Shift+5".
    pub shortcut: String,
    /// Stops a recording. `None` means `DEFAULT_STOP_SHORTCUT`.
    pub stop_shortcut: Option<String>,
    /// Where recordings are saved. `None` means the Downloads folder.
    pub save_dir: Option<PathBuf>,
    /// Ask GitHub Releases for a newer version on launch.
    pub check_for_updates: bool,
    /// The first-run onboarding has been finished.
    pub onboarding_complete: bool,
    /// Rec has asked macOS for Screen Recording. macOS shows its dialog only
    /// once, so after that "Grant access" opens System Settings instead.
    pub asked_screen_recording: bool,
    /// The same, for Accessibility.
    pub asked_accessibility: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            shortcut: DEFAULT_SHORTCUT.to_string(),
            stop_shortcut: None,
            save_dir: None,
            check_for_updates: true,
            onboarding_complete: false,
            asked_screen_recording: false,
            asked_accessibility: false,
        }
    }
}

impl Settings {
    /// Load settings, falling back to defaults when the file is missing or
    /// unreadable (a broken file must never stop the app from starting).
    pub fn load(path: &Path) -> Settings {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                log::warn!("ignoring unreadable settings {}: {e}", path.display());
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        write_atomic(path, text.as_bytes())
    }

    /// The keys that stop a recording.
    pub fn effective_stop_shortcut(&self) -> &str {
        self.stop_shortcut
            .as_deref()
            .unwrap_or(DEFAULT_STOP_SHORTCUT)
    }

    /// The folder recordings go to.
    pub fn resolved_save_dir(&self, downloads: &Path) -> PathBuf {
        self.save_dir
            .clone()
            .unwrap_or_else(|| downloads.to_path_buf())
    }
}

/// Write via a temporary file and rename, so a crash never leaves half a file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_spec() {
        let s = Settings::default();
        assert_eq!(s.shortcut, "Control+Shift+5");
        assert_eq!(s.save_dir, None);
        assert!(s.check_for_updates);
        assert!(!s.onboarding_complete);
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/settings.json");
        let s = Settings {
            shortcut: "Alt+Shift+R".into(),
            stop_shortcut: Some("Alt+Shift+S".into()),
            save_dir: Some(PathBuf::from("/tmp/videos")),
            check_for_updates: false,
            onboarding_complete: true,
            asked_screen_recording: true,
            asked_accessibility: false,
        };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn missing_or_broken_files_give_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert_eq!(Settings::load(&path), Settings::default());
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"shortcut":"Alt+Shift+R","futureField":1}"#).unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.shortcut, "Alt+Shift+R");
        assert!(s.check_for_updates);
    }

    #[test]
    fn stop_shortcut_defaults_to_command_enter() {
        let mut s = Settings::default();
        assert_eq!(s.effective_stop_shortcut(), DEFAULT_STOP_SHORTCUT);
        assert!(crate::shortcut::parse(DEFAULT_STOP_SHORTCUT).is_ok());
        s.stop_shortcut = Some("Alt+Shift+S".into());
        assert_eq!(s.effective_stop_shortcut(), "Alt+Shift+S");
    }

    #[test]
    fn save_dir_defaults_to_downloads() {
        let downloads = Path::new("/Users/me/Downloads");
        assert_eq!(Settings::default().resolved_save_dir(downloads), downloads);
        let custom = Settings {
            save_dir: Some("/x".into()),
            ..Settings::default()
        };
        assert_eq!(custom.resolved_save_dir(downloads), Path::new("/x"));
    }
}
