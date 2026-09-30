//! Bounded, versioned local reading history. The UI owns when to persist it.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::position;

pub const MAX_ENTRIES: usize = 12;
const VERSION: u32 = 1;
const MAX_HISTORY_BYTES: u64 = 64 * 1024;
const MAX_PATH_BYTES: usize = 4096;
const MAX_TITLE_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentKind {
    Html,
    Pdf,
    Epub,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub path: PathBuf,
    pub title: String,
    pub fingerprint: String,
    pub kind: DocumentKind,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct History {
    version: u32,
    entries: Vec<Entry>,
}

#[derive(Serialize)]
struct HistoryRef<'a> {
    version: u32,
    entries: &'a [Entry],
}

/// A missing history is normal. Invalid or inaccessible history is an error,
/// rather than an empty list that a subsequent save could overwrite.
pub fn load() -> Result<Vec<Entry>, String> {
    load_at(&history_path())
}

/// Atomically replace the history after all entries have been validated.
pub fn save(entries: &[Entry]) -> Result<(), String> {
    save_at(&history_path(), entries)
}

/// Move an explicitly opened document to the front; edited files replace old metadata.
pub fn remember(entries: &mut Vec<Entry>, entry: Entry) {
    remove(entries, &entry.path);
    entries.insert(0, entry);
    entries.truncate(MAX_ENTRIES);
}

pub fn remove(entries: &mut Vec<Entry>, path: &Path) {
    entries.retain(|entry| !same_path(&entry.path, path));
}

/// Transfer a matching typed reading position from the former canonical path.
/// The old record is retained, including when the file has moved away.
pub fn relocate(old: &Entry, new: &Entry) -> Result<(), String> {
    validate_entry(old)?;
    validate_entry(new)?;
    if old.kind != new.kind {
        return Err("Cannot locate document: the selected file has a different format".into());
    }
    if !old.fingerprint.eq_ignore_ascii_case(&new.fingerprint) {
        return Err(
            "Cannot locate document: SHA-256 fingerprint does not match the missing file".into(),
        );
    }
    if same_path(&old.path, &new.path) {
        return Ok(());
    }
    position::transfer_saved_position(&old.path, &new.path, &old.fingerprint, old.kind)
}

/// Read the old EPUB chapter before committing a move. A stale fingerprint
/// behaves like an absent resume point; the saved record is not modified.
/// The caller must verify the newly selected document before using this hint.
pub fn saved_epub_for_relocation(
    old: &Entry,
) -> Result<Option<position::EpubReadingPosition>, String> {
    validate_entry(old)?;
    if old.kind != DocumentKind::Epub {
        return Err("Cannot locate EPUB position: the saved file is not an EPUB".into());
    }
    Ok(position::load_saved_epub(&old.path)?
        .filter(|saved| saved.fingerprint.eq_ignore_ascii_case(&old.fingerprint)))
}

fn history_path() -> PathBuf {
    position::storage_base().join("simPl").join("recent.json")
}

