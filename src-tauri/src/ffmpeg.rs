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

#[cfg(test)]
mod tests {
    use super::*;

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
