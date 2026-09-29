//! Bookmarks, highlights and notes: one small, versioned file per book
//! fingerprint, so they follow the content when the file is moved or imported
//! again. Records keep the selected text next to their anchor, which lets a
//! reader find a passage again when item IDs change (a parser change) and lets
//! the list show something meaningful even when the passage cannot be found.
//!
//! Storage mirrors `position.rs`: `%LOCALAPPDATA%/simPl/annotations/<sha256>.json`
//! (process temp directory when LOCALAPPDATA is unavailable), atomic replacement,
//! bounded size, and errors instead of silent resets for unreadable files.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const VERSION: u32 = 1;

/// Largest annotation file that is read or written.
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// Longest passage a single highlight may cover.
pub const MAX_QUOTE_BYTES: usize = 8 * 1024;
pub const MAX_NOTE_BYTES: usize = 16 * 1024;
pub const MAX_BOOKMARKS: usize = 2_000;
pub const MAX_HIGHLIGHTS: usize = 5_000;
const MAX_ITEM_ID_BYTES: usize = 1024;
const MAX_HREF_BYTES: usize = 4096;
const MAX_LABEL_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HighlightColor {
    #[default]
    Yellow,
    Green,
    Blue,
    Pink,
}

impl HighlightColor {
    pub const ALL: [Self; 4] = [Self::Yellow, Self::Green, Self::Blue, Self::Pink];

    pub fn label(self) -> &'static str {
        match self {
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Pink => "Pink",
        }
    }
}

/// A position inside one reflow item: a UTF-8 byte offset in its text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReflowPoint {
    pub item_id: String,
    pub byte: usize,
}

/// A glyph position in a PDF page's text layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PdfPoint {
    pub page: u32,
    pub index: usize,
}

/// A selected passage. Reflow ranges are end-exclusive byte offsets; PDF ranges
/// count glyphs and include their last glyph, like a PDF selection focus.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Place {
    Reflow {
        /// Canonical package-relative EPUB chapter href; `None` for single-section books.
        chapter: Option<String>,
        from: ReflowPoint,
        to: ReflowPoint,
    },
    Pdf {
        from: PdfPoint,
        to: PdfPoint,
    },
}

