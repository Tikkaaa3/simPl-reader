//! Persistent shelf independent of the capped recent-files list.
use std::{
    fs::File,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

use image::{
    ImageDecoder, ImageEncoder,
    codecs::png::{PngDecoder, PngEncoder},
    imageops::FilterType,
};
use serde::{Deserialize, Serialize};

use crate::{ImageAsset, position, recent};

const VERSION: u32 = 1;
const MAX_ENTRIES: usize = 4096;
const MAX_LIBRARY_BYTES: u64 = 4 * 1024 * 1024;
const MAX_AUTHOR_BYTES: usize = 1024;
const MAX_COVER_BYTES: u64 = 512 * 1024;
const MAX_SOURCE_RGBA: usize = 128 * 1024 * 1024;
const COVER_WIDTH: u32 = 240;
const COVER_HEIGHT: u32 = 360;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub document: recent::Entry,
    pub author: Option<String>,
    pub byte_len: u64,
    pub opened_at: u64,
    pub progress: f32,
    pub current: u32,
    pub total: u32,
    pub cover: bool,
    #[serde(default)]
    pub favourite: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<recent::DocumentKind>,
}

impl Entry {
    pub fn format(&self) -> recent::DocumentKind {
        self.source_kind.unwrap_or(self.document.kind)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Library {
    version: u32,
    entries: Vec<Entry>,
}
#[derive(Serialize)]
struct LibraryRef<'a> {
    version: u32,
    entries: &'a [Entry],
}

fn library_path() -> PathBuf {
    position::storage_base().join("simPl").join("library.json")
}

fn cover_path(fingerprint: &str) -> Result<PathBuf, String> {
    validate_fingerprint(fingerprint)?;
    Ok(position::storage_base()
        .join("simPl")
        .join("covers")
        .join(format!("{}.png", fingerprint.to_ascii_lowercase())))
}

fn validate_fingerprint(value: &str) -> Result<(), String> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid cover SHA-256 fingerprint".into());
    }
    Ok(())
}

fn validate(entries: &[Entry]) -> Result<(), String> {
    if entries.len() > MAX_ENTRIES {
        return Err(format!("library exceeds {MAX_ENTRIES} entries"));
    }
    let mut paths = Vec::with_capacity(entries.len());
    for entry in entries {
        let document = &entry.document;
        let path = document
            .path
            .to_str()
            .ok_or("library path is not Unicode")?;
        if !document.path.is_absolute()
            || path.len() > 4096
            || path.contains('\0')
            || document.title.trim().is_empty()
            || document.title.len() > 1024
        {
            return Err("library document has invalid path or title".into());
        }
        validate_fingerprint(&document.fingerprint)?;
        if entry
            .author
            .as_ref()
            .is_some_and(|author| author.trim().is_empty() || author.len() > MAX_AUTHOR_BYTES)
            || !entry.progress.is_finite()
            || !(0.0..=1.0).contains(&entry.progress)
            || (entry.total == 0 && entry.current != 0)
            || (entry.total != 0 && (entry.current == 0 || entry.current > entry.total))
        {
            return Err("library entry has invalid author, progress or location".into());
        }
        paths.push(recent::path_key(&document.path));
    }
    paths.sort_unstable_by(|first, second| recent::compare_path_keys(first, second));
    if paths
        .windows(2)
        .any(|pair| recent::compare_path_keys(&pair[0], &pair[1]).is_eq())
    {
        return Err("library contains duplicate paths".into());
    }
    Ok(())
}

/// A missing library bootstraps the recent list once; an existing empty library stays empty.
pub fn load() -> Result<Vec<Entry>, String> {
    let path = library_path();
    match File::open(&path) {
        Ok(file) => load_file(file, &path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let entries = recent::load()?
                .into_iter()
                .map(|document| Entry {
                    source_kind: crate::managed::source_kind(&document.path),
                    document,
                    author: None,
                    byte_len: 0,
                    opened_at: 0,
                    progress: 0.0,
                    current: 0,
                    total: 0,
                    cover: false,
                    favourite: false,
                })
                .collect::<Vec<_>>();
            validate(&entries)?;
            Ok(entries)
        }
        Err(error) => Err(format!("cannot open library {}: {error}", path.display())),
    }
}

#[cfg(test)]
fn load_at(path: &Path) -> Result<Vec<Entry>, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("cannot open library {}: {error}", path.display())),
    };
    load_file(file, path)
}

