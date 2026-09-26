//! One bounded, versioned position record per canonical source path.
//! When LOCALAPPDATA is unavailable, records live in the process temp directory
//! under `simPl/positions`; failures in a configured LOCALAPPDATA are not hidden.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
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

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum PdfZoom {
    FitWidth,
    Scale(f32),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PdfReadingPosition {
    pub fingerprint: String,
    pub page: u32,
    pub within: f32,
    pub horizontal: f32,
    pub zoom: PdfZoom,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EpubReadingPosition {
    pub fingerprint: String,
    /// Canonical package-relative spine href, not a temporary filesystem path.
    pub chapter: String,
    pub item_id: String,
    pub within: f32,
    pub font_size: f32,
}

#[derive(Serialize, Deserialize)]
struct Record<T = ReadingPosition> {
    version: u32,
    position: T,
}

#[derive(Serialize)]
struct RecordRef<'a, T> {
    version: u32,
    position: &'a T,
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

/// PDF records use a separate suffix; existing HTML records remain unchanged.
pub fn load_pdf(path: &Path) -> Result<Option<PdfReadingPosition>, String> {
    load_pdf_record(&record_path(path)?.with_extension("pdf.json"))
}

pub fn save_pdf(path: &Path, position: &PdfReadingPosition) -> Result<(), String> {
    save_pdf_record(&record_path(path)?.with_extension("pdf.json"), position)
}

fn load_pdf_record(path: &Path) -> Result<Option<PdfReadingPosition>, String> {
    let position = read_record::<PdfReadingPosition>(path)?;
    if let Some(position) = &position {
        validate_pdf(position)?;
    }
    Ok(position)
}

fn save_pdf_record(path: &Path, position: &PdfReadingPosition) -> Result<(), String> {
    validate_pdf(position)?;
    write_record(path, position)
}

pub fn load_epub(path: &Path) -> Result<Option<EpubReadingPosition>, String> {
    load_epub_record(&record_path(path)?.with_extension("epub.json"))
}

pub fn save_epub(path: &Path, position: &EpubReadingPosition) -> Result<(), String> {
    save_epub_record(&record_path(path)?.with_extension("epub.json"), position)
}

fn load_epub_record(path: &Path) -> Result<Option<EpubReadingPosition>, String> {
    let position = read_record::<EpubReadingPosition>(path)?;
    if let Some(position) = &position {
        validate_epub(position)?;
    }
    Ok(position)
}

fn save_epub_record(path: &Path, position: &EpubReadingPosition) -> Result<(), String> {
    validate_epub(position)?;
    write_record(path, position)
}

fn validate_epub(position: &EpubReadingPosition) -> Result<(), String> {
    if position.chapter.is_empty()
        || position.chapter.len() > 4096
        || position.chapter.contains(['\\', ':', '\0'])
        || position
            .chapter
            .split('/')
            .any(|part| matches!(part, "" | "." | ".."))
    {
        return Err("EPUB reading position has an invalid chapter href".into());
    }
    validate_reflow(
        &position.fingerprint,
        &position.item_id,
        position.within,
        position.font_size,
    )
}

fn validate_pdf(position: &PdfReadingPosition) -> Result<(), String> {
    validate_fingerprint(&position.fingerprint)?;
    if position.page >= 100_000 {
        return Err("PDF reading position has an invalid page index".into());
    }
    if [position.within, position.horizontal]
        .into_iter()
        .any(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
    {
        return Err("PDF reading position has an invalid page offset".into());
    }
    if let PdfZoom::Scale(scale) = position.zoom
        && (!scale.is_finite() || !(0.25..=4.0).contains(&scale))
    {
        return Err("PDF reading position has an invalid zoom".into());
    }
    Ok(())
}

fn validate_fingerprint(fingerprint: &str) -> Result<(), String> {
    if fingerprint.len() != 64 || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("reading position has an invalid SHA-256 fingerprint".into());
    }
    Ok(())
}

fn validate(position: &ReadingPosition) -> Result<(), String> {
    validate_reflow(
        &position.fingerprint,
        &position.item_id,
        position.within,
        position.font_size,
    )
}

fn validate_reflow(
    fingerprint: &str,
    item_id: &str,
    within: f32,
    font_size: f32,
) -> Result<(), String> {
    validate_fingerprint(fingerprint)?;
    if item_id.is_empty() || item_id.len() > MAX_ITEM_ID_BYTES {
        return Err("reading position has an invalid item ID".into());
    }
    if !within.is_finite() || !(0.0..=1.0).contains(&within) {
        return Err("reading position has an invalid intra-item fraction".into());
    }
    if !font_size.is_finite() || !(0.0..=256.0).contains(&font_size) || font_size == 0.0 {
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
    let position = read_record::<ReadingPosition>(path)?;
    if let Some(position) = &position {
        validate(position)?;
    }
    Ok(position)
}

fn read_record<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
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
    let record: Record<T> = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid reading position {}: {error}", path.display()))?;
    if record.version != VERSION {
        return Err(format!(
            "unsupported reading position version {}",
            record.version
        ));
    }
    Ok(Some(record.position))
}

fn save_record(path: &Path, position: &ReadingPosition) -> Result<(), String> {
    validate(position)?;
    write_record(path, position)
}

fn write_record<T: Serialize>(path: &Path, position: &T) -> Result<(), String> {
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

    #[test]
    fn pdf_zoom_and_page_offsets_survive_replacement_without_touching_html() {
        let dir = scratch();
        let html = dir.join("book.json");
        let pdf = dir.join("book.pdf.json");
        save_record(&html, &sample("chapter")).unwrap();
        let mut position = PdfReadingPosition {
            fingerprint: "b".repeat(64),
            page: 37,
            within: 0.375,
            horizontal: 0.2,
            zoom: PdfZoom::Scale(1.5),
        };
        save_pdf_record(&pdf, &position).unwrap();
        let restored = load_pdf_record(&pdf).unwrap().unwrap();
        assert_eq!(restored.page, 37);
        assert_eq!(restored.within, 0.375);
        assert_eq!(restored.horizontal, 0.2);
        assert_eq!(restored.zoom, PdfZoom::Scale(1.5));
        position.zoom = PdfZoom::FitWidth;
        save_pdf_record(&pdf, &position).unwrap();
        position.horizontal = f32::NAN;
        assert!(save_pdf_record(&pdf, &position).is_err());
        assert_eq!(
            load_pdf_record(&pdf).unwrap().unwrap().zoom,
            PdfZoom::FitWidth
        );
        assert_eq!(load_record(&html).unwrap().unwrap().item_id, "chapter");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn epub_chapter_resume_rejects_invalid_replacement_without_clobbering_records() {
        let dir = scratch();
        let epub = dir.join("book.epub.json");
        let html = dir.join("book.json");
        save_record(&html, &sample("html-item")).unwrap();
        let mut position = EpubReadingPosition {
            fingerprint: "c".repeat(64),
            chapter: "OEBPS/Text/日本語.xhtml".into(),
            item_id: "item-000017".into(),
            within: 0.375,
            font_size: 24.0,
        };
        save_epub_record(&epub, &position).unwrap();
        let restored = load_epub_record(&epub).unwrap().unwrap();
        assert_eq!(restored.chapter, position.chapter);
        assert_eq!(restored.item_id, "item-000017");
        assert_eq!(restored.within, 0.375);
        assert_eq!(restored.font_size, 24.0);
        for chapter in ["../outside.xhtml", "/absolute.xhtml", "C:/book.xhtml", ""] {
            position.chapter = chapter.into();
            assert!(save_epub_record(&epub, &position).is_err());
        }
        position.chapter = "OEBPS/Text/next.xhtml".into();
        position.within = f32::NAN;
        assert!(save_epub_record(&epub, &position).is_err());
        assert_eq!(
            load_epub_record(&epub).unwrap().unwrap().chapter,
            restored.chapter
        );
        assert_eq!(load_record(&html).unwrap().unwrap().item_id, "html-item");
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