/// Where a bookmark returns to: the top of a page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BookmarkPlace {
    Reflow {
        chapter: Option<String>,
        item_id: String,
        /// Fraction of the item above the page top, 0.0 to 1.0.
        within: f32,
        /// Global page index in the book's page map. A bookmark is a page, and
        /// page numbers stay the same in every reading theme.
        page_number: u32,
    },
    Pdf {
        page: u32,
        within: f32,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: u64,
    /// Seconds since the Unix epoch.
    pub created: u64,
    pub place: BookmarkPlace,
    /// Page label when it was added, for display only.
    pub page: String,
    /// The first words of the page, for display only.
    pub excerpt: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Highlight {
    pub id: u64,
    pub created: u64,
    pub color: HighlightColor,
    pub place: Place,
    /// Page label when it was added, for display only.
    pub page: String,
    /// The highlighted text as it was when highlighted.
    pub quote: String,
    pub note: Option<String>,
}

/// Everything saved for one book.
#[derive(Clone, Debug, PartialEq)]
pub struct Annotations {
    pub fingerprint: String,
    pub bookmarks: Vec<Bookmark>,
    pub highlights: Vec<Highlight>,
    next_id: u64,
}

#[derive(Serialize, Deserialize)]
struct FileRecord {
    version: u32,
    fingerprint: String,
    next_id: u64,
    bookmarks: Vec<Bookmark>,
    highlights: Vec<Highlight>,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn valid_fingerprint(fingerprint: &str) -> Result<(), String> {
    if fingerprint.len() != 64 || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("annotations need a SHA-256 book fingerprint".into());
    }
    Ok(())
}

fn valid_fraction(value: f32) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

impl Annotations {
    pub fn new(fingerprint: &str) -> Result<Self, String> {
        valid_fingerprint(fingerprint)?;
        Ok(Self {
            fingerprint: fingerprint.to_ascii_lowercase(),
            bookmarks: Vec::new(),
            highlights: Vec::new(),
            next_id: 1,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.bookmarks.is_empty() && self.highlights.is_empty()
    }

    pub fn bookmark(&self, id: u64) -> Option<&Bookmark> {
        self.bookmarks.iter().find(|bookmark| bookmark.id == id)
    }

    pub fn highlight(&self, id: u64) -> Option<&Highlight> {
        self.highlights.iter().find(|highlight| highlight.id == id)
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn add_bookmark(
        &mut self,
        place: BookmarkPlace,
        page: String,
        excerpt: String,
    ) -> Result<u64, String> {
        if self.next_id == u64::MAX {
            return Err("This book cannot hold more annotations".into());
        }
        if self.bookmarks.len() >= MAX_BOOKMARKS {
            return Err(format!("A book can have up to {MAX_BOOKMARKS} bookmarks"));
        }
        let bookmark = Bookmark {
            id: self.next_id,
            created: now(),
            place,
            page,
            excerpt,
        };
        validate_bookmark(&bookmark)?;
        self.take_id();
        let id = bookmark.id;
        self.bookmarks.push(bookmark);
        Ok(id)
    }

    pub fn remove_bookmark(&mut self, id: u64) -> bool {
        let before = self.bookmarks.len();
        self.bookmarks.retain(|bookmark| bookmark.id != id);
        self.bookmarks.len() != before
    }

    /// Highlights a passage. Repeating the same color and passage reuses its
    /// record; another color creates an independent annotation, even there.
    pub fn add_highlight(
        &mut self,
        place: Place,
        color: HighlightColor,
        page: String,
        quote: String,
    ) -> Result<u64, String> {
        if let Some(existing) = self
            .highlights
            .iter_mut()
            .find(|highlight| highlight.place == place && highlight.color == color)
        {
            return Ok(existing.id);
        }
        if self.next_id == u64::MAX {
            return Err("This book cannot hold more annotations".into());
        }
        if self.highlights.len() >= MAX_HIGHLIGHTS {
            return Err(format!("A book can have up to {MAX_HIGHLIGHTS} highlights"));
        }
        let highlight = Highlight {
            id: self.next_id,
            created: now(),
            color,
            place,
            page,
            quote,
            note: None,
        };
        validate_highlight(&highlight)?;
        self.take_id();
        let id = highlight.id;
        self.highlights.push(highlight);
        Ok(id)
    }

    /// Replaces a connected set of same-color highlights with their union.
    /// The retained ID and its note remain stable. Several distinct notes are
    /// intentionally left as separate records by the caller.
    pub fn merge_highlights(
        &mut self,
        ids: &[u64],
        place: Place,
        color: HighlightColor,
        page: String,
        quote: String,
    ) -> Result<u64, String> {
        if ids.is_empty() {
            return self.add_highlight(place, color, page, quote);
        }
        let index: std::collections::HashMap<_, _> = self
            .highlights
            .iter()
            .map(|highlight| (highlight.id, highlight))
            .collect();
        let members: Vec<_> = ids
            .iter()
            .map(|id| {
                index
                    .get(id)
                    .copied()
                    .ok_or_else(|| "This highlight no longer exists.".to_owned())
            })
            .collect::<Result<_, _>>()?;
        if members
            .iter()
            .filter(|highlight| highlight.note.is_some())
            .count()
            > 1
        {
            return Err("Separate notes cannot be merged into one highlight.".into());
        }
        let mut retained = members
            .iter()
            .copied()
            .find(|highlight| highlight.note.is_some())
            .unwrap_or(members[0])
            .clone();
        retained.place = place;
        retained.color = color;
        retained.page = page;
        retained.quote = quote;
        validate_highlight(&retained)?;
        let keep = retained.id;
        let merged: std::collections::HashSet<_> = ids.iter().copied().collect();
        self.highlights
            .retain(|highlight| highlight.id == keep || !merged.contains(&highlight.id));
        *self
            .highlights
            .iter_mut()
            .find(|highlight| highlight.id == keep)
            .unwrap() = retained;
        Ok(keep)
    }

    pub fn remove_highlight(&mut self, id: u64) -> bool {
        let before = self.highlights.len();
        self.highlights.retain(|highlight| highlight.id != id);
        self.highlights.len() != before
    }

    pub fn set_color(&mut self, id: u64, color: HighlightColor) -> bool {
        self.highlights
            .iter_mut()
            .find(|highlight| highlight.id == id)
            .map(|highlight| highlight.color = color)
            .is_some()
    }

    /// Sets or, for blank text, removes a highlight's note.
    pub fn set_note(&mut self, id: u64, note: &str) -> Result<bool, String> {
        let note = note.trim();
        if note.len() > MAX_NOTE_BYTES {
            return Err(format!(
                "A note can hold up to {} characters",
                MAX_NOTE_BYTES / 4
            ));
        }
        Ok(self
            .highlights
            .iter_mut()
            .find(|highlight| highlight.id == id)
            .map(|highlight| {
                highlight.note = (!note.is_empty()).then(|| note.to_owned());
            })
            .is_some())
    }
}

fn validate_bookmark(bookmark: &Bookmark) -> Result<(), String> {
    if bookmark.page.len() > MAX_LABEL_BYTES || bookmark.excerpt.len() > MAX_LABEL_BYTES {
        return Err("bookmark text is too long".into());
    }
    match &bookmark.place {
        BookmarkPlace::Reflow {
            chapter,
            item_id,
            within,
            page_number,
        } => {
            validate_chapter(chapter.as_deref())?;
            if item_id.is_empty()
                || item_id.len() > MAX_ITEM_ID_BYTES
                || !valid_fraction(*within)
                || *page_number >= 100_000
            {
                return Err("bookmark has an invalid position".into());
            }
        }
        BookmarkPlace::Pdf { page, within } => {
            if *page >= 100_000 || !valid_fraction(*within) {
                return Err("bookmark has an invalid position".into());
            }
        }
    }
    Ok(())
}

fn validate_chapter(chapter: Option<&str>) -> Result<(), String> {
    match chapter {
        Some(href) if href.is_empty() || href.len() > MAX_HREF_BYTES || href.contains('\0') => {
            Err("annotation has an invalid chapter".into())
        }
        _ => Ok(()),
    }
}

fn validate_highlight(highlight: &Highlight) -> Result<(), String> {
    if highlight.quote.len() > MAX_QUOTE_BYTES {
        return Err(format!(
            "Select at most about {} characters to highlight at once",
            MAX_QUOTE_BYTES / 4
        ));
    }
    if highlight.page.len() > MAX_LABEL_BYTES
        || highlight
            .note
            .as_ref()
            .is_some_and(|note| note.len() > MAX_NOTE_BYTES)
    {
        return Err("highlight text is too long".into());
    }
    match &highlight.place {
        Place::Reflow { chapter, from, to } => {
            validate_chapter(chapter.as_deref())?;
            if [from, to]
                .iter()
                .any(|point| point.item_id.is_empty() || point.item_id.len() > MAX_ITEM_ID_BYTES)
            {
                return Err("highlight has an invalid position".into());
            }
        }
        Place::Pdf { from, to } => {
            if from.page >= 100_000 || to.page >= 100_000 || from > to {
                return Err("highlight has an invalid position".into());
            }
        }
    }
    Ok(())
}

fn validate(annotations: &Annotations) -> Result<(), String> {
    valid_fingerprint(&annotations.fingerprint)?;
    if annotations.bookmarks.len() > MAX_BOOKMARKS || annotations.highlights.len() > MAX_HIGHLIGHTS
    {
        return Err("annotation file holds too many records".into());
    }
    let mut ids = std::collections::HashSet::new();
    for id in annotations
        .bookmarks
        .iter()
        .map(|bookmark| bookmark.id)
        .chain(annotations.highlights.iter().map(|highlight| highlight.id))
    {
        if id >= annotations.next_id || !ids.insert(id) {
            return Err("annotation file has duplicate or invalid record IDs".into());
        }
    }
    annotations
        .bookmarks
        .iter()
        .try_for_each(validate_bookmark)?;
    annotations
        .highlights
        .iter()
        .try_for_each(validate_highlight)
}

fn directory() -> PathBuf {
    crate::position::storage_base()
        .join("simPl")
        .join("annotations")
}

fn file_path(directory: &Path, fingerprint: &str) -> Result<PathBuf, String> {
    valid_fingerprint(fingerprint)?;
    Ok(directory.join(format!("{}.json", fingerprint.to_ascii_lowercase())))
}

/// A book without a file has no annotations yet. Unreadable, oversized or
/// invalid files are errors: they are left in place so nothing is lost silently.
pub fn load(fingerprint: &str) -> Result<Annotations, String> {
    load_from(&directory(), fingerprint)
}

/// Writes all annotations of one book, or removes its file when none are left.
pub fn save(annotations: &Annotations) -> Result<(), String> {
    save_to(&directory(), annotations)
}

fn load_from(directory: &Path, fingerprint: &str) -> Result<Annotations, String> {
    let path = file_path(directory, fingerprint)?;
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Annotations::new(fingerprint);
        }
        Err(error) => {
            return Err(format!(
                "cannot open annotations {}: {error}",
                path.display()
            ));
        }
    };
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read annotations {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(format!(
            "annotations {} exceed the size limit",
            path.display()
        ));
    }
    let record: FileRecord = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid annotations {}: {error}", path.display()))?;
    if record.version != VERSION {
        return Err(format!(
            "unsupported annotations version {}",
            record.version
        ));
    }
    if !record.fingerprint.eq_ignore_ascii_case(fingerprint) {
        return Err(format!(
            "annotations {} belong to another book",
            path.display()
        ));
    }
    let annotations = Annotations {
        fingerprint: record.fingerprint.to_ascii_lowercase(),
        bookmarks: record.bookmarks,
        highlights: record.highlights,
        next_id: record.next_id,
    };
    validate(&annotations)?;
    Ok(annotations)
}

fn save_to(directory: &Path, annotations: &Annotations) -> Result<(), String> {
    validate(annotations)?;
    let path = file_path(directory, &annotations.fingerprint)?;
    if annotations.is_empty() {
        return match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!(
                "cannot remove annotations {}: {error}",
                path.display()
            )),
        };
    }
    let bytes = serde_json::to_vec(&FileRecord {
        version: VERSION,
        fingerprint: annotations.fingerprint.clone(),
        next_id: annotations.next_id,
        bookmarks: annotations.bookmarks.clone(),
        highlights: annotations.highlights.clone(),
    })
    .map_err(|error| format!("cannot encode annotations: {error}"))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("This book has more annotations than simPl can store".into());
    }
    crate::position::atomic_write(&path, &bytes, "annotations", "annotations", ".annotations")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(digit: char) -> String {
        digit.to_string().repeat(64)
    }

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "simpl-annotations-{name}-{}-{}",
            std::process::id(),
            now()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn span(item: &str, from: usize, to: usize) -> Place {
        Place::Reflow {
            chapter: Some("OEBPS/ch1.xhtml".into()),
            from: ReflowPoint {
                item_id: item.into(),
                byte: from,
            },
            to: ReflowPoint {
                item_id: item.into(),
                byte: to,
            },
        }
    }

    #[test]
    fn records_round_trip_and_follow_the_fingerprint() {
        let dir = folder("round-trip");
        let mut notes = Annotations::new(&fingerprint('a')).unwrap();
        let bookmark = notes
            .add_bookmark(
                BookmarkPlace::Reflow {
                    chapter: None,
                    item_id: "item-000004".into(),
                    within: 0.25,
                    page_number: 11,
                },
                "12".into(),
                "It was a bright cold day".into(),
            )
            .unwrap();
        let highlight = notes
            .add_highlight(
                span("item-000009", 4, 19),
                HighlightColor::Green,
                "13".into(),
                "quiet lighthouse".into(),
            )
            .unwrap();
        assert!(
            notes
                .set_note(highlight, "  Compare with chapter 2  ")
                .unwrap()
        );
        save_to(&dir, &notes).unwrap();

        let loaded = load_from(&dir, &fingerprint('a')).unwrap();
        assert_eq!(loaded, notes);
        assert_eq!(loaded.bookmark(bookmark).unwrap().page, "12");
        assert_eq!(
            loaded.highlight(highlight).unwrap().note.as_deref(),
            Some("Compare with chapter 2")
        );
        // Another book never sees these records, even for a similar fingerprint.
        assert!(load_from(&dir, &fingerprint('b')).unwrap().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ids_are_never_reused_and_different_colors_can_share_a_passage() {
        let mut notes = Annotations::new(&fingerprint('c')).unwrap();
        let first = notes
            .add_highlight(
                span("a", 0, 3),
                HighlightColor::Yellow,
                "1".into(),
                "abc".into(),
            )
            .unwrap();
        let again = notes
            .add_highlight(
                span("a", 0, 3),
                HighlightColor::Pink,
                "1".into(),
                "abc".into(),
            )
            .unwrap();
        assert_ne!(first, again);
        assert_eq!(notes.highlights.len(), 2);
        assert_eq!(notes.highlights[0].color, HighlightColor::Yellow);
        let repeated = notes
            .add_highlight(
                span("a", 0, 3),
                HighlightColor::Pink,
                "1".into(),
                "abc".into(),
            )
            .unwrap();
        assert_eq!(repeated, again);
        assert!(notes.remove_highlight(first));
        let second = notes
            .add_highlight(
                span("a", 0, 3),
                HighlightColor::Blue,
                "1".into(),
                "abc".into(),
            )
            .unwrap();
        assert_ne!(first, second);
        assert!(!notes.remove_highlight(first));
    }

    #[test]
    fn merging_same_color_highlights_preserves_a_note_and_rejects_two_notes() {
        let mut data = Annotations::new(&fingerprint('7')).unwrap();
        let place = |from, to| Place::Pdf {
            from: PdfPoint {
                page: 0,
                index: from,
            },
            to: PdfPoint { page: 0, index: to },
        };
        let first = data
            .add_highlight(
                place(1, 4),
                HighlightColor::Yellow,
                "1".into(),
                "one".into(),
            )
            .unwrap();
        let second = data
            .add_highlight(
                place(3, 7),
                HighlightColor::Yellow,
                "1".into(),
                "two".into(),
            )
            .unwrap();
        data.set_note(second, "Keep this note").unwrap();
        let retained = data
            .merge_highlights(
                &[first, second],
                place(1, 7),
                HighlightColor::Yellow,
                "1".into(),
                "one and two".into(),
            )
            .unwrap();
        assert_eq!(retained, second);
        assert_eq!(data.highlights.len(), 1);
        assert_eq!(
            data.highlight(second).unwrap().note.as_deref(),
            Some("Keep this note")
        );
        assert_eq!(data.highlight(second).unwrap().quote, "one and two");

        let third = data
            .add_highlight(
                place(7, 10),
                HighlightColor::Yellow,
                "1".into(),
                "three".into(),
            )
            .unwrap();
        data.set_note(third, "Another note").unwrap();
        let unchanged = data.clone();
        assert!(
            data.merge_highlights(
                &[second, third],
                place(1, 10),
                HighlightColor::Yellow,
                "1".into(),
                "everything".into()
            )
            .is_err()
        );
        assert_eq!(data, unchanged);
    }

    #[test]
    fn blank_notes_remove_the_note_and_an_empty_book_removes_its_file() {
        let dir = folder("empty");
        let mut notes = Annotations::new(&fingerprint('d')).unwrap();
        let id = notes
            .add_highlight(
                span("a", 1, 2),
                HighlightColor::Yellow,
                "1".into(),
                "b".into(),
            )
            .unwrap();
        notes.set_note(id, "keep").unwrap();
        notes.set_note(id, "   ").unwrap();
        assert_eq!(notes.highlight(id).unwrap().note, None);
        save_to(&dir, &notes).unwrap();
        assert!(file_path(&dir, &fingerprint('d')).unwrap().exists());
        notes.remove_highlight(id);
        save_to(&dir, &notes).unwrap();
        assert!(!file_path(&dir, &fingerprint('d')).unwrap().exists());
        // Removing what is not there is not an error.
        save_to(&dir, &notes).unwrap();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn limits_are_enforced_when_adding() {
        let mut notes = Annotations::new(&fingerprint('e')).unwrap();
        let long = "x".repeat(MAX_QUOTE_BYTES + 1);
        assert!(
            notes
                .add_highlight(span("a", 0, 1), HighlightColor::Yellow, "1".into(), long)
                .is_err()
        );
        let id = notes
            .add_highlight(
                span("a", 0, 1),
                HighlightColor::Yellow,
                "1".into(),
                "x".into(),
            )
            .unwrap();
        assert!(notes.set_note(id, &"n".repeat(MAX_NOTE_BYTES + 1)).is_err());
        assert!(
            notes
                .add_bookmark(
                    BookmarkPlace::Pdf {
                        page: 1,
                        within: f32::NAN
                    },
                    "2".into(),
                    String::new()
                )
                .is_err()
        );
        assert!(Annotations::new("not-a-fingerprint").is_err());
    }

    #[test]
    fn unreadable_files_are_reported_and_left_alone() {
        let dir = folder("damaged");
        fs::create_dir_all(&dir).unwrap();
        let path = file_path(&dir, &fingerprint('f')).unwrap();
        fs::write(&path, b"{ not json").unwrap();
        assert!(load_from(&dir, &fingerprint('f')).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{ not json");

        let mut notes = Annotations::new(&fingerprint('f')).unwrap();
        notes
            .add_highlight(
                span("a", 0, 1),
                HighlightColor::Yellow,
                "1".into(),
                "x".into(),
            )
            .unwrap();
        let mut record = serde_json::to_value(FileRecord {
            version: VERSION,
            fingerprint: notes.fingerprint.clone(),
            next_id: notes.next_id,
            bookmarks: notes.bookmarks.clone(),
            highlights: notes.highlights.clone(),
        })
        .unwrap();
        record["version"] = 99.into();
        fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(
            load_from(&dir, &fingerprint('f'))
                .unwrap_err()
                .contains("version")
        );
        // A record copied from another book is rejected.
        record["version"] = VERSION.into();
        record["fingerprint"] = fingerprint('9').into();
        fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(
            load_from(&dir, &fingerprint('f'))
                .unwrap_err()
                .contains("another book")
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn duplicate_ids_are_rejected_on_load() {
        let dir = folder("duplicate");
        fs::create_dir_all(&dir).unwrap();
        let mut notes = Annotations::new(&fingerprint('1')).unwrap();
        notes
            .add_bookmark(
                BookmarkPlace::Pdf {
                    page: 0,
                    within: 0.0,
                },
                "1".into(),
                String::new(),
            )
            .unwrap();
        notes
            .add_highlight(
                Place::Pdf {
                    from: PdfPoint { page: 0, index: 1 },
                    to: PdfPoint { page: 0, index: 4 },
                },
                HighlightColor::Blue,
                "1".into(),
                "text".into(),
            )
            .unwrap();
        notes.highlights[0].id = notes.bookmarks[0].id;
        assert!(save_to(&dir, &notes).is_err());
        let _ = fs::remove_dir_all(dir);
    }
}