fn load_file(file: File, path: &Path) -> Result<Vec<Entry>, String> {
    let mut bytes = Vec::new();
    file.take(MAX_LIBRARY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read library {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_LIBRARY_BYTES {
        return Err("library exceeds size limit".into());
    }
    let mut library: Library = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid library {}: {error}", path.display()))?;
    if library.version != VERSION {
        return Err(format!("unsupported library version {}", library.version));
    }
    validate(&library.entries)?;
    for entry in &mut library.entries {
        if entry.source_kind.is_none() {
            entry.source_kind = crate::managed::source_kind(&entry.document.path);
        }
    }
    Ok(library.entries)
}

/// Atomically replace the entire shelf; never silently discard old entries.
pub fn save(entries: &[Entry]) -> Result<(), String> {
    save_at(&library_path(), entries)
}

fn save_at(path: &Path, entries: &[Entry]) -> Result<(), String> {
    validate(entries)?;
    let bytes = serde_json::to_vec(&LibraryRef {
        version: VERSION,
        entries,
    })
    .map_err(|error| format!("cannot encode library: {error}"))?;
    if bytes.len() as u64 > MAX_LIBRARY_BYTES {
        return Err("library exceeds size limit".into());
    }
    position::atomic_write(path, &bytes, "library", "library", ".library")
}

/// Update a validated shelf without cloning older entries or silently evicting books.
pub fn remember(
    entries: &mut Vec<Entry>,
    entry: Entry,
    replaced: Option<&Path>,
) -> Result<(), String> {
    validate(std::slice::from_ref(&entry))?;
    let mut existing = entries
        .iter()
        .position(|existing| recent::same_path(&existing.document.path, &entry.document.path));
    let old = replaced
        .and_then(|path| {
            entries
                .iter()
                .position(|entry| recent::same_path(&entry.document.path, path))
        })
        .filter(|index| Some(*index) != existing);
    let mut entry = entry;
    entry.favourite |= existing.or(old).is_some_and(|i| entries[i].favourite);
    let retained = entries.len() - usize::from(old.is_some());
    if entries.len() > MAX_ENTRIES || (existing.is_none() && retained == MAX_ENTRIES) {
        return Err(format!("library exceeds {MAX_ENTRIES} entries"));
    }
    if let Some(old) = old {
        entries.remove(old);
        if let Some(index) = existing.as_mut()
            && *index > old
        {
            *index -= 1;
        }
    }
    if let Some(index) = existing {
        entries[index] = entry;
        entries[..=index].rotate_right(1);
    } else {
        entries.insert(0, entry);
    }
    Ok(())
}

/// Removes the entry at `path`; returns the fingerprints no longer in the library.
pub fn remove(entries: &mut Vec<Entry>, path: &Path) -> Vec<String> {
    let mut removed = Vec::new();
    entries.retain(|entry| {
        let keep = !recent::same_path(&entry.document.path, path);
        if !keep {
            removed.push(entry.document.fingerprint.clone());
        }
        keep
    });
    removed.retain(|fingerprint| {
        !entries
            .iter()
            .any(|entry| entry.document.fingerprint == *fingerprint)
    });
    removed
}

/// Decode only small, validated PNG thumbnails, never full-size covers in library state.
pub fn cached_cover(fingerprint: &str) -> Result<Option<ImageAsset>, String> {
    let path = cover_path(fingerprint)?;
    cached_cover_at(&path)
}

fn cached_cover_at(path: &Path) -> Result<Option<ImageAsset>, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot open cover {}: {error}", path.display())),
    };
    let mut bytes = Vec::new();
    file.take(MAX_COVER_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read cover {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_COVER_BYTES {
        return Err("cached cover exceeds encoded size limit".into());
    }
    let decoder = PngDecoder::new(Cursor::new(&bytes))
        .map_err(|error| format!("invalid cached cover: {error}"))?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || width > COVER_WIDTH || height > COVER_HEIGHT {
        return Err("cached cover exceeds thumbnail dimensions".into());
    }
    let rgba = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .map_err(|error| format!("cannot decode cached cover: {error}"))?
        .into_rgba8()
        .into_raw();
    Ok(Some(ImageAsset {
        width,
        height,
        rgba,
    }))
}

/// Scale and persist a thumbnail atomically, keyed by content fingerprint.
pub fn cache_cover(fingerprint: &str, asset: &ImageAsset) -> Result<(), String> {
    let path = cover_path(fingerprint)?;
    cache_cover_at(&path, asset)
}

fn cache_cover_at(path: &Path, asset: &ImageAsset) -> Result<(), String> {
    let length = (asset.width as usize)
        .checked_mul(asset.height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("cover dimensions overflow")?;
    if asset.width == 0
        || asset.height == 0
        || length > MAX_SOURCE_RGBA
        || length != asset.rgba.len()
    {
        return Err("invalid cover image dimensions or RGBA data".into());
    }
    let image = image::ImageBuffer::<image::Rgba<u8>, &[u8]>::from_raw(
        asset.width,
        asset.height,
        asset.rgba.as_slice(),
    )
    .ok_or("invalid cover RGBA data")?;
    // Retain the union of the 2:3 library and 5:7 resume center crops before
    // downsampling; invisible landscape margins must not consume thumbnail detail.
    let crop_width = if u64::from(asset.width) * 7 > u64::from(asset.height) * 5 {
        (u64::from(asset.height) * 5).div_ceil(7) as u32
    } else {
        asset.width
    };
    let crop_height = if u64::from(asset.width) * 3 < u64::from(asset.height) * 2 {
        (u64::from(asset.width) * 3).div_ceil(2) as u32
    } else {
        asset.height
    };
    let cropped = image::imageops::crop_imm(
        &image,
        (asset.width - crop_width) / 2,
        (asset.height - crop_height) / 2,
        crop_width,
        crop_height,
    );
    let scale = (COVER_WIDTH as f64 / f64::from(crop_width))
        .min(COVER_HEIGHT as f64 / f64::from(crop_height))
        .min(1.0);
    let width = (f64::from(crop_width) * scale).floor().max(1.0) as u32;
    let height = (f64::from(crop_height) * scale).floor().max(1.0) as u32;
    let mut bytes = Vec::new();
    if width == asset.width && height == asset.height {
        PngEncoder::new(&mut bytes)
            .write_image(&asset.rgba, width, height, image::ExtendedColorType::Rgba8)
            .map_err(|error| format!("cannot encode cover: {error}"))?;
    } else {
        let thumbnail = image::imageops::resize(&*cropped, width, height, FilterType::Triangle);
        PngEncoder::new(&mut bytes)
            .write_image(
                thumbnail.as_raw(),
                width,
                height,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|error| format!("cannot encode cover: {error}"))?;
    }
    if bytes.len() as u64 > MAX_COVER_BYTES {
        return Err("cached cover exceeds encoded size limit".into());
    }
    position::atomic_write(path, &bytes, "cover", "covers", ".cover")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recent::DocumentKind;

    fn entry(index: usize) -> Entry {
        Entry {
            document: recent::Entry {
                path: std::env::temp_dir().join(format!("book-{index}.epub")),
                title: format!("Book {index}"),
                fingerprint: format!("{index:064x}"),
                kind: DocumentKind::Epub,
            },
            author: None,
            byte_len: 1,
            opened_at: 1,
            progress: 0.0,
            current: 1,
            total: 1,
            cover: false,
            favourite: false,
            source_kind: None,
        }
    }

    #[test]
    fn retained_beyond_recent_and_rejects_full_shelf_without_mutation() {
        let mut entries = (0..MAX_ENTRIES).map(entry).collect::<Vec<_>>();
        validate(&entries).unwrap();
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert!(remember(&mut entries, entry(MAX_ENTRIES), None).is_err());
        assert_eq!(entries.len(), MAX_ENTRIES);
        remember(&mut entries, entry(0), None).unwrap();
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(entries[0].document.title, "Book 0");
    }

    #[test]
    fn moved_document_replaces_its_old_slot_at_capacity() {
        let mut entries = (0..MAX_ENTRIES).map(entry).collect::<Vec<_>>();
        let old = entries[5].document.path.clone();
        let mut moved = entries[5].clone();
        let new = entry(MAX_ENTRIES).document.path;
        moved.document.path = new.clone();
        remember(&mut entries, moved, Some(&old)).unwrap();
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(entries[0].document.path, new);
        assert_eq!(entries[0].document.fingerprint, format!("{:064x}", 5));
        assert!(
            !entries
                .iter()
                .any(|entry| recent::same_path(&entry.document.path, &old))
        );
    }

    #[test]
    fn relocation_coalesces_an_existing_destination() {
        let mut entries = vec![entry(0), entry(1), entry(2)];
        let old = entries[0].document.path.clone();
        let mut moved = entry(1);
        moved.progress = 0.5;
        remember(&mut entries, moved, Some(&old)).unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.document.path.clone())
                .collect::<Vec<_>>(),
            vec![entry(1).document.path, entry(2).document.path]
        );
        assert_eq!(entries[0].progress, 0.5);
    }

    #[test]
    fn favourite_survives_reopen_update_and_legacy_records_default_to_false() {
        let mut original = entry(7);
        original.favourite = true;
        let encoded = serde_json::to_vec(&original).unwrap();
        let decoded: Entry = serde_json::from_slice(&encoded).unwrap();
        assert!(decoded.favourite);
        let mut entries = vec![decoded];
        remember(&mut entries, entry(7), None).unwrap();
        assert!(entries[0].favourite);
        let old = entries[0].document.path.clone();
        let mut moved = entry(7);
        moved.document.path = std::env::temp_dir().join("moved-favourite.epub");
        remember(&mut entries, moved, Some(&old)).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].favourite);
        let mut legacy = serde_json::to_value(original).unwrap();
        legacy.as_object_mut().unwrap().remove("favourite");
        assert!(!serde_json::from_value::<Entry>(legacy).unwrap().favourite);
    }

    #[test]
    fn bounded_storage_rejects_duplicate_and_invalid_records() {
        let folder =
            std::env::temp_dir().join(format!("simpl-library-test-{}", std::process::id()));
        let path = folder.join("library.json");
        let _ = std::fs::remove_file(&path);
        let original = entry(1);
        save_at(&path, std::slice::from_ref(&original)).unwrap();
        assert_eq!(load_at(&path).unwrap().len(), 1);
        assert!(save_at(&path, &[original.clone(), original]).is_err());
        assert_eq!(load_at(&path).unwrap().len(), 1);
        std::fs::write(&path, vec![b' '; MAX_LIBRARY_BYTES as usize + 1]).unwrap();
        assert!(load_at(&path).is_err());
        std::fs::remove_file(path).unwrap();
        let _ = std::fs::remove_dir(folder);
    }

    #[test]
    fn invalid_cover_key_and_image_are_rejected() {
        assert!(cached_cover("../outside").is_err());
        assert!(
            cache_cover(
                &"a".repeat(64),
                &ImageAsset {
                    width: 240,
                    height: 360,
                    rgba: vec![0; 4]
                }
            )
            .is_err()
        );
    }

    #[test]
    fn thumbnails_round_trip_with_bounds_and_corruption_errors() {
        let folder = std::env::temp_dir().join(format!("simpl-cover-test-{}", std::process::id()));
        let path = folder.join("cover.png");
        let source = ImageAsset {
            width: 480,
            height: 720,
            rgba: vec![42; 480 * 720 * 4],
        };
        cache_cover_at(&path, &source).unwrap();
        let thumbnail = cached_cover_at(&path).unwrap().unwrap();
        assert_eq!((thumbnail.width, thumbnail.height), (240, 360));
        assert_eq!(thumbnail.rgba.len(), 240 * 360 * 4);
        image::RgbaImage::from_pixel(241, 361, image::Rgba([0, 0, 0, 255]))
            .save(&path)
            .unwrap();
        assert!(cached_cover_at(&path).is_err());
        std::fs::write(&path, vec![0; MAX_COVER_BYTES as usize + 1]).unwrap();
        assert!(cached_cover_at(&path).is_err());
        std::fs::remove_file(&path).unwrap();
        let _ = std::fs::remove_dir(folder);
    }

    #[test]
    fn wide_and_tall_thumbnails_preserve_the_two_center_crop_frames() {
        let folder =
            std::env::temp_dir().join(format!("simpl-cover-crop-test-{}", std::process::id()));
        let path = folder.join("cover.png");
        for (width, height, expected) in [(500, 200, (143, 200)), (100, 500, (100, 150))] {
            let mut source =
                image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]));
            for (x, y, pixel) in source.enumerate_pixels_mut() {
                if (width > height && (150..350).contains(&x))
                    || (height > width && (175..325).contains(&y))
                {
                    *pixel = image::Rgba([20, 30, 200, 255]);
                }
            }
            cache_cover_at(
                &path,
                &ImageAsset {
                    width,
                    height,
                    rgba: source.into_raw(),
                },
            )
            .unwrap();
            let thumbnail = cached_cover_at(&path).unwrap().unwrap();
            assert_eq!((thumbnail.width, thumbnail.height), expected);
            assert!(
                thumbnail
                    .rgba
                    .chunks_exact(4)
                    .all(|pixel| pixel == [20, 30, 200, 255])
            );
        }
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(folder).unwrap();
    }
}