pub(crate) fn load_at(path: &Path) -> Result<Vec<Entry>, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "cannot open recent files {}: {error}",
                path.display()
            ));
        }
    };
    let mut bytes = Vec::new();
    file.take(MAX_HISTORY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read recent files {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_HISTORY_BYTES {
        return Err(format!(
            "recent files {} exceeds size limit",
            path.display()
        ));
    }
    let history: History = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid recent files {}: {error}", path.display()))?;
    if history.version != VERSION {
        return Err(format!(
            "unsupported recent files version {}",
            history.version
        ));
    }
    validate_entries(&history.entries)?;
    Ok(history.entries)
}

fn save_at(path: &Path, entries: &[Entry]) -> Result<(), String> {
    validate_entries(entries)?;
    let bytes = serde_json::to_vec(&HistoryRef {
        version: VERSION,
        entries,
    })
    .map_err(|error| format!("cannot encode recent files: {error}"))?;
    if bytes.len() as u64 > MAX_HISTORY_BYTES {
        return Err("recent files exceed size limit".into());
    }
    position::atomic_write(path, &bytes, "recent files", "recent files", ".recent")
}

fn validate_entries(entries: &[Entry]) -> Result<(), String> {
    if entries.len() > MAX_ENTRIES {
        return Err(format!("recent files exceed the {MAX_ENTRIES}-entry limit"));
    }
    for (index, entry) in entries.iter().enumerate() {
        validate_entry(entry)?;
        if entries[..index]
            .iter()
            .any(|previous| same_path(&previous.path, &entry.path))
        {
            return Err("recent files contain duplicate paths".into());
        }
    }
    Ok(())
}

fn validate_entry(entry: &Entry) -> Result<(), String> {
    let path = entry
        .path
        .to_str()
        .ok_or("recent file path is not Unicode")?;
    if !entry.path.is_absolute() || path.len() > MAX_PATH_BYTES || path.contains('\0') {
        return Err("recent file has an invalid absolute path".into());
    }
    if entry.title.trim().is_empty() || entry.title.len() > MAX_TITLE_BYTES {
        return Err("recent file has an invalid title".into());
    }
    if entry.fingerprint.len() != 64
        || !entry
            .fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("recent file has an invalid SHA-256 fingerprint".into());
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) fn path_key(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .map(|unit| {
            if unit == u16::from(b'/') {
                u16::from(b'\\')
            } else {
                unit
            }
        })
        .collect()
}

#[cfg(windows)]
pub(crate) fn compare_path_keys(first: &[u16], second: &[u16]) -> std::cmp::Ordering {
    use windows_sys::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
    // Persisted paths are validated at <=4096 UTF-8 bytes; opened Windows paths
    // are bounded by the OS. Both counted UTF-16 buffers are valid for the call.
    let result = unsafe {
        CompareStringOrdinal(
            first.as_ptr(),
            first.len() as i32,
            second.as_ptr(),
            second.len() as i32,
            1,
        )
    };
    result.cmp(&CSTR_EQUAL)
}

#[cfg(windows)]
pub(crate) fn same_path(first: &Path, second: &Path) -> bool {
    first == second || compare_path_keys(&path_key(first), &path_key(second)).is_eq()
}

#[cfg(not(windows))]
pub(crate) fn path_key(path: &Path) -> PathBuf {
    path.to_path_buf()
}

#[cfg(not(windows))]
pub(crate) fn compare_path_keys(first: &Path, second: &Path) -> std::cmp::Ordering {
    first.cmp(second)
}

#[cfg(not(windows))]
pub(crate) fn same_path(first: &Path, second: &Path) -> bool {
    first == second
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "simpl-recent-test-{}-{}",
                std::process::id(),
                NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed)
            )))
        }
        fn history(&self) -> PathBuf {
            self.0.join("recent.json")
        }
        fn entry(&self, name: &str) -> Entry {
            Entry {
                path: self.0.join(name),
                title: name.to_owned(),
                fingerprint: "a".repeat(64),
                kind: DocumentKind::Html,
            }
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn mru_reopens_and_bounds_history() {
        let scratch = Scratch::new();
        let mut entries = Vec::new();
        for i in 0..MAX_ENTRIES + 2 {
            remember(&mut entries, scratch.entry(&format!("{i}.html")));
        }
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(entries[0].title, "13.html");
        assert!(!entries.iter().any(|entry| entry.title == "0.html"));
        let mut edited = scratch.entry("4.html");
        edited.title = "edited".into();
        edited.fingerprint = "b".repeat(64);
        remember(&mut entries, edited);
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(entries[0].title, "edited");
        assert_eq!(entries[0].fingerprint, "b".repeat(64));
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.path == scratch.entry("4.html").path)
                .count(),
            1
        );
        remove(&mut entries, &scratch.entry("4.html").path);
        assert_eq!(entries.len(), MAX_ENTRIES - 1);
        save_at(&scratch.history(), &entries).unwrap();
        let reopened = load_at(&scratch.history()).unwrap();
        assert_eq!(
            reopened
                .iter()
                .map(|entry| entry.title.as_str())
                .collect::<Vec<_>>(),
            entries
                .iter()
                .map(|entry| entry.title.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn invalid_and_oversized_history_never_replaces_existing_file() {
        let scratch = Scratch::new();
        let path = scratch.history();
        assert!(load_at(&path).unwrap().is_empty());
        save_at(&path, &[scratch.entry("book.html")]).unwrap();
        let valid = fs::read(&path).unwrap();
        let mut duplicate = scratch.entry("book.html");
        duplicate.title = "other title".into();
        assert!(save_at(&path, &[scratch.entry("book.html"), duplicate]).is_err());
        assert!(save_at(&path, &vec![scratch.entry("book.html"); MAX_ENTRIES + 1]).is_err());
        assert_eq!(fs::read(&path).unwrap(), valid);

        let mut invalid = scratch.entry("book.html");
        invalid.title.clear();
        assert!(save_at(&path, &[invalid]).is_err());
        assert_eq!(fs::read(&path).unwrap(), valid);

        for corrupt in [
            b"{bad json".to_vec(),
            vec![b'x'; MAX_HISTORY_BYTES as usize + 1],
            br#"{"version":2,"entries":[]}"#.to_vec(),
            br#"{"version":1,"entries":[{"path":"relative.html","title":"bad","fingerprint":"abc","kind":"Html"}]}"#.to_vec(),
            serde_json::to_vec(&HistoryRef { version: VERSION,
                entries: &vec![scratch.entry("duplicate.html"); 2] }).unwrap(),
            serde_json::to_vec(&HistoryRef { version: VERSION,
                entries: &vec![scratch.entry("overflow.html"); MAX_ENTRIES + 1] }).unwrap(),
        ] {
            fs::write(&path, &corrupt).unwrap();
            assert!(load_at(&path).is_err());
            assert_eq!(fs::read(&path).unwrap(), corrupt);
        }
    }

    #[test]
    fn mismatched_relocation_rejects_without_changing_history_or_new_file() {
        let scratch = Scratch::new();
        let old = scratch.entry("missing.html");
        let new_path = scratch.0.join("selected.html");
        fs::create_dir_all(&scratch.0).unwrap();
        fs::write(&new_path, b"a different document").unwrap();
        let mut selected = scratch.entry("selected.html");
        selected.fingerprint = "b".repeat(64);
        let mut history = vec![old.clone()];
        save_at(&scratch.history(), &history).unwrap();
        let saved_history = fs::read(scratch.history()).unwrap();
        let saved_document = fs::read(&new_path).unwrap();

        assert!(
            relocate(&old, &selected)
                .unwrap_err()
                .contains("SHA-256 fingerprint")
        );
        selected.fingerprint = old.fingerprint.clone();
        selected.kind = DocumentKind::Pdf;
        assert!(
            relocate(&old, &selected)
                .unwrap_err()
                .contains("different format")
        );
        assert_eq!(fs::read(&new_path).unwrap(), saved_document);
        assert_eq!(fs::read(scratch.history()).unwrap(), saved_history);
        assert_eq!(history.len(), 1);
        remove(&mut history, &old.path);
        assert!(history.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn windows_path_case_and_separator_deduplication() {
        let scratch = Scratch::new();
        let mut entries = vec![scratch.entry("Novel.HTML")];
        let mut reopened = scratch.entry("novel.html");
        reopened.title = "new name".into();
        remember(&mut entries, reopened);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "new name");
        let slash = PathBuf::from(entries[0].path.to_string_lossy().replace('\\', "/"));
        remove(&mut entries, &slash);
        assert!(entries.is_empty());
        remember(&mut entries, scratch.entry("Σ.html"));
        remember(&mut entries, scratch.entry("σ.html"));
        assert_eq!(entries.len(), 1);
        remember(&mut entries, scratch.entry("straße.html"));
        remember(&mut entries, scratch.entry("STRASSE.html"));
        assert_eq!(entries.len(), 3);
        remember(&mut entries, scratch.entry("K.html"));
        remember(&mut entries, scratch.entry("K.html"));
        assert_eq!(entries.len(), 5);
    }
}
