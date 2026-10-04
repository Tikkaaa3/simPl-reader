//! One bounded, versioned position record per canonical source path.
//! Records live under `simPl/positions` in the profile (`reader_profile`): LOCALAPPDATA
//! on the desktop, else the process temp directory; failures in a configured
//! profile are not hidden.

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PdfMode {
    #[default]
    Document,
    Book,
}
#[derive(Serialize, Deserialize)]
struct PdfModeRecord {
    fingerprint: String,
    mode: PdfMode,
}

pub fn load_pdf_book(path: &Path) -> Result<Option<ReadingPosition>, String> {
    load_record(&record_path(path)?.with_extension("pdf-book.json"))
}
pub fn save_pdf_book(path: &Path, position: &ReadingPosition) -> Result<(), String> {
    save_record(
        &record_path(path)?.with_extension("pdf-book.json"),
        position,
    )
}
pub fn load_pdf_mode(path: &Path, fingerprint: &str) -> Result<PdfMode, String> {
    let record = read_record::<PdfModeRecord>(&record_path(path)?.with_extension("pdf-mode.json"))?;
    if let Some(record) = &record {
        validate_fingerprint(&record.fingerprint)?;
    }
    Ok(record
        .filter(|r| r.fingerprint == fingerprint)
        .map_or(PdfMode::Document, |r| r.mode))
}
pub fn save_pdf_mode(path: &Path, fingerprint: &str, mode: PdfMode) -> Result<(), String> {
    validate_fingerprint(fingerprint)?;
    write_record(
        &record_path(path)?.with_extension("pdf-mode.json"),
        &PdfModeRecord {
            fingerprint: fingerprint.into(),
            mode,
        },
    )
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

/// Read a typed record by the historical canonical path even if the file moved.
pub(crate) fn load_saved_epub(path: &Path) -> Result<Option<EpubReadingPosition>, String> {
    load_epub_record(&saved_record_path(path).with_extension("epub.json"))
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

/// The saved source path is already canonical and may no longer exist.
/// Only a record for the same content can be carried to the new document.
pub(crate) fn transfer_saved_position(
    old_path: &Path,
    new_path: &Path,
    fingerprint: &str,
    kind: crate::recent::DocumentKind,
) -> Result<(), String> {
    let old_record = saved_record_path(old_path);
    let new_record = record_path(new_path)?;
    transfer_records(&old_record, &new_record, fingerprint, kind)
}

fn transfer_records(
    old_record: &Path,
    new_record: &Path,
    fingerprint: &str,
    kind: crate::recent::DocumentKind,
) -> Result<(), String> {
    use crate::recent::DocumentKind;
    match kind {
        DocumentKind::Html => {
            if let Some(position) = load_record(old_record)?
                && position.fingerprint.eq_ignore_ascii_case(fingerprint)
            {
                save_record(new_record, &position)?;
            }
        }
        DocumentKind::Pdf => {
            if let Some(position) = load_record(&old_record.with_extension("pdf-book.json"))?
                && position.fingerprint.eq_ignore_ascii_case(fingerprint)
            {
                save_record(&new_record.with_extension("pdf-book.json"), &position)?;
            }
            if let Some(mode) =
                read_record::<PdfModeRecord>(&old_record.with_extension("pdf-mode.json"))?
                && mode.fingerprint.eq_ignore_ascii_case(fingerprint)
            {
                write_record(&new_record.with_extension("pdf-mode.json"), &mode)?;
            }
            if let Some(position) = load_pdf_record(&old_record.with_extension("pdf.json"))?
                && position.fingerprint.eq_ignore_ascii_case(fingerprint)
            {
                save_pdf_record(&new_record.with_extension("pdf.json"), &position)?;
            }
        }
        DocumentKind::Epub => {
            if let Some(position) = load_epub_record(&old_record.with_extension("epub.json"))?
                && position.fingerprint.eq_ignore_ascii_case(fingerprint)
            {
                save_epub_record(&new_record.with_extension("epub.json"), &position)?;
            }
        }
    }
    Ok(())
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
    let base = storage_base();
    Ok(base
        .join("simPl")
        .join("positions")
        .join(format!("{}.json", path_key(&canonical))))
}

pub(crate) fn storage_base() -> PathBuf {
    reader_profile::storage_base()
}

// Recent history keeps the path captured on the last successful open. A moved
// source cannot be canonicalized again, but its original position key remains.
pub(crate) fn saved_record_path(document: &Path) -> PathBuf {
    storage_base()
        .join("simPl")
        .join("positions")
        .join(format!("{}.json", path_key(document)))
}

pub(crate) fn path_key(path: &Path) -> String {
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
    atomic_write(
        path,
        &bytes,
        "reading position",
        "reading positions",
        ".position",
    )
}

pub(crate) fn atomic_write(
    path: &Path,
    bytes: &[u8],
    label: &str,
    directory_label: &str,
    temporary_prefix: &str,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{label} path has no parent"))?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "cannot create {directory_label} directory {}: {error}",
            parent.display()
        )
    })?;
    let suffix = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        "{temporary_prefix}-{}-{suffix}.tmp",
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| {
            format!(
                "cannot create temporary {label} {}: {error}",
                temporary.display()
            )
        })?;
    let result = (|| {
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("cannot write {label} {}: {error}", temporary.display()))?;
        drop(file);
        replace_file(&temporary, path, label)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
pub(crate) fn replace_file(
    temporary: &Path,
    destination: &Path,
    label: &str,
) -> Result<(), String> {
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
            "cannot replace {label} {}: {}",
            destination.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn replace_file(
    temporary: &Path,
    destination: &Path,
    label: &str,
) -> Result<(), String> {
    fs::rename(temporary, destination)
        .map_err(|error| format!("cannot replace {label} {}: {error}", destination.display()))
}

pub(crate) fn validate_backup_record(path: &Path) -> Result<(), String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid position filename")?;
    if name.ends_with(".pdf.json") {
        load_pdf_record(path)?;
    } else if name.ends_with(".epub.json") {
        load_epub_record(path)?;
    } else if name.ends_with(".pdf-mode.json") {
        if let Some(record) = read_record::<PdfModeRecord>(path)? {
            validate_fingerprint(&record.fingerprint)?;
        }
    } else if name.ends_with(".json") {
        load_record(path)?;
    } else {
        return Err("Unsupported position record".into());
    }
    Ok(())
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
    fn pdf_book_and_original_positions_transfer_independently() {
        let dir = scratch();
        let old = dir.join("old.json");
        let new = dir.join("new.json");
        let book = sample("pdf-b1-p000004-c0000012");
        let pdf = PdfReadingPosition {
            fingerprint: book.fingerprint.clone(),
            page: 2,
            within: 0.3,
            horizontal: 0.2,
            zoom: PdfZoom::Scale(1.5),
        };
        save_record(&old.with_extension("pdf-book.json"), &book).unwrap();
        save_pdf_record(&old.with_extension("pdf.json"), &pdf).unwrap();
        write_record(
            &old.with_extension("pdf-mode.json"),
            &PdfModeRecord {
                fingerprint: book.fingerprint.clone(),
                mode: PdfMode::Book,
            },
        )
        .unwrap();
        transfer_records(
            &old,
            &new,
            &book.fingerprint,
            crate::recent::DocumentKind::Pdf,
        )
        .unwrap();
        assert_eq!(
            load_record(&new.with_extension("pdf-book.json"))
                .unwrap()
                .unwrap()
                .item_id,
            book.item_id
        );
        assert_eq!(
            load_pdf_record(&new.with_extension("pdf.json"))
                .unwrap()
                .unwrap()
                .page,
            2
        );
        assert_eq!(
            read_record::<PdfModeRecord>(&new.with_extension("pdf-mode.json"))
                .unwrap()
                .unwrap()
                .mode,
            PdfMode::Book
        );
        let mut changed = book;
        changed.font_size = 30.0;
        save_record(&new.with_extension("pdf-book.json"), &changed).unwrap();
        assert_eq!(
            load_pdf_record(&new.with_extension("pdf.json"))
                .unwrap()
                .unwrap()
                .zoom,
            PdfZoom::Scale(1.5)
        );
        fs::remove_dir_all(dir).unwrap();
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

    #[test]
    fn transfer_carries_only_matching_typed_positions_and_retains_sources() {
        use crate::recent::DocumentKind;

        let dir = scratch();
        let old = dir.join("old.json");
        let new = dir.join("new.json");
        let fingerprint = "a".repeat(64);
        let other = "b".repeat(64);

        save_record(&old, &sample("original-html")).unwrap();
        save_record(&new, &sample("unrelated-html")).unwrap();
        let old_bytes = fs::read(&old).unwrap();
        transfer_records(&old, &new, &fingerprint, DocumentKind::Html).unwrap();
        assert_eq!(load_record(&new).unwrap().unwrap().item_id, "original-html");
        assert_eq!(fs::read(&old).unwrap(), old_bytes);

        let pdf_old = old.with_extension("pdf.json");
        let pdf_new = new.with_extension("pdf.json");
        save_pdf_record(
            &pdf_old,
            &PdfReadingPosition {
                fingerprint: fingerprint.clone(),
                page: 53,
                within: 0.25,
                horizontal: 0.75,
                zoom: PdfZoom::Scale(1.75),
            },
        )
        .unwrap();
        save_pdf_record(
            &pdf_new,
            &PdfReadingPosition {
                fingerprint: fingerprint.clone(),
                page: 2,
                within: 0.0,
                horizontal: 0.0,
                zoom: PdfZoom::FitWidth,
            },
        )
        .unwrap();
        let old_bytes = fs::read(&pdf_old).unwrap();
        transfer_records(&old, &new, &fingerprint, DocumentKind::Pdf).unwrap();
        let restored = load_pdf_record(&pdf_new).unwrap().unwrap();
        assert_eq!(restored.page, 53);
        assert_eq!(restored.within, 0.25);
        assert_eq!(restored.horizontal, 0.75);
        assert_eq!(restored.zoom, PdfZoom::Scale(1.75));
        assert_eq!(fs::read(&pdf_old).unwrap(), old_bytes);

        let epub_old = old.with_extension("epub.json");
        let epub_new = new.with_extension("epub.json");
        save_epub_record(
            &epub_old,
            &EpubReadingPosition {
                fingerprint: fingerprint.clone(),
                chapter: "OPS/chapter-7.xhtml".into(),
                item_id: "paragraph-8".into(),
                within: 0.625,
                font_size: 22.0,
            },
        )
        .unwrap();
        save_epub_record(
            &epub_new,
            &EpubReadingPosition {
                fingerprint: fingerprint.clone(),
                chapter: "OPS/chapter-1.xhtml".into(),
                item_id: "paragraph-1".into(),
                within: 0.0,
                font_size: 16.0,
            },
        )
        .unwrap();
        let old_bytes = fs::read(&epub_old).unwrap();
        transfer_records(&old, &new, &fingerprint, DocumentKind::Epub).unwrap();
        let restored = load_epub_record(&epub_new).unwrap().unwrap();
        assert_eq!(restored.chapter, "OPS/chapter-7.xhtml");
        assert_eq!(restored.item_id, "paragraph-8");
        assert_eq!(restored.within, 0.625);
        assert_eq!(restored.font_size, 22.0);
        assert_eq!(fs::read(&epub_old).unwrap(), old_bytes);

        for (kind, target) in [
            (DocumentKind::Html, &new),
            (DocumentKind::Pdf, &pdf_new),
            (DocumentKind::Epub, &epub_new),
        ] {
            let before = fs::read(target).unwrap();
            transfer_records(&old, &new, &other, kind).unwrap();
            assert_eq!(fs::read(target).unwrap(), before);
        }
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
