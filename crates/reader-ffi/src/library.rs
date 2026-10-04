//! Serialized catalog mutations using the desktop storage schemas.
use std::{path::Path, sync::Mutex, time::SystemTime};

use reader_document::{ImageAsset, Item, library, managed, recent, shelves};

use crate::{CoreError, DocumentFormat, complete, inspect_document};

static CATALOG: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, uniffi::Record)]
pub struct LibraryBook {
    pub path: String,
    pub fingerprint: String,
    pub title: String,
    pub author: Option<String>,
    pub format: DocumentFormat,
    pub byte_len: u64,
    pub opened_at: u64,
    pub progress: f32,
    pub current: u32,
    pub total: u32,
    pub cover: bool,
    pub favourite: bool,
}

impl From<library::Entry> for LibraryBook {
    fn from(entry: library::Entry) -> Self {
        let format = entry.format().into();
        Self {
            path: entry.document.path.to_string_lossy().into_owned(),
            fingerprint: entry.document.fingerprint,
            title: entry.document.title,
            author: entry.author,
            format,
            byte_len: entry.byte_len,
            opened_at: entry.opened_at,
            progress: entry.progress,
            current: entry.current,
            total: entry.total,
            cover: entry.cover,
            favourite: entry.favourite,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct LibraryShelf {
    pub id: u64,
    pub name: String,
    pub books: Vec<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct LibrarySnapshot {
    pub books: Vec<LibraryBook>,
    pub shelves: Vec<LibraryShelf>,
}

fn snapshot() -> Result<LibrarySnapshot, CoreError> {
    Ok(LibrarySnapshot {
        books: library::load()?.into_iter().map(Into::into).collect(),
        shelves: shelves::load()?
            .shelves
            .into_iter()
            .map(|shelf| LibraryShelf {
                id: shelf.id,
                name: shelf.name,
                books: shelf.books,
            })
            .collect(),
    })
}

fn lock() -> Result<std::sync::MutexGuard<'static, ()>, CoreError> {
    CATALOG
        .lock()
        .map_err(|_| "Library is unavailable".to_owned().into())
}

#[uniffi::export]
pub fn load_library() -> Result<LibrarySnapshot, CoreError> {
    let _guard = lock()?;
    snapshot()
}

/// Import a private staged file or HTML folder. Validate it before publishing
/// an entry, and coalesce identical content without resetting reading state.
#[uniffi::export]
pub fn import_library_book(source: String) -> Result<LibraryBook, CoreError> {
    let _guard = lock()?;
    let mut entries = library::load()?;
    let previous_paths = entries
        .iter()
        .map(|entry| entry.document.path.clone())
        .collect::<Vec<_>>();
    let path = managed::import(Path::new(&source))?;
    let result = (|| {
        let summary = inspect_document(path.to_string_lossy().into_owned())?;
        if let Some(existing) = entries.iter().find(|entry| {
            entry.document.fingerprint == summary.fingerprint && entry.document.path.is_file()
        }) {
            if existing.document.path != path {
                managed::remove(&path)?;
            }
            return Ok(existing.clone().into());
        }
        let kind = match extension(&path).as_str() {
            "pdf" => recent::DocumentKind::Pdf,
            "epub" => recent::DocumentKind::Epub,
            _ => recent::DocumentKind::Html,
        };
        // Cover failures do not prevent opening a valid book.
        let cover = generate_cover(&path, &summary.fingerprint).unwrap_or(false);
        let previous = entries
            .iter()
            .find(|entry| entry.document.fingerprint == summary.fingerprint)
            .cloned();
        let entry = library::Entry {
            document: recent::Entry {
                path: path.clone(),
                title: summary.title,
                fingerprint: summary.fingerprint,
                kind,
            },
            author: summary.author,
            byte_len: std::fs::metadata(&path).map_err(|e| e.to_string())?.len(),
            opened_at: previous.as_ref().map_or(0, |entry| entry.opened_at),
            progress: previous.as_ref().map_or(0.0, |entry| entry.progress),
            current: previous.as_ref().map_or(0, |entry| entry.current),
            total: previous.as_ref().map_or(0, |entry| entry.total),
            cover,
            favourite: previous.as_ref().is_some_and(|entry| entry.favourite),
            source_kind: managed::source_kind(&path),
        };
        library::remember(
            &mut entries,
            entry.clone(),
            previous.as_ref().map(|entry| entry.document.path.as_path()),
        )?;
        library::save(&entries)?;
        Ok(entry.into())
    })();
    if result.is_err() && !previous_paths.contains(&path) {
        let _ = managed::remove(&path);
    }
    result
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn generate_cover(path: &Path, fingerprint: &str) -> Result<bool, String> {
    if library::cached_cover(fingerprint)?.is_some() {
        return Ok(true);
    }
    let asset = match extension(path).as_str() {
        "epub" => reader_document::epub::open(path)?.cover()?,
        "pdf" => {
            let document = complete(reader_pdf::open(path.to_owned()))?;
            if let Some(page) = document.pages.first() {
                let width = (336.0 * page.width / page.height)
                    .clamp(240.0, 1024.0)
                    .ceil() as u32;
                let rendered = complete(document.session.render(0, width))?;
                Some(ImageAsset {
                    width: rendered.width,
                    height: rendered.height,
                    rgba: rendered.rgba,
                })
            } else {
                None
            }
        }
        _ => {
            let mut document = reader_document::load_html(path)?;
            let key = document.items.iter().find_map(|item| match item {
                Item::Image { asset_path, .. } => Some(asset_path.clone()),
                _ => None,
            });
            key.and_then(|key| document.images.remove(&key))
        }
    };
    if let Some(asset) = asset {
        library::cache_cover(fingerprint, &asset)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct CoverThumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[uniffi::export]
pub fn library_cover(fingerprint: String) -> Result<Option<CoverThumbnail>, CoreError> {
    Ok(
        library::cached_cover(&fingerprint)?.map(|image| CoverThumbnail {
            width: image.width,
            height: image.height,
            rgba: image.rgba,
        }),
    )
}

#[uniffi::export]
pub fn set_library_favourite(fingerprint: String, favourite: bool) -> Result<(), CoreError> {
    let _guard = lock()?;
    let mut entries = library::load()?;
    find(&mut entries, &fingerprint)?.favourite = favourite;
    library::save(&entries)?;
    Ok(())
}

#[uniffi::export]
pub fn open_library_book(fingerprint: String) -> Result<LibraryBook, CoreError> {
    let _guard = lock()?;
    let mut entries = library::load()?;
    let entry = find(&mut entries, &fingerprint)?;
    if !entry.document.path.is_file() {
        return Err("The private copy is missing. Import this book again."
            .to_owned()
            .into());
    }
    entry.opened_at = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let opened = entry.clone();
    library::remember(&mut entries, opened.clone(), None)?;
    library::save(&entries)?;
    let mut recent = recent::load()?;
    recent::remember(&mut recent, opened.document.clone());
    recent::save(&recent)?;
    Ok(opened.into())
}

fn find<'a>(
    entries: &'a mut [library::Entry],
    fingerprint: &str,
) -> Result<&'a mut library::Entry, CoreError> {
    entries
        .iter_mut()
        .find(|entry| entry.document.fingerprint == fingerprint)
        .ok_or_else(|| "That book is no longer in the library.".to_owned().into())
}

/// Remove only the managed copy; source documents and reading annotations stay.
#[uniffi::export]
pub fn remove_library_book(fingerprint: String) -> Result<(), CoreError> {
    let _guard = lock()?;
    let mut entries = library::load()?;
    let path = find(&mut entries, &fingerprint)?.document.path.clone();
    let forgotten = library::remove(&mut entries, &path);
    library::save(&entries)?;
    let mut shelves = shelves::load()?;
    for fingerprint in forgotten {
        shelves.forget(&fingerprint);
    }
    shelves::save(&shelves)?;
    let mut recent = recent::load()?;
    recent::remove(&mut recent, &path);
    recent::save(&recent)?;
    managed::remove(&path)?;
    Ok(())
}

#[uniffi::export]
pub fn create_library_shelf(name: String) -> Result<u64, CoreError> {
    let _guard = lock()?;
    let mut shelves = shelves::load()?;
    let id = shelves.create(&name)?;
    shelves::save(&shelves)?;
    Ok(id)
}

#[uniffi::export]
pub fn rename_library_shelf(id: u64, name: String) -> Result<(), CoreError> {
    let _guard = lock()?;
    let mut shelves = shelves::load()?;
    shelves.rename(id, &name)?;
    shelves::save(&shelves)?;
    Ok(())
}

#[uniffi::export]
pub fn delete_library_shelf(id: u64) -> Result<(), CoreError> {
    let _guard = lock()?;
    let mut shelves = shelves::load()?;
    shelves.delete(id);
    shelves::save(&shelves)?;
    Ok(())
}

#[uniffi::export]
pub fn toggle_library_shelf(id: u64, fingerprint: String) -> Result<bool, CoreError> {
    let _guard = lock()?;
    find(&mut library::load()?, &fingerprint)?;
    let mut shelves = shelves::load()?;
    let included = shelves.toggle(id, &fingerprint)?;
    shelves::save(&shelves)?;
    Ok(included)
}
