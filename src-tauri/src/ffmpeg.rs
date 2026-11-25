//! Onboarding's optional ffmpeg check. Rec itself never needs ffmpeg; the
//! agent you paste into does, to read the video. Any other tool works too.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegStatus {
    pub found: bool,
    pub path: Option<String>,
    pub install_command: String,
}

/// The one install command we suggest for each OS.
pub fn install_command(os: &str) -> &'static str {
    match os {
        "macos" => "brew install ffmpeg",
        "windows" => "winget install ffmpeg",
        _ => "sudo apt install ffmpeg",
    }
}

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

/// Folders to look in: PATH first, then where package managers install to.
/// GUI apps on macOS do not inherit the shell's PATH, so Homebrew's folders
/// must be checked explicitly.
pub fn search_dirs(path_env: Option<OsString>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = path_env
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let extra: &[&str] = if cfg!(windows) {
        &[r"C:\ProgramData\chocolatey\bin", r"C:\ffmpeg\bin"]
    } else {
        &[
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/opt/local/bin",
            "/usr/bin",
            "/snap/bin",
            "/nix/var/nix/profiles/default/bin",
        ]
    };
    dirs.extend(extra.iter().map(PathBuf::from));
    if let Some(home) = home {
        if cfg!(windows) {
            dirs.push(home.join("scoop").join("shims"));
            dirs.push(home.join(r"AppData\Local\Microsoft\WinGet\Links"));
        } else {
            dirs.push(home.join(".local/bin"));
            dirs.push(home.join(".nix-profile/bin"));
        }
    }
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    dirs
}

pub fn find_in(dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter()
        .map(|d| d.join(exe_name()))
        .find(|p| p.is_file())
}

/// Ask the user's login shell, which knows PATH additions from their dotfiles.
#[cfg(unix)]
fn from_login_shell() -> Option<PathBuf> {
    use std::process::{Command, Stdio};
    use std::time::Duration;
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
    let mut child = Command::new(shell)
        .args(["-l", "-c", "command -v ffmpeg"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50))
            }
            _ => {
                let _ = child.kill();
                return None;
            }
        }
    }
    let out = child.wait_with_output().ok()?;
    let line = String::from_utf8_lossy(&out.stdout)
        .lines()
        .last()?
        .trim()
        .to_string();
    let p = PathBuf::from(line);
    p.is_file().then_some(p)
}

#[cfg(not(unix))]
fn from_login_shell() -> Option<PathBuf> {
    None
}

pub fn detect(os: &str) -> FfmpegStatus {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    let found =
        find_in(&search_dirs(std::env::var_os("PATH"), home.as_deref())).or_else(from_login_shell);
    FfmpegStatus {
        found: found.is_some(),
        path: found.map(|p| p.to_string_lossy().into_owned()),
        install_command: install_command(os).to_string(),
    }
}

/// The script Terminal runs on macOS. It finds Homebrew even when the login
/// shell doesn't set it up, and sends people to brew.sh when it's missing.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub const MACOS_INSTALL_SCRIPT: &str = r#"#!/bin/zsh -l
clear
for b in /opt/homebrew/bin/brew /usr/local/bin/brew; do
  [ -x "$b" ] && eval "$("$b" shellenv)"
done
if ! command -v brew >/dev/null 2>&1; then
  echo "ffmpeg installs with Homebrew, which isn't on this Mac yet."
  echo "Opening https://brew.sh. Install Homebrew, then click Install in Rec again."
  open "https://brew.sh"
  exit 1
fi
echo "Installing ffmpeg for Rec..."
echo
brew install ffmpeg && echo && echo "Done. You can close this window and go back to Rec."
"#;

/// Open a terminal window that runs the install command, so people see the
/// progress and can answer prompts (a password, Homebrew questions). Rec
/// notices the result by checking again.
#[cfg(target_os = "macos")]
pub fn open_install_terminal() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let script = std::env::temp_dir().join("rec-install-ffmpeg.command");
    std::fs::write(&script, MACOS_INSTALL_SCRIPT).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    let opened = std::process::Command::new("open")
        .arg(&script)
        .status()
        .map_err(|e| e.to_string())?;
    if opened.success() {
        Ok(())
    } else {
        Err("Couldn't open Terminal. Run this in a terminal: brew install ffmpeg".into())
    }
}

#[cfg(windows)]
pub fn open_install_terminal() -> Result<(), String> {
    std::process::Command::new("cmd")
        .args(["/C", "start", "", "cmd", "/K", install_command("windows")])
        .spawn()
        .map(|_| ())
        .map_err(|e| {
            format!("Couldn't open a terminal ({e}). Run this in a terminal: winget install ffmpeg")
        })
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn open_install_terminal() -> Result<(), String> {
    let run = format!(
        "{}; echo; printf 'Press Enter to close. '; read _",
        install_command("linux")
    );
    let terminals: [(&str, &[&str]); 4] = [
        ("x-terminal-emulator", &["-e"]),
        ("gnome-terminal", &["--"]),
        ("konsole", &["-e"]),
        ("xterm", &["-e"]),
    ];
    for (term, flag) in terminals {
        let spawned = std::process::Command::new(term)
            .args(flag)
            .args(["sh", "-c", run.as_str()])
            .spawn();
        if spawned.is_ok() {
            return Ok(());
        }
    }
    Err("Couldn't open a terminal. Run this in a terminal: sudo apt install ffmpeg".into())
}

#[cfg(not(any(unix, windows)))]
pub fn open_install_terminal() -> Result<(), String> {
    Err("Installing ffmpeg from Rec isn't supported here.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_script_installs_with_homebrew() {
        assert!(MACOS_INSTALL_SCRIPT.starts_with("#!/bin/zsh -l\n"));
        assert!(MACOS_INSTALL_SCRIPT.contains(install_command("macos")));
    }

    #[test]
    fn install_commands_match_the_spec() {
        assert_eq!(install_command("macos"), "brew install ffmpeg");
        assert_eq!(install_command("windows"), "winget install ffmpeg");
        assert_eq!(install_command("linux"), "sudo apt install ffmpeg");
    }

    #[test]
    fn finds_ffmpeg_in_a_listed_folder() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        assert_eq!(find_in(std::slice::from_ref(&bin)), None);
        std::fs::write(bin.join(exe_name()), b"").unwrap();
        assert_eq!(
            find_in(&[dir.path().to_path_buf(), bin.clone()]),
            Some(bin.join(exe_name()))
        );
    }

    #[test]
    fn search_dirs_include_path_and_package_manager_folders_once() {
        let path = std::env::join_paths(["/a", "/b", "/a"]).unwrap();
        let dirs = search_dirs(Some(path), Some(Path::new("/home/me")));
        assert_eq!(&dirs[..2], &[PathBuf::from("/a"), PathBuf::from("/b")]);
        assert_eq!(
            dirs.iter()
                .filter(|d| d.as_path() == Path::new("/a"))
                .count(),
            1
        );
        if cfg!(unix) {
            assert!(dirs.contains(&PathBuf::from("/opt/homebrew/bin")));
            assert!(dirs.contains(&PathBuf::from("/home/me/.local/bin")));
        }
    }
}
