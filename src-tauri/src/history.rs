//! The list of recordings Rec has made, stored as JSON in the app data folder.
//! Nothing is ever deleted automatically: entries go only when the user
//! deletes them.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::settings::write_atomic;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub path: PathBuf,
    pub created_at: DateTime<Utc>,
    pub duration_secs: f64,
    pub width: u32,
    pub height: u32,
    pub thumbnail: Option<PathBuf>,
}

/// An entry as the History screen shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    #[serde(flatten)]
    pub entry: Entry,
    /// The file was moved or deleted outside Rec.
    pub missing: bool,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct FileFormat {
    version: u32,
    entries: Vec<Entry>,
}

pub struct History {
    path: PathBuf,
    entries: Vec<Entry>,
}

impl History {
    pub fn load(path: &Path) -> History {
        let entries = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| match serde_json::from_str::<FileFormat>(&text) {
                Ok(f) => Some(f.entries),
                Err(e) => {
                    log::warn!("ignoring unreadable history {}: {e}", path.display());
                    None
                }
            })
            .unwrap_or_default();
        History {
            path: path.to_path_buf(),
            entries,
        }
    }

    fn save(&self) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file = FileFormat {
            version: 1,
            entries: self.entries.clone(),
        };
        let text = serde_json::to_string_pretty(&file).map_err(std::io::Error::other)?;
        write_atomic(&self.path, text.as_bytes())
    }

    pub fn add(&mut self, entry: Entry) -> std::io::Result<()> {
        self.entries.retain(|e| e.id != entry.id);
        self.entries.push(entry);
        self.save()
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Remove an entry from the list and return it.
    pub fn remove(&mut self, id: &str) -> std::io::Result<Option<Entry>> {
        let Some(pos) = self.entries.iter().position(|e| e.id == id) else {
            return Ok(None);
        };
        let entry = self.entries.remove(pos);
        self.save()?;
        Ok(Some(entry))
    }

    /// All entries, newest first, with their on-disk state.
    pub fn items(&self) -> Vec<Item> {
        let mut items: Vec<Item> = self
            .entries
            .iter()
            .map(|e| {
                let meta = std::fs::metadata(&e.path).ok().filter(|m| m.is_file());
                Item {
                    entry: e.clone(),
                    missing: meta.is_none(),
                    size_bytes: meta.map(|m| m.len()),
                }
            })
            .collect();
        items.sort_by_key(|i| std::cmp::Reverse(i.entry.created_at));
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn entry(id: &str, path: &Path, minute: u32) -> Entry {
        Entry {
            id: id.into(),
            path: path.to_path_buf(),
            created_at: Utc.with_ymd_and_hms(2026, 11, 25, 14, minute, 0).unwrap(),
            duration_secs: 4.2,
            width: 800,
            height: 600,
            thumbnail: None,
        }
    }

    #[test]
    fn lists_newest_first_and_flags_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.mp4");
        let b = dir.path().join("b.mp4");
        std::fs::write(&a, b"12345").unwrap();
        let mut h = History::load(&dir.path().join("history.json"));
        h.add(entry("a", &a, 1)).unwrap();
        h.add(entry("b", &b, 2)).unwrap();

        let items = h.items();
        assert_eq!(
            items
                .iter()
                .map(|i| i.entry.id.as_str())
                .collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert!(items[0].missing, "b.mp4 was never written");
        assert!(!items[1].missing);
        assert_eq!(items[1].size_bytes, Some(5));
    }

    #[test]
    fn persists_between_loads() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("data/history.json");
        let mut h = History::load(&file);
        h.add(entry("a", Path::new("/x/a.mp4"), 1)).unwrap();
        let again = History::load(&file);
        assert_eq!(again.get("a").unwrap().path, Path::new("/x/a.mp4"));
    }

    #[test]
    fn remove_deletes_only_the_entry() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("history.json");
        let mut h = History::load(&file);
        h.add(entry("a", Path::new("/x/a.mp4"), 1)).unwrap();
        h.add(entry("b", Path::new("/x/b.mp4"), 2)).unwrap();
        let removed = h.remove("a").unwrap().unwrap();
        assert_eq!(removed.id, "a");
        assert!(h.remove("a").unwrap().is_none());
        assert_eq!(History::load(&file).items().len(), 1);
    }

    #[test]
    fn broken_file_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("history.json");
        std::fs::write(&file, "garbage").unwrap();
        assert!(History::load(&file).items().is_empty());
    }

    #[test]
    fn serializes_for_the_frontend_in_camel_case() {
        let item = Item {
            entry: entry("a", Path::new("/x/a.mp4"), 1),
            missing: true,
            size_bytes: None,
        };
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["id"], "a");
        assert_eq!(json["durationSecs"], 4.2);
        assert_eq!(json["missing"], true);
        assert!(json.get("createdAt").is_some());
    }
}
