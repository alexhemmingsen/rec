//! File names and the text that goes on the clipboard.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

/// `rec-2026-11-25-14-03-12.mp4`: sorts by date and has no spaces, so the
/// pasted path never needs quoting.
pub fn file_name(at: DateTime<Local>) -> String {
    format!("rec-{}.mp4", at.format("%Y-%m-%d-%H-%M-%S"))
}

/// A path in `dir` for a recording started at `at` that does not exist yet.
/// Two recordings in the same second get `-2`, `-3`, ...
pub fn unique_path(dir: &Path, at: DateTime<Local>) -> PathBuf {
    let first = dir.join(file_name(at));
    if !first.exists() {
        return first;
    }
    let stem = format!("rec-{}", at.format("%Y-%m-%d-%H-%M-%S"));
    (2..)
        .map(|n| dir.join(format!("{stem}-{n}.mp4")))
        .find(|p| !p.exists())
        .expect("an unused file name")
}

/// The text Rec puts on the clipboard: the absolute path, plain. If a
/// user-chosen folder contains whitespace, the path is wrapped in double
/// quotes so terminals and agents read it as one argument.
pub fn clipboard_text(path: &Path) -> String {
    let s = path.to_string_lossy();
    if s.chars().any(char::is_whitespace) {
        format!("\"{s}\"")
    } else {
        s.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 11, 25, 14, 3, 12).unwrap()
    }

    #[test]
    fn file_name_matches_the_spec() {
        assert_eq!(file_name(at()), "rec-2026-11-25-14-03-12.mp4");
    }

    #[test]
    fn unique_path_avoids_collisions() {
        let dir = tempfile::tempdir().unwrap();
        let first = unique_path(dir.path(), at());
        assert_eq!(first.file_name().unwrap(), "rec-2026-11-25-14-03-12.mp4");
        std::fs::write(&first, b"x").unwrap();
        let second = unique_path(dir.path(), at());
        assert_eq!(second.file_name().unwrap(), "rec-2026-11-25-14-03-12-2.mp4");
        std::fs::write(&second, b"x").unwrap();
        let third = unique_path(dir.path(), at());
        assert_eq!(third.file_name().unwrap(), "rec-2026-11-25-14-03-12-3.mp4");
    }

    #[test]
    fn clipboard_text_is_the_bare_path() {
        let p = Path::new("/Users/me/Downloads/rec-2026-11-25-14-03-12.mp4");
        assert_eq!(
            clipboard_text(p),
            "/Users/me/Downloads/rec-2026-11-25-14-03-12.mp4"
        );
    }

    #[test]
    fn clipboard_text_quotes_paths_with_spaces() {
        let p = Path::new("/Users/me/My Videos/rec-2026-11-25-14-03-12.mp4");
        assert_eq!(
            clipboard_text(p),
            "\"/Users/me/My Videos/rec-2026-11-25-14-03-12.mp4\""
        );
    }
}
