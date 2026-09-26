//! One bounded, versioned position record per canonical source path.
//! When LOCALAPPDATA is unavailable, records live in the process temp directory
//! under `simPl/positions`; failures in a configured LOCALAPPDATA are not hidden.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const VERSION: u32 = 1;
const MAX_RECORD_BYTES: u64 = 16 * 1024;
const MAX_ITEM_ID_BYTES: usize = 1024;
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReadingPosition {
    pub fingerprint: String,
    pub item_id: String,
    pub within: f32,
    pub font_size: f32,
}

#[derive(Serialize, Deserialize)]
struct Record {
    version: u32,
    position: ReadingPosition,
}

#[derive(Serialize)]
struct RecordRef<'a> {
    version: u32,
    position: &'a ReadingPosition,
}

/// A missing record is normal. Corrupt, oversized or inaccessible records are errors.
pub fn load(path: &Path) -> Result<Option<ReadingPosition>, String> {
    let record = record_path(path)?;
    load_record(&record)
}

/// Atomically replace this document's position without touching other books.
pub fn save(path: &Path, position: &ReadingPosition) -> Result<(), String> {
    let record = record_path(path)?;
    save_record(&record, position)
}

fn validate(position: &ReadingPosition) -> Result<(), String> {
    if position.fingerprint.len() != 64
        || !position
            .fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("reading position has an invalid SHA-256 fingerprint".into());
    }
    if position.item_id.is_empty() || position.item_id.len() > MAX_ITEM_ID_BYTES {
        return Err("reading position has an invalid item ID".into());
    }
    if !position.within.is_finite() || !(0.0..=1.0).contains(&position.within) {
        return Err("reading position has an invalid intra-item fraction".into());
    }
    if !position.font_size.is_finite()
        || !(0.0..=256.0).contains(&position.font_size)
        || position.font_size == 0.0
    {
        return Err("reading position has an invalid font size".into());
    }
    Ok(())
}

fn record_path(document: &Path) -> Result<PathBuf, String> {
    let canonical = document
        .canonicalize()
        .map_err(|error| format!("cannot resolve document {}: {error}", document.display()))?;
    let base = std::env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    Ok(base
        .join("simPl")
        .join("positions")
        .join(format!("{}.json", path_key(&canonical))))
}

fn path_key(path: &Path) -> String {
    let mut digest = Sha256::new();
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        for unit in path.as_os_str().encode_wide() {
            digest.update(unit.to_le_bytes());
        }
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::ffi::OsStrExt;
        digest.update(path.as_os_str().as_bytes());
    }
    format!("{:x}", digest.finalize())
}

fn load_record(path: &Path) -> Result<Option<ReadingPosition>, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "cannot open reading position {}: {error}",
                path.display()
            ));
        }
    };
    let mut bytes = Vec::new();
    file.take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read reading position {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err(format!(
            "reading position {} exceeds size limit",
            path.display()
        ));
    }
    let record: Record = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid reading position {}: {error}", path.display()))?;
    if record.version != VERSION {
        return Err(format!(
            "unsupported reading position version {}",
            record.version
        ));
    }
    validate(&record.position)?;
    Ok(Some(record.position))
}

fn save_record(path: &Path, position: &ReadingPosition) -> Result<(), String> {
    validate(position)?;
    let bytes = serde_json::to_vec(&RecordRef {
        version: VERSION,
        position,
    })
    .map_err(|error| format!("cannot encode reading position: {error}"))?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err("reading position exceeds size limit".into());
    }
    let parent = path.parent().ok_or("reading position path has no parent")?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "cannot create reading positions directory {}: {error}",
            parent.display()
        )
    })?;
    let suffix = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(".position-{}-{suffix}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| {
            format!(
                "cannot create temporary reading position {}: {error}",
                temporary.display()
            )
        })?;
    let result = (|| {
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| {
                format!(
                    "cannot write reading position {}: {error}",
                    temporary.display()
                )
            })?;
        drop(file);
        replace_file(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
fn replace_file(temporary: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let from: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // Both paths are in the same directory; readers see either entire record.
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(format!(
            "cannot replace reading position {}: {}",
            destination.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(temporary: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(temporary, destination).map_err(|error| {
        format!(
            "cannot replace reading position {}: {error}",
            destination.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(item: &str) -> ReadingPosition {
        ReadingPosition {
            fingerprint: "a".repeat(64),
            item_id: item.into(),
            within: 0.75,
            font_size: 19.0,
        }
    }

    fn scratch() -> PathBuf {
        std::env::temp_dir().join(format!(
            "simpl-position-test-{}-{}",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn missing_then_replacement_survives_reopen_and_preserves_other_book() {
        let dir = scratch();
        let first = dir.join("first.json");
        let second = dir.join("second.json");
        assert!(load_record(&first).unwrap().is_none());
        save_record(&first, &sample("chapter-a")).unwrap();
        save_record(&second, &sample("other-book")).unwrap();
        save_record(&first, &sample("chapter-b")).unwrap();
        let restored = load_record(&first).unwrap().unwrap();
        assert_eq!(restored.item_id, "chapter-b");
        assert_eq!(restored.fingerprint, "a".repeat(64));
        assert_eq!(restored.within, 0.75);
        assert_eq!(restored.font_size, 19.0);
        assert_eq!(load_record(&second).unwrap().unwrap().item_id, "other-book");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_records_and_positions_are_errors() {
        let dir = scratch();
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.json");
        fs::write(
            &path,
            serde_json::to_vec(&Record {
                version: 2,
                position: sample("chapter"),
            })
            .unwrap(),
        )
        .unwrap();
        assert!(load_record(&path).unwrap_err().contains("unsupported"));
        fs::write(&path, b"{broken json").unwrap();
        assert!(load_record(&path).is_err());
        fs::write(&path, vec![b'x'; MAX_RECORD_BYTES as usize + 1]).unwrap();
        assert!(load_record(&path).is_err());
        let mut invalid = sample("chapter");
        invalid.within = f32::NAN;
        assert!(save_record(&path, &invalid).is_err());
        invalid.within = -0.1;
        assert!(save_record(&path, &invalid).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn path_key_preserves_windows_wide_units() {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        let one = OsString::from_wide(&[b'C' as u16, b':' as u16, 0xd800]);
        let two = OsString::from_wide(&[b'C' as u16, b':' as u16, 0xd801]);
        assert_eq!(one.to_string_lossy(), two.to_string_lossy());
        assert_ne!(path_key(Path::new(&one)), path_key(Path::new(&two)));
    }
}
