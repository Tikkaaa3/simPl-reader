//! The normal, fixture-independent local HTML, PDF and EPUB reader.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::{
    book_pages,
    book_style::{self, BookStyle, MINIMAL},
    chrome, find, notes, pdf_reader, shelf, themes, ui,
};
use iced::keyboard::{self, Key, key};
use iced::widget::{column, container, image, row, scrollable, text};
use iced::{Element, Length, Size, Subscription, Task, mouse, window};
use iced_shell::{reader, selection, virtual_reader};
use reader_document::library;
use reader_document::position::{self, EpubReadingPosition, PdfReadingPosition, ReadingPosition};
use reader_document::preferences::{self, Appearance, Preferences, WindowControls};
use reader_document::recent::{self, DocumentKind, Entry};
use reader_document::{BaseDirection, Endpoint, Item};

#[path = "book_map.rs"]
mod book_map;
#[path = "book_zoom.rs"]
mod book_zoom;
#[path = "notes_ui.rs"]
mod notes_ui;
#[path = "profile_ui.rs"]
mod profile_ui;
#[path = "read_aloud.rs"]
mod read_aloud;
#[path = "reading_ui.rs"]
mod reading_ui;
#[path = "word_translation.rs"]
pub(crate) mod word_translation;

#[cfg(test)]
#[path = "golden_atlas.rs"]
mod golden_atlas;
#[cfg(test)]
#[path = "release_audit.rs"]
mod release_audit;

const DEFAULT_FONT_SIZE: f32 = MINIMAL.default_size;
const OVERSCAN: f32 = 600.0;
const CONTENT_ROW_HEIGHT: f32 = 34.0;
const RECENT_ROW_HEIGHT: f32 = 52.0;
const RECENT_ROW_SPACING: f32 = 8.0;

#[cfg(test)]
use reader_layout::book::EpubChapter;
use reader_layout::book::{Book, DisplayImage, display_book, load_epub_chapter};
#[cfg(test)]
use std::sync::OnceLock;
type PdfBookRaster = (u64, u32, Result<HashMap<String, DisplayImage>, String>);

fn load_book(
    path: PathBuf,
    locate: Option<&Entry>,
    carried: Option<&SavedPosition>,
) -> Result<Book, String> {
    let mut document = reader_document::load_html(&path)?;
    let cover = match cache_html_cover(&document) {
        Ok(cover) => cover,
        Err(error) => {
            document
                .warnings
                .push(format!("Cover preview unavailable: {error}"));
            false
        }
    };
    if let Some(old) = locate {
        recent::relocate(
            old,
            &Entry {
                path: document.path.clone(),
                title: document.title.clone(),
                fingerprint: document.fingerprint.clone(),
                kind: DocumentKind::Html,
            },
        )?;
    }
    if let Some(position) = carried {
        position.save_at(&document.path)?;
    }
    let restored = match position::load(&document.path) {
        Ok(position) => position.filter(|position| position.fingerprint == document.fingerprint),
        Err(error) => {
            document
                .warnings
                .push(format!("Could not restore the reading position: {error}"));
            None
        }
    };
    let mut book = display_book(document, restored, None);
    book.cover = cover;
    Ok(book)
}

fn load_epub(
    path: PathBuf,
    locate: Option<&Entry>,
    carried: Option<&SavedPosition>,
) -> Result<Book, String> {
    let document = Arc::new(reader_document::epub::open(&path)?);
    if let Some(old) = locate
        && (old.kind != DocumentKind::Epub
            || !old.fingerprint.eq_ignore_ascii_case(&document.fingerprint))
    {
        return Err(
            "Cannot locate document: selected EPUB does not match the saved fingerprint".into(),
        );
    }
    let mut warnings = Vec::new();
    let current = match carried {
        Some(SavedPosition::Epub(_, position)) => Some(position),
        _ => None,
    };
    let stored = if current.is_some() {
        None
    } else {
        let old_position = locate
            .map(recent::saved_epub_for_relocation)
            .transpose()?
            .flatten();
        match old_position.map_or_else(
            || position::load_epub(&document.path),
            |position| Ok(Some(position)),
        ) {
            Ok(position) => position,
            Err(error) => {
                warnings.push(format!("Could not restore the reading position: {error}"));
                None
            }
        }
    };
    let restored = current
        .or(stored.as_ref())
        .filter(|position| position.fingerprint == document.fingerprint);
    let index = restored.and_then(|position| document.section_index(&position.chapter));
    let restored = index.and(restored).map(|position| ReadingPosition {
        fingerprint: position.fingerprint.clone(),
        item_id: position.item_id.clone(),
        within: position.within,
        font_size: position.font_size,
    });
    let mut book = load_epub_chapter(document.clone(), index.unwrap_or(0), restored)?;
    if let Some(old) = locate {
        recent::relocate(
            old,
            &Entry {
                path: document.path.clone(),
                title: document.title.clone(),
                fingerprint: document.fingerprint.clone(),
                kind: DocumentKind::Epub,
            },
        )?;
    }
    if let Some(position) = carried {
        position.save_at(&document.path)?;
    }
    book.cover = match cache_epub_cover(&document) {
        Ok(cover) => cover,
        Err(error) => {
            warnings.push(format!("Cover preview unavailable: {error}"));
            false
        }
    };
    book.warnings.extend(warnings);
    Ok(book)
}

#[derive(Clone, Debug)]
enum LoadedDocument {
    Reflow(Arc<Book>),
    Pdf {
        document: Arc<reader_pdf::Document>,
        restored: Option<PdfReadingPosition>,
        warnings: Vec<String>,
    },
}

#[derive(Clone, Debug)]
struct LoadReply {
    document: LoadedDocument,
    catalog: Option<library::Entry>,
}

enum SavedPosition {
    Html(PathBuf, ReadingPosition),
    PdfBook(PathBuf, ReadingPosition),
    Pdf(PathBuf, PdfReadingPosition),
    Epub(PathBuf, EpubReadingPosition),
}

impl SavedPosition {
    fn save(&self) -> Result<(), String> {
        match self {
            Self::Html(path, _)
            | Self::Pdf(path, _)
            | Self::PdfBook(path, _)
            | Self::Epub(path, _) => self.save_at(path),
        }
    }

    fn save_at(&self, path: &Path) -> Result<(), String> {
        match self {
            Self::Html(_, position) => position::save(path, position),
            Self::Pdf(_, position) => {
                position::save_pdf(path, position)?;
                position::save_pdf_mode(path, &position.fingerprint, position::PdfMode::Document)
            }
            Self::PdfBook(_, position) => {
                position::save_pdf_book(path, position)?;
                position::save_pdf_mode(path, &position.fingerprint, position::PdfMode::Book)
            }
            Self::Epub(_, position) => position::save_epub(path, position),
        }
    }

    fn matches_entry(&self, old: &Entry) -> bool {
        match self {
            Self::Html(path, position) => {
                old.kind == DocumentKind::Html
                    && *path == old.path
                    && position.fingerprint.eq_ignore_ascii_case(&old.fingerprint)
            }
            Self::Pdf(path, position) => {
                old.kind == DocumentKind::Pdf
                    && *path == old.path
                    && position.fingerprint.eq_ignore_ascii_case(&old.fingerprint)
            }
            Self::PdfBook(path, position) => {
                old.kind == DocumentKind::Pdf
                    && *path == old.path
                    && position.fingerprint.eq_ignore_ascii_case(&old.fingerprint)
            }
            Self::Epub(path, position) => {
                old.kind == DocumentKind::Epub
                    && *path == old.path
                    && position.fingerprint.eq_ignore_ascii_case(&old.fingerprint)
            }
        }
    }
}

fn document_kind(path: &Path) -> Result<DocumentKind, String> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("html" | "htm" | "xhtml") => Ok(DocumentKind::Html),
        Some("pdf") => Ok(DocumentKind::Pdf),
        Some("epub") => Ok(DocumentKind::Epub),
        _ => Err(format!(
            "Unsupported document format: {}. Open an HTML, XHTML, PDF, EPUB, text or Markdown file.",
            path.display()
        )),
    }
}

fn catalog_entry(document: Entry, author: Option<String>, cover: bool) -> library::Entry {
    let byte_len = document
        .path
        .metadata()
        .map_or(0, |metadata| metadata.len());
    library::Entry {
        source_kind: reader_document::managed::source_kind(&document.path),
        document,
        author,
        byte_len,
        opened_at: shelf::now(),
        progress: 0.0,
        current: 0,
        total: 0,
        cover,
        favourite: false,
    }
}

fn cache_html_cover(document: &reader_document::Document) -> Result<bool, String> {
    if library::cached_cover(&document.fingerprint)?.is_some() {
        return Ok(true);
    }
    let asset = document.items.iter().find_map(|item| match item {
        Item::Image { asset_path, .. } => document.images.get(asset_path),
        _ => None,
    });
    if let Some(asset) = asset {
        library::cache_cover(&document.fingerprint, asset)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn cache_epub_cover(document: &reader_document::epub::Epub) -> Result<bool, String> {
    if library::cached_cover(&document.fingerprint)?.is_some() {
        return Ok(true);
    }
    if let Some(asset) = document.cover()? {
        library::cache_cover(&document.fingerprint, &asset)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

async fn cache_pdf_cover(document: &reader_pdf::Document) -> Result<bool, String> {
    if library::cached_cover(&document.fingerprint)?.is_some() {
        return Ok(true);
    }
    // Keep enough detail for the 240px-wide 5:7 center crop on landscape pages.
    let width = document.pages.first().map_or(240, |page| {
        (336.0 * page.width / page.height)
            .clamp(240.0, 1024.0)
            .ceil() as u32
    });
    let page = document.session.render(0, width).await?;
    library::cache_cover(
        &document.fingerprint,
        &reader_document::ImageAsset {
            width: page.width,
            height: page.height,
            rgba: page.rgba,
        },
    )?;
    Ok(true)
}

impl LoadedDocument {
    fn recent_entry(&self) -> Entry {
        match self {
            Self::Reflow(book) => Entry {
                path: book.path.clone(),
                title: book.title.clone(),
                fingerprint: book.fingerprint.clone(),
                kind: if book.pdf_source.is_some() {
                    DocumentKind::Pdf
                } else if book.epub.is_some() {
                    DocumentKind::Epub
                } else {
                    DocumentKind::Html
                },
            },
            Self::Pdf { document, .. } => Entry {
                path: document.path.clone(),
                title: document.title.clone(),
                fingerprint: document.fingerprint.clone(),
                kind: DocumentKind::Pdf,
            },
        }
    }
}

async fn load_document(
    path: PathBuf,
    previous: Option<SavedPosition>,
    locate: Option<Entry>,
) -> Result<LoadReply, String> {
    let kind = document_kind(&path)?;
    if let Some(old) = &locate
        && kind != old.kind
    {
        return Err(format!(
            "Selected file is not the same format as {}.",
            old.path.display()
        ));
    }
    let carried = previous.as_ref().filter(|position| {
        locate
            .as_ref()
            .is_some_and(|old| position.matches_entry(old))
    });
    let save_warning = if carried.is_none() {
        previous.as_ref().and_then(|position| position.save().err())
    } else {
        None
    };
    if kind == DocumentKind::Pdf {
        let document = reader_pdf::open(path).await?;
        if let Some(old) = &locate {
            recent::relocate(
                old,
                &Entry {
                    path: document.path.clone(),
                    title: document.title.clone(),
                    fingerprint: document.fingerprint.clone(),
                    kind,
                },
            )?;
        }
        if let Some(position) = carried {
            position.save_at(&document.path)?;
        }
        let mut warnings = Vec::new();
        if let Some(error) = save_warning {
            warnings.push(format!(
                "Could not save the previous reading position: {error}"
            ));
        }
        let restored = match position::load_pdf(&document.path) {
            Ok(position) => position.filter(|position| {
                position.fingerprint == document.fingerprint
                    && (position.page as usize) < document.pages.len()
            }),
            Err(error) => {
                warnings.push(format!("Could not restore the reading position: {error}"));
                None
            }
        };
        let cover = match cache_pdf_cover(&document).await {
            Ok(cover) => cover,
            Err(error) => {
                warnings.push(format!("Cover preview unavailable: {error}"));
                false
            }
        };
        let catalog = Some(catalog_entry(
            Entry {
                path: document.path.clone(),
                title: document.title.clone(),
                fingerprint: document.fingerprint.clone(),
                kind: DocumentKind::Pdf,
            },
            document.author.clone(),
            cover,
        ));
        match position::load_pdf_mode(&document.path, &document.fingerprint) {
            Ok(position::PdfMode::Book) => {
                let origin = restored
                    .clone()
                    .unwrap_or_else(|| default_pdf_position(&document));
                match load_pdf_book(document.clone(), origin, DEFAULT_FONT_SIZE, true).await {
                    Ok(book) => {
                        return Ok(LoadReply {
                            document: LoadedDocument::Reflow(Arc::new(book)),
                            catalog,
                        });
                    }
                    Err(error) => warnings.push(format!(
                        "Book mode unavailable; showing the original PDF: {error}"
                    )),
                }
            }
            Err(error) => warnings.push(format!("Could not restore PDF mode: {error}")),
            _ => {}
        }
        Ok(LoadReply {
            document: LoadedDocument::Pdf {
                document,
                restored,
                warnings,
            },
            catalog,
        })
    } else {
        let mut book = if kind == DocumentKind::Epub {
            load_epub(path, locate.as_ref(), carried)?
        } else {
            load_book(path, locate.as_ref(), carried)?
        };
        if let Some(error) = save_warning {
            book.warnings.push(format!(
                "Could not save the previous reading position: {error}"
            ));
        }
        let catalog = Some(catalog_entry(
            Entry {
                path: book.path.clone(),
                title: book.title.clone(),
                fingerprint: book.fingerprint.clone(),
                kind,
            },
            book.author.take(),
            book.cover,
        ));
        Ok(LoadReply {
            document: LoadedDocument::Reflow(Arc::new(book)),
            catalog,
        })
    }
}

fn default_pdf_position(document: &reader_pdf::Document) -> PdfReadingPosition {
    PdfReadingPosition {
        fingerprint: document.fingerprint.clone(),
        page: 0,
        within: 0.0,
        horizontal: 0.0,
        zoom: position::PdfZoom::FitWidth,
    }
}

fn pdf_source_range(
    conversion: &reader_pdf::book::Conversion,
    anchor: Anchor,
) -> Option<&reader_pdf::book::SourceRange> {
    let block = conversion.blocks.get(anchor.row)?;
    let index = ((block.sources.len() as f32 * anchor.fraction + 0.0001) as usize)
        .min(block.sources.len().saturating_sub(1));
    block.sources.get(index)
}
fn pdf_book_anchor(
    conversion: &reader_pdf::book::Conversion,
    position: &PdfReadingPosition,
) -> Anchor {
    for (row, block) in conversion.blocks.iter().enumerate() {
        if let Some(index) = block.sources.iter().position(|source| {
            source.page > position.page
                || (source.page == position.page && source.bottom >= position.within)
        }) {
            return Anchor {
                row,
                fraction: index as f32 / block.sources.len().max(1) as f32,
            };
        }
    }
    Anchor {
        row: conversion.blocks.len().saturating_sub(1),
        fraction: 0.0,
    }
}

async fn load_pdf_book(
    document: Arc<reader_pdf::Document>,
    original_position: PdfReadingPosition,
    font_size: f32,
    resume_saved: bool,
) -> Result<Book, String> {
    let conversion = document.session.book().await?;
    let mut warnings = conversion.warnings.clone();
    let stored = match position::load_pdf_book(&document.path) {
        Ok(position) => position
            .filter(|p| p.fingerprint == document.fingerprint)
            .and_then(|mut p| {
                if conversion.blocks.iter().any(|b| b.id == p.item_id) {
                    return Some(p);
                }
                // Old converter anchors still identify their original source page,
                // even when that page is now preserved as an image.
                let suffix = p
                    .item_id
                    .strip_prefix("pdf-b")?
                    .split_once('-')
                    .filter(|(version, _)| version.bytes().all(|byte| byte.is_ascii_digit()))?
                    .1;
                let migrated = format!("pdf-b{}-{suffix}", reader_pdf::book::VERSION);
                if conversion.blocks.iter().any(|b| b.id == migrated) {
                    p.item_id = migrated;
                } else {
                    let page = suffix
                        .strip_prefix('p')?
                        .split('-')
                        .next()?
                        .parse::<u32>()
                        .ok()?;
                    p.item_id = conversion
                        .blocks
                        .iter()
                        .find(|b| b.sources.first().is_some_and(|s| s.page == page))?
                        .id
                        .clone();
                    p.within = 0.0;
                }
                Some(p)
            }),
        Err(error) => {
            warnings.push(format!("Could not restore Book position: {error}"));
            None
        }
    };
    let mut target = original_position.clone();
    if !resume_saved {
        target.within = 0.0;
    }
    let anchor = pdf_book_anchor(&conversion, &target);
    let restored = stored.filter(|_| resume_saved).or_else(|| {
        conversion
            .blocks
            .get(anchor.row)
            .map(|block| ReadingPosition {
                fingerprint: document.fingerprint.clone(),
                item_id: block.id.clone(),
                within: anchor.fraction,
                font_size,
            })
    });
    reader_layout::book::pdf_book(document, conversion, original_position, restored, warnings)
}

fn forward_pdf(document: u64, task: Task<pdf_reader::Message>) -> Task<Message> {
    task.map(move |message| Message::Pdf { document, message })
}

#[derive(Clone, Copy, Debug)]
struct Anchor {
    row: usize,
    fraction: f32,
}

#[derive(Clone, Debug)]
struct ReturnLocation {
    chapter: Option<usize>,
    item_id: String,
    within: f32,
}
#[derive(Clone, Debug)]
enum Navigation {
    Preserve,
    End,
    Push(ReturnLocation),
    Pop,
}

/// A section laid out for the chosen reading theme: its own row heights, and the
/// same page boundaries (and so the same page numbers) as the default layout.
#[derive(Debug)]
struct AdaptedLayout {
    fingerprint: String,
    section: usize,
    theme: &'static str,
    layout: book_map::Section,
}

/// Sections kept laid out for the current theme, so turning back a chapter is instant.
const ADAPTED_SECTIONS: usize = 6;

/// The open find bar and its results, in reading order.
#[derive(Debug, Default)]
struct Find {
    query: String,
    matches: Vec<find::Match>,
    /// Matches when a PDF is open; `matches` is used for books.
    pdf_matches: Vec<find::PdfMatch>,
    current: Option<usize>,
    /// The text of the rest of the book is still being read.
    indexing: bool,
    searching: bool,
    /// Why search cannot cover everything, if it cannot.
    notice: Option<String>,
}

impl Find {
    fn count(&self) -> usize {
        self.matches.len() + self.pdf_matches.len()
    }
}

/// Page text of an open PDF, in glyph order, kept while it stays open.
#[derive(Clone, Debug)]
struct PdfFindIndex {
    document: u64,
    pages: Arc<Vec<String>>,
    /// Reading stopped early to bound memory; only these pages are searched.
    truncated: bool,
}

/// Bounds the retained EPUB/PDF search text (item and index metadata are additional).
const MAX_FIND_INDEX_BYTES: usize = 64 * 1024 * 1024;
const ASYNC_FIND_BYTES: usize = 64 * 1024;

/// Text of every EPUB chapter, kept while the book stays open so search spans the book.
#[derive(Clone, Debug)]
struct FindIndex {
    fingerprint: String,
    chapters: Arc<Vec<Vec<Item>>>,
    truncated: bool,
}

#[derive(Debug)]
struct FindJob {
    _handle: iced::task::Handle,
    cancel: Arc<AtomicBool>,
}

impl Drop for FindJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

enum SearchCorpus {
    Book(Arc<Book>),
    Chapters(Arc<Vec<Vec<Item>>>),
    Pdf(Arc<Vec<String>>),
}

#[derive(Clone, Debug)]
enum SearchOrigin {
    Book(Option<usize>, usize, usize),
    Pdf(u32, usize),
}

#[derive(Clone, Debug)]
struct SearchResults {
    origin: SearchOrigin,
    matches: Vec<find::Match>,
    pdf_matches: Vec<find::PdfMatch>,
}

impl SearchCorpus {
    fn is_large(&self) -> bool {
        let mut bytes = 0;
        let mut large = |size| {
            bytes += size;
            bytes >= ASYNC_FIND_BYTES
        };
        match self {
            Self::Book(book) => book
                .items
                .iter()
                .filter_map(Item::text)
                .any(|text| large(text.len())),
            Self::Chapters(chapters) => chapters
                .iter()
                .flatten()
                .filter_map(Item::text)
                .any(|text| large(text.len())),
            Self::Pdf(pages) => pages.iter().any(|text| large(text.len())),
        }
    }

    fn search(&self, needle: &[char], origin: SearchOrigin, cancel: &AtomicBool) -> SearchResults {
        let mut results = SearchResults {
            origin,
            matches: Vec::new(),
            pdf_matches: Vec::new(),
        };
        match self {
            Self::Book(book) => find::search_items_cancellable(
                &book.items,
                book.epub.as_ref().map(|chapter| chapter.index),
                needle,
                &mut results.matches,
                cancel,
            ),
            Self::Chapters(chapters) => {
                for (chapter, items) in chapters.iter().enumerate() {
                    if results.matches.len() >= find::MAX_MATCHES || cancel.load(Ordering::Relaxed)
                    {
                        break;
                    }
                    find::search_items_cancellable(
                        items,
                        Some(chapter),
                        needle,
                        &mut results.matches,
                        cancel,
                    );
                }
            }
            Self::Pdf(pages) => {
                find::search_pages_cancellable(pages, needle, &mut results.pdf_matches, cancel)
            }
        }
        results
    }
}

#[derive(Debug)]
struct Reader {
    book: Option<Arc<Book>>,
    theme: &'static themes::ReadingTheme,
    adapted: Vec<AdaptedLayout>,
    find: Option<Find>,
    find_index: Option<FindIndex>,
    pdf_find_index: Option<PdfFindIndex>,
    find_task: Option<FindJob>,
    find_generation: u64,
    query_task: Option<FindJob>,
    query_generation: u64,
    pdf_book_raster: Option<PdfBookRaster>,
    pdf_book_pending: Option<(u64, u32)>,
    returns: Vec<ReturnLocation>,
    pending_navigation: Option<Navigation>,
    show_conversion: bool,
    removing: Option<PathBuf>,
    confirm_remove: Option<Entry>,
    pdf: Option<pdf_reader::Reader>,
    pdf_warnings: Vec<String>,
    window_size: Size,
    scale_factor: f64,
    opening_task: Option<iced::task::Handle>,
    opening: Option<PathBuf>,
    loading_frame: u8,
    recent: Vec<Entry>,
    shelf: shelf::Shelf,
    show_search: bool,
    search_query: String,
    search_results: Vec<usize>,
    search_selected: usize,
    reading: reading_ui::State,
    fit_width: bool,
    fit_previous_zoom: f32,
    fullscreen: bool,
    before_fullscreen_maximized: bool,
    profile: profile_ui::State,
    show_settings: bool,
    dropping: bool,
    recent_loading: bool,
    recent_pending: Vec<(Entry, Option<Entry>)>,
    recent_saving: bool,
    recent_dirty: bool,
    history_corrupt: bool,
    pending_exit: bool,
    history_notice: Option<String>,
    recent_open: Option<Entry>,
    missing_recent: Option<Entry>,
    dialog_locate: Option<Entry>,
    pending_locate: Option<Entry>,
    show_recent: bool,
    toolbar_expanded: bool,
    show_help: bool,
    focused: Option<Control>,
    focus_generation: u64,
    focus_pending: bool,
    error: Option<String>,
    dialog_open: bool,
    show_contents: bool,
    contents_offset: f32,
    request: u64,
    generation: u64,
    appearance: Appearance,
    window_controls: WindowControls,
    maximized: bool,
    preferences_loading: bool,
    preferences_dirty: bool,
    preferences_saving: bool,
    preferences_writable: bool,
    preferences_notice: Option<String>,
    font_size: f32,
    zoom: f32,
    atlas: Option<Arc<book_map::Atlas>>,
    page_input: Option<String>,
    pending_page: Option<(String, usize, usize)>,
    width: f32,
    viewport: f32,
    offset: f32,
    heights: virtual_reader::HeightIndex,
    measurements: virtual_reader::Measurements,
    pending_anchor: Option<Anchor>,
    pagination: Option<Arc<AtomicBool>>,
    pagination_end: bool,
    measured_layout: Option<(f32, f32)>,
    selection: selection::SelectionState,
    notes: notes::Notes,
    read_aloud: read_aloud::ReadAloud,
    word_translation: word_translation::WordTranslation,
    /// Installed voices, read when the settings panel opens.
    voice_choices: Vec<read_aloud::VoiceChoice>,
    /// Last known pointer position in window coordinates.
    pointer: iced::Point,
    saving: bool,
    failed_close: Option<CloseAction>,
    pending_notes_close: Option<CloseAction>,
    window: Option<window::Id>,
}

impl Default for Reader {
    fn default() -> Self {
        Self {
            book: None,
            theme: themes::default_theme(),
            adapted: Vec::new(),
            find: None,
            find_index: None,
            pdf_find_index: None,
            find_task: None,
            find_generation: 0,
            query_task: None,
            query_generation: 0,
            pdf_book_raster: None,
            pdf_book_pending: None,
            returns: Vec::new(),
            pending_navigation: None,
            show_conversion: false,
            removing: None,
            confirm_remove: None,
            pdf: None,
            pdf_warnings: Vec::new(),
            window_size: Size::new(1280.0, 800.0),
            scale_factor: 1.0,
            opening_task: None,
            opening: None,
            loading_frame: 0,
            recent: Vec::new(),
            shelf: shelf::Shelf::default(),
            show_search: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_selected: 0,
            reading: Default::default(),
            fit_width: false,
            fit_previous_zoom: 1.0,
            fullscreen: false,
            before_fullscreen_maximized: false,
            profile: Default::default(),
            show_settings: false,
            dropping: false,
            recent_loading: true,
            recent_pending: Vec::new(),
            recent_saving: false,
            recent_dirty: false,
            history_corrupt: false,
            pending_exit: false,
            history_notice: None,
            recent_open: None,
            missing_recent: None,
            dialog_locate: None,
            pending_locate: None,
            show_recent: false,
            show_help: false,
            toolbar_expanded: true,
            focused: None,
            focus_generation: 0,
            focus_pending: false,
            error: None,
            dialog_open: false,
            show_contents: false,
            contents_offset: 0.0,
            request: 0,
            generation: 0,
            appearance: Appearance::Light,
            window_controls: WindowControls::default(),
            maximized: false,
            preferences_loading: false,
            preferences_dirty: false,
            preferences_saving: false,
            preferences_writable: true,
            preferences_notice: None,
            font_size: DEFAULT_FONT_SIZE,
            zoom: 1.0,
            atlas: None,
            page_input: None,
            pending_page: None,
            width: MINIMAL.width(1280.0, DEFAULT_FONT_SIZE),
            viewport: 620.0,
            offset: 0.0,
            heights: virtual_reader::HeightIndex::new(Vec::new()),
            measurements: Default::default(),
            pending_anchor: None,
            pagination: None,
            pagination_end: false,
            measured_layout: None,
            selection: Default::default(),
            notes: Default::default(),
            read_aloud: Default::default(),
            word_translation: Default::default(),
            voice_choices: Vec::new(),
            pointer: iced::Point::ORIGIN,
            saving: false,
            failed_close: None,
            pending_notes_close: None,
            window: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum CloseAction {
    Document,
    Window,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Control {
    Reading(reading_ui::Focus),
    FitBookWidth,
    Fullscreen,
    Profile(profile_ui::Focus),
    Dictionary(word_translation::Focus),
    Chrome(chrome::Action),
    Shelf(shelf::Control),
    SearchInput,
    SearchResult(usize),
    LocateLibrary(usize),
    RemoveLibrary(usize),
    ConfirmRemove,
    DismissOverlay,
    SettingsFontDown,
    SettingsFontUp,
    SettingsWindowControls(WindowControls),
    SettingsHelp,
    SettingsTheme(usize),
    Close,
    ToggleToolbar,
    FontDown,
    FontUp,
    ToggleAppearance,
    Back,
    BookMode,
    DocumentMode,
    BookLink(usize, usize),
    Pdf(pdf_reader::FocusControl),
    BookPageInput,
    PreviousBookPage,
    NextBookPage,
    Contents,
    ContentsEntry(usize),
    Find,
    FindInput,
    FindPrevious,
    FindNext,
    FindClose,
    DismissError,
    LocateMissing,
    CloseWithoutSaving,
    ResetRecent,
    HideRecent,
    OpenRecent(usize),
    LocateRecent(usize),
    RemoveRecent(usize),
    HideHelp,
    Notes(notes::Focus),
    ReadAloud,
    ReadAloudPause,
    ReadAloudSlower,
    ReadAloudFaster,
}

#[derive(Clone, Debug)]
enum Message {
    Reading(reading_ui::Action),
    ReadingLoaded {
        profile: u64,
        book: String,
        result: Result<Option<reader_document::reading::Options>, String>,
    },
    ReadingSaved(Result<(String, Option<reader_document::reading::Options>), String>),
    FitBookWidth,
    ToggleFullscreen,
    Profile(profile_ui::Action),
    BackupEstimate {
        generation: u64,
        result: Result<reader_document::backup::Summary, String>,
    },
    ProfileReply(Result<profile_ui::Reply, String>),
    PdfBookRaster {
        document: u64,
        page: u32,
        result: Result<Arc<reader_pdf::RenderedPage>, String>,
    },
    Chrome(chrome::Action),
    Shelf(shelf::Message),
    SearchChanged(String),
    SearchSubmit,
    OpenLibrary(usize),
    LocateLibrary(usize),
    RemoveLibrary(usize),
    ConfirmRemove,
    RemovedLibrary {
        path: PathBuf,
        result: Result<(), String>,
    },
    DismissOverlay,
    OpenDialog,
    RecentLoaded(Result<Vec<Entry>, String>),
    RecentSaved(Result<(), String>),
    ToggleRecent,
    ToggleToolbar,
    ToggleHelp,
    FocusReady(u64),
    OpenRecent(usize),
    LocateRecent(usize),
    LocateMissing,
    RemoveRecent(usize),
    ResetRecent,
    DismissError,
    DialogChosen(Result<Option<PathBuf>, String>),
    Loaded {
        request: u64,
        result: Result<LoadReply, String>,
    },
    LoadingFrame(u8),
    Pdf {
        document: u64,
        message: pdf_reader::Message,
    },
    EpubChapter {
        index: usize,
        fragment: Option<String>,
    },
    ToggleContents,
    ContentsScrolled(f32),
    ScaleFactor(f64),
    Zoom(f32),
    /// Ctrl+wheel or touchpad pinch over the page, in wheel notches.
    ZoomBy(f32),
    PageInput(String),
    PageSubmit,
    FindOpen,
    FindChanged(String),
    FindStep(bool),
    FindClose,
    FindIndexed {
        generation: u64,
        result: Result<FindIndex, String>,
    },
    PdfFindIndexed {
        generation: u64,
        result: Result<PdfFindIndex, String>,
    },
    FindSearched {
        generation: u64,
        results: SearchResults,
    },
    AtlasReady {
        generation: u64,
        result: Result<Option<book_map::Atlas>, String>,
    },
    SetTheme(usize),
    ThemeLayoutReady {
        generation: u64,
        section: usize,
        theme: &'static str,
        result: Result<Option<book_map::Section>, String>,
    },
    ToggleAppearance,
    PreferencesLoaded(Result<Preferences, String>),
    SetWindowControls(WindowControls),
    WindowMaximized(bool),
    PreferencesSaved(Result<(), String>),
    BookMode,
    CancelConversion,
    DocumentMode {
        source_page: bool,
    },
    BookPage(bool),
    FollowLink(String),
    Back,
    SelectStart(Endpoint),
    SelectMove(Endpoint),
    /// Right click on text of a reflowed book.
    Context(Endpoint),
    /// Right click on the page but not on text.
    ContextBlank,
    Notes(notes::Action),
    NotesLoaded {
        profile: u64,
        fingerprint: String,
        result: Result<reader_document::annotations::Annotations, String>,
    },
    NotesSaved(Result<(), String>),
    ReadAloud(read_aloud::Action),
    Dictionary(word_translation::Action),
    DictionaryReady {
        generation: u64,
        result: Result<
            Option<reader_document::dictionary::Translation>,
            reader_document::dictionary::LookupError,
        >,
    },
    DictionaryInventory {
        generation: u64,
        states: Vec<reader_document::dictionary::PackageState>,
    },
    DictionaryProgress {
        generation: u64,
        bytes: u64,
    },
    DictionaryPackageReady {
        generation: u64,
        result: Result<Option<reader_document::dictionary::PackageId>, String>,
    },
    DictionaryText {
        document: u64,
        generation: u64,
        result: Result<String, String>,
    },
    ReadAloudTick,
    ReadAloudLine {
        generation: u64,
        cursor: read_aloud::BookCursor,
        page_top: f32,
        line: Option<iced::Rectangle>,
    },
    /// The text of a PDF selection to read aloud.
    ReadAloudText {
        document: u64,
        generation: u64,
        result: Result<String, String>,
    },
    /// The text of a PDF page to read aloud.
    ReadAloudPdfPage {
        document: u64,
        generation: u64,
        page: usize,
        result: Result<String, String>,
    },
    /// The text of a PDF selection, read so it can be saved with its highlight.
    PdfHighlightText {
        document: u64,
        from: reader_document::annotations::PdfPoint,
        to: reader_document::annotations::PdfPoint,
        merge_ids: Vec<u64>,
        color: reader_document::annotations::HighlightColor,
        then_note: bool,
        result: Result<String, String>,
    },
    Copy,
    LayoutReady(u64),
    Scroll {
        generation: u64,
        offset: f32,
        viewport: f32,
        page_top: f32,
    },
    Close(CloseAction),
    Saved {
        action: CloseAction,
        result: Result<(), String>,
    },
    CloseWithoutSaving,
    Event(iced::Event, window::Id),
}

fn image_size(image: &DisplayImage, width: f32) -> Size {
    let scale = (width / image.width.max(1) as f32).min(1.0);
    Size::new(image.width as f32 * scale, image.height as f32 * scale)
}

impl Reader {
    fn interactive(&self) -> bool {
        self.opening.is_none()
            && !self.saving
            && !self.dialog_open
            && self.removing.is_none()
            && !self.profile.busy
    }

    fn request_pdf_book_raster(&mut self) -> Task<Message> {
        let Some(source) = self.book.as_ref().and_then(|b| b.pdf_source.as_ref()) else {
            self.pdf_book_raster = None;
            self.pdf_book_pending = None;
            return Task::none();
        };
        let Some(page) = self.active_page().map(|p| p.number) else {
            return Task::none();
        };
        if !source.illustration_pages.contains(&page) {
            self.pdf_book_raster = None;
            self.pdf_book_pending = None;
            return Task::none();
        }
        let key = (source.document.id, page);
        if self.pdf_book_pending == Some(key)
            || self
                .pdf_book_raster
                .as_ref()
                .is_some_and(|(doc, p, _)| (*doc, *p) == key)
        {
            return Task::none();
        }
        self.pdf_book_raster = None;
        self.pdf_book_pending = Some(key);
        let session = source.document.session.clone();
        Task::perform(
            async move { session.render(page, 1600).await.map(Arc::new) },
            move |result| Message::PdfBookRaster {
                document: key.0,
                page,
                result,
            },
        )
    }

    fn contents(&self) -> &[reader_document::epub::TocEntry] {
        let Some(book) = &self.book else {
            return &[];
        };
        if let Some(chapter) = &book.epub {
            return &chapter.document.contents;
        }
        if book.pdf_source.is_some() {
            return &[];
        }
        book.contents.get_or_init(|| {
            book.items
                .iter()
                .enumerate()
                .filter_map(|(row, item)| match item {
                    Item::Heading { text, level, .. } if *level <= 2 => {
                        Some(reader_document::epub::TocEntry {
                            label: text.clone(),
                            chapter: row,
                            fragment: None,
                            depth: level.saturating_sub(1) as usize,
                        })
                    }
                    _ => None,
                })
                .collect()
        })
    }

    fn reading_title(&self) -> Option<String> {
        if let Some(pdf) = &self.pdf {
            return Some(pdf.document().title.clone());
        }
        let book = self.book.as_ref()?;
        let section = if let Some(chapter) = &book.epub {
            chapter
                .document
                .section(chapter.index)
                .map(|c| c.title.clone())
        } else {
            let row = self.anchor().row;
            self.contents()
                .iter()
                .rev()
                .find(|c| c.chapter <= row)
                .map(|c| c.label.clone())
        };
        Some(match section.filter(|s| s != &book.title) {
            Some(section) => format!("{} ~ {section}", book.title),
            None => book.title.clone(),
        })
    }

    fn pages(&self) -> &[book_pages::Page] {
        let Some(book) = &self.book else {
            return &[];
        };
        if self.heights.len() != book.items.len() {
            return &[];
        }
        self.section_layout()
            .map_or(&[], |section| section.pages.as_slice())
    }

    /// Whether this book is laid out for the theme rather than with the default
    /// measurements. PDF Book keeps the default typography (its colors still follow the theme).
    fn line_height(&self) -> f32 {
        self.book.as_ref().map_or(MINIMAL.line_height, |book| {
            self.reading_style(book).line_height
        })
    }

    fn needs_adaptation(&self) -> bool {
        (self.theme.id != themes::DEFAULT_ID
            || self.reading_options() != reader_document::reading::Options::default())
            && self
                .book
                .as_ref()
                .is_some_and(|book| book.pdf_source.is_none())
    }

    /// Row heights and pages of the current section for the current theme, if ready.
    fn section_layout(&self) -> Option<&book_map::Section> {
        let book = self.book.as_ref()?;
        let section = book.epub.as_ref().map_or(0, |c| c.index);
        if self.needs_adaptation() {
            return self
                .adapted
                .iter()
                .find(|a| {
                    a.fingerprint == book.fingerprint
                        && a.section == section
                        && a.theme == self.theme.id
                })
                .map(|a| &a.layout);
        }
        self.atlas
            .as_ref()
            .filter(|a| a.fingerprint == book.fingerprint)
            .and_then(|a| a.section(section))
    }

    /// Switches the reading theme. Pages keep their numbers and boundaries; only the
    /// look and the way each page is filled change, so the reading place is kept.
    fn apply_theme(&mut self, theme: &'static themes::ReadingTheme) -> Task<Message> {
        if self.theme.id == theme.id {
            return Task::none();
        }
        let relayout = self.switch_theme(theme);
        self.preferences_dirty = true;
        Task::batch([self.persist_preferences(), relayout])
    }

    /// Changes the theme and lays the open book out for it, without saving anything.
    fn switch_theme(&mut self, theme: &'static themes::ReadingTheme) -> Task<Message> {
        if self.theme.id == theme.id {
            return Task::none();
        }
        let anchor = self.book.is_some().then(|| self.anchor());
        self.theme = theme;
        self.adapted.clear();
        match anchor {
            Some(anchor) if self.pagination.is_none() => self.rebuild_geometry(anchor),
            _ => Task::none(),
        }
    }

    fn reading_width(&self, _window: f32) -> f32 {
        book_map::PAPER - 2.0 * self.reading_options().margin as f32
    }

    fn page_total(&self) -> usize {
        self.atlas.as_ref().map_or(0, |a| a.total)
    }

    fn jump_page(&mut self) -> Task<Message> {
        let Some(value) = self.page_input.take() else {
            return Task::none();
        };
        let Some(atlas) = &self.atlas else {
            return Task::none();
        };
        let Some((section, page)) = atlas.target(&value) else {
            return Task::none();
        };
        self.go_to_atlas_page(section, page)
    }

    fn go_to_local_page(&mut self, page: usize) -> Task<Message> {
        self.word_translation.reset();
        let pages = self.pages();
        let Some(page) = pages.get(page).cloned() else {
            return Task::none();
        };
        self.generation = self.generation.wrapping_add(1);
        self.offset = page.top;
        self.pending_anchor = Some(Anchor {
            row: page.rows.start,
            fraction: (page.content.start - self.heights.start(page.rows.start))
                / self.heights.height(page.rows.start).max(1.0),
        });
        self.selection.end_drag();
        self.restore_book_scroll()
    }

    fn active_page(&self) -> Option<book_pages::Page> {
        book_pages::current(self.pages(), self.offset).cloned()
    }

    fn local_offset(&self) -> f32 {
        self.active_page()
            .map_or(0.0, |p| (self.offset - p.top).max(0.0))
    }

    fn restore_book_scroll(&self) -> Task<Message> {
        scroll_to(self.local_offset() * self.zoom)
    }

    fn can_turn(&self, next: bool) -> bool {
        if self.pagination.is_some() {
            return false;
        }
        let pages = self.pages();
        let Some(page) = book_pages::current(pages, self.offset) else {
            return false;
        };
        if next && pages.last().is_some_and(|p| p.top > page.top)
            || !next && pages.first().is_some_and(|p| p.top < page.top)
        {
            return true;
        }
        self.book
            .as_ref()
            .and_then(|b| b.epub.as_ref())
            .is_some_and(|c| {
                if next {
                    c.index + 1 < c.document.chapters.len()
                } else {
                    c.index > 0 && c.index < c.document.chapters.len()
                }
            })
    }

    fn scroll_extent(&self) -> f32 {
        self.active_page()
            .map_or(0.0, |page| page.top + page.height)
    }

    fn adjacent_book_page(&mut self, next: bool) -> Task<Message> {
        if self.pagination.is_some() || !self.interactive() {
            return Task::none();
        }
        let pages = self.pages();
        let Some(page) = book_pages::current(pages, self.offset) else {
            return Task::none();
        };
        let current = pages.iter().position(|p| p.top == page.top).unwrap_or(0);
        if ((next && current + 1 == pages.len()) || (!next && current == 0))
            && let Some(chapter) = self.book.as_ref().and_then(|b| b.epub.as_ref())
        {
            let index = if next {
                chapter.index.checked_add(1)
            } else {
                chapter.index.checked_sub(1)
            };
            if let Some(index) = index.filter(|i| *i < chapter.document.chapters.len()) {
                return self.navigate(
                    Some(index),
                    None,
                    None,
                    if next {
                        Navigation::Preserve
                    } else {
                        Navigation::End
                    },
                );
            }
        }
        let target = if next {
            (current + 1).min(pages.len() - 1)
        } else {
            current.saturating_sub(1)
        };
        self.go_to_local_page(target)
    }

    fn anchor(&self) -> Anchor {
        if let Some(anchor) = self.pending_anchor {
            return anchor;
        }
        if self.heights.is_empty() {
            return Anchor {
                row: 0,
                fraction: 0.0,
            };
        }
        let offset = book_pages::content_offset(self.pages(), &self.heights, self.offset);
        let row = self.heights.window(offset, 0.0, 0.0).start;
        Anchor {
            row,
            fraction: ((offset - self.heights.start(row)) / self.heights.height(row).max(1.0))
                .clamp(0.0, 1.0),
        }
    }

    fn offset_for(&self, anchor: Anchor) -> f32 {
        if self.heights.is_empty() {
            return 0.0;
        }
        let row = anchor.row.min(self.heights.len() - 1);
        let content = self.heights.start(row) + anchor.fraction * self.heights.height(row);
        let pages = self.pages();
        if let Some(page) = pages
            .iter()
            .find(|p| {
                p.content.is_empty()
                    && (p.top - self.offset).abs() < 0.05
                    && (content - p.content.start).abs() < 0.05
            })
            .or_else(|| {
                pages
                    .iter()
                    .find(|p| content + 0.05 >= p.content.start && content < p.content.end - 0.05)
            })
            .or_else(|| pages.last())
        {
            let inset = if (content - page.content.start).abs() < 0.05 {
                0.0
            } else {
                book_pages::TOP
            };
            (page.top + inset + content - page.content.start)
                .clamp(page.top, page.top + (page.height - self.viewport).max(0.0))
        } else {
            content.min((self.heights.total() - self.viewport).max(0.0))
        }
    }

    fn saved_position(&self) -> Option<(PathBuf, ReadingPosition)> {
        let book = self.book.as_ref()?;
        let anchor = self.anchor();
        let item = book.items.get(anchor.row)?;
        Some((
            book.path.clone(),
            ReadingPosition {
                fingerprint: book.fingerprint.clone(),
                item_id: item.id().to_owned(),
                within: anchor.fraction,
                font_size: self.font_size,
            },
        ))
    }

    fn document_position(&self) -> Option<SavedPosition> {
        if let Some(pdf) = &self.pdf {
            Some(SavedPosition::Pdf(
                pdf.document().path.clone(),
                pdf.position(),
            ))
        } else {
            let (path, position) = self.saved_position()?;
            if let Some(chapter) = self.book.as_ref()?.epub.as_ref() {
                Some(SavedPosition::Epub(
                    path,
                    EpubReadingPosition {
                        fingerprint: position.fingerprint,
                        chapter: chapter.document.section(chapter.index)?.href.clone(),
                        item_id: position.item_id,
                        within: position.within,
                        font_size: position.font_size,
                    },
                ))
            } else if self.book.as_ref()?.pdf_source.is_some() {
                Some(SavedPosition::PdfBook(path, position))
            } else {
                Some(SavedPosition::Html(path, position))
            }
        }
    }

    fn cancel_open(&mut self) {
        self.pending_page = None;
        if let Some(task) = self.opening_task.take() {
            task.abort();
        }
        self.request = self.request.wrapping_add(1);
        self.opening = None;
        self.pending_locate = None;
        self.pending_navigation = None;
        self.recent_open = None;
    }

    fn clear_content(&mut self) {
        self.show_conversion = false;
        self.page_input = None;
        self.book = None;
        self.returns.clear();
        self.pdf = None;
        self.pdf_warnings.clear();
        self.selection.clear();
        self.focused = None;
        if let Some(cancel) = self.pagination.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.pagination_end = false;
        self.measured_layout = None;
        self.heights = virtual_reader::HeightIndex::new(Vec::new());
        self.measurements.lock().clear();
        self.pending_anchor = None;
        self.offset = 0.0;
        self.generation = self.generation.wrapping_add(1);
        self.show_contents = false;
        self.contents_offset = 0.0;
    }

    fn rebuild_geometry(&mut self, anchor: Anchor) -> Task<Message> {
        self.width = self.reading_width(self.window_size.width);
        self.font_size = self.reading_options().size as f32;
        self.generation = self.generation.wrapping_add(1);
        self.measurements.lock().clear();
        if let Some(cancel) = self.pagination.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.pending_anchor = Some(anchor);
        let Some(book) = &self.book else {
            return Task::none();
        };
        let section = book.epub.as_ref().map_or(0, |c| c.index);
        let canonical = self
            .atlas
            .as_ref()
            .filter(|a| a.fingerprint == book.fingerprint)
            .and_then(|a| a.section(section))
            .cloned();
        if let Some(canonical) = canonical {
            if self.needs_adaptation() && self.section_layout().is_none() {
                // The theme measures text differently: lay this section out for it,
                // cutting pages at the same places as the default layout.
                self.measured_layout = None;
                self.heights = virtual_reader::HeightIndex::new(Vec::new());
                let cancel = Arc::new(AtomicBool::new(false));
                self.pagination = Some(cancel.clone());
                let book = book.clone();
                let theme = self.theme;
                let reading = self.reading_options();
                let generation = self.generation;
                return Task::perform(
                    async move {
                        book_map::adapt_section_with(book, &canonical, theme, reading, &cancel)
                    },
                    move |result| Message::ThemeLayoutReady {
                        generation,
                        section,
                        theme: theme.id,
                        result,
                    },
                );
            }
            let heights = self.section_layout().map_or_else(
                || canonical.heights.clone(),
                |layout| layout.heights.clone(),
            );
            self.heights = virtual_reader::HeightIndex::new(heights);
            self.measured_layout = Some((self.width, self.font_size));
            self.offset = self.offset_for(anchor);
            if let Some((fingerprint, target_section, page)) = self.pending_page.take()
                && fingerprint == book.fingerprint
                && target_section == section
            {
                return self.go_to_local_page(page);
            }
            return self.restore_book_scroll();
        }
        self.atlas = None;
        self.measured_layout = None;
        self.heights = virtual_reader::HeightIndex::new(Vec::new());
        let cancel = Arc::new(AtomicBool::new(false));
        self.pagination = Some(cancel.clone());
        let book = book.clone();
        let generation = self.generation;
        Task::perform(
            async move { book_map::build(book, &cancel) },
            move |result| Message::AtlasReady { generation, result },
        )
    }

    fn finish_atlas(
        &mut self,
        generation: u64,
        result: Result<Option<book_map::Atlas>, String>,
    ) -> Task<Message> {
        if generation != self.generation || self.pagination.is_none() {
            return Task::none();
        }
        match result {
            Ok(Some(atlas)) => {
                self.atlas = Some(Arc::new(atlas));
                let end = self.pagination_end;
                let task = self.rebuild_geometry(self.pending_anchor.unwrap_or(Anchor {
                    row: 0,
                    fraction: 0.0,
                }));
                self.pagination_end = false;
                if end {
                    return self.go_to_local_page(self.pages().len().saturating_sub(1));
                }
                task
            }
            Ok(None) => Task::none(),
            Err(error) => {
                self.pagination = None;
                self.error = Some(format!("Could not prepare book pages: {error}"));
                Task::none()
            }
        }
    }

    fn refine_geometry(&mut self) -> Task<Message> {
        if self.pagination.is_some() || self.book.is_none() || self.opening.is_some() {
            return Task::none();
        }
        let pdf_book = self
            .book
            .as_ref()
            .is_some_and(|book| book.pdf_source.is_some());
        let anchor = self.anchor();
        let mut changed = false;
        let mut pending = self.measurements.lock();
        for (row, height, width, generation) in pending.drain(..) {
            if generation != self.generation || width != self.width || row >= self.heights.len() {
                continue;
            }
            if (self.measured_layout.is_none() || pdf_book)
                && (self.heights.height(row) - height).abs() > 0.01
            {
                self.heights.refine(row, height, self.offset);
                changed = true;
            }
        }
        drop(pending);
        if changed
            && pdf_book
            && let (Some(book), Some(atlas)) = (&self.book, &mut self.atlas)
        {
            book_map::refine_pdf(Arc::make_mut(atlas), book, &self.heights);
        }
        // A native layout also acknowledges an exact estimate. Keeping the
        // anchor locked in that case would discard subsequent wheel scrolling.
        let restore = self.pending_anchor.take().is_some();
        if changed || restore {
            self.offset = self.offset_for(anchor);
            self.restore_book_scroll()
        } else {
            Task::none()
        }
    }

    fn open(&mut self, path: PathBuf, locate: Option<Entry>) -> Task<Message> {
        self.open_with_import(path, locate, false)
    }
    fn open_with_import(
        &mut self,
        path: PathBuf,
        locate: Option<Entry>,
        import: bool,
    ) -> Task<Message> {
        if self.saving
            || self.dialog_open
            || self.failed_close.is_some()
            || self.removing.is_some()
            || self.confirm_remove.is_some()
        {
            return Task::none();
        }
        self.show_conversion = false;
        self.word_translation.reset();
        self.capture_progress();
        let library_task = self.shelf.persist().map(Message::Shelf);
        self.show_search = false;
        self.show_settings = false;
        self.dropping = false;
        let previous = self.document_position();
        self.cancel_open();
        let request = self.request;
        self.opening = Some(path.clone());
        self.pending_locate = locate.clone();
        self.recent_open = if locate.is_some() {
            None
        } else {
            self.recent
                .iter()
                .find(|entry| entry.path == path)
                .cloned()
                .or_else(|| {
                    self.shelf
                        .entries
                        .iter()
                        .find(|entry| entry.document.path == path)
                        .map(|entry| entry.document.clone())
                })
        };
        self.error = None;
        self.missing_recent = None;
        self.selection.end_drag();
        let (task, handle) = Task::perform(
            async move {
                let path = if import {
                    reader_document::managed::import(&path)?
                } else {
                    path
                };
                load_document(path, previous, locate).await
            },
            move |result| Message::Loaded { request, result },
        )
        .abortable();
        self.opening_task = Some(handle.abort_on_drop());
        Task::batch([task, library_task])
    }

    fn persist_recent(&mut self) -> Task<Message> {
        if self.recent_loading || self.recent_saving || self.history_corrupt || !self.recent_dirty {
            return Task::none();
        }
        self.recent_saving = true;
        self.recent_dirty = false;
        let entries = self.recent.clone();
        Task::perform(async move { recent::save(&entries) }, Message::RecentSaved)
    }

    fn remember_recent(&mut self, entry: Entry, old: Option<&Entry>) -> Task<Message> {
        if self.recent_loading {
            self.recent_pending.push((entry, old.cloned()));
            return Task::none();
        }
        if let Some(old) = old {
            recent::remove(&mut self.recent, &old.path);
        }
        recent::remember(&mut self.recent, entry);
        self.recent_dirty = true;
        self.persist_recent()
    }
    fn capture_progress(&mut self) {
        if let Some(pdf) = &self.pdf {
            let position = pdf.position();
            let total = pdf.document().pages.len() as u32;
            let fraction = (position.page as f32 + position.within) / total.max(1) as f32;
            self.shelf
                .progress(&pdf.document().path, fraction, position.page + 1, total);
        } else if let Some(book) = &self.book {
            let pages = self.pages();
            let Some(page) = book_pages::current(pages, self.offset) else {
                return;
            };
            let total = self.page_total() as u32;
            let current = page.number + 1;
            let within = ((self.offset - page.top) / page.height).clamp(0.0, 1.0);
            let fraction = (page.number as f32 + within) / total.max(1) as f32;
            self.shelf.progress(&book.path, fraction, current, total);
        }
    }

    fn persist_preferences(&mut self) -> Task<Message> {
        if self.preferences_loading
            || self.preferences_saving
            || !self.preferences_dirty
            || !self.preferences_writable
        {
            return Task::none();
        }
        self.preferences_dirty = false;
        self.preferences_saving = true;
        let preferences = Preferences {
            appearance: self.appearance,
            window_controls: self.window_controls,
            theme: self.theme.id.to_owned(),
            voice: self.read_aloud.voice.clone(),
            speech_rate: self.read_aloud.rate,
            dictionary: self.word_translation.settings,
            reading: self.reading.defaults,
        };
        Task::perform(
            async move { preferences::save(preferences) },
            Message::PreferencesSaved,
        )
    }

    fn exit_ready(&self) -> bool {
        self.removing.is_none()
            && !self.profile.busy
            && self.reading.settled()
            && !self.preferences_loading
            && !self.preferences_saving
            && !(self.preferences_dirty && self.preferences_writable)
            && !self.recent_loading
            && !self.recent_saving
            && !(self.recent_dirty && !self.history_corrupt)
            && self.notes.settled()
            && !self.shelf.loading
            && !self.shelf.saving
            && !(self.shelf.dirty && !self.shelf.blocked)
            && self.shelf.shelves_settled()
    }

    fn locate(&mut self, entry: Entry) -> Task<Message> {
        self.show_search = false;
        self.dialog_locate = Some(entry);
        self.dialog_open = true;
        self.selection.end_drag();
        Task::perform(
            async { crate::platform::open_document_dialog(true) },
            Message::DialogChosen,
        )
    }

    fn contents_range(&self) -> std::ops::Range<usize> {
        if !self.show_contents {
            return 0..0;
        }
        let count = self.contents().len();
        let height = (self.window_size.height * 0.35).clamp(100.0, 240.0);
        let start = ((self.contents_offset / CONTENT_ROW_HEIGHT) as usize)
            .saturating_sub(1)
            .min(count);
        start..(start + (height / CONTENT_ROW_HEIGHT).ceil() as usize + 3).min(count)
    }

    fn controls(&self) -> impl Iterator<Item = Control> + Clone + '_ {
        let book = self.book.as_ref();
        let overlay = self.show_search || self.show_settings || self.confirm_remove.is_some();
        let has_document = book.is_some() || self.pdf.is_some();
        let history = self.show_recent && !overlay;
        let pdf_page = self.pdf.as_ref().map(pdf_reader::Reader::page_index);
        let expanded = has_document && self.toolbar_expanded && !overlay;
        let finding = self.find.is_some() && has_document && !overlay;
        let has_matches = self.find.as_ref().is_some_and(|find| find.count() > 0);
        let primary = [
            finding.then_some(Control::FindInput),
            (finding && has_matches).then_some(Control::FindPrevious),
            (finding && has_matches).then_some(Control::FindNext),
            finding.then_some(Control::FindClose),
            (has_document && !overlay && self.read_aloud.active())
                .then_some(Control::ReadAloudSlower),
            (has_document && !overlay && self.read_aloud.active())
                .then_some(Control::ReadAloudFaster),
            (has_document && !overlay && self.read_aloud.active())
                .then_some(Control::ReadAloudPause),
            expanded.then_some(Control::Find),
            expanded.then_some(Control::ReadAloud),
            (expanded && self.book.is_some()).then_some(Control::BookPageInput),
            (expanded && self.can_turn(false)).then_some(Control::PreviousBookPage),
            (expanded && self.can_turn(true)).then_some(Control::NextBookPage),
            expanded.then_some(Control::Close),
            (expanded && self.pdf.as_ref().is_some_and(|p| p.document().can_copy))
                .then_some(Control::BookMode),
            (expanded && book.is_some_and(|b| b.pdf_source.is_some()))
                .then_some(Control::DocumentMode),
            (expanded && !self.returns.is_empty()).then_some(Control::Back),
            pdf_page
                .filter(|page| expanded && *page > 0)
                .map(|_| Control::Pdf(pdf_reader::FocusControl::Previous)),
            (expanded && self.pdf.is_some())
                .then_some(Control::Pdf(pdf_reader::FocusControl::Page)),
            pdf_page
                .filter(|page| {
                    expanded
                        && self
                            .pdf
                            .as_ref()
                            .is_some_and(|pdf| *page + 1 < pdf.document().pages.len())
                })
                .map(|_| Control::Pdf(pdf_reader::FocusControl::Next)),
            (expanded && self.pdf.is_some())
                .then_some(Control::Pdf(pdf_reader::FocusControl::ZoomOut)),
            (expanded && self.pdf.is_some())
                .then_some(Control::Pdf(pdf_reader::FocusControl::ZoomIn)),
            (expanded && self.pdf.is_some())
                .then_some(Control::Pdf(pdf_reader::FocusControl::ActualSize)),
            (expanded && self.pdf.is_some())
                .then_some(Control::Pdf(pdf_reader::FocusControl::FitWidth)),
            (expanded && !self.contents().is_empty()).then_some(Control::Contents),
            (expanded && book.is_some() && self.zoom > 0.25).then_some(Control::FontDown),
            (expanded && book.is_some() && self.zoom < 3.0).then_some(Control::FontUp),
            (expanded && book.is_some()).then_some(Control::FitBookWidth),
            expanded.then_some(Control::ToggleAppearance),
        ];
        let secondary = [
            self.error.as_ref().map(|_| Control::DismissError),
            self.missing_recent.as_ref().map(|_| Control::LocateMissing),
            self.failed_close.map(|_| Control::CloseWithoutSaving),
            self.shelf
                .has_failed_save()
                .then_some(Control::Shelf(shelf::Control::RetrySave)),
            (self.history_corrupt && self.history_notice.is_some()).then_some(Control::ResetRecent),
            history.then_some(Control::HideRecent),
        ];
        // Title-bar focus follows the visual order of the chosen window controls.
        let native = match self.window_controls {
            WindowControls::Mac => [
                Some(Control::Chrome(chrome::Action::Close)),
                Some(Control::Chrome(chrome::Action::Minimize)),
                Some(Control::Chrome(chrome::Action::Maximize)),
                None,
            ],
            WindowControls::Windows => [
                Some(Control::Chrome(chrome::Action::Minimize)),
                Some(Control::Chrome(chrome::Action::Maximize)),
                Some(Control::Chrome(chrome::Action::Close)),
                None,
            ],
        };
        let tools = match self.window_controls {
            WindowControls::Mac => [
                Some(Control::Chrome(chrome::Action::Search)),
                (!has_document).then_some(Control::Chrome(chrome::Action::ToggleAppearance)),
                has_document.then_some(Control::ToggleToolbar),
                Some(Control::Chrome(chrome::Action::Settings)),
            ],
            WindowControls::Windows => [
                Some(Control::Chrome(chrome::Action::Settings)),
                has_document.then_some(Control::ToggleToolbar),
                (!has_document).then_some(Control::Chrome(chrome::Action::ToggleAppearance)),
                Some(Control::Chrome(chrome::Action::Search)),
            ],
        };
        let (first, last) = match self.window_controls {
            WindowControls::Mac => (native, tools),
            WindowControls::Windows => (tools, native),
        };
        first
            .into_iter()
            .chain(last)
            .flatten()
            .filter(move |_| !overlay && !self.fullscreen)
            .map(Some)
            .chain([
                self.show_search.then_some(Control::SearchInput),
                overlay.then_some(Control::DismissOverlay),
                self.confirm_remove
                    .is_some()
                    .then_some(Control::ConfirmRemove),
                (self.show_settings && self.zoom > 0.25).then_some(Control::SettingsFontDown),
                (self.show_settings && self.zoom < 3.0).then_some(Control::SettingsFontUp),
                self.show_settings
                    .then_some(Control::SettingsWindowControls(WindowControls::Windows)),
                self.show_settings
                    .then_some(Control::SettingsWindowControls(WindowControls::Mac)),
                (self.show_settings && self.read_aloud.rate > crate::speech::MIN_RATE)
                    .then_some(Control::ReadAloudSlower),
                (self.show_settings && self.read_aloud.rate < crate::speech::MAX_RATE)
                    .then_some(Control::ReadAloudFaster),
                self.show_settings.then_some(Control::SettingsHelp),
                self.show_settings.then_some(Control::Fullscreen),
            ])
            .flatten()
            .chain(reading_ui::controls(self))
            .chain(self.dictionary_controls())
            .chain(profile_ui::controls(self).filter(move |_| self.show_settings))
            .chain(
                (0..themes::THEMES.len())
                    .filter(move |_| self.show_settings)
                    .map(Control::SettingsTheme),
            )
            .chain(
                primary
                    .into_iter()
                    .flatten()
                    .filter(move |_| has_document && !overlay),
            )
            .chain(
                self.contents_range()
                    .map(Control::ContentsEntry)
                    .filter(move |_| expanded),
            )
            .chain(book.into_iter().flat_map(move |book| {
                self.heights
                    .window(
                        book_pages::content_offset(self.pages(), &self.heights, self.offset),
                        self.viewport,
                        0.0,
                    )
                    .filter(move |_| !overlay)
                    .filter_map(|row| book.items.get(row).map(|item| (row, item)))
                    .flat_map(|(row, item)| {
                        (0..book.structure.get(item.id()).map_or(0, |s| s.links.len()))
                            .map(move |link| Control::BookLink(row, link))
                    })
            }))
            .chain(secondary.into_iter().flatten().filter(move |_| !overlay))
            .chain(
                (0..self.recent.len())
                    .filter(move |_| history)
                    .flat_map(|index| {
                        [
                            Control::OpenRecent(index),
                            Control::LocateRecent(index),
                            Control::RemoveRecent(index),
                        ]
                    }),
            )
            .chain(
                self.show_help
                    .then_some(Control::HideHelp)
                    .filter(move |_| !overlay),
            )
            .chain(
                self.notes_controls()
                    .into_iter()
                    .filter(move |_| has_document && !overlay),
            )
            .chain(
                self.shelf
                    .controls()
                    .map(Control::Shelf)
                    .filter(move |_| !has_document && !overlay),
            )
            .chain(
                self.search_results
                    .iter()
                    .copied()
                    .take(8)
                    .filter(move |_| self.show_search)
                    .flat_map(|index| {
                        [
                            Control::SearchResult(index),
                            Control::LocateLibrary(index),
                            Control::RemoveLibrary(index),
                        ]
                    }),
            )
    }

    fn activate(&mut self, control: Control) -> Task<Message> {
        if !self.controls().any(|visible| visible == control) {
            return Task::none();
        }
        let message = match control {
            Control::Reading(focus) => Message::Reading(reading_ui::focus_action(self, focus)),
            Control::FitBookWidth => Message::FitBookWidth,
            Control::Fullscreen => Message::ToggleFullscreen,
            Control::Profile(focus) => return self.profile_focus(focus),
            Control::Dictionary(focus) => {
                Message::Dictionary(word_translation::focus_action(focus))
            }
            Control::Chrome(action) => Message::Chrome(action),
            Control::Shelf(control) => Message::Shelf(shelf::Message::Activate(control)),
            Control::SearchInput => return iced::widget::operation::focus(search_id()),
            Control::SearchResult(index) => Message::OpenLibrary(index),
            Control::LocateLibrary(index) => Message::LocateLibrary(index),
            Control::RemoveLibrary(index) => Message::RemoveLibrary(index),
            Control::ConfirmRemove => Message::ConfirmRemove,
            Control::DismissOverlay => Message::DismissOverlay,
            Control::HideRecent => Message::ToggleRecent,
            Control::Close => Message::Close(CloseAction::Document),
            Control::ToggleToolbar => Message::ToggleToolbar,
            Control::HideHelp | Control::SettingsHelp => Message::ToggleHelp,
            Control::SettingsTheme(index) => Message::SetTheme(index),
            Control::FontDown | Control::SettingsFontDown => Message::Zoom(self.zoom - 0.1),
            Control::FontUp | Control::SettingsFontUp => Message::Zoom(self.zoom + 0.1),
            Control::ToggleAppearance => Message::ToggleAppearance,
            Control::SettingsWindowControls(controls) => Message::SetWindowControls(controls),
            Control::BookMode => Message::BookMode,
            Control::DocumentMode => Message::DocumentMode { source_page: true },
            Control::Back => Message::Back,
            Control::BookLink(row, link) => {
                let Some(href) = self
                    .book
                    .as_ref()
                    .and_then(|book| {
                        book.structure
                            .get(book.items.get(row)?.id())?
                            .links
                            .get(link)
                    })
                    .map(|link| link.href.clone())
                else {
                    return Task::none();
                };
                Message::FollowLink(href)
            }
            Control::Pdf(action) => {
                if action == pdf_reader::FocusControl::Page {
                    return iced::widget::operation::focus(pdf_reader::page_input_id());
                }
                let Some(pdf) = &self.pdf else {
                    return Task::none();
                };
                let document = pdf.document().id;
                let message = match action {
                    pdf_reader::FocusControl::Previous => pdf_reader::Message::Previous,
                    pdf_reader::FocusControl::Next => pdf_reader::Message::Next,
                    pdf_reader::FocusControl::ZoomOut => pdf_reader::Message::ZoomOut,
                    pdf_reader::FocusControl::ZoomIn => pdf_reader::Message::ZoomIn,
                    pdf_reader::FocusControl::ActualSize => pdf_reader::Message::ActualSize,
                    pdf_reader::FocusControl::FitWidth => pdf_reader::Message::FitWidth,
                    pdf_reader::FocusControl::Page => unreachable!(),
                };
                return update_inner(self, Message::Pdf { document, message });
            }
            Control::BookPageInput => {
                return iced::widget::operation::focus(iced::advanced::widget::Id::new(
                    "book-page",
                ));
            }
            Control::Find => Message::FindOpen,
            Control::ReadAloud => Message::ReadAloud(read_aloud::Action::Toggle),
            Control::ReadAloudPause => Message::ReadAloud(
                if self
                    .read_aloud
                    .session
                    .as_ref()
                    .is_some_and(|session| session.paused)
                {
                    read_aloud::Action::Resume
                } else {
                    read_aloud::Action::Pause
                },
            ),
            Control::ReadAloudSlower => Message::ReadAloud(read_aloud::Action::Slower),
            Control::ReadAloudFaster => Message::ReadAloud(read_aloud::Action::Faster),
            Control::FindInput => return iced::widget::operation::focus(find_id()),
            Control::FindPrevious => Message::FindStep(false),
            Control::FindNext => Message::FindStep(true),
            Control::FindClose => Message::FindClose,
            Control::PreviousBookPage => return self.adjacent_book_page(false),
            Control::NextBookPage => return self.adjacent_book_page(true),
            Control::Contents => Message::ToggleContents,
            Control::ContentsEntry(index) => {
                let contents = self.contents();
                let Some(entry) = contents.get(index) else {
                    return Task::none();
                };
                Message::EpubChapter {
                    index: entry.chapter,
                    fragment: entry.fragment.clone(),
                }
            }
            Control::DismissError => Message::DismissError,
            Control::LocateMissing => Message::LocateMissing,
            Control::CloseWithoutSaving => Message::CloseWithoutSaving,
            Control::ResetRecent => Message::ResetRecent,
            Control::OpenRecent(index) => Message::OpenRecent(index),
            Control::LocateRecent(index) => Message::LocateRecent(index),
            Control::RemoveRecent(index) => Message::RemoveRecent(index),
            Control::Notes(focus) => Message::Notes(focus.action()),
        };
        update_inner(self, message)
    }

    fn convert_pdf(&mut self) -> Task<Message> {
        if !self.interactive() || !self.show_conversion {
            return Task::none();
        }
        let Some(pdf) = &self.pdf else {
            return Task::none();
        };
        let document = pdf.document().clone();
        if !document.can_copy {
            self.error = Some("PDF permissions prohibit Book extraction".into());
            return Task::none();
        }
        let original = pdf.position();
        let font_size = self.font_size;
        self.cancel_open();
        let request = self.request;
        self.opening = Some(document.path.clone());
        self.error = None;
        let (task, handle) = Task::perform(
            async move {
                let book =
                    load_pdf_book(document.clone(), original.clone(), font_size, false).await?;
                position::save_pdf(&document.path, &original)?;
                position::save_pdf_mode(
                    &document.path,
                    &document.fingerprint,
                    position::PdfMode::Book,
                )?;
                Ok(LoadReply {
                    document: LoadedDocument::Reflow(Arc::new(book)),
                    catalog: None,
                })
            },
            move |result| Message::Loaded { request, result },
        )
        .abortable();
        self.opening_task = Some(handle.abort_on_drop());
        task
    }

    fn pdf_return_position(&self) -> Option<PdfReadingPosition> {
        let source = self.book.as_ref()?.pdf_source.as_ref()?;
        let mut original = source.original_position.clone();
        let range = pdf_source_range(&source.conversion, self.anchor());
        let pages = self.pages();
        let page = book_pages::current(pages, self.offset)
            .map(|p| p.number)
            .or_else(|| range.map(|r| r.page))?;
        original.page = page;
        original.within = 0.0;
        original.horizontal = 0.0;
        Some(original)
    }

    fn show_original(&mut self, _source_page: bool) -> Task<Message> {
        if !self.interactive() {
            return Task::none();
        }
        let Some(book) = &self.book else {
            return Task::none();
        };
        let Some(source) = &book.pdf_source else {
            return Task::none();
        };
        let document = source.document.clone();
        let Some(original) = self.pdf_return_position() else {
            return Task::none();
        };
        let previous = self.saved_position().map(|(_, p)| p);
        self.cancel_open();
        let request = self.request;
        self.opening = Some(document.path.clone());
        self.error = None;
        let (task, handle) = Task::perform(
            async move {
                if let Some(previous) = previous {
                    position::save_pdf_book(&document.path, &previous)?;
                }
                position::save_pdf_mode(
                    &document.path,
                    &document.fingerprint,
                    position::PdfMode::Document,
                )?;
                Ok(LoadReply {
                    document: LoadedDocument::Pdf {
                        document,
                        restored: Some(original),
                        warnings: Vec::new(),
                    },
                    catalog: None,
                })
            },
            move |result| Message::Loaded { request, result },
        )
        .abortable();
        self.opening_task = Some(handle.abort_on_drop());
        task
    }

    fn return_location(&self) -> Option<ReturnLocation> {
        let book = self.book.as_ref()?;
        let anchor = self.anchor();
        Some(ReturnLocation {
            chapter: book.epub.as_ref().map(|c| c.index),
            item_id: book.items.get(anchor.row)?.id().to_owned(),
            within: anchor.fraction,
        })
    }
    fn commit_navigation(&mut self, navigation: Navigation) {
        match navigation {
            Navigation::Push(location) => {
                if self.returns.len() == 64 {
                    self.returns.remove(0);
                }
                self.returns.push(location);
            }
            Navigation::Pop => {
                self.returns.pop();
            }
            Navigation::Preserve | Navigation::End => {}
        }
    }
    fn follow_link(&mut self, href: String) -> Task<Message> {
        if !self.interactive() {
            return Task::none();
        }
        let Some(book) = &self.book else {
            return Task::none();
        };
        if let Some(source) = &book.pdf_source {
            let Some(page) = href
                .strip_prefix("pdf-page:")
                .and_then(|value| value.parse::<u32>().ok())
            else {
                return Task::none();
            };
            let Some(row) = source.conversion.blocks.iter().position(|block| {
                block
                    .sources
                    .first()
                    .is_some_and(|source| source.page == page)
            }) else {
                return Task::none();
            };
            let navigation = self
                .return_location()
                .map_or(Navigation::Preserve, Navigation::Push);
            self.commit_navigation(navigation);
            self.show_contents = false;
            self.selection.clear();
            self.focused = None;
            return self.rebuild_geometry(Anchor { row, fraction: 0.0 });
        }
        let target = if let Some(chapter) = &book.epub {
            chapter
                .document
                .resolve_link(chapter.index, &href)
                .map(|(index, fragment)| (Some(index), fragment))
        } else {
            reader_document::resolve_html_link(&book.path, &href).map(|fragment| (None, fragment))
        };
        match target {
            Ok((chapter, fragment)) => {
                let navigation = self
                    .return_location()
                    .map_or(Navigation::Preserve, Navigation::Push);
                self.navigate(chapter, fragment, None, navigation)
            }
            Err(error) => {
                self.error = Some(error);
                Task::none()
            }
        }
    }
    fn go_back(&mut self) -> Task<Message> {
        let Some(location) = self.returns.last().cloned() else {
            return Task::none();
        };
        self.navigate(location.chapter, None, Some(location), Navigation::Pop)
    }
    fn open_chapter(&mut self, index: usize, fragment: Option<String>) -> Task<Message> {
        self.word_translation.reset();
        if self
            .book
            .as_ref()
            .is_some_and(|b| b.epub.is_none() && b.pdf_source.is_none() && index < b.items.len())
        {
            self.show_contents = false;
            self.focused = None;
            self.selection.clear();
            return self.rebuild_geometry(Anchor {
                row: index,
                fraction: 0.0,
            });
        }
        self.navigate(Some(index), fragment, None, Navigation::Preserve)
    }
    fn navigate(
        &mut self,
        index: Option<usize>,
        fragment: Option<String>,
        location: Option<ReturnLocation>,
        navigation: Navigation,
    ) -> Task<Message> {
        if !self.interactive() {
            return Task::none();
        }
        let Some(book) = &self.book else {
            return Task::none();
        };
        if index == book.epub.as_ref().map(|c| c.index) {
            let target = location.as_ref().map(|l| l.item_id.as_str()).or_else(|| {
                fragment
                    .as_ref()
                    .and_then(|f| book.anchors.get(f).map(String::as_str))
            });
            let row = target.and_then(|id| book.items.iter().position(|item| item.id() == id));
            if (fragment.is_some() || location.is_some()) && row.is_none() {
                self.error = Some("The link target is absent from this section.".into());
                return Task::none();
            }
            let anchor = Anchor {
                row: row.unwrap_or(0),
                fraction: location.as_ref().map_or(0.0, |l| l.within),
            };
            self.commit_navigation(navigation);
            self.show_contents = false;
            self.selection.clear();
            self.focused = None;
            self.error = None;
            return self.rebuild_geometry(anchor);
        }
        let Some(chapter) = &book.epub else {
            return Task::none();
        };
        let Some(index) = index.filter(|index| chapter.document.section(*index).is_some()) else {
            return Task::none();
        };
        let document = chapter.document.clone();
        let path = book.path.clone();
        let font_size = self.font_size;
        let previous = self.document_position();
        self.cancel_open();
        let request = self.request;
        let at_end = matches!(navigation, Navigation::End);
        self.pending_navigation = Some(navigation);
        self.opening = Some(path);
        self.error = None;
        self.failed_close = None;
        self.selection.end_drag();
        let (task, handle) = Task::perform(
            async move {
                let mut book = load_epub_chapter(document, index, None)?;
                let target = location
                    .as_ref()
                    .map(|l| l.item_id.clone())
                    .or_else(|| fragment.as_ref().and_then(|f| book.anchors.get(f).cloned()));
                if (fragment.is_some() || location.is_some())
                    && !target
                        .as_ref()
                        .is_some_and(|id| book.items.iter().any(|item| item.id() == id))
                {
                    return Err("The link target is absent from this section.".into());
                }
                book.restored = (if at_end {
                    book.items.last()
                } else {
                    book.items.first()
                })
                .map(|first| ReadingPosition {
                    fingerprint: book.fingerprint.clone(),
                    item_id: target.unwrap_or_else(|| first.id().to_owned()),
                    within: location.map_or(0.0, |l| l.within),
                    font_size,
                });
                if let Some(error) = previous.and_then(|position| position.save().err()) {
                    book.warnings.push(format!(
                        "Could not save the previous reading position: {error}"
                    ));
                }
                Ok(LoadReply {
                    document: LoadedDocument::Reflow(Arc::new(book)),
                    catalog: None,
                })
            },
            move |result| Message::Loaded { request, result },
        )
        .abortable();
        self.opening_task = Some(handle.abort_on_drop());
        task
    }

    fn adjacent_chapter(&mut self, next: bool) -> Task<Message> {
        let Some(chapter) = self.book.as_ref().and_then(|book| book.epub.as_ref()) else {
            return Task::none();
        };
        if chapter.index >= chapter.document.chapters.len() {
            return Task::none();
        }
        let count = chapter.document.chapters.len();
        let index = if next {
            chapter.index.checked_add(1)
        } else {
            chapter.index.checked_sub(1)
        };
        index
            .filter(|index| *index < count)
            .map_or_else(Task::none, |index| self.open_chapter(index, None))
    }

    fn finish_close(&mut self, action: CloseAction) -> Task<Message> {
        self.failed_close = None;
        // A note being written is kept, not lost, when the book or window closes.
        let note = if self.notes.editor.is_some() {
            self.notes_action(notes::Action::SaveNote)
        } else {
            self.persist_notes()
        };
        if self.notes.editor.is_some() {
            return note;
        }
        if !self.notes.settled() {
            self.pending_notes_close = Some(action);
            return note;
        }
        self.pending_notes_close = None;
        match action {
            CloseAction::Window => {
                if self.shelf.has_failed_save() {
                    self.pending_exit = false;
                    self.failed_close = Some(action);
                    self.error = self.shelf.notice.clone();
                    return note;
                }
                if !self.exit_ready() {
                    self.pending_exit = true;
                    Task::batch([
                        note,
                        self.persist_recent(),
                        self.persist_preferences(),
                        self.shelf.persist().map(Message::Shelf),
                    ])
                } else {
                    iced::exit()
                }
            }
            CloseAction::Document => {
                self.capture_progress();
                self.cancel_open();
                self.clear_content();
                self.reset_find();
                self.adapted.clear();
                self.notes.reset(None);
                self.error = None;
                Task::batch([
                    note,
                    self.shelf.persist().map(Message::Shelf),
                    self.shelf.show(true).map(Message::Shelf),
                ])
            }
        }
    }

    fn close(&mut self, action: CloseAction) -> Task<Message> {
        self.word_translation.reset();
        if self.saving || self.dialog_open {
            return Task::none();
        }
        self.failed_close = None;
        self.notes.failed_save = false;
        let library_retry = self.shelf.retry_failed_saves().map(Message::Shelf);
        self.cancel_open();
        self.selection.end_drag();
        self.show_search = false;
        self.show_settings = false;
        self.capture_progress();
        if let Some(position) = self.document_position() {
            self.saving = true;
            Task::batch([
                library_retry,
                Task::perform(async move { position.save() }, move |result| {
                    Message::Saved { action, result }
                }),
            ])
        } else {
            Task::batch([library_retry, self.finish_close(action)])
        }
    }

    /// Opens the find bar and starts reading the text of the rest of the document
    /// (every EPUB chapter, or every PDF page) so search spans all of it.
    fn open_find(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        let mut notice = None;
        if let Some(book) = &self.book {
            if let Some(chapter) = &book.epub
                && chapter.document.chapters.len() > 1
                && self
                    .find_index
                    .as_ref()
                    .is_none_or(|index| index.fingerprint != book.fingerprint)
                && self.find_task.is_none()
            {
                let document = chapter.document.clone();
                let fingerprint = book.fingerprint.clone();
                self.find_generation = self.find_generation.wrapping_add(1);
                let generation = self.find_generation;
                let cancel = Arc::new(AtomicBool::new(false));
                let worker_cancel = cancel.clone();
                let (task, handle) = Task::perform(
                    async move {
                        let mut chapters = Vec::new();
                        let mut total = 0;
                        let mut truncated = false;
                        for index in 0..document.chapters.len() {
                            if worker_cancel.load(Ordering::Relaxed) {
                                return Err("Search cancelled".to_owned());
                            }
                            // Preserve chapter indices when a chapter cannot be read.
                            let items = document
                                .load_chapter_text(index)
                                .map(|chapter| chapter.document.items)
                                .unwrap_or_default();
                            let bytes = items
                                .iter()
                                .filter_map(Item::text)
                                .map(str::len)
                                .sum::<usize>();
                            if bytes > MAX_FIND_INDEX_BYTES.saturating_sub(total) {
                                truncated = true;
                                break;
                            }
                            total += bytes;
                            chapters.push(items);
                        }
                        Ok(FindIndex {
                            fingerprint,
                            chapters: Arc::new(chapters),
                            truncated,
                        })
                    },
                    move |result| Message::FindIndexed { generation, result },
                )
                .abortable();
                self.find_task = Some(FindJob {
                    _handle: handle.abort_on_drop(),
                    cancel,
                });
                tasks.push(task);
            }
        } else if let Some(pdf) = &self.pdf {
            let document = pdf.document().clone();
            if !document.can_copy {
                notice = Some("This PDF does not permit text search".to_owned());
            } else if self
                .pdf_find_index
                .as_ref()
                .is_none_or(|index| index.document != document.id)
                && self.find_task.is_none()
            {
                self.find_generation = self.find_generation.wrapping_add(1);
                let generation = self.find_generation;
                let cancel = Arc::new(AtomicBool::new(false));
                let worker_cancel = cancel.clone();
                let (task, handle) = Task::perform(
                    async move {
                        let mut pages = Vec::with_capacity(document.pages.len());
                        let (mut total, mut truncated) = (0, false);
                        for page in 0..document.pages.len() as u32 {
                            if worker_cancel.load(Ordering::Relaxed) {
                                return Err("Search cancelled".to_owned());
                            }
                            // Each page is its own worker request, so page renders interleave.
                            let text = document.session.page_text(page).await.unwrap_or_default();
                            if text.len() > MAX_FIND_INDEX_BYTES.saturating_sub(total) {
                                truncated = true;
                                break;
                            }
                            total += text.len();
                            pages.push(text);
                        }
                        Ok(PdfFindIndex {
                            document: document.id,
                            pages: Arc::new(pages),
                            truncated,
                        })
                    },
                    move |result| Message::PdfFindIndexed { generation, result },
                )
                .abortable();
                self.find_task = Some(FindJob {
                    _handle: handle.abort_on_drop(),
                    cancel,
                });
                tasks.push(task);
            }
        } else {
            return Task::none();
        }
        let indexing = self.find_task.is_some();
        let find = self.find.get_or_insert_with(Find::default);
        find.indexing = indexing;
        find.notice = notice;
        self.focused = Some(Control::FindInput);
        tasks.push(iced::widget::operation::focus(find_id()));
        Task::batch(tasks)
    }

    /// Forgets the find bar and the text index of the previous document.
    fn reset_find(&mut self) {
        self.find = None;
        self.find_index = None;
        self.pdf_find_index = None;
        self.cancel_find_jobs();
    }

    /// A chapter load clears the highlight; bring it back if the match is in this chapter.
    fn restore_find_selection(&mut self) {
        let here = self
            .book
            .as_ref()
            .and_then(|book| book.epub.as_ref())
            .map(|chapter| chapter.index);
        if self
            .find
            .as_ref()
            .and_then(|find| find.matches.get(find.current?))
            .is_some_and(|m| m.chapter == here)
        {
            self.select_match();
        }
    }

    fn cancel_find_jobs(&mut self) {
        self.find_task = None;
        self.query_task = None;
        self.find_generation = self.find_generation.wrapping_add(1);
        self.query_generation = self.query_generation.wrapping_add(1);
    }

    fn close_find(&mut self) {
        if self.find.take().is_some() {
            self.selection.clear();
            if let Some(pdf) = &mut self.pdf {
                pdf.clear_selection();
            }
        }
        self.cancel_find_jobs();
        if self.focused.is_some_and(|control| {
            matches!(
                control,
                Control::FindInput | Control::FindPrevious | Control::FindNext | Control::FindClose
            )
        }) {
            self.focused = None;
        }
    }

    /// Finds `query` in the open document and moves to the first match at or after
    /// the current match (or the reading position when there is none yet).
    fn search_document(&mut self, query: String) -> Task<Message> {
        if self.pdf.is_some() {
            return self.search_pdf(query);
        }
        let Some(book) = self.book.clone() else {
            return Task::none();
        };
        let reading_origin = SearchOrigin::Book(
            book.epub.as_ref().map(|chapter| chapter.index),
            self.anchor().row,
            0,
        );
        let Some(find) = &mut self.find else {
            return Task::none();
        };
        let origin = find
            .current
            .and_then(|index| find.matches.get(index))
            .map_or(reading_origin, |m| {
                SearchOrigin::Book(m.chapter, m.item_index, m.start)
            });
        find.query = query;
        find.matches.clear();
        find.current = None;
        let index = self
            .find_index
            .as_ref()
            .filter(|index| index.fingerprint == book.fingerprint);
        find.notice = index
            .filter(|index| index.truncated)
            .map(|index| format!("Searching the first {} chapters", index.chapters.len()));
        let corpus = index.map_or_else(
            || SearchCorpus::Book(book),
            |index| SearchCorpus::Chapters(index.chapters.clone()),
        );
        self.start_query(corpus, origin)
    }

    fn search_pdf(&mut self, query: String) -> Task<Message> {
        let Some(pdf) = &self.pdf else {
            return Task::none();
        };
        let document = pdf.document().id;
        let reading_page = pdf.page_index() as u32;
        let can_copy = pdf.document().can_copy;
        let Some(find) = &mut self.find else {
            return Task::none();
        };
        let origin = find
            .current
            .and_then(|index| find.pdf_matches.get(index))
            .map_or(SearchOrigin::Pdf(reading_page, 0), |m| {
                SearchOrigin::Pdf(m.page, m.first)
            });
        find.query = query;
        find.pdf_matches.clear();
        find.current = None;
        let index = self
            .pdf_find_index
            .as_ref()
            .filter(|index| index.document == document);
        find.notice = if !can_copy {
            Some("This PDF does not permit text search".to_owned())
        } else {
            index
                .filter(|index| index.truncated)
                .map(|index| format!("Searching the first {} pages", index.pages.len()))
        };
        let corpus = SearchCorpus::Pdf(
            index.map_or_else(|| Arc::new(Vec::new()), |index| index.pages.clone()),
        );
        self.start_query(corpus, origin)
    }

    fn start_query(&mut self, corpus: SearchCorpus, origin: SearchOrigin) -> Task<Message> {
        self.query_task = None;
        self.query_generation = self.query_generation.wrapping_add(1);
        let generation = self.query_generation;
        let Some(find) = &mut self.find else {
            return Task::none();
        };
        find.searching = false;
        self.selection.clear();
        if let Some(pdf) = &mut self.pdf {
            pdf.clear_selection();
        }
        let Some(needle) = find::needle(&find.query) else {
            return Task::none();
        };
        if !corpus.is_large() {
            let results = corpus.search(&needle, origin, &AtomicBool::new(false));
            return self.finish_query(generation, results);
        }
        find.searching = true;
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (task, handle) = Task::perform(
            async move { corpus.search(&needle, origin, &worker_cancel) },
            move |results| Message::FindSearched {
                generation,
                results,
            },
        )
        .abortable();
        self.query_task = Some(FindJob {
            _handle: handle.abort_on_drop(),
            cancel,
        });
        task
    }

    fn finish_query(&mut self, generation: u64, results: SearchResults) -> Task<Message> {
        if generation != self.query_generation {
            return Task::none();
        }
        let Some(find) = &mut self.find else {
            return Task::none();
        };
        self.query_task = None;
        find.searching = false;
        find.matches = results.matches;
        find.pdf_matches = results.pdf_matches;
        find.current = match results.origin {
            SearchOrigin::Book(chapter, item, start) => (!find.matches.is_empty()).then(|| {
                find.matches
                    .iter()
                    .position(|m| (m.chapter, m.item_index, m.start) >= (chapter, item, start))
                    .unwrap_or(0)
            }),
            SearchOrigin::Pdf(page, first) => (!find.pdf_matches.is_empty()).then(|| {
                find.pdf_matches
                    .iter()
                    .position(|m| (m.page, m.first) >= (page, first))
                    .unwrap_or(0)
            }),
        };
        if find.current.is_none() {
            return Task::none();
        }
        self.reveal_match()
    }

    fn step_match(&mut self, forward: bool) -> Task<Message> {
        let Some(find) = &mut self.find else {
            return Task::none();
        };
        let count = find.count();
        let Some(current) = find.current.filter(|_| count > 0) else {
            return Task::none();
        };
        find.current = Some(if forward {
            (current + 1) % count
        } else {
            (current + count - 1) % count
        });
        self.reveal_match()
    }

    /// Selects the current match and brings it into view, loading its chapter if needed.
    fn reveal_match(&mut self) -> Task<Message> {
        if let Some(pdf) = &mut self.pdf {
            let Some(m) = self
                .find
                .as_ref()
                .and_then(|find| find.pdf_matches.get(find.current?))
                .copied()
            else {
                return Task::none();
            };
            let id = pdf.document().id;
            return forward_pdf(id, pdf.show_match(m.page, m.first, m.last));
        }
        let Some(m) = self
            .find
            .as_ref()
            .and_then(|find| find.matches.get(find.current?))
            .cloned()
        else {
            return Task::none();
        };
        let Some(book) = &self.book else {
            return Task::none();
        };
        if let Some(chapter) = m.chapter
            && book.epub.as_ref().is_some_and(|c| c.index != chapter)
        {
            let location = ReturnLocation {
                chapter: Some(chapter),
                item_id: m.item_id.clone(),
                within: m.fraction,
            };
            return self.navigate(Some(chapter), None, Some(location), Navigation::Preserve);
        }
        self.select_match();
        let anchor = Anchor {
            row: m.item_index,
            fraction: m.fraction,
        };
        if self.pagination.is_some() || self.pages().is_empty() {
            return self.rebuild_geometry(anchor);
        }
        self.reveal_anchor(anchor)
    }

    /// Scrolls so `anchor` sits about a third of the way down the viewport, turning to
    /// its page if needed. A position already comfortably in view stays where it is.
    fn reveal_anchor(&mut self, anchor: Anchor) -> Task<Message> {
        let Some((position, page)) = self.unclamped_offset_for(anchor) else {
            return Task::none();
        };
        let bottom = page.top + (page.height - self.viewport).max(0.0);
        let current = self.active_page().map(|p| p.top);
        let in_view = current == Some(page.top)
            && position >= self.offset + self.viewport * 0.08
            && position <= self.offset + self.viewport * 0.85;
        let offset = if in_view {
            self.offset
        } else {
            (position - self.viewport * 0.3).clamp(page.top, bottom)
        };
        self.generation = self.generation.wrapping_add(1);
        self.pending_anchor = None;
        self.offset = offset;
        // Pinning the resulting position lets the scroll be repeated once the new page is laid out.
        self.pending_anchor = Some(self.anchor());
        self.selection.end_drag();
        self.restore_book_scroll()
    }

    /// Where `anchor` lies in page-offset space before it is limited to what the viewport can show.
    fn unclamped_offset_for(&self, anchor: Anchor) -> Option<(f32, book_pages::Page)> {
        if self.heights.is_empty() {
            return None;
        }
        let row = anchor.row.min(self.heights.len() - 1);
        let content = self.heights.start(row) + anchor.fraction * self.heights.height(row);
        let pages = self.pages();
        let page = pages
            .iter()
            .find(|p| content + 0.05 >= p.content.start && content < p.content.end - 0.05)
            .or_else(|| pages.last())?;
        let inset = if (content - page.content.start).abs() < 0.05 {
            0.0
        } else {
            book_pages::TOP
        };
        Some((
            page.top + inset + content - page.content.start,
            page.clone(),
        ))
    }

    /// Shows the current match with the reader's selection highlight.
    fn select_match(&mut self) {
        let Some(m) = self
            .find
            .as_ref()
            .and_then(|find| find.matches.get(find.current?))
        else {
            return;
        };
        let (from, to) = m.endpoints();
        self.selection.begin(from);
        self.selection.extend(to);
        self.selection.end_drag();
    }

    fn jump(&mut self, offset: f32) -> Task<Message> {
        self.word_translation.reset();
        if !self.interactive() || self.book.is_none() || self.pagination.is_some() {
            return Task::none();
        }
        self.pending_anchor = None;
        let Some(page) = self.active_page() else {
            return Task::none();
        };
        self.offset = offset.clamp(page.top, page.top + (page.height - self.viewport).max(0.0));
        self.restore_book_scroll()
    }
}

fn update(reader: &mut Reader, message: Message) -> Task<Message> {
    let previous_generation = reader.generation;
    let previous_pdf_page = reader
        .pdf
        .as_ref()
        .map(|pdf| (pdf.document().id, pdf.page_index()));
    let previous_focus = reader.focused;
    let previous_panels = (reader.show_recent, reader.show_help, reader.show_contents);
    let previous_toolbar = reader.toolbar_expanded;
    let resized = matches!(
        &message,
        Message::Event(iced::Event::Window(window::Event::Resized(_)), _)
    );
    let task = update_inner(reader, message);
    if reader.word_translation.popup.is_some()
        && (reader.generation != previous_generation
            || previous_pdf_page
                != reader
                    .pdf
                    .as_ref()
                    .map(|pdf| (pdf.document().id, pdf.page_index()))
            || reader.show_search
            || reader.show_settings
            || reader.notes.editor.is_some()
            || reader.confirm_remove.is_some())
    {
        reader.word_translation.dismiss();
    }
    reader.sync_read_aloud();
    let task = Task::batch([
        task,
        reader.request_pdf_book_raster(),
        reader.sync_reading_settings(),
        reader.refit_book(),
    ]);
    let panels_changed =
        previous_panels != (reader.show_recent, reader.show_help, reader.show_contents);
    if reader.focused != previous_focus
        || resized
        || panels_changed
        || previous_toolbar != reader.toolbar_expanded
    {
        reader.focus_generation = reader.focus_generation.wrapping_add(1);
        reader.focus_pending = reader.focused.is_some();
    }
    if reader.focused != previous_focus
        && let Some(Control::Shelf(control)) = reader.focused
    {
        Task::batch([task, reader.shelf.reveal(control).map(Message::Shelf)])
    } else {
        task
    }
}

fn update_inner(reader: &mut Reader, message: Message) -> Task<Message> {
    if matches!(
        &message,
        Message::EpubChapter { .. }
            | Message::BookPage(_)
            | Message::Back
            | Message::FollowLink(_)
            | Message::DocumentMode { .. }
            | Message::BookMode
            | Message::OpenDialog
            | Message::Chrome(_)
            | Message::ContextBlank
            | Message::FindOpen
    ) {
        reader.word_translation.reset();
    }
    match message {
        Message::Reading(action) => reader.reading_action(action),
        Message::ReadingLoaded {
            profile,
            book,
            result,
        } => {
            if profile == reader.profile.epoch {
                reader.reading_loaded(book, result)
            } else {
                Task::none()
            }
        }
        Message::ReadingSaved(result) => reader.reading_saved(result),
        Message::FitBookWidth => reader.fit_book_width(),
        Message::ToggleFullscreen => reader.toggle_fullscreen(),
        Message::Profile(action) => reader.profile_action(action),
        Message::ProfileReply(result) => reader.profile_reply(result),
        Message::BackupEstimate { generation, result } => {
            reader.profile_estimated(generation, result);
            Task::none()
        }
        Message::Dictionary(action) => reader.dictionary_action(action),
        Message::DictionaryInventory { generation, states } => {
            reader.dictionary_inventory(generation, states);
            Task::none()
        }
        Message::DictionaryProgress { generation, bytes } => {
            reader.dictionary_progress(generation, bytes);
            Task::none()
        }
        Message::DictionaryPackageReady { generation, result } => {
            reader.dictionary_package_ready(generation, result)
        }
        Message::DictionaryReady { generation, result } => {
            reader.dictionary_ready(generation, result);
            Task::none()
        }
        Message::DictionaryText {
            document,
            generation,
            result,
        } => reader.dictionary_text(document, generation, result),
        Message::LoadingFrame(frame) => {
            if reader.opening.is_some() || reader.pagination.is_some() {
                reader.loading_frame = frame;
            }
            Task::none()
        }
        Message::PdfBookRaster {
            document,
            page,
            result,
        } => {
            let current = reader
                .book
                .as_ref()
                .and_then(|b| b.pdf_source.as_ref())
                .is_some_and(|s| s.document.id == document)
                && reader.active_page().is_some_and(|p| p.number == page);
            if current && reader.pdf_book_pending == Some((document, page)) {
                reader.pdf_book_pending = None;
                reader.pdf_book_raster = Some((
                    document,
                    page,
                    result.and_then(|raster| {
                        let source = reader.book.as_ref().unwrap().pdf_source.as_ref().unwrap();
                        source
                            .conversion
                            .blocks
                            .iter()
                            .filter(|b| b.sources[0].page == page)
                            .filter_map(|b| {
                                source
                                    .conversion
                                    .illustrations
                                    .get(&b.id)
                                    .map(|r| (&b.id, *r))
                            })
                            .map(|(id, rect)| {
                                let image = reader_pdf::book::crop(&raster, rect)?;
                                Ok((
                                    id.clone(),
                                    DisplayImage {
                                        handle: image::Handle::from_rgba(
                                            image.width,
                                            image.height,
                                            image.rgba,
                                        ),
                                        width: image.width,
                                        height: image.height,
                                    },
                                ))
                            })
                            .collect()
                    }),
                ));
            }
            Task::none()
        }
        Message::Chrome(action) => match action {
            chrome::Action::ToggleToolbar => update_inner(reader, Message::ToggleToolbar),
            chrome::Action::ToggleAppearance => update_inner(reader, Message::ToggleAppearance),
            chrome::Action::Close => reader.close(CloseAction::Window),
            chrome::Action::Search if reader.interactive() => {
                reader.show_search = !reader.show_search;
                reader.show_settings = false;
                reader.search_query.clear();
                reader.search_results = reader.shelf.search("");
                reader.search_selected = 0;
                reader.focused = Some(if reader.show_search {
                    Control::SearchInput
                } else {
                    Control::Chrome(chrome::Action::Search)
                });
                if reader.show_search {
                    iced::widget::operation::focus(search_id())
                } else {
                    iced::advanced::widget::operate(
                        iced::advanced::widget::operation::focusable::unfocus(),
                    )
                }
            }
            chrome::Action::Settings if reader.interactive() => {
                reader.show_settings = !reader.show_settings;
                if reader.show_settings {
                    reader.voice_choices = reader.read_aloud.choices();
                }
                reader.show_search = false;
                reader.focused = Some(if reader.show_settings {
                    Control::DismissOverlay
                } else {
                    Control::Chrome(chrome::Action::Settings)
                });
                if reader.show_settings {
                    Task::batch([reader.refresh_dictionaries(), reader.estimate_backup()])
                } else {
                    Task::none()
                }
            }
            action => reader.window.map_or_else(Task::none, |id| match action {
                chrome::Action::Drag => window::drag(id),
                chrome::Action::Menu => window::show_system_menu(id),
                chrome::Action::Minimize => window::minimize(id, true),
                chrome::Action::Maximize => window::toggle_maximize(id),
                chrome::Action::Resize(direction) => window::drag_resize(id, direction),
                _ => Task::none(),
            }),
        },
        Message::ReadAloud(action) => reader.read_aloud(action),
        Message::ReadAloudTick => reader.read_aloud_tick(),
        Message::ReadAloudLine {
            generation,
            cursor,
            page_top,
            line,
        } => reader.read_aloud_line(generation, cursor, page_top, line),
        Message::ReadAloudText {
            document,
            generation,
            result,
        } => {
            reader.read_aloud_text(document, generation, result);
            Task::none()
        }
        Message::ReadAloudPdfPage {
            document,
            generation,
            page,
            result,
        } => reader.read_aloud_pdf_page(document, generation, page, result),
        Message::Shelf(shelf::Message::Activate(shelf::Control::Add)) => {
            update_inner(reader, Message::OpenDialog)
        }
        Message::Shelf(shelf::Message::Activate(
            shelf::Control::Resume(index)
            | shelf::Control::Document(index)
            | shelf::Control::FavouriteDocument(index),
        )) => update_inner(reader, Message::OpenLibrary(index)),
        Message::Shelf(shelf::Message::Activate(shelf::Control::Remove(index, _))) => {
            update_inner(reader, Message::RemoveLibrary(index))
        }
        Message::Shelf(shelf::Message::Activate(
            shelf::Control::Favourite(..)
            | shelf::Control::ToggleShelf(..)
            | shelf::Control::DeleteShelf,
        )) if !reader.interactive() => Task::none(),
        Message::Shelf(message) => {
            if matches!(message, shelf::Message::Activate(shelf::Control::RetrySave))
                && matches!(reader.failed_close, Some(CloseAction::Window))
            {
                reader.failed_close = None;
                reader.error = None;
                reader.pending_exit = true;
            }
            let task = reader.shelf.update(message).map(Message::Shelf);
            if reader.pending_exit && reader.shelf.has_failed_save() {
                reader.pending_exit = false;
                reader.failed_close = Some(CloseAction::Window);
                reader.error = reader.shelf.notice.clone();
                task
            } else if reader.pending_exit && reader.exit_ready() {
                iced::exit()
            } else {
                task
            }
        }
        Message::SearchChanged(query) => {
            reader.search_results = reader.shelf.search(&query);
            reader.search_query = query;
            reader.search_selected = 0;
            reader.focused = Some(Control::SearchInput);
            Task::none()
        }
        Message::SearchSubmit if reader.show_search => reader
            .search_results
            .get(reader.search_selected)
            .copied()
            .map_or_else(Task::none, |index| {
                update_inner(reader, Message::OpenLibrary(index))
            }),
        Message::OpenLibrary(index) if reader.interactive() => {
            let Some(entry) = reader.shelf.entries.get(index) else {
                return Task::none();
            };
            let path = entry.document.path.clone();
            let prior = entry.document.clone();
            reader.show_recent = false;
            reader.open_with_import(path, Some(prior), true)
        }
        Message::LocateLibrary(index) if reader.interactive() => {
            let Some(entry) = reader.shelf.entries.get(index) else {
                return Task::none();
            };
            reader.locate(entry.document.clone())
        }
        Message::RemoveLibrary(index)
            if reader.interactive() && reader.confirm_remove.is_none() =>
        {
            let Some(entry) = reader.shelf.entries.get(index) else {
                return Task::none();
            };
            reader.confirm_remove = Some(entry.document.clone());
            reader.show_search = false;
            reader.show_settings = false;
            reader.focused = Some(Control::DismissOverlay);
            Task::none()
        }
        Message::ConfirmRemove if reader.interactive() => {
            let Some(entry) = reader.confirm_remove.take() else {
                return Task::none();
            };
            let path = entry.path;
            if !reader.shelf.entries.iter().any(|e| e.document.path == path) {
                return Task::none();
            }
            reader.focused = None;
            if reader.book.as_ref().is_some_and(|b| b.path == path)
                || reader
                    .pdf
                    .as_ref()
                    .is_some_and(|p| p.document().path == path)
            {
                reader.clear_content();
            }
            reader.removing = Some(path.clone());
            let removed = path.clone();
            Task::perform(
                async move { reader_document::managed::remove(&removed) },
                move |result| Message::RemovedLibrary {
                    path: path.clone(),
                    result,
                },
            )
        }
        Message::RemovedLibrary { path, result } => {
            reader.removing = None;
            if let Err(error) = result {
                reader.error = Some(error);
                return Task::none();
            }
            let library_task = reader.shelf.remove(&path).map(Message::Shelf);
            recent::remove(&mut reader.recent, &path);
            reader.recent_dirty = true;
            reader.search_results = reader.shelf.search(&reader.search_query);
            reader.search_selected = 0;
            reader.focused = None;
            Task::batch([
                library_task,
                reader.persist_recent(),
                reader
                    .shelf
                    .show(reader.book.is_none() && reader.pdf.is_none())
                    .map(Message::Shelf),
            ])
        }
        Message::FindOpen if reader.interactive() && reader.confirm_remove.is_none() => {
            reader.open_find()
        }
        Message::FindChanged(query) if reader.find.is_some() => reader.search_document(query),
        Message::FindStep(forward) if reader.interactive() => reader.step_match(forward),
        Message::FindClose => {
            reader.close_find();
            iced::advanced::widget::operate(iced::advanced::widget::operation::focusable::unfocus())
        }
        Message::FindSearched {
            generation,
            results,
        } => reader.finish_query(generation, results),
        Message::FindIndexed { generation, result } => {
            if generation != reader.find_generation || reader.find.is_none() {
                return Task::none();
            }
            reader.find_task = None;
            let Ok(index) = result else {
                if let Some(find) = &mut reader.find {
                    find.indexing = false;
                }
                return Task::none();
            };
            if reader
                .book
                .as_ref()
                .is_none_or(|book| book.fingerprint != index.fingerprint)
            {
                return Task::none();
            }
            reader.find_index = Some(index);
            let query = reader.find.as_mut().map(|find| {
                find.indexing = false;
                find.query.clone()
            });
            // Widen a search typed while chapters were still being read to the whole book.
            query.map_or_else(Task::none, |query| reader.search_document(query))
        }
        Message::PdfFindIndexed { generation, result } => {
            if generation != reader.find_generation || reader.find.is_none() {
                return Task::none();
            }
            reader.find_task = None;
            let Ok(index) = result else {
                if let Some(find) = &mut reader.find {
                    find.indexing = false;
                    find.notice = Some("Could not read the PDF text".to_owned());
                }
                return Task::none();
            };
            if reader
                .pdf
                .as_ref()
                .is_none_or(|pdf| pdf.document().id != index.document)
            {
                return Task::none();
            }
            reader.pdf_find_index = Some(index);
            let query = reader.find.as_mut().map(|find| {
                find.indexing = false;
                find.query.clone()
            });
            query.map_or_else(Task::none, |query| reader.search_document(query))
        }
        Message::DismissOverlay => {
            reader.confirm_remove = None;
            reader.show_search = false;
            reader.show_settings = false;
            reader.focused = None;
            iced::advanced::widget::operate(iced::advanced::widget::operation::focusable::unfocus())
        }
        Message::FocusReady(generation) if generation == reader.focus_generation => {
            reader.focus_pending = false;
            ui::reveal_focus()
        }
        Message::ToggleAppearance if reader.interactive() => {
            reader.appearance = reader.appearance.toggled();
            reader.preferences_dirty = true;
            reader.persist_preferences()
        }
        Message::WindowMaximized(maximized) => {
            reader.maximized = maximized;
            Task::none()
        }
        Message::SetWindowControls(controls) if reader.interactive() => {
            if reader.window_controls == controls {
                return Task::none();
            }
            reader.window_controls = controls;
            reader.preferences_dirty = true;
            reader.persist_preferences()
        }
        Message::PreferencesLoaded(result) => {
            reader.preferences_loading = false;
            let mut theme_task = Task::none();
            match result {
                Ok(loaded) if !reader.preferences_dirty => {
                    reader.reading.defaults = loaded.reading.validated();
                    reader.appearance = loaded.appearance;
                    reader.window_controls = loaded.window_controls;
                    reader.read_aloud.voice = loaded.voice;
                    reader.read_aloud.rate = loaded.speech_rate;
                    if reader.word_translation.settings != loaded.dictionary.validated() {
                        reader.word_translation.reset();
                    }
                    reader.word_translation.settings = loaded.dictionary.validated();
                    // A book opened from the command line may already be on screen.
                    theme_task = reader.switch_theme(themes::find(&loaded.theme));
                    if reader.book.is_some() {
                        reader.adapted.clear();
                        theme_task = reader.rebuild_geometry(
                            reader.pending_anchor.unwrap_or_else(|| reader.anchor()),
                        );
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    reader.preferences_writable = false;
                    reader.preferences_notice = Some(format!(
                        "{error}. Setting changes apply for this session; the existing preferences file is preserved."
                    ));
                }
            }
            let task = reader.persist_preferences();
            if reader.pending_exit && reader.exit_ready() {
                iced::exit()
            } else {
                Task::batch([theme_task, task])
            }
        }
        Message::PreferencesSaved(result) => {
            reader.preferences_saving = false;
            reader.preferences_notice = result
                .err()
                .map(|error| format!("Could not save settings: {error}"));
            let task = reader.persist_preferences();
            if reader.pending_exit && reader.exit_ready() {
                iced::exit()
            } else {
                task
            }
        }
        Message::RecentLoaded(result) => {
            reader.recent_loading = false;
            match result {
                Ok(entries) => reader.recent = entries,
                Err(error) => {
                    reader.history_corrupt = true;
                    reader.history_notice = Some(format!(
                        "Recent history could not be read: {error}. Its original file will remain untouched until you choose Reset recent history."
                    ));
                }
            }
            for (entry, old) in std::mem::take(&mut reader.recent_pending) {
                if let Some(old) = old {
                    recent::remove(&mut reader.recent, &old.path);
                }
                recent::remember(&mut reader.recent, entry);
                reader.recent_dirty = true;
            }
            let task = reader.persist_recent();
            if reader.pending_exit && reader.exit_ready() {
                iced::exit()
            } else {
                task
            }
        }
        Message::RecentSaved(result) => {
            reader.recent_saving = false;
            match result {
                Ok(()) => reader.history_notice = None,
                Err(error) => {
                    reader.history_notice = Some(format!("Could not save recent history: {error}"));
                }
            }
            let task = reader.persist_recent();
            if reader.pending_exit && reader.exit_ready() {
                iced::exit()
            } else {
                task
            }
        }
        Message::ResetRecent if reader.interactive() && reader.history_corrupt => {
            reader.history_corrupt = false;
            reader.recent_dirty = true;
            reader.persist_recent()
        }
        Message::ToggleToolbar if reader.book.is_some() || reader.pdf.is_some() => {
            reader.toolbar_expanded = !reader.toolbar_expanded;
            if !reader.toolbar_expanded {
                reader.show_contents = false;
                reader.show_help = false;
            }
            reader.focused = Some(Control::ToggleToolbar);
            restore_viewport(reader)
        }
        Message::ToggleRecent if reader.interactive() => {
            reader.show_settings = false;
            reader.show_search = false;
            reader.show_recent = !reader.show_recent;
            reader.focused = if reader.show_recent {
                Some(if reader.recent.is_empty() {
                    Control::HideRecent
                } else {
                    Control::OpenRecent(0)
                })
            } else {
                None
            };
            restore_viewport(reader)
        }
        Message::ToggleHelp if reader.interactive() => {
            reader.show_settings = false;
            reader.show_search = false;
            reader.show_help = !reader.show_help;
            reader.focused = reader.show_help.then_some(Control::HideHelp);
            restore_viewport(reader)
        }
        Message::OpenRecent(index) if reader.interactive() => {
            let Some(entry) = reader.recent.get(index).cloned() else {
                return Task::none();
            };
            reader.show_recent = false;
            reader.open(entry.path, None)
        }
        Message::LocateRecent(index) if reader.interactive() => {
            let Some(entry) = reader.recent.get(index).cloned() else {
                return Task::none();
            };
            reader.locate(entry)
        }
        Message::LocateMissing if reader.interactive() => {
            let Some(entry) = reader.missing_recent.clone() else {
                return Task::none();
            };
            reader.locate(entry)
        }
        Message::RemoveRecent(index) if reader.interactive() => {
            let Some(entry) = reader.recent.get(index).cloned() else {
                return Task::none();
            };
            recent::remove(&mut reader.recent, &entry.path);
            reader.recent_dirty = true;
            Task::batch([
                reader.persist_recent(),
                reader.shelf.remove(&entry.path).map(Message::Shelf),
            ])
        }
        Message::DismissError => {
            reader.error = None;
            reader.missing_recent = None;
            reader.failed_close = None;
            restore_viewport(reader)
        }
        Message::OpenDialog if reader.interactive() => {
            reader.show_search = false;
            reader.show_settings = false;
            reader.dialog_open = true;
            reader.selection.end_drag();
            Task::perform(
                async { crate::platform::open_document_dialog(false) },
                Message::DialogChosen,
            )
        }
        Message::DialogChosen(result) => {
            reader.dialog_open = false;
            let locate = reader.dialog_locate.take();
            let task = match result {
                Ok(Some(path)) => {
                    let import = locate.is_none();
                    let prior = locate.or_else(|| {
                        reader
                            .shelf
                            .entries
                            .iter()
                            .find(|e| {
                                e.document.path
                                    == path.canonicalize().unwrap_or_else(|_| path.clone())
                            })
                            .map(|e| e.document.clone())
                    });
                    reader.open_with_import(path, prior, import)
                }
                Ok(None) => Task::none(),
                Err(error) => {
                    reader.error = Some(error);
                    Task::none()
                }
            };
            if let Some(window) = reader.window {
                Task::batch([task, window::gain_focus(window)])
            } else {
                task
            }
        }
        Message::Loaded { request, result } if request == reader.request && !reader.saving => {
            reader.word_translation.reset();
            reader.opening = None;
            reader.opening_task = None;
            let navigation = reader.pending_navigation.take();
            let relocated = reader.pending_locate.take();
            let recent_open = reader.recent_open.take();
            let history_task = result.as_ref().ok().map_or_else(Task::none, |document| {
                reader.missing_recent = None;
                reader.remember_recent(document.document.recent_entry(), relocated.as_ref())
            });
            let (result, library_task) = match result {
                Ok(reply) => {
                    let task = reply.catalog.map_or_else(Task::none, |entry| {
                        reader
                            .shelf
                            .remember(entry, relocated.as_ref().map(|entry| entry.path.as_path()))
                            .map(Message::Shelf)
                    });
                    (Ok(reply.document), task)
                }
                Err(error) => (Err(error), Task::none()),
            };
            let content_task = match result {
                Ok(LoadedDocument::Reflow(book)) => {
                    let same_document = reader
                        .book
                        .as_ref()
                        .is_some_and(|current| current.fingerprint == book.fingerprint);
                    let at_end = matches!(navigation, Some(Navigation::End));
                    let returns = navigation
                        .as_ref()
                        .map(|_| std::mem::take(&mut reader.returns));
                    reader.clear_content();
                    if let Some(returns) = returns {
                        reader.returns = returns;
                    }
                    if let Some(navigation) = navigation {
                        reader.commit_navigation(navigation);
                    }
                    reader.font_size = DEFAULT_FONT_SIZE;
                    let anchor = book
                        .restored
                        .as_ref()
                        .and_then(|position| {
                            book.items
                                .iter()
                                .position(|item| {
                                    item.id() == position.item_id
                                        || book
                                            .anchors
                                            .get(&position.item_id)
                                            .is_some_and(|id| item.id() == id)
                                })
                                .map(|row| Anchor {
                                    row,
                                    fraction: position.within,
                                })
                        })
                        .unwrap_or(Anchor {
                            row: 0,
                            fraction: 0.0,
                        });
                    reader.book = Some(book);
                    reader.error = None;
                    let anchor = if let Some(id) = reader.notes.pending_reveal.take() {
                        reader.refresh_marks();
                        match reader.notes.marks.iter().find(|mark| mark.id == id) {
                            Some(mark) => {
                                let row = mark.bounds.start_item;
                                let length = reader
                                    .book
                                    .as_ref()
                                    .and_then(|book| book.items.get(row))
                                    .and_then(Item::text)
                                    .map_or(1, |text| text.len().max(1));
                                Anchor {
                                    row,
                                    fraction: (mark.bounds.start_byte as f32 / length as f32)
                                        .clamp(0.0, 1.0),
                                }
                            }
                            None => {
                                reader.error =
                                    Some("This passage was not found in the book any more.".into());
                                anchor
                            }
                        }
                    } else {
                        anchor
                    };
                    reader.selection.clear();
                    if same_document {
                        reader.restore_find_selection();
                    } else {
                        reader.reset_find();
                        reader.adapted.clear();
                    }
                    let task = reader.rebuild_geometry(anchor);
                    if at_end && reader.pagination.is_some() {
                        reader.pagination_end = true;
                        task
                    } else if at_end && let Some(last) = reader.pages().last() {
                        let anchor = Anchor {
                            row: last.rows.start,
                            fraction: (last.content.start - reader.heights.start(last.rows.start))
                                / reader.heights.height(last.rows.start),
                        };
                        reader.offset = reader.offset_for(anchor);
                        reader.pending_anchor = Some(anchor);
                        reader.restore_book_scroll()
                    } else {
                        task
                    }
                }
                Ok(LoadedDocument::Pdf {
                    document,
                    restored,
                    warnings,
                    ..
                }) => {
                    reader.clear_content();
                    reader.reset_find();
                    if reader.notes.book.as_deref() != Some(document.fingerprint.as_str()) {
                        reader.notes.list = None;
                    }
                    let id = document.id;
                    let (pdf, task) = pdf_reader::Reader::new(
                        document,
                        restored,
                        reader.document_view_size(),
                        reader.scale_factor,
                    );
                    reader.pdf = Some(pdf);
                    reader.pdf_warnings = warnings;
                    reader.error = None;
                    forward_pdf(id, task)
                }
                Err(error) => {
                    reader.notes.pending_reveal = None;
                    reader.missing_recent = relocated.or(recent_open);
                    reader.error = Some(if let Some(entry) = &reader.missing_recent {
                        format!(
                            "Could not open {}: {error}. Use Locate... to select its new location, or Remove it from Recent.",
                            entry.path.display()
                        )
                    } else {
                        error
                    });
                    restore_viewport(reader)
                }
            };
            reader.capture_progress();
            let notes_task = reader.sync_notes();
            let reading_task = reader.sync_reading_settings();
            let has_document = reader.book.is_some() || reader.pdf.is_some();
            Task::batch([
                content_task,
                notes_task,
                reading_task,
                history_task,
                library_task,
                reader.shelf.show(!has_document).map(Message::Shelf),
            ])
        }
        Message::Pdf { document, message } => {
            if reader.pdf.as_ref().map(|pdf| pdf.document().id) != Some(document) {
                return Task::none();
            }
            let mut double_word = None;
            match &message {
                pdf_reader::Message::Context(point) => {
                    reader.context_pdf(*point);
                    return Task::none();
                }
                pdf_reader::Message::SelectStart(point) => {
                    if reader.word_click() {
                        double_word = Some(*point);
                    }
                    reader.notes.pressed = reader.pdf.as_ref().and_then(|pdf| pdf.mark_at(*point));
                }
                pdf_reader::Message::Scroll { .. }
                | pdf_reader::Message::ClearSelection
                | pdf_reader::Message::Previous
                | pdf_reader::Message::Next
                | pdf_reader::Message::PageSubmit
                | pdf_reader::Message::ZoomIn
                | pdf_reader::Message::ZoomOut
                | pdf_reader::Message::ActualSize
                | pdf_reader::Message::FitWidth
                | pdf_reader::Message::ZoomBy(_) => reader.word_translation.dismiss(),
                _ => {}
            }
            match &mut reader.pdf {
                Some(pdf) => {
                    let task = forward_pdf(document, pdf.update(message));
                    if let Some(point) = double_word {
                        pdf.select_word(point);
                    }
                    task
                }
                None => Task::none(),
            }
        }
        Message::EpubChapter { index, fragment } => reader.open_chapter(index, fragment),
        Message::BookMode if reader.interactive() => {
            reader.show_conversion = true;
            reader.convert_pdf()
        }
        Message::CancelConversion => {
            reader.cancel_open();
            reader.show_conversion = false;
            reader.error = None;
            restore_viewport(reader)
        }
        Message::DocumentMode { source_page } => reader.show_original(source_page),
        Message::BookPage(next) => reader.adjacent_book_page(next),
        Message::FollowLink(href) => reader.follow_link(href),
        Message::Back => reader.go_back(),
        Message::ToggleContents if reader.interactive() => {
            if !reader.contents().is_empty() {
                reader.show_contents = !reader.show_contents;
                if reader.show_contents {
                    reader.toolbar_expanded = true;
                }
                reader.contents_offset = 0.0;
                reader.focused = if reader.show_contents && !reader.contents_range().is_empty() {
                    Some(Control::ContentsEntry(0))
                } else {
                    Some(Control::Contents)
                };
                return Task::batch([
                    restore_viewport(reader),
                    scroll_to_widget(iced::advanced::widget::Id::new("epub-contents"), 0.0),
                ]);
            }
            Task::none()
        }
        Message::ContentsScrolled(offset) => {
            reader.contents_offset = offset;
            Task::none()
        }
        Message::ScaleFactor(scale) => {
            if !scale.is_finite() || scale <= 0.0 || scale == reader.scale_factor {
                return Task::none();
            }
            reader.scale_factor = scale;
            let view_size = reader.document_view_size();
            if let Some(pdf) = &mut reader.pdf {
                forward_pdf(pdf.document().id, pdf.resize(view_size, scale))
            } else {
                Task::none()
            }
        }
        Message::AtlasReady { generation, result } => reader.finish_atlas(generation, result),
        Message::SetTheme(index) if reader.interactive() => {
            let theme = &themes::THEMES[index.min(themes::THEMES.len() - 1)];
            reader.apply_theme(theme)
        }
        Message::ThemeLayoutReady {
            generation,
            section,
            theme,
            result,
        } => {
            if generation != reader.generation || reader.pagination.is_none() {
                return Task::none();
            }
            let anchor = reader.pending_anchor.unwrap_or(Anchor {
                row: 0,
                fraction: 0.0,
            });
            match result {
                Ok(Some(layout)) => {
                    let Some(fingerprint) = reader.book.as_ref().map(|b| b.fingerprint.clone())
                    else {
                        return Task::none();
                    };
                    reader.adapted.retain(|a| {
                        !(a.fingerprint == fingerprint && a.section == section && a.theme == theme)
                    });
                    reader.adapted.push(AdaptedLayout {
                        fingerprint,
                        section,
                        theme,
                        layout,
                    });
                    if reader.adapted.len() > ADAPTED_SECTIONS {
                        reader.adapted.remove(0);
                    }
                    let end = reader.pagination_end;
                    let task = reader.rebuild_geometry(anchor);
                    reader.pagination_end = false;
                    if end {
                        return reader.go_to_local_page(reader.pages().len().saturating_sub(1));
                    }
                    task
                }
                Ok(None) => Task::none(),
                Err(error) => {
                    // Fall back to the default layout rather than leaving the book blank.
                    reader.pagination = None;
                    reader.error = Some(format!("Could not lay out the {theme} theme: {error}"));
                    reader.theme = themes::default_theme();
                    reader.adapted.clear();
                    reader.preferences_dirty = true;
                    let saved = reader.persist_preferences();
                    Task::batch([saved, reader.rebuild_geometry(anchor)])
                }
            }
        }
        Message::PageInput(value) => {
            reader.page_input = Some(value.chars().take(32).collect());
            Task::none()
        }
        Message::PageSubmit => {
            reader.focused = None;
            Task::batch([
                reader.jump_page(),
                iced::advanced::widget::operate(
                    iced::advanced::widget::operation::focusable::unfocus(),
                ),
            ])
        }
        Message::ZoomBy(notches) if notches.is_finite() => update_inner(
            reader,
            Message::Zoom(reader.zoom * 1.1_f32.powf(notches.clamp(-10.0, 10.0))),
        ),
        Message::Zoom(zoom) if reader.interactive() && zoom.is_finite() => {
            let anchor = reader.anchor();
            if reader.pagination.is_none() {
                reader.generation = reader.generation.wrapping_add(1);
            }
            let old = reader.zoom;
            reader.fit_width = false;
            reader.zoom = zoom.clamp(0.25, 3.0);
            reader.viewport *= old / reader.zoom;
            reader.offset = reader.offset_for(anchor);
            reader.pending_anchor = Some(anchor);
            reader.restore_book_scroll()
        }
        Message::SelectStart(endpoint) if reader.interactive() => {
            let double = reader.word_click();
            reader.notes.pressed = reader
                .book
                .as_ref()
                .and_then(|book| notes::mark_at(&reader.notes.marks, &book.items, &endpoint));
            reader.selection.begin(endpoint.clone());
            if double {
                reader.select_word(&endpoint);
            }
            Task::none()
        }
        Message::Context(endpoint) => {
            reader.context_at(endpoint);
            Task::none()
        }
        Message::ContextBlank => {
            if reader.book.is_some() {
                reader.open_popup(notes::Target::Page, true);
            }
            Task::none()
        }
        Message::Notes(action) => reader.notes_action(action),
        Message::NotesLoaded {
            profile,
            fingerprint,
            result,
        } => {
            if profile == reader.profile.epoch {
                reader.notes_loaded(fingerprint, result);
            }
            Task::none()
        }
        Message::NotesSaved(result) => {
            let task = reader.notes_saved(result);
            if reader.pending_exit && reader.exit_ready() {
                iced::exit()
            } else {
                task
            }
        }
        Message::PdfHighlightText {
            document,
            from,
            to,
            merge_ids,
            color,
            then_note,
            result,
        } => reader.pdf_highlight_text(document, (from, to), merge_ids, color, then_note, result),
        Message::SelectMove(endpoint) if reader.interactive() => {
            if let Some((from, to)) = &reader.word_translation.word
                && from.item_id == endpoint.item_id
                && from.byte_offset <= endpoint.byte_offset
                && endpoint.byte_offset <= to.byte_offset
            {
                return Task::none();
            }
            reader.word_translation.word = None;
            reader.selection.extend(endpoint);
            Task::none()
        }
        Message::Copy => reader
            .book
            .as_ref()
            .and_then(|book| reader.selection.copy_text(&book.items))
            .filter(|text| !text.is_empty())
            .map_or_else(Task::none, iced::clipboard::write),
        Message::LayoutReady(generation) if generation == reader.generation => {
            reader.refine_geometry()
        }
        Message::Scroll {
            generation,
            offset,
            viewport,
            page_top,
        } if generation == reader.generation => {
            if let Some(page) = reader.active_page()
                && page.top == page_top
            {
                reader.viewport = viewport / reader.zoom;
                if reader.pending_anchor.is_none() {
                    reader.offset = page.top
                        + (offset / reader.zoom)
                            .clamp(0.0, (page.height - reader.viewport).max(0.0));
                }
            }
            Task::none()
        }
        Message::Close(action) => reader.close(action),
        Message::Saved { action, result } => {
            reader.saving = false;
            match result {
                Ok(()) => reader.finish_close(action),
                Err(error) => {
                    reader.error = Some(format!("Could not save your reading position: {error}"));
                    reader.failed_close = Some(action);
                    Task::none()
                }
            }
        }
        Message::CloseWithoutSaving => reader.failed_close.map_or_else(Task::none, |action| {
            reader.notes.discard_failed_save();
            reader.shelf.discard_failed_saves();
            reader.finish_close(action)
        }),
        Message::Event(event, id) => {
            if let iced::Event::Mouse(mouse::Event::CursorMoved { position }) = &event {
                reader.pointer = *position;
            }
            // The note editor is modal: it keeps the keyboard until it is saved or cancelled.
            if reader.word_translation.popup.is_some() {
                if matches!(&event, iced::Event::Window(window::Event::Unfocused))
                    || matches!(
                        &event,
                        iced::Event::Mouse(
                            mouse::Event::ButtonPressed(mouse::Button::Left)
                                | mouse::Event::WheelScrolled { .. }
                        )
                    ) && !reader
                        .word_translation
                        .contains(reader.pointer, reader.window_size)
                {
                    reader.word_translation.dismiss();
                }
                if let iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key,
                    modifiers,
                    repeat: false,
                    ..
                }) = &event
                {
                    match key.as_ref() {
                        Key::Named(key::Named::Escape) => {
                            reader.word_translation.dismiss();
                            reader.focused = None;
                            return Task::none();
                        }
                        Key::Named(key::Named::Tab) => {
                            let controls = reader.dictionary_controls();
                            if !controls.is_empty() {
                                let current = controls
                                    .iter()
                                    .position(|control| Some(*control) == reader.focused);
                                let next = match current {
                                    Some(index) if modifiers.shift() => {
                                        (index + controls.len() - 1) % controls.len()
                                    }
                                    Some(index) => (index + 1) % controls.len(),
                                    None if modifiers.shift() => controls.len() - 1,
                                    None => 0,
                                };
                                reader.focused = Some(controls[next]);
                            }
                            return Task::none();
                        }
                        Key::Named(key::Named::Enter | key::Named::Space) => {
                            if let Some(control @ Control::Dictionary(_)) = reader.focused {
                                return reader.activate(control);
                            }
                        }
                        _ => {}
                    }
                }
            }
            if reader.show_settings
                && reader.reading.picker.is_some()
                && matches!(
                    &event,
                    iced::Event::Keyboard(keyboard::Event::KeyPressed {
                        key: Key::Named(key::Named::Escape),
                        ..
                    })
                )
            {
                reader.reading.picker = None;
                return Task::none();
            }
            if reader.show_settings
                && reader.word_translation.picker.is_some()
                && matches!(
                    &event,
                    iced::Event::Keyboard(keyboard::Event::KeyPressed {
                        key: Key::Named(key::Named::Escape),
                        ..
                    })
                )
            {
                reader.word_translation.picker = None;
                return Task::none();
            }
            if reader.notes.editor.is_some()
                && let iced::Event::Keyboard(keyboard_event) = &event
            {
                if let keyboard::Event::KeyPressed { key, modifiers, .. } = keyboard_event {
                    match key.as_ref() {
                        Key::Named(key::Named::Escape) => {
                            return reader.notes_action(notes::Action::CancelNote);
                        }
                        Key::Named(key::Named::Enter) if modifiers.control() => {
                            return reader.notes_action(notes::Action::SaveNote);
                        }
                        _ => {}
                    }
                }
                return Task::none();
            }
            // A click anywhere but the menu, or scrolling, puts the menu away.
            if reader.notes.popup.is_some()
                && (matches!(
                    &event,
                    iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                ) && !reader.popup_contains(reader.pointer)
                    || matches!(
                        &event,
                        iced::Event::Mouse(mouse::Event::WheelScrolled { .. })
                    ))
            {
                reader.notes.popup = None;
            }
            if reader.confirm_remove.is_some() && matches!(&event, iced::Event::Keyboard(_)) {
                if let iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key,
                    repeat: false,
                    ..
                }) = &event
                {
                    return match key.as_ref() {
                        Key::Named(key::Named::Escape) => {
                            update_inner(reader, Message::DismissOverlay)
                        }
                        Key::Named(key::Named::Tab) => {
                            reader.focused =
                                Some(if reader.focused == Some(Control::ConfirmRemove) {
                                    Control::DismissOverlay
                                } else {
                                    Control::ConfirmRemove
                                });
                            Task::none()
                        }
                        Key::Named(key::Named::Enter | key::Named::Space) => reader
                            .focused
                            .map_or_else(Task::none, |control| reader.activate(control)),
                        _ => Task::none(),
                    };
                }
                return Task::none();
            }
            if reader.show_conversion
                && let iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key: Key::Named(key::Named::Escape),
                    ..
                }) = &event
            {
                return update_inner(reader, Message::CancelConversion);
            }
            if let iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                repeat,
                ..
            }) = &event
                && reader.interactive()
            {
                match key.as_ref() {
                    Key::Named(key::Named::Escape) if !repeat && reader.notes.popup.is_some() => {
                        reader.notes.popup = None;
                        reader.focused = None;
                        return Task::none();
                    }
                    Key::Named(key::Named::ArrowDown | key::Named::ArrowUp)
                        if reader.notes.popup.is_some() =>
                    {
                        reader.step_popup_focus(matches!(
                            key.as_ref(),
                            Key::Named(key::Named::ArrowDown)
                        ));
                        return Task::none();
                    }
                    // Ctrl+D bookmarks the page, Ctrl+H highlights the selection, Ctrl+B lists them.
                    Key::Character(value)
                        if modifiers.control()
                            && !modifiers.shift()
                            && !repeat
                            && (reader.book.is_some() || reader.pdf.is_some())
                            && !reader.show_search
                            && !reader.show_settings
                            && matches!(value.to_ascii_lowercase().as_str(), "d" | "h" | "b") =>
                    {
                        reader.notes.popup = None;
                        return match value.to_ascii_lowercase().as_str() {
                            "d" => reader.toggle_bookmark(),
                            "h" => reader.highlight_shortcut(),
                            _ => reader.notes_action(notes::Action::ToggleList),
                        };
                    }
                    // Ctrl+Shift+U reads aloud, or stops reading.
                    Key::Character(value)
                        if modifiers.control()
                            && modifiers.shift()
                            && value.eq_ignore_ascii_case("u")
                            && !repeat
                            && (reader.book.is_some() || reader.pdf.is_some())
                            && !reader.show_search
                            && !reader.show_settings =>
                    {
                        reader.notes.popup = None;
                        return reader.read_aloud(read_aloud::Action::Toggle);
                    }
                    Key::Character(value)
                        if modifiers.control() && value.eq_ignore_ascii_case("k") && !repeat =>
                    {
                        return update_inner(reader, Message::Chrome(chrome::Action::Search));
                    }
                    // Both reading modes use Ctrl+Shift+F for fit width.
                    Key::Character(value)
                        if modifiers.control()
                            && modifiers.shift()
                            && value.eq_ignore_ascii_case("f")
                            && !repeat
                            && reader.book.is_some()
                            && !reader.show_search
                            && !reader.show_settings =>
                    {
                        return reader.fit_book_width();
                    }
                    // Ctrl+F toggles the find bar.
                    Key::Character(value)
                        if modifiers.control()
                            && !modifiers.shift()
                            && value.eq_ignore_ascii_case("f")
                            && !repeat
                            && (reader.book.is_some() || reader.pdf.is_some())
                            && !reader.show_search
                            && !reader.show_settings =>
                    {
                        return update_inner(
                            reader,
                            if reader.find.is_some() {
                                Message::FindClose
                            } else {
                                Message::FindOpen
                            },
                        );
                    }
                    Key::Named(key::Named::Enter)
                        if reader.find.is_some()
                            && !reader.show_search
                            && !reader.show_settings
                            && matches!(reader.focused, None | Some(Control::FindInput)) =>
                    {
                        return update_inner(reader, Message::FindStep(!modifiers.shift()));
                    }
                    Key::Named(key::Named::Escape)
                        if !repeat
                            && reader.find.is_some()
                            && !reader.show_search
                            && !reader.show_settings =>
                    {
                        return update_inner(reader, Message::FindClose);
                    }
                    Key::Named(key::Named::ArrowDown | key::Named::ArrowUp)
                        if reader.show_search =>
                    {
                        let count = reader.search_results.len().min(8);
                        reader.search_selected =
                            if matches!(key.as_ref(), Key::Named(key::Named::ArrowDown)) {
                                (reader.search_selected + 1).min(count.saturating_sub(1))
                            } else {
                                reader.search_selected.saturating_sub(1)
                            };
                        reader.focused = reader
                            .search_results
                            .get(reader.search_selected)
                            .copied()
                            .map(Control::SearchResult);
                        return Task::none();
                    }
                    Key::Named(key::Named::ArrowLeft) if modifiers.alt() && !repeat => {
                        return reader.go_back();
                    }
                    Key::Named(key::Named::Tab) if !repeat => {
                        let mut controls = reader.controls();
                        let count = controls.clone().count();
                        let current = reader.focused.and_then(|focused| {
                            controls.clone().position(|control| control == focused)
                        });
                        let next = if modifiers.shift() {
                            current.map_or(count - 1, |index| (index + count - 1) % count)
                        } else {
                            current.map_or(0, |index| (index + 1) % count)
                        };
                        let selected = controls.nth(next);
                        drop(controls);
                        reader.focused = selected;
                        let unfocus = iced::advanced::widget::operate(
                            iced::advanced::widget::operation::focusable::unfocus(),
                        );
                        return match reader.focused {
                            Some(Control::BookPageInput) => iced::widget::operation::focus(
                                iced::advanced::widget::Id::new("book-page"),
                            ),
                            Some(Control::SearchInput) => {
                                iced::widget::operation::focus(search_id())
                            }
                            Some(Control::FindInput) => iced::widget::operation::focus(find_id()),
                            Some(Control::Pdf(pdf_reader::FocusControl::Page)) => {
                                iced::widget::operation::focus(pdf_reader::page_input_id())
                            }
                            _ => unfocus,
                        };
                    }
                    Key::Named(key::Named::ArrowDown | key::Named::ArrowUp)
                        if matches!(reader.focused, Some(Control::ContentsEntry(_))) =>
                    {
                        let Control::ContentsEntry(index) = reader.focused.unwrap() else {
                            unreachable!()
                        };
                        let count = reader.contents().len();
                        let next = if matches!(key.as_ref(), Key::Named(key::Named::ArrowDown)) {
                            (index + 1).min(count.saturating_sub(1))
                        } else {
                            index.saturating_sub(1)
                        };
                        reader.focused = Some(Control::ContentsEntry(next));
                        reader.contents_offset = next as f32 * CONTENT_ROW_HEIGHT;
                        return Task::batch([
                            scroll_to_widget(
                                iced::advanced::widget::Id::new("epub-contents"),
                                reader.contents_offset,
                            ),
                            restore_viewport(reader),
                        ]);
                    }
                    Key::Named(key::Named::Enter | key::Named::Space)
                        if reader.focused.is_some() && !repeat && !modifiers.control() =>
                    {
                        return reader.activate(reader.focused.unwrap());
                    }
                    Key::Named(key::Named::F11) if !repeat => {
                        return update_inner(reader, Message::ToggleFullscreen);
                    }
                    Key::Named(key::Named::F8) if !repeat => {
                        return update_inner(reader, Message::ToggleToolbar);
                    }
                    Key::Named(key::Named::F1) if !repeat => {
                        return update_inner(reader, Message::ToggleHelp);
                    }
                    Key::Named(key::Named::Escape) if !repeat => {
                        if reader.show_conversion {
                            return update_inner(reader, Message::CancelConversion);
                        }
                        if reader.show_search || reader.show_settings {
                            reader.focused = None;
                            return update_inner(reader, Message::DismissOverlay);
                        }
                        // In the library, Escape first closes a shelf menu or name field.
                        if reader.book.is_none() && reader.pdf.is_none() && reader.shelf.dismiss() {
                            return Task::none();
                        }
                        reader.focused = None;
                        if reader.show_help {
                            return update_inner(reader, Message::ToggleHelp);
                        }
                        if reader.show_recent {
                            return update_inner(reader, Message::ToggleRecent);
                        }
                        if reader.show_contents {
                            return update_inner(reader, Message::ToggleContents);
                        }
                        if reader.error.is_some() {
                            return update_inner(reader, Message::DismissError);
                        }
                        if reader.fullscreen {
                            return reader.toggle_fullscreen();
                        }
                    }
                    Key::Character(value)
                        if modifiers.control() && value.eq_ignore_ascii_case("r") && !repeat =>
                    {
                        return update_inner(reader, Message::ToggleRecent);
                    }
                    Key::Character(value)
                        if modifiers.control()
                            && value.eq_ignore_ascii_case("l")
                            && reader.book.is_some() =>
                    {
                        reader.toolbar_expanded = true;
                        reader.focused = Some(Control::BookPageInput);
                        return iced::widget::operation::focus(iced::advanced::widget::Id::new(
                            "book-page",
                        ));
                    }
                    Key::Character(value)
                        if modifiers.control()
                            && value.eq_ignore_ascii_case("l")
                            && reader.pdf.is_some() =>
                    {
                        reader.toolbar_expanded = true;
                        reader.focused = Some(Control::Pdf(pdf_reader::FocusControl::Page));
                        return Task::batch([
                            restore_viewport(reader),
                            iced::widget::operation::focus(pdf_reader::page_input_id()),
                        ]);
                    }
                    _ => {}
                }
            }
            if (reader.show_search || reader.show_settings)
                && matches!(&event, iced::Event::Keyboard(_))
            {
                return Task::none();
            }
            if matches!(
                &event,
                iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            ) {
                reader.focused = None;
            }
            let pdf_input = matches!(
                &event,
                iced::Event::Keyboard(keyboard::Event::KeyPressed { .. })
                    | iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                    | iced::Event::Mouse(
                        mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft
                    )
                    | iced::Event::Window(window::Event::Unfocused)
            );
            let global_shortcut = matches!(&event,
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key: Key::Character(value), modifiers, ..
                }) if modifiers.control() && (value.eq_ignore_ascii_case("o") || value.eq_ignore_ascii_case("w") || value.eq_ignore_ascii_case("r") || value.eq_ignore_ascii_case("l") || value.eq_ignore_ascii_case("k")));
            if pdf_input && !global_shortcut && !reader.show_search && !reader.show_settings {
                let accepted = reader.interactive() || !matches!(event, iced::Event::Keyboard(_));
                if let Some(pdf) = &mut reader.pdf {
                    let was_dragging = pdf.is_dragging();
                    let task = if accepted {
                        forward_pdf(pdf.document().id, pdf.event(&event))
                    } else {
                        Task::none()
                    };
                    if matches!(
                        &event,
                        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                    ) {
                        return Task::batch([task, reader.selection_released(was_dragging)]);
                    }
                    return task;
                }
            }
            match event {
                iced::Event::Window(window::Event::Opened { size, .. }) => {
                    reader.window = Some(id);
                    reader.window_size = size;
                    reader.width = reader.reading_width(size.width);
                    reader.viewport = (size.height - 100.0).max(100.0);
                    let view_size = reader.document_view_size();
                    let task = if let Some(pdf) = &mut reader.pdf {
                        forward_pdf(
                            pdf.document().id,
                            pdf.resize(view_size, reader.scale_factor),
                        )
                    } else if reader.book.is_some() {
                        reader.rebuild_geometry(reader.anchor())
                    } else {
                        Task::none()
                    };
                    Task::batch([
                        task,
                        reader.shelf.resize(size).map(Message::Shelf),
                        window::scale_factor(id).map(|scale| Message::ScaleFactor(scale.into())),
                        window::is_maximized(id).map(Message::WindowMaximized),
                    ])
                }
                iced::Event::Window(window::Event::Resized(size)) => {
                    let anchor = reader.anchor();
                    let width = reader.reading_width(size.width);
                    let width_changed = (width - reader.width).abs() > 0.5;
                    reader.width = width;
                    reader.viewport = (reader.viewport
                        + (size.height - reader.window_size.height) / reader.zoom)
                        .max(1.0);
                    reader.window_size = size;
                    let view_size = reader.document_view_size();
                    let task = if let Some(pdf) = &mut reader.pdf {
                        forward_pdf(
                            pdf.document().id,
                            pdf.resize(view_size, reader.scale_factor),
                        )
                    } else if width_changed && reader.book.is_some() {
                        reader.rebuild_geometry(anchor)
                    } else {
                        Task::none()
                    };
                    Task::batch([
                        task,
                        reader.shelf.resize(size).map(Message::Shelf),
                        window::scale_factor(id).map(|scale| Message::ScaleFactor(scale.into())),
                        window::is_maximized(id).map(Message::WindowMaximized),
                    ])
                }
                iced::Event::Window(window::Event::CloseRequested) => {
                    reader.close(CloseAction::Window)
                }
                iced::Event::Window(window::Event::FileHovered(_)) => {
                    reader.dropping = true;
                    Task::none()
                }
                iced::Event::Window(window::Event::FilesHoveredLeft) => {
                    reader.dropping = false;
                    Task::none()
                }
                iced::Event::Window(window::Event::FileDropped(path)) => {
                    reader.open_with_import(path, None, true)
                }
                iced::Event::Window(window::Event::Unfocused)
                | iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    let was_dragging = reader.selection.is_dragging();
                    reader.selection.end_drag();
                    if matches!(event, iced::Event::Mouse(_)) {
                        return reader.selection_released(was_dragging);
                    }
                    Task::none()
                }
                iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    match key.as_ref() {
                        Key::Character(value) if modifiers.control() => {
                            match value.to_ascii_lowercase().as_str() {
                                "o" => update_inner(reader, Message::OpenDialog),
                                "w" => reader.close(CloseAction::Document),
                                "c" => update_inner(reader, Message::Copy),
                                "a" if reader.interactive() => {
                                    if let Some(book) = &reader.book {
                                        let first = book.items.iter().find(|item| {
                                            item.text().is_some_and(|text| !text.is_empty())
                                        });
                                        let last = book.items.iter().rfind(|item| {
                                            item.text().is_some_and(|text| !text.is_empty())
                                        });
                                        if let (Some(first), Some(last)) = (first, last) {
                                            reader.selection.begin(Endpoint {
                                                item_id: first.id().to_owned(),
                                                byte_offset: 0,
                                            });
                                            reader.selection.extend(Endpoint {
                                                item_id: last.id().to_owned(),
                                                byte_offset: last.text().map_or(0, str::len),
                                            });
                                            reader.selection.end_drag();
                                        }
                                    }
                                    Task::none()
                                }
                                "+" | "=" => update_inner(reader, Message::Zoom(reader.zoom + 0.1)),
                                "-" => update_inner(reader, Message::Zoom(reader.zoom - 0.1)),
                                "0" => update_inner(reader, Message::Zoom(1.0)),
                                "t" => update_inner(reader, Message::ToggleContents),
                                _ => Task::none(),
                            }
                        }
                        Key::Named(key::Named::PageDown)
                            if modifiers.control()
                                && reader.book.as_ref().is_some_and(|book| book.epub.is_some()) =>
                        {
                            reader.adjacent_chapter(true)
                        }
                        Key::Named(key::Named::PageUp)
                            if modifiers.control()
                                && reader.book.as_ref().is_some_and(|book| book.epub.is_some()) =>
                        {
                            reader.adjacent_chapter(false)
                        }
                        Key::Named(key::Named::Space)
                            if reader.book.is_none() && reader.pdf.is_none() =>
                        {
                            reader
                                .shelf
                                .first_resume()
                                .map_or_else(Task::none, |index| {
                                    update_inner(reader, Message::OpenLibrary(index))
                                })
                        }
                        Key::Named(key::Named::ArrowRight | key::Named::ArrowLeft)
                            if !modifiers.control()
                                && !modifiers.alt()
                                && reader.book.is_some() =>
                        {
                            reader.adjacent_book_page(matches!(
                                key.as_ref(),
                                Key::Named(key::Named::ArrowRight)
                            ))
                        }
                        Key::Named(key::Named::ArrowUp) => {
                            reader.jump(reader.offset - reader.font_size * reader.line_height())
                        }
                        Key::Named(key::Named::ArrowDown) => {
                            reader.jump(reader.offset + reader.font_size * reader.line_height())
                        }
                        Key::Named(key::Named::PageDown | key::Named::Space) => {
                            reader.jump(reader.offset + reader.viewport * 0.9)
                        }
                        Key::Named(key::Named::PageUp) => {
                            reader.jump(reader.offset - reader.viewport * 0.9)
                        }
                        Key::Named(key::Named::Home) if modifiers.control() => reader.jump(0.0),
                        Key::Named(key::Named::End) if modifiers.control() => {
                            reader.jump(reader.scroll_extent())
                        }
                        Key::Named(key::Named::Escape) => {
                            reader.selection.clear();
                            if reader.show_contents {
                                update_inner(reader, Message::ToggleContents)
                            } else {
                                Task::none()
                            }
                        }
                        _ => Task::none(),
                    }
                }
                _ => Task::none(),
            }
        }
        _ => Task::none(),
    }
}

fn restore_viewport(reader: &Reader) -> Task<Message> {
    if let Some(pdf) = &reader.pdf {
        forward_pdf(pdf.document().id, pdf.restore_scroll())
    } else if reader.book.is_some() {
        reader.restore_book_scroll()
    } else {
        Task::none()
    }
}

fn scroll_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("html-document")
}

fn scroll_to(offset: f32) -> Task<Message> {
    scroll_to_widget(scroll_id(), offset)
}

fn scroll_to_widget(id: iced::advanced::widget::Id, offset: f32) -> Task<Message> {
    use iced::advanced::widget::{
        operate,
        operation::scrollable::{self, AbsoluteOffset},
    };
    operate(scrollable::scroll_to(
        id,
        AbsoluteOffset {
            x: None,
            y: Some(offset),
        },
    ))
}

/// Measure a complete reflowable section on the task executor. Only scalar heights survive.
#[cfg(test)]
use reader_layout::measure::measure_book;

/// Spacing and heading scales for a book: the theme's, except that PDF Book
/// keeps the default typography its layout was built around.
fn style_for(theme: &'static themes::ReadingTheme, book: &Book) -> &'static BookStyle {
    if book.pdf_source.is_some() {
        &MINIMAL
    } else {
        &theme.style
    }
}

/// The bundled family replacing Literata for this book's text, if the theme has one.
fn family_for(theme: &'static themes::ReadingTheme, book: &Book) -> Option<&'static str> {
    if book.pdf_source.is_some() {
        None
    } else {
        theme.family
    }
}

fn styled_item_size(style: &BookStyle, book: &Book, index: usize, item: &Item, body: f32) -> f32 {
    if let Some(source) = &book.pdf_source
        && let Some(block) = source.conversion.blocks.get(index)
    {
        return body * block.size_ratio.clamp(0.8, 2.0);
    }
    style.block_size(item, book.structure.get(item.id()), body)
}

fn segment_styles(
    styles: &[reader_document::StyleRun],
    start: usize,
    end: usize,
) -> Vec<reader_document::StyleRun> {
    styles
        .iter()
        .filter_map(|run| {
            let from = run.start_byte.max(start);
            let to = run.end_byte.min(end);
            (from < to).then_some(reader_document::StyleRun {
                start_byte: from - start,
                end_byte: to - start,
                style: run.style,
            })
        })
        .collect()
}

#[cfg(test)]
fn settle_pagination(reader: &mut Reader) {
    if let Some(cancel) = reader.pagination.clone() {
        let result = book_map::build(reader.book.clone().unwrap(), &cancel);
        let _ = reader.finish_atlas(reader.generation, result);
    }
    settle_theme(reader);
}

/// Completes the pending theme layout (if the theme needs one) on this thread.
#[cfg(test)]
fn settle_theme(reader: &mut Reader) {
    while let Some(cancel) = reader.pagination.clone() {
        let book = reader.book.clone().unwrap();
        let section = book.epub.as_ref().map_or(0, |c| c.index);
        let Some(canonical) = reader
            .atlas
            .as_ref()
            .and_then(|atlas| atlas.section(section))
            .cloned()
        else {
            return;
        };
        let theme = reader.theme;
        let result = book_map::adapt_section_with(
            book,
            &canonical,
            theme,
            reader.reading_options(),
            &cancel,
        );
        let generation = reader.generation;
        let _ = update_inner(
            reader,
            Message::ThemeLayoutReady {
                generation,
                section,
                theme: theme.id,
                result,
            },
        );
    }
}

fn render_item(
    reader: &Reader,
    book: &Book,
    index: usize,
    item: &Item,
    bounds: Option<selection::SelectionBounds>,
) -> Element<'static, Message> {
    let style = reader.reading_style(book);
    let family = reader.reading_family(book);
    if let Some(source) = &book.pdf_source
        && let Some(rect) = source.conversion.illustrations.get(item.id())
    {
        let page = source.conversion.blocks[index].sources[0].page;
        let info = source.document.pages[page as usize];
        // Keep the illustration's size and horizontal position relative to the
        // printed text block instead of stretching it across the reading width.
        let placement = source
            .conversion
            .placements
            .get(item.id())
            .copied()
            .unwrap_or(reader_pdf::book::Placement {
                offset: 0.0,
                width: 1.0,
            });
        let width = reader.width * placement.width;
        let height = width * info.height * (rect.bottom - rect.top)
            / (info.width * (rect.right - rect.left));
        let content: Element<'static, Message> = match reader
            .pdf_book_raster
            .as_ref()
            .filter(|(doc, p, _)| *doc == source.document.id && *p == page)
            .map(|(_, _, raster)| raster)
        {
            Some(Ok(assets)) => match assets.get(item.id()) {
                Some(asset) => image(asset.handle.clone())
                    .width(width)
                    .height(height)
                    .into(),
                None => text("Illustration unavailable").size(14).into(),
            },
            Some(Err(error)) => text(format!("Cannot display illustration: {error}"))
                .size(14)
                .into(),
            None => text("Loading illustration…")
                .size(14)
                .style(ui::muted_text)
                .into(),
        };
        let figure = container(content)
            .width(width)
            .height(height)
            .center_x(width);
        return container(figure)
            .width(reader.width)
            .height(height)
            .padding(iced::Padding {
                left: reader.width * placement.offset,
                ..iced::Padding::ZERO
            })
            .into();
    }
    if let Item::Image { asset_path, .. } = item {
        return match book.images.get(asset_path) {
            Some(asset) => {
                let size = image_size(asset, reader.width);
                container(
                    image(asset.handle.clone())
                        .width(size.width)
                        .height(size.height),
                )
                .center_x(Length::Fill)
                .into()
            }
            None => text("Image unavailable").size(reader.font_size).into(),
        };
    }
    let logical = item.text().unwrap_or_default();
    let marks: Vec<selection::TextMark> = notes::ranges_for(&reader.notes.marks, index, logical)
        .into_iter()
        .map(|(range, color, note)| selection::TextMark {
            range,
            tint: notes::tint(color),
            note,
        })
        .collect();
    let heading_style;
    let (direction, styles) = match item {
        Item::Paragraph {
            base_direction,
            style_runs,
            ..
        } => (*base_direction, style_runs.as_slice()),
        _ => {
            heading_style = [reader_document::StyleRun {
                start_byte: 0,
                end_byte: logical.len(),
                style: reader_document::InlineStyle::Bold,
            }];
            // The native shaper resolves heading direction from its first strong character.
            (BaseDirection::Ltr, heading_style.as_slice())
        }
    };
    let pdf_layout = book
        .pdf_source
        .as_ref()
        .and_then(|source| source.conversion.blocks.get(index))
        .map(|block| &block.layout);
    if let Some(reader_pdf::book::BlockLayout::Toc {
        number_start,
        indent,
    }) = pdf_layout
        && *number_start <= logical.len()
        && logical.is_char_boundary(*number_start)
    {
        let title_end = number_start.saturating_sub(1);
        let title_text = &logical[..title_end];
        let number_text = &logical[*number_start..];
        let size = styled_item_size(&style, book, index, item, reader.font_size);
        let selection = bounds.and_then(|bounds| bounds.range_for_item(index, logical));
        let make_part = |source: &str, start: usize, end: usize, alignment| {
            let part_styles = segment_styles(styles, start, end);
            let part_links = book
                .structure
                .get(item.id())
                .map_or_else(Vec::new, |semantics| {
                    semantics
                        .links
                        .iter()
                        .filter_map(|link| {
                            let from = link.start_byte.max(start);
                            let to = link.end_byte.min(end);
                            (from < to).then(|| reader_document::Link {
                                start_byte: from,
                                end_byte: to,
                                href: link.href.clone(),
                                kind: link.kind,
                            })
                        })
                        .collect()
                });
            let mapped = reader::map_document_paragraph(source, direction, &part_styles);
            match mapped {
                Ok(mapped) => selection::selectable_text(
                    selection::SelectableParagraphConfig {
                        item_id: item.id().to_owned(),
                        logical_text: source.to_owned(),
                        mapped,
                        item_offset: start,
                        alignment,
                        font_size: size,
                        line_height: size * style.line_height,
                        selection: selection.clone(),
                        dragging: reader.selection.is_dragging(),
                        track_hit_test: false,
                        spoken: reader.spoken_word(item.id()),
                        links: part_links,
                        focused_link: None,
                        font_family: family,
                        marks: marks.clone(),
                    },
                    Message::SelectStart,
                    |endpoint, _| Message::SelectMove(endpoint),
                    Some(Message::FollowLink),
                    Some(Message::Context),
                ),
                Err(error) => text(format!("Cannot display this entry: {error}")).into(),
            }
        };
        let title = make_part(
            title_text,
            0,
            title_end,
            iced::advanced::text::Alignment::Default,
        );
        let number = make_part(
            number_text,
            *number_start,
            logical.len(),
            iced::advanced::text::Alignment::Right,
        );
        let number_width =
            (number_text.chars().count() as f32 * size * 0.85 + size).max(size * 3.0);
        return container(
            row![
                container(title).width(Length::Fill),
                container(number)
                    .width(number_width)
                    .align_x(iced::alignment::Horizontal::Right),
            ]
            .spacing(size * 0.5),
        )
        .width(reader.width)
        .padding(iced::Padding {
            left: reader.width * indent.clamp(0.0, 0.2),
            ..iced::Padding::ZERO
        })
        .into();
    }
    let mut mapped = match reader::map_document_paragraph(logical, direction, styles) {
        Ok(mapped) => mapped,
        Err(error) => return text(format!("Cannot display this paragraph: {error}")).into(),
    };
    if matches!(item, Item::Heading { .. }) {
        for run in &mut mapped.runs {
            if run.role == reader::FontRole::EditorialBold {
                run.role = reader::FontRole::EditorialMedium;
            }
        }
    }
    let semantics = book.structure.get(item.id());
    if semantics.is_some_and(|s| {
        matches!(
            s.kind,
            reader_document::BlockKind::Preformatted | reader_document::BlockKind::Formula
        )
    }) {
        for run in &mut mapped.runs {
            use reader::FontRole::*;
            run.role = match run.role {
                EditorialBold | SystemBold => CodeBold,
                EditorialItalic | SystemItalic => CodeItalic,
                EditorialBoldItalic | SystemBoldItalic => CodeBoldItalic,
                _ => Code,
            };
        }
    }
    let size = styled_item_size(&style, book, index, item, reader.font_size);
    let paragraph = selection::selectable_text(
        selection::SelectableParagraphConfig {
            item_id: item.id().to_owned(),
            logical_text: logical.to_owned(),
            mapped,
            item_offset: 0,
            alignment: match pdf_layout {
                Some(reader_pdf::book::BlockLayout::Centered) => {
                    iced::advanced::text::Alignment::Center
                }
                Some(reader_pdf::book::BlockLayout::Right) => {
                    iced::advanced::text::Alignment::Right
                }
                _ => iced::advanced::text::Alignment::Default,
            },
            font_size: size,
            line_height: size * style.line_height,
            selection: bounds.and_then(|bounds| bounds.range_for_item(index, logical)),
            dragging: reader.selection.is_dragging(),
            track_hit_test: false,
            spoken: reader.spoken_word(item.id()),
            links: book
                .structure
                .get(item.id())
                .map_or_else(Vec::new, |s| s.links.clone()),
            focused_link: match reader.focused {
                Some(Control::BookLink(row, link)) if row == index => Some(link),
                _ => None,
            },
            font_family: family,
            marks,
        },
        Message::SelectStart,
        |endpoint, _| Message::SelectMove(endpoint),
        Some(Message::FollowLink),
        Some(Message::Context),
    );
    let kind = semantics.map(|s| s.kind).unwrap_or_default();
    let quote = semantics.is_some_and(|s| s.quote_depth > 0);
    let mut padding = style.block_padding(item, semantics, reader.font_size, reader.width);
    if let Some(source) = &book.pdf_source
        && let Some(block) = source.conversion.blocks.get(index)
    {
        let page = block.sources[0].page as usize;
        if let Some(info) = source.document.pages.get(page) {
            padding.top +=
                reader.width * (info.height / info.width.max(1.0)) * block.top_gap.clamp(0.0, 0.55);
        }
    }
    if let Some(
        reader_pdf::book::BlockLayout::List { indent }
        | reader_pdf::book::BlockLayout::Inset { indent },
    ) = pdf_layout
    {
        padding.left += reader.width * indent.clamp(0.0, 0.2);
    }
    let mut block = container(paragraph)
        .padding(padding)
        .style(move |theme| book_style::block_surface(theme, kind, quote));
    match pdf_layout {
        Some(reader_pdf::book::BlockLayout::Centered) => {
            block = block.center_x(Length::Fill);
        }
        Some(reader_pdf::book::BlockLayout::Right) => {
            block = block
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Right);
        }
        _ => {}
    }
    if matches!(reader.focused, Some(Control::BookLink(row, _)) if row == index) {
        block = block.id(iced::advanced::widget::Id::new(ui::FOCUSED_CONTROL));
    }
    block.into()
}

fn control_button<'a>(
    reader: &Reader,
    control: Control,
    label: impl Into<Element<'a, Message>>,
    message: Option<Message>,
) -> Element<'a, Message> {
    // Chrome stays quiet; only a few actions carry the tinted accent or danger.
    let tone = match control {
        Control::ConfirmRemove
        | Control::Profile(profile_ui::Focus::Confirm)
        | Control::Notes(notes::Focus::Remove(_)) => ui::ButtonTone::Destructive,
        Control::LocateMissing | Control::OpenRecent(_) if !reader.show_search => {
            ui::ButtonTone::Quiet
        }
        _ => ui::ButtonTone::Surface,
    };
    toned_button(reader, control, label, message, tone, false)
}

fn toned_button<'a>(
    reader: &Reader,
    control: Control,
    label: impl Into<Element<'a, Message>>,
    message: Option<Message>,
    tone: ui::ButtonTone,
    selected: bool,
) -> Element<'a, Message> {
    let focused = reader.focused == Some(control);
    let compact = reader.window_size.width < 780.0 && reader.book.is_some();
    let button = iced::widget::button(label)
        .padding([7, if compact { 7 } else { 11 }])
        .on_press_maybe(message)
        .style(move |theme, status| ui::button_style(theme, status, tone, focused, selected))
        .into();
    if focused
        && !matches!(
            control,
            Control::HideHelp
                | Control::OpenRecent(_)
                | Control::LocateRecent(_)
                | Control::RemoveRecent(_)
        )
    {
        container(button)
            .id(iced::advanced::widget::Id::new(ui::FOCUSED_CONTROL))
            .into()
    } else {
        button
    }
}

fn recent_panel(reader: &Reader, active: bool, has_document: bool) -> Element<'_, Message> {
    let mut heading = row![
        text("Recent documents").size(16).font(ui::SEMIBOLD),
        iced::widget::Space::new().width(Length::Fill),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);
    heading = heading.push(control_button(
        reader,
        Control::HideRecent,
        text("Hide").font(ui::MEDIUM).size(13),
        active.then_some(Message::ToggleRecent),
    ));
    let mut history = column![heading].spacing(10);
    if reader.recent_loading {
        history = history.push(text("Loading your library…").size(13).style(ui::muted_text));
    } else if reader.recent.is_empty() {
        history = history.push(
            text("Your recent documents will appear here after you open a file.")
                .size(13)
                .style(ui::muted_text),
        );
    }
    let mut entries = column![].spacing(RECENT_ROW_SPACING);
    for (index, entry) in reader.recent.iter().enumerate() {
        let kind = match entry.kind {
            DocumentKind::Html => "HTML",
            DocumentKind::Pdf => "PDF",
            DocumentKind::Epub => "EPUB",
        };
        let mut card = container(
            row![
                container(
                    text(kind)
                        .size(10)
                        .font(ui::SEMIBOLD)
                        .style(ui::accent_text)
                )
                .width(48)
                .center_x(48)
                .padding([5, 0])
                .style(ui::inset),
                container(
                    column![
                        text(&entry.title)
                            .size(13)
                            .font(ui::SEMIBOLD)
                            .wrapping(iced::widget::text::Wrapping::None),
                        text(entry.path.display().to_string())
                            .size(11)
                            .style(ui::muted_text)
                            .wrapping(iced::widget::text::Wrapping::None),
                    ]
                    .spacing(2),
                )
                .width(Length::Fill)
                .clip(true),
                control_button(
                    reader,
                    Control::OpenRecent(index),
                    text("Open").font(ui::MEDIUM).size(12),
                    active.then_some(Message::OpenRecent(index)),
                ),
                toned_button(
                    reader,
                    Control::LocateRecent(index),
                    text("Locate").font(ui::MEDIUM).size(12),
                    active.then_some(Message::LocateRecent(index)),
                    ui::ButtonTone::Subtle,
                    false,
                ),
                toned_button(
                    reader,
                    Control::RemoveRecent(index),
                    text("Remove").font(ui::MEDIUM).size(12),
                    active.then_some(Message::RemoveRecent(index)),
                    ui::ButtonTone::Destructive,
                    false,
                ),
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center),
        )
        .height(RECENT_ROW_HEIGHT)
        .padding([3, 8])
        .style(ui::inset);
        if matches!(reader.focused,
            Some(Control::OpenRecent(focused) | Control::LocateRecent(focused) | Control::RemoveRecent(focused))
                if focused == index)
        {
            card = card.id(iced::advanced::widget::Id::new(ui::FOCUSED_CONTROL));
        }
        entries = entries.push(card);
    }
    if !reader.recent.is_empty() {
        history = history.push(
            scrollable(entries)
                .direction(ui::vertical_scrollbar())
                .style(ui::scroll_style)
                .id(iced::advanced::widget::Id::new("recent-documents"))
                .height(if has_document {
                    (reader.window_size.height * 0.24).clamp(72.0, 190.0)
                } else {
                    let chrome_height = if reader.window_size.height < 500.0 {
                        300.0
                    } else {
                        350.0
                    };
                    (reader.window_size.height - chrome_height).clamp(52.0, 300.0)
                }),
        );
    }
    container(history)
        .width(Length::Fill)
        .padding(12)
        .style(ui::panel)
        .into()
}

fn search_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("library-search")
}

fn find_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("book-find")
}

fn overlays<'a>(reader: &'a Reader, base: Element<'a, Message>) -> Element<'a, Message> {
    use iced::widget::{Space, mouse_area, opaque, stack, text_input};
    if reader.notes.editor.is_some() {
        return notes_ui::editor_overlay(reader, base);
    }
    if !reader.show_search && !reader.show_settings && reader.confirm_remove.is_none() {
        // Keep the reader and shelf in the same widget-tree slot when a modal opens.
        let mut layers = stack![base];
        if let Some(popup) = word_translation::popup_layer(reader) {
            layers = layers.push(popup);
        }
        if let Some(popup) = notes_ui::popup_layer(reader) {
            layers = layers.push(popup);
        }
        return layers.into();
    }
    let heading = row![
        text(if reader.confirm_remove.is_some() {
            "Remove book?"
        } else if reader.show_search {
            "Quick Switcher"
        } else {
            "Reading Settings"
        })
        .font(ui::HEADING)
        .size(22)
        .shaping(text::Shaping::Advanced),
        Space::new().width(Length::Fill),
        if reader.confirm_remove.is_some() {
            Space::new().into()
        } else {
            toned_button(
                reader,
                Control::DismissOverlay,
                text("×").font(ui::SANS).size(18),
                Some(Message::DismissOverlay),
                ui::ButtonTone::Subtle,
                false,
            )
        },
    ]
    .align_y(iced::Alignment::Center)
    .spacing(12);
    let mut contents = column![heading].spacing(16);
    if let Some(entry) = &reader.confirm_remove {
        contents = contents
            .push(text(entry.title.clone()).size(16))
            .push(
                text("This book will be removed from simPl. Your original file will be kept.")
                    .size(13)
                    .style(ui::secondary_text),
            )
            .push(
                row![
                    Space::new().width(Length::Fill),
                    control_button(
                        reader,
                        Control::DismissOverlay,
                        text("Cancel").font(ui::MEDIUM).size(13),
                        Some(Message::DismissOverlay)
                    ),
                    control_button(
                        reader,
                        Control::ConfirmRemove,
                        text("Remove").font(ui::MEDIUM).size(13),
                        Some(Message::ConfirmRemove)
                    ),
                ]
                .spacing(8),
            );
    } else if reader.show_search {
        let mut input = container(
            text_input("Find a title, author, or filename…", &reader.search_query)
                .id(search_id())
                .font(ui::SANS)
                .size(14)
                .padding(12)
                .style(ui::input_style)
                .on_input(Message::SearchChanged)
                .on_submit(Message::SearchSubmit),
        );
        if reader.focused == Some(Control::SearchInput) {
            input = input.id(iced::advanced::widget::Id::new(ui::FOCUSED_CONTROL));
        }
        contents = contents.push(input);
        let mut results = column![].spacing(6);
        for (position, index) in reader.search_results.iter().copied().take(8).enumerate() {
            let Some(entry) = reader.shelf.entries.get(index) else {
                continue;
            };
            let description = column![
                container(
                    text(&entry.document.title)
                        .size(14)
                        .font(ui::SEMIBOLD)
                        .wrapping(text::Wrapping::None)
                        .shaping(text::Shaping::Advanced)
                )
                .width(Length::Fill)
                .clip(true),
                container(
                    text(format!(
                        "{} · {}",
                        shelf::format_name(entry.format()),
                        entry.author.as_deref().unwrap_or("Local document")
                    ))
                    .size(11)
                    .style(ui::muted_text)
                    .wrapping(text::Wrapping::None)
                    .shaping(text::Shaping::Advanced)
                )
                .width(Length::Fill)
                .clip(true),
            ]
            .spacing(4)
            .width(Length::Fill);
            results = results.push(
                row![
                    container(toned_button(
                        reader,
                        Control::SearchResult(index),
                        description,
                        Some(Message::OpenLibrary(index)),
                        ui::ButtonTone::Surface,
                        position == reader.search_selected
                    ))
                    .width(Length::Fill),
                    control_button(
                        reader,
                        Control::LocateLibrary(index),
                        text("Locate").font(ui::MEDIUM).size(11),
                        Some(Message::LocateLibrary(index))
                    ),
                    toned_button(
                        reader,
                        Control::RemoveLibrary(index),
                        text("Remove").font(ui::MEDIUM).size(11),
                        Some(Message::RemoveLibrary(index)),
                        ui::ButtonTone::Destructive,
                        false
                    ),
                ]
                .spacing(4)
                .align_y(iced::Alignment::Center),
            );
        }
        if reader.search_results.is_empty() {
            results = results.push(
                text(if reader.shelf.loading {
                    "Loading the library…"
                } else {
                    "No matching documents."
                })
                .size(13)
                .style(ui::muted_text),
            );
        }
        contents = contents
            .push(results)
            .push(
                text(if reader.search_results.len() > 8 {
                    "Showing the first 8 matches. Keep typing to narrow the results."
                } else {
                    "↑ ↓ Select · Enter Open · Esc Close"
                })
                .size(11)
                .style(ui::muted_text),
            )
            .push(
                text("Remove deletes simPl’s copy; your original file is kept.")
                    .size(11)
                    .style(ui::muted_text),
            );
    } else {
        let section = |title: &'static str| {
            text(title.to_uppercase())
                .font(ui::MEDIUM)
                .size(11)
                .style(ui::muted_text)
        };
        let note = |value: &'static str| {
            text(value)
                .size(12)
                .width(Length::Fill)
                .style(ui::muted_text)
        };
        let rule = || {
            container(Space::new().height(1))
                .width(Length::Fill)
                .style(ui::rule)
        };
        contents = contents
            .push(section("Theme"))
            .push(theme_picker(reader))
            .push(note("A theme sets the reading font, spacing and colors. Page numbers stay the same in every theme."))
            .push(rule())
            .push(column![
                row![
                    text("Book zoom").font(ui::MEDIUM).size(13), Space::new().width(Length::Fill),
                    control_button(reader, Control::SettingsFontDown, text("−").size(14),
                        (reader.zoom > 0.25).then_some(Message::Zoom(reader.zoom - 0.1))),
                    text(format!("{:.0}%", reader.zoom * 100.0)).size(12).style(ui::muted_text),
                    control_button(reader, Control::SettingsFontUp, text("+").size(14),
                        (reader.zoom < 3.0).then_some(Message::Zoom(reader.zoom + 0.1))),
                ].spacing(12).align_y(iced::Alignment::Center),
                note("Scales Book pages without changing their boundaries or total count. Applies during this session."),
            ].spacing(6))
            .push(rule())
            .push(column![
                row![
                    text("Window controls").font(ui::MEDIUM).size(13), Space::new().width(Length::Fill),
                    window_controls_button(reader, WindowControls::Windows, "Windows"),
                    window_controls_button(reader, WindowControls::Mac, "macOS"),
                ].spacing(8).align_y(iced::Alignment::Center),
                note("Windows places minimize, maximize and close at the right of the title bar; macOS uses round buttons at the left."),
            ].spacing(6))
            .push(rule())
            .push(reading_ui::settings(reader))
            .push(rule())
            .push(control_button(reader, Control::Fullscreen, text(if reader.fullscreen { "Exit fullscreen (F11)" } else { "Fullscreen (F11)" }).size(12), Some(Message::ToggleFullscreen)))
            .push(rule())
            .push(section("Read aloud"))
            .push(read_aloud::settings(reader, reader.voice_choices.clone()))
            .push(rule())
            .push(section("Word translation"))
            .push(word_translation::settings(reader))
            .push(rule())
            .push(section("Library & data"))
            .push(profile_ui::settings(reader))
            .push(rule())
            .push(row![
                column![
                    text("Keyboard shortcuts").font(ui::MEDIUM).size(13),
                    note("Every action has a key; press F1 at any time."),
                ].spacing(4),
                Space::new().width(Length::Fill),
                control_button(reader, Control::SettingsHelp, text("Show").font(ui::MEDIUM).size(12), Some(Message::ToggleHelp)),
            ].spacing(8).align_y(iced::Alignment::Center));
    }
    let panel = container(
        scrollable(
            container(contents)
                .padding(iced::Padding {
                    right: 14.0,
                    ..Default::default()
                })
                // Keep scrolling damage contiguous. Otherwise tiny-skia repaints
                // the underlying reader once for each moving text/control region.
                // This moving fill has the panel color and is clipped by the scrollable.
                .style(|theme| container::Style {
                    background: Some(ui::palette(theme).surface.into()),
                    ..container::Style::default()
                }),
        )
        .id(iced::advanced::widget::Id::new("workspace-overlay"))
        .height(Length::Shrink)
        .direction(ui::vertical_scrollbar())
        .style(ui::scroll_style),
    )
    .padding(24)
    .width(
        (reader.window_size.width - 32.0).min(if reader.confirm_remove.is_some() {
            420.0
        } else {
            680.0
        }),
    )
    .max_height((reader.window_size.height - 64.0).max(160.0))
    .style(ui::panel);
    let backdrop = mouse_area(
        container(Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Color::BLACK.scale_alpha(0.55).into()),
                ..container::Style::default()
            }),
    )
    .on_press(Message::DismissOverlay);
    stack![
        base,
        backdrop,
        container(opaque(panel))
            .center_x(Length::Fill)
            .center_y(Length::Fill)
    ]
    .into()
}

fn find_bar<'a>(reader: &'a Reader, find: &'a Find, active: bool) -> Element<'a, Message> {
    let mut input = container(
        iced::widget::text_input("Find in book…", &find.query)
            .id(find_id())
            .font(ui::SANS)
            .size(13)
            .padding([6, 10])
            .style(ui::input_style)
            .on_input(Message::FindChanged),
    )
    .width(Length::Fill);
    if reader.focused == Some(Control::FindInput) {
        input = input.id(iced::advanced::widget::Id::new(ui::FOCUSED_CONTROL));
    }
    let count = find.count();
    let status = if find.query.trim().is_empty() {
        String::new()
    } else if find.searching {
        "Searching…".to_owned()
    } else if count == 0 {
        "No matches".to_owned()
    } else {
        format!(
            "{} of {count}{}",
            find.current.map_or(0, |index| index + 1),
            if count >= find::MAX_MATCHES { "+" } else { "" }
        )
    };
    let steps = active && count > 0;
    let mut bar = row![
        input,
        text(status).size(12).style(ui::muted_text),
        hinted_control(
            reader,
            Control::FindPrevious,
            "↑",
            "Previous match (Shift+Enter)",
            steps.then_some(Message::FindStep(false)),
        ),
        hinted_control(
            reader,
            Control::FindNext,
            "↓",
            "Next match (Enter)",
            steps.then_some(Message::FindStep(true)),
        ),
        hinted_control(
            reader,
            Control::FindClose,
            text("×").font(ui::SANS).size(16),
            "Close find (Esc)",
            Some(Message::FindClose),
        ),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);
    if let Some(notice) = &find.notice {
        bar = bar.push(text(notice).size(11).style(ui::muted_text));
    } else if find.indexing {
        bar = bar.push(
            text(if reader.pdf.is_some() {
                "Reading pages…"
            } else {
                "Reading chapters…"
            })
            .size(11)
            .style(ui::muted_text),
        );
    }
    container(bar).padding([8, 12]).style(ui::panel).into()
}

/// The bundled reading themes as cards, each previewing its own font and colors.
fn theme_picker(reader: &Reader) -> Element<'_, Message> {
    let card = |index: usize| -> Element<'_, Message> {
        let theme = &themes::THEMES[index];
        let colors = theme.palette(reader.appearance);
        let font = theme.family.map_or(ui::SERIF, iced::Font::with_name);
        let sample = container(
            column![
                text("Aa")
                    .font(font)
                    .size(26)
                    .color(colors.text)
                    .shaping(text::Shaping::Advanced),
                text("The quiet hours of reading.")
                    .font(font)
                    .size(13)
                    .color(colors.secondary)
                    .shaping(text::Shaping::Advanced),
            ]
            .spacing(2),
        )
        .padding(10)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(colors.surface.into()),
            border: iced::Border {
                color: colors.border,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..container::Style::default()
        });
        let chosen = reader.theme.id == theme.id;
        let name = row![
            text(theme.name).font(ui::MEDIUM).size(14),
            iced::widget::Space::new().width(Length::Fill),
        ]
        .align_y(iced::Alignment::Center);
        let name = if chosen {
            name.push(
                text("✓")
                    .font(iced::Font::with_name("Segoe UI Symbol"))
                    .size(14)
                    .style(ui::accent_text),
            )
        } else {
            name
        };
        let label = column![
            name,
            text(theme.summary).size(11).style(ui::muted_text),
            sample,
        ]
        .spacing(6)
        .width(Length::Fill);
        container(toned_button(
            reader,
            Control::SettingsTheme(index),
            label,
            Some(Message::SetTheme(index)),
            ui::ButtonTone::Surface,
            chosen,
        ))
        .width(Length::Fill)
        .into()
    };
    let mut rows = column![].spacing(8);
    for pair in (0..themes::THEMES.len()).collect::<Vec<_>>().chunks(2) {
        let mut line = row![].spacing(8);
        for &index in pair {
            line = line.push(card(index));
        }
        rows = rows.push(line);
    }
    rows.into()
}

fn window_controls_button(
    reader: &Reader,
    controls: WindowControls,
    label: &'static str,
) -> Element<'static, Message> {
    toned_button(
        reader,
        Control::SettingsWindowControls(controls),
        text(label).size(12),
        Some(Message::SetWindowControls(controls)),
        ui::ButtonTone::Surface,
        reader.window_controls == controls,
    )
}

fn hinted_control<'a>(
    reader: &Reader,
    control: Control,
    label: impl Into<Element<'a, Message>>,
    hint: &'static str,
    message: Option<Message>,
) -> Element<'a, Message> {
    iced::widget::tooltip(
        control_button(reader, control, label, message),
        text(hint).size(12),
        iced::widget::tooltip::Position::Bottom,
    )
    .gap(4)
    .padding(6)
    .style(ui::panel)
    .into()
}

fn appearance_button(reader: &Reader, active: bool) -> Element<'static, Message> {
    hinted_control(
        reader,
        Control::ToggleAppearance,
        text(if reader.appearance == Appearance::Light {
            "☾"
        } else {
            "☼"
        })
        .font(iced::Font::with_name("Segoe UI Symbol"))
        .size(16)
        .line_height(1.0),
        reader.appearance.toggle_label(),
        active.then_some(Message::ToggleAppearance),
    )
}

fn find_button(reader: &Reader, active: bool) -> Element<'static, Message> {
    hinted_control(
        reader,
        Control::Find,
        text("Find").font(ui::MEDIUM).size(13),
        "Find in document (Ctrl+F)",
        active.then_some(Message::FindOpen),
    )
}

fn listen_button(reader: &Reader, active: bool) -> Element<'static, Message> {
    iced::widget::tooltip(
        toned_button(
            reader,
            Control::ReadAloud,
            text("Listen").font(ui::MEDIUM).size(13),
            active.then_some(Message::ReadAloud(read_aloud::Action::Toggle)),
            ui::ButtonTone::Surface,
            reader.read_aloud.active(),
        ),
        text(if reader.read_aloud.active() {
            "Stop reading aloud (Ctrl+Shift+U)"
        } else {
            "Read aloud the selection or from this page (Ctrl+Shift+U)"
        })
        .size(12),
        iced::widget::tooltip::Position::Bottom,
    )
    .gap(4)
    .padding(6)
    .style(ui::panel)
    .into()
}

fn document_toolbar(reader: &Reader, active: bool) -> Element<'_, Message> {
    if reader.book.is_none() && reader.pdf.is_none() {
        return iced::widget::Space::new().height(0).into();
    }
    if !reader.toolbar_expanded {
        return iced::widget::Space::new().height(0).into();
    }
    let toolbar: Element<'_, Message> = {
        let library = control_button(
            reader,
            Control::Close,
            row![
                text("←").font(ui::SANS).size(16),
                text("Library").font(ui::MEDIUM).size(13)
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center),
            active.then_some(Message::Close(CloseAction::Document)),
        );
        if let Some(pdf) = &reader.pdf {
            let id = pdf.document().id;
            let mode = hinted_control(
                reader,
                Control::BookMode,
                text("Book").font(ui::MEDIUM).size(13),
                "Read this PDF in Book mode",
                (active && pdf.document().can_copy).then_some(Message::BookMode),
            );
            let focused = reader.focused.and_then(|control| {
                if let Control::Pdf(control) = control {
                    Some(control)
                } else {
                    None
                }
            });
            let forward = move |message| Message::Pdf {
                document: id,
                message,
            };
            if reader.window_size.width < 900.0 {
                column![
                    row![
                        library,
                        mode,
                        iced::widget::Space::new().width(Length::Fill),
                        find_button(reader, active),
                        listen_button(reader, active),
                        appearance_button(reader, active)
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                    pdf.toolbar(focused).map(forward)
                ]
                .spacing(6)
                .into()
            } else {
                // Same three columns as the Book toolbar: Library / pages / zoom.
                row![
                    container(
                        row![library, mode]
                            .spacing(8)
                            .align_y(iced::Alignment::Center)
                    )
                    .width(Length::FillPortion(1)),
                    pdf.navigation(focused).map(forward),
                    container(
                        row![
                            pdf.zoom_controls(focused).map(forward),
                            find_button(reader, active),
                            listen_button(reader, active),
                            appearance_button(reader, active)
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center)
                    )
                    .width(Length::FillPortion(1))
                    .align_x(iced::alignment::Horizontal::Right)
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center)
                .into()
            }
        } else {
            let mut library = row![library].spacing(6).align_y(iced::Alignment::Center);
            if !reader.returns.is_empty() {
                library = library.push(hinted_control(
                    reader,
                    Control::Back,
                    text("↶").size(16),
                    "Back to previous passage (Alt+Left)",
                    active.then_some(Message::Back),
                ));
            }
            let fonts = row![
                find_button(reader, active),
                listen_button(reader, active),
                hinted_control(
                    reader,
                    Control::FontDown,
                    text("−").size(16),
                    "Zoom out",
                    (active && reader.zoom > 0.25).then_some(Message::Zoom(reader.zoom - 0.1))
                ),
                hinted_control(
                    reader,
                    Control::FitBookWidth,
                    text(if reader.fit_width {
                        "Fit".to_owned()
                    } else {
                        format!("{:.0}%", reader.zoom * 100.0)
                    })
                    .size(12),
                    "Fit width / restore zoom (Ctrl+Shift+F)",
                    active.then_some(Message::FitBookWidth)
                ),
                hinted_control(
                    reader,
                    Control::FontUp,
                    text("+").size(16),
                    "Zoom in",
                    (active && reader.zoom < 3.0).then_some(Message::Zoom(reader.zoom + 0.1))
                ),
                appearance_button(reader, active),
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center);
            if reader.book.as_ref().is_some_and(|b| b.pdf_source.is_some()) {
                library = library.push(control_button(
                    reader,
                    Control::DocumentMode,
                    text("Document").font(ui::MEDIUM).size(13),
                    active.then_some(Message::DocumentMode { source_page: true }),
                ));
            }
            let pages = reader.pages();
            let current = book_pages::current(pages, reader.offset)
                .map_or_else(|| "1".to_owned(), |p| p.label.clone());
            let mut navigation = row![
                hinted_control(
                    reader,
                    Control::PreviousBookPage,
                    "←",
                    "Previous page (Left arrow)",
                    (active && reader.can_turn(false)).then_some(Message::BookPage(false))
                ),
                iced::widget::text_input(
                    if current.is_empty() { "—" } else { "Page" },
                    reader.page_input.as_deref().unwrap_or(&current),
                )
                .id(iced::advanced::widget::Id::new("book-page"))
                .on_input(Message::PageInput)
                .on_submit(Message::PageSubmit)
                .width(64)
                .size(13)
                .style(ui::input_style),
                text(format!("/ {}", reader.page_total()))
                    .size(12)
                    .style(ui::muted_text),
                hinted_control(
                    reader,
                    Control::NextBookPage,
                    "→",
                    "Next page (Right arrow)",
                    (active && reader.can_turn(true)).then_some(Message::BookPage(true))
                ),
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center);
            if !reader.contents().is_empty() {
                navigation = navigation.push(toned_button(
                    reader,
                    Control::Contents,
                    text("Contents").font(ui::MEDIUM).size(13),
                    active.then_some(Message::ToggleContents),
                    ui::ButtonTone::Surface,
                    reader.show_contents,
                ));
            }
            if reader.window_size.width < 900.0 {
                column![
                    row![
                        library,
                        iced::widget::Space::new().width(Length::Fill),
                        fonts
                    ]
                    .spacing(8)
                    .align_y(iced::Alignment::Center),
                    container(navigation).center_x(Length::Fill),
                ]
                .spacing(6)
                .into()
            } else {
                row![
                    container(library).width(Length::FillPortion(1)),
                    navigation,
                    container(row![fonts].spacing(4).align_y(iced::Alignment::Center))
                        .width(Length::FillPortion(1))
                        .align_x(iced::alignment::Horizontal::Right)
                ]
                .spacing(4)
                .align_y(iced::Alignment::Center)
                .into()
            }
        }
    };
    container(toolbar)
        .width(Length::Fill)
        .padding([6, 20])
        .style(ui::header)
        .into()
}

fn paged_book_view<'a>(reader: &'a Reader, book: &'a Book) -> Element<'a, Message> {
    if reader.pagination.is_some() {
        return container(text("Preparing pages…").size(14).style(ui::muted_text))
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(book_style::desk)
            .into();
    }
    let Some(page) = reader.active_page() else {
        return iced::widget::Space::new().into();
    };
    let mut sheets = column![];
    let margin = reader.reading_options().margin as f32;
    let paper_width = reader.width + margin * 2.0;
    let bounds = reader.selection.bounds(&book.items);
    {
        let content_start = page.content.start;
        let offset = (reader.offset - page.top - book_pages::TOP).max(0.0) + content_start;
        let window = reader.heights.window(offset, reader.viewport, OVERSCAN);
        let range = window.start.max(page.rows.start).min(page.rows.end)
            ..window.end.min(page.rows.end).max(page.rows.start);
        let rows = range
            .clone()
            .map(|index| render_item(reader, book, index, &book.items[index], bounds))
            .collect();
        let blocks = virtual_reader::VisibleRows::new(
            range,
            &reader.heights,
            reader.width,
            reader.reading_style(book).gap(reader.font_size),
            rows,
            virtual_reader::LayoutReports {
                measurements: reader.measurements.clone(),
                generation: reader.generation,
                counters: None,
            },
        )
        .slice(page.content.clone());
        let content_height = page.content.end - content_start;
        let fallback = book
            .pdf_source
            .as_ref()
            .is_some_and(|source| source.conversion.fallback_pages.contains(&page.number));
        let footer: Element<'static, Message> = if fallback {
            iced::widget::Space::new().height(book_pages::BOTTOM).into()
        } else {
            container(text(page.label.clone()).size(12).style(ui::muted_text))
                .center_x(Length::Fill)
                .center_y(book_pages::BOTTOM)
                .into()
        };
        let sheet = container(column![
            iced::widget::Space::new().height(book_pages::TOP),
            blocks,
            iced::widget::Space::new().height(
                (page.height - book_pages::TOP - book_pages::BOTTOM - content_height).max(0.0)
            ),
            footer,
        ])
        .padding(iced::Padding {
            left: margin,
            right: margin,
            ..Default::default()
        })
        .width(paper_width)
        .style(book_style::paper);
        sheets = sheets.push(iced::widget::stack![
            sheet,
            notes_ui::ribbon(reader, paper_width)
        ]);
    }
    let available_width = (reader.window_size.width - notes_ui::sidebar_width(reader)).max(1.0);
    let side_padding = MINIMAL.side_padding(available_width);
    let centered = container(book_zoom::wrap(sheets.into(), paper_width, reader.zoom))
        .center_x(available_width.max(paper_width * reader.zoom + 14.0 + side_padding * 2.0))
        .padding(iced::Padding {
            right: 14.0,
            ..Default::default()
        });
    let generation = reader.generation;
    // A right click on text opens the note menu from the paragraph; a click on
    // the desk or margins reaches this area instead.
    iced::widget::mouse_area(
        container(crate::document_scroll::wrap(
            Message::ZoomBy,
            scrollable(centered)
                .direction(iced::widget::scrollable::Direction::Both {
                    vertical: ui::scrollbar(),
                    horizontal: ui::scrollbar(),
                })
                .id(scroll_id())
                .height(Length::Fill)
                .on_scroll(move |viewport| Message::Scroll {
                    generation,
                    page_top: page.top,
                    offset: viewport.absolute_offset().y,
                    viewport: viewport.bounds().height,
                }),
        ))
        .style(book_style::desk)
        .clip(true)
        .height(Length::Fill),
    )
    .on_right_press(Message::ContextBlank)
    .into()
}

fn view(reader: &Reader) -> Element<'_, Message> {
    let active = reader.interactive() && reader.confirm_remove.is_none();
    let has_document = reader.book.is_some() || reader.pdf.is_some();
    // The toolbar occupies one stable slot for every document format.
    let toolbar = document_toolbar(reader, active);
    // This slot is always present, so expanding any panel never replaces the
    // reading scrollable's widget-tree position or its native scroll state.
    let mut auxiliary = column![].spacing(8);
    if let Some(find) = reader
        .find
        .as_ref()
        .filter(|_| reader.book.is_some() || reader.pdf.is_some())
    {
        auxiliary = auxiliary.push(find_bar(reader, find, active));
    }
    if reader.read_aloud.active() {
        auxiliary = auxiliary.push(read_aloud::player(reader, active));
    }
    let contents = reader.contents();
    if !contents.is_empty() && reader.show_contents && reader.toolbar_expanded {
        let height = (reader.window_size.height * 0.35).clamp(100.0, 240.0);
        let range = reader.contents_range();
        let start = range.start;
        let end = range.end;
        let mut entries =
            column![iced::widget::Space::new().height(start as f32 * CONTENT_ROW_HEIGHT)];
        for (relative, entry) in contents[start..end].iter().enumerate() {
            let label = text(entry.label.clone())
                .size(13)
                .wrapping(iced::widget::text::Wrapping::None);
            entries = entries.push(
                container(toned_button(
                    reader,
                    Control::ContentsEntry(start + relative),
                    label,
                    active.then(|| Message::EpubChapter {
                        index: entry.chapter,
                        fragment: entry.fragment.clone(),
                    }),
                    ui::ButtonTone::Subtle,
                    reader
                        .book
                        .as_ref()
                        .and_then(|b| b.epub.as_ref())
                        .map_or(entry.chapter <= reader.anchor().row, |c| {
                            entry.chapter == c.index
                        }),
                ))
                .height(CONTENT_ROW_HEIGHT)
                .width(Length::Fill)
                .clip(true)
                .padding(iced::Padding {
                    left: entry.depth.min(6) as f32 * 12.0,
                    ..iced::Padding::default()
                }),
            );
        }
        entries = entries.push(
            iced::widget::Space::new().height((contents.len() - end) as f32 * CONTENT_ROW_HEIGHT),
        );
        auxiliary = auxiliary.push(
            container(
                scrollable(entries)
                    .direction(ui::vertical_scrollbar())
                    .style(ui::scroll_style)
                    .id(iced::advanced::widget::Id::new("epub-contents"))
                    .height(height)
                    .on_scroll(|viewport| Message::ContentsScrolled(viewport.absolute_offset().y)),
            )
            .padding(8)
            .style(ui::panel),
        );
    }
    if let Some(error) = &reader.error {
        let mut notice = row![
            text(error)
                .size(13)
                .width(Length::Fill)
                .wrapping(iced::advanced::text::Wrapping::WordOrGlyph)
                .style(ui::danger_text),
            toned_button(
                reader,
                Control::DismissError,
                text("Dismiss").size(12),
                Some(Message::DismissError),
                ui::ButtonTone::Subtle,
                false,
            ),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);
        if reader.missing_recent.is_some() {
            notice = notice.push(control_button(
                reader,
                Control::LocateMissing,
                text("Locate").font(ui::MEDIUM).size(12),
                active.then_some(Message::LocateMissing),
            ));
        }
        if reader.failed_close.is_some() {
            notice = notice.push(toned_button(
                reader,
                Control::CloseWithoutSaving,
                text("Close without saving").size(12),
                Some(Message::CloseWithoutSaving),
                ui::ButtonTone::Destructive,
                false,
            ));
        }
        auxiliary = auxiliary.push(container(notice).padding(12).style(ui::panel));
    }
    if let Some(notice) = &reader.history_notice {
        let label = text(notice)
            .size(13)
            .style(ui::danger_text)
            .wrapping(iced::advanced::text::Wrapping::WordOrGlyph);
        let content: Element<'_, Message> = if reader.history_corrupt {
            row![
                label.width(Length::Fill),
                toned_button(
                    reader,
                    Control::ResetRecent,
                    text("Reset history").size(12),
                    active.then_some(Message::ResetRecent),
                    ui::ButtonTone::Destructive,
                    false,
                ),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center)
            .into()
        } else {
            label.into()
        };
        auxiliary = auxiliary.push(container(content).padding(12).style(ui::panel));
    }
    if reader.show_recent {
        auxiliary = auxiliary.push(recent_panel(reader, active, has_document));
    }
    if reader.show_help {
        let help = column![
            row![
                text("Shortcuts").size(16).font(ui::SEMIBOLD),
                iced::widget::Space::new().width(Length::Fill),
                control_button(reader, Control::HideHelp, text("Hide").font(ui::MEDIUM).size(12),
                    Some(Message::ToggleHelp)),
            ]
            .align_y(iced::Alignment::Center),
            text("Tab / Shift+Tab  ·  Controls    Enter / Space  ·  Activate    Escape  ·  Dismiss    F1  ·  Help")
                .size(12).style(ui::muted_text),
            text("Ctrl+O  Open    Ctrl+K  Quick switcher    Ctrl+R  Recent    Ctrl+W  Library    Ctrl+C  Copy    Ctrl+A  Select all")
                .size(12).style(ui::muted_text),
            text("Page Up / Down  Read    Ctrl+Home / End  Ends    Ctrl+plus / minus / 0  Zoom")
                .size(12).style(ui::muted_text),
            text("Book  Left / Right: previous / next page · Up / Down: scroll within page").size(12).style(ui::secondary_text),
            text("Ctrl+L  Page    Ctrl+F  Find, again to close (Enter / Shift+Enter next / previous)    EPUB  Ctrl+T contents · Ctrl+Page Up / Down chapters    Ctrl+Shift+F fit width / previous zoom · F11 fullscreen")
                .size(12).style(ui::muted_text),
            text("Select text to highlight or add a note    Right-click a highlight to edit or remove it    Right-click a page to bookmark    Ctrl+H  Highlight    Ctrl+D  Bookmark page    Ctrl+B  Notes sidebar")
                .size(12).style(ui::muted_text),
            text("Ctrl+Shift+U  Read aloud / stop    Listen reads the selection, or continues from the current page")
                .size(12).style(ui::muted_text),
            text("Hold middle button  Scroll · Move away to change speed · Release / Escape to stop    F8 / title-bar panel icon  Hide / show toolbar")
                .size(12).style(ui::muted_text),
        ]
        .spacing(6);
        let mut panel = container(help).padding(12).style(ui::panel);
        if reader.focused == Some(Control::HideHelp) {
            panel = panel.id(iced::advanced::widget::Id::new(ui::FOCUSED_CONTROL));
        }
        auxiliary = auxiliary.push(panel);
    }
    if let Some(notice) = &reader.preferences_notice {
        auxiliary = auxiliary.push(
            container(text(notice).size(12).style(ui::danger_text))
                .padding(12)
                .style(ui::panel),
        );
    }
    if let Some(notice) = &reader.shelf.notice {
        let mut content = row![
            text(notice)
                .size(12)
                .style(ui::danger_text)
                .width(Length::Fill)
        ];
        if reader.shelf.has_failed_save() {
            content = content.push(control_button(
                reader,
                Control::Shelf(shelf::Control::RetrySave),
                text("Retry save").size(12),
                Some(Message::Shelf(shelf::Message::Activate(
                    shelf::Control::RetrySave,
                ))),
            ));
        }
        auxiliary = auxiliary.push(
            container(content.spacing(8).align_y(iced::Alignment::Center))
                .padding(12)
                .style(ui::panel),
        );
    }
    if reader.saving {
        auxiliary = auxiliary.push(
            text("Saving reading position…")
                .size(12)
                .style(ui::muted_text),
        );
    }
    let loading: Element<'_, Message> = if reader.opening.is_some() || reader.pagination.is_some() {
        let dots = ["● · ·", "· ● ·", "· · ●", "· ● ·"][usize::from(reader.loading_frame) % 4];
        let label = if reader.pagination.is_some() {
            "Preparing pages…"
        } else if reader.show_conversion {
            "Preparing Book…"
        } else {
            "Opening…"
        };
        container(
            row![
                text(dots).size(12).style(ui::accent_text),
                text(label).size(12).style(ui::muted_text),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        )
        .center_x(Length::Fill)
        .height(22)
        .style(ui::header)
        .into()
    } else {
        iced::widget::Space::new().height(0).into()
    };
    let header: Element<'_, Message> = if reader.fullscreen {
        iced::widget::Space::new().height(0).into()
    } else {
        chrome::view(
            reader.focused.and_then(|control| match control {
                Control::Chrome(action) => Some(action),
                Control::ToggleToolbar => Some(chrome::Action::ToggleToolbar),
                _ => None,
            }),
            active,
            reader.reading_title(),
            chrome::Controls {
                style: reader.window_controls,
                maximized: reader.maximized,
                tooltips: !reader.show_settings
                    && !reader.show_search
                    && reader.confirm_remove.is_none()
                    && reader.notes.editor.is_none(),
            },
            (!has_document).then_some(reader.appearance),
            has_document.then_some(reader.toolbar_expanded),
        )
        .map(Message::Chrome)
    };
    let mut page = column![
        header,
        toolbar,
        loading,
        container(
            scrollable(auxiliary)
                .id(iced::advanced::widget::Id::new("reader-panels"))
                .direction(ui::vertical_scrollbar())
                .style(ui::scroll_style)
                .height(Length::Shrink),
        )
        .width(Length::Fill)
        .max_height(reader.window_size.height * 0.38)
        .padding([0, 20]),
    ];
    let content: Element<'_, Message> = if let Some(pdf) = &reader.pdf {
        let document = pdf.document().id;
        pdf.view()
            .map(move |message| Message::Pdf { document, message })
    } else if let Some(book) = &reader.book {
        paged_book_view(reader, book)
    } else {
        reader
            .shelf
            .view(
                reader.focused.and_then(|control| {
                    if let Control::Shelf(control) = control {
                        Some(control)
                    } else {
                        None
                    }
                }),
                active,
                reader.dropping,
            )
            .map(Message::Shelf)
    };
    page = page.push(if has_document {
        row![
            container(content).width(Length::Fill).height(Length::Fill),
            notes_ui::sidebar(reader, active),
        ]
        .height(Length::Fill)
        .into()
    } else {
        content
    });
    let base = container(page)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    chrome::frame(overlays(reader, base), |direction| {
        Message::Chrome(chrome::Action::Resize(direction))
    })
}

fn title(reader: &Reader) -> String {
    reader
        .reading_title()
        .map_or_else(|| "simPl".into(), |title| format!("{title} — simPl"))
}

fn subscription(reader: &Reader) -> Subscription<Message> {
    use iced_futures::subscription::{self, Event};

    #[derive(Hash)]
    struct HtmlEvents;

    let measurements = reader.measurements.clone();
    let generation = reader.generation;
    let pending = reader.pending_anchor.is_some();
    let active = reader.book.is_some() && reader.opening.is_none() && reader.pagination.is_none();
    let focus = reader.focus_pending.then_some(reader.focus_generation);
    let searching = reader.show_search;
    let finding = reader.find.is_some();
    let noting = reader.notes.editor.is_some();
    let events = subscription::filter_map(
        (
            HtmlEvents, generation, pending, active, focus, searching, finding, noting,
        ),
        move |event| match event {
            Event::Interaction {
                event: iced::Event::Window(window::Event::RedrawRequested(_)),
                ..
            } => {
                // Operations issued by the key event see the preceding widget
                // tree. Reveal focus after its new marker has been laid out.
                // Gate this just like measurement refinement: no idle redraw loop.
                focus.map(Message::FocusReady).or_else(|| {
                    (active && (pending || !measurements.lock().is_empty()))
                        .then_some(Message::LayoutReady(generation))
                })
            }
            Event::Interaction {
                event: iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }),
                status: iced::event::Status::Captured,
                ..
            } if !matches!(&key, Key::Character(value)
                if modifiers.control() && (value.eq_ignore_ascii_case("o")
                    || value.eq_ignore_ascii_case("w")
                    || value.eq_ignore_ascii_case("r")
                    || value.eq_ignore_ascii_case("l")
                    || value.eq_ignore_ascii_case("k")))
                && !matches!(
                    &key,
                    Key::Named(
                        key::Named::Tab
                            | key::Named::F1
                            | key::Named::F8
                            | key::Named::F11
                            | key::Named::Escape
                    )
                )
                && !(searching
                    && matches!(
                        &key,
                        Key::Named(key::Named::ArrowDown | key::Named::ArrowUp)
                    ))
                && !(noting
                    && modifiers.control()
                    && matches!(&key, Key::Named(key::Named::Enter)))
                && !(finding
                    && match &key {
                        Key::Named(key::Named::Enter) => true,
                        Key::Character(value) => {
                            modifiers.control() && value.eq_ignore_ascii_case("f")
                        }
                        _ => false,
                    }) =>
            {
                None
            }
            Event::Interaction { event, window, .. } => Some(Message::Event(event, window)),
            Event::SystemThemeChanged(_) => None,
        },
    );
    let mut subscriptions = vec![events];
    if reader.opening.is_some() || reader.pagination.is_some() {
        subscriptions.push(Subscription::run(loading_frames).map(Message::LoadingFrame));
    }
    if reader.read_aloud.ticking() {
        subscriptions.push(Subscription::run(read_aloud::ticks).map(|()| Message::ReadAloudTick));
    }
    Subscription::batch(subscriptions)
}

fn loading_frames() -> impl iced_futures::futures::Stream<Item = u8> {
    use iced_futures::futures::{SinkExt, StreamExt};

    iced::stream::channel(
        1,
        |mut output: iced_futures::futures::channel::mpsc::Sender<u8>| async move {
            let (mut sender, mut receiver) = iced_futures::futures::channel::mpsc::channel(1);
            std::thread::spawn(move || {
                let mut frame = 0_u8;
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(180));
                    frame = (frame + 1) % 4;
                    if sender
                        .try_send(frame)
                        .is_err_and(|error| error.is_disconnected())
                    {
                        break;
                    }
                }
            });
            while let Some(frame) = receiver.next().await {
                if output.send(frame).await.is_err() {
                    break;
                }
            }
        },
    )
}

pub fn run(path: Option<PathBuf>, error: Option<String>) -> iced::Result {
    let mut application = iced::application(
        move || {
            let mut reader = Reader {
                error: error.clone(),
                preferences_loading: true,
                ..Reader::default()
            };
            let tasks = [
                Task::perform(async { preferences::load() }, Message::PreferencesLoaded),
                Task::perform(async { recent::load() }, Message::RecentLoaded),
                shelf::Shelf::load().map(Message::Shelf),
                path.clone()
                    .map_or_else(Task::none, |path| reader.open_with_import(path, None, true)),
            ];
            (reader, Task::batch(tasks))
        },
        update,
        view,
    )
    .title(title)
    .subscription(subscription)
    .theme(|reader: &Reader| ui::theme(reader.appearance, reader.theme))
    .settings(iced::Settings {
        default_font: ui::SANS,
        default_text_size: 13.into(),
        ..iced::Settings::default()
    })
    .window(window::Settings {
        size: Size::new(1280.0, 800.0),
        min_size: Some(Size::new(540.0, 360.0)),
        position: window::Position::Centered,
        exit_on_close_request: false,
        decorations: false,
        ..window::Settings::default()
    });
    for font in ui::font_data() {
        application = application.font(font);
    }
    application.run()
}

#[cfg(test)]
#[path = "book_preview.rs"]
mod book_preview;
#[cfg(test)]
#[path = "promotion_preview.rs"]
mod promotion_preview;
#[cfg(test)]
#[path = "settings_profile.rs"]
mod settings_profile;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consecutive_source_markers_keep_blank_pages_addressable() {
        let mut source = book("blank-page");
        let book = Arc::get_mut(&mut source).unwrap();
        book.anchors.insert("same".into(), "paragraph-0".into());
        book.page_breaks = vec![("1".into(), "same".into()), ("2".into(), "same".into())];
        let mut reader = Reader {
            book: Some(source),
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 0,
            fraction: 0.0,
        });
        settle_pagination(&mut reader);
        assert_eq!(reader.page_total(), 2);
        assert_eq!(reader.pages().len(), 2);
        assert!(reader.pages()[0].content.is_empty());
        assert_eq!(reader.atlas.as_ref().unwrap().target("1"), Some((0, 0)));
        let _ = reader.go_to_local_page(0);
        let _ = reader.refine_geometry();
        assert_eq!(reader.offset, reader.pages()[0].top);
    }

    #[test]
    fn publisher_pages_override_generated_count_and_zoom_preserves_every_boundary() {
        let mut source = book("source-pages");
        let book = Arc::get_mut(&mut source).unwrap();
        book.anchors.insert("start".into(), "paragraph-0".into());
        book.anchors.insert("second".into(), "paragraph-6".into());
        book.page_breaks = vec![
            ("i".into(), "start".into()),
            ("400".into(), "second".into()),
        ];
        let mut reader = Reader {
            book: Some(source),
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 0,
            fraction: 0.0,
        });
        settle_pagination(&mut reader);
        assert_eq!(reader.page_total(), 2);
        assert!(reader.atlas.as_ref().unwrap().source_pages);
        let before = serde_json::to_vec(&**reader.atlas.as_ref().unwrap()).unwrap();
        let _ = update_inner(&mut reader, Message::PageInput("400".into()));
        let _ = update_inner(&mut reader, Message::PageSubmit);
        assert_eq!(reader.anchor().row, 6);
        for zoom in [0.4, 1.0, 1.5, 3.0] {
            let _ = update_inner(&mut reader, Message::Zoom(zoom));
            assert_eq!(reader.font_size, DEFAULT_FONT_SIZE);
            assert_eq!(reader.page_total(), 2);
            assert_eq!(
                serde_json::to_vec(&**reader.atlas.as_ref().unwrap()).unwrap(),
                before
            );
            assert_eq!(reader.anchor().row, 6);
        }
        let _ = update_inner(&mut reader, Message::PageInput("i".into()));
        let _ = update_inner(&mut reader, Message::PageSubmit);
        assert_eq!(reader.anchor().row, 0);
    }

    #[test]
    fn complete_pagination_is_atomic_cancellable_and_stable_during_scrolling() {
        let mut reader = Reader {
            book: Some(book("pagination")),
            ..Reader::default()
        };
        let anchor = Anchor {
            row: 3,
            fraction: 0.4,
        };
        let _ = reader.rebuild_geometry(anchor);
        let old_generation = reader.generation;
        let old_cancel = reader.pagination.clone().unwrap();
        assert!(
            reader.pages().is_empty(),
            "no estimated page count while measuring"
        );
        assert!(!reader.can_turn(true));
        let _ = reader.jump(9999.0);
        assert_eq!(
            reader.anchor().row,
            3,
            "scroll must not discard the pending position"
        );
        reader.font_size = 24.0;
        let _ = reader.rebuild_geometry(reader.anchor());
        assert!(old_cancel.load(Ordering::Relaxed));
        assert!(
            measure_book(
                reader.book.clone().unwrap(),
                reader.width,
                20.0,
                &old_cancel
            )
            .is_none()
        );
        let _ = reader.finish_atlas(old_generation, Ok(None));
        assert!(
            reader.pages().is_empty(),
            "obsolete worker must not install its layout"
        );
        settle_pagination(&mut reader);
        assert!(reader.pagination.is_none());
        let pages = reader
            .pages()
            .iter()
            .map(|p| (p.rows.clone(), p.content.clone(), p.top, p.height))
            .collect::<Vec<_>>();
        // Layout corrections must never move already committed reflow page boundaries.
        reader
            .measurements
            .lock()
            .push((0, 9999.0, reader.width, reader.generation));
        let _ = reader.refine_geometry();
        let _ = reader.jump(3000.0);
        let _ = reader.refine_geometry();
        assert_eq!(
            pages,
            reader
                .pages()
                .iter()
                .map(|p| (p.rows.clone(), p.content.clone(), p.top, p.height))
                .collect::<Vec<_>>()
        );
        let _ = reader.rebuild_geometry(anchor);
        assert!(
            reader.pagination.is_none(),
            "same-width navigation reuses completed measurements"
        );
        reader.window_size.width = 540.0;
        let _ = reader.rebuild_geometry(anchor);
        assert!(
            reader.pagination.is_none(),
            "window changes preserve canonical pagination"
        );
        reader.clear_content();
        let _ = reader.finish_atlas(reader.generation - 1, Ok(None));
        assert!(reader.book.is_none());
        assert!(reader.heights.is_empty());
    }

    #[test]
    fn escape_cancels_pending_conversion_without_replacing_content() {
        let current = book("before-conversion");
        let mut reader = Reader {
            book: Some(current.clone()),
            request: 5,
            show_conversion: true,
            opening: Some(current.path.clone()),
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 3,
            fraction: 0.4,
        });
        settle_pagination(&mut reader);
        let _ = update_inner(&mut reader, Message::CancelConversion);
        assert!(reader.opening.is_none());
        assert!(!reader.show_conversion);
        assert_eq!(reader.request, 6);
        assert!(Arc::ptr_eq(reader.book.as_ref().unwrap(), &current));
        let _ = update_inner(
            &mut reader,
            Message::Loaded {
                request: 5,
                result: Ok(LoadReply {
                    document: LoadedDocument::Reflow(book("stale-conversion")),
                    catalog: None,
                }),
            },
        );
        assert!(Arc::ptr_eq(reader.book.as_ref().unwrap(), &current));
        assert_eq!(reader.saved_position().unwrap().1.item_id, "paragraph-3");
    }

    #[test]
    fn converted_pdf_anchors_cross_page_ranges_in_both_directions() {
        use reader_pdf::book::{Block, Conversion, SourceRange};
        let conversion = Conversion {
            illustrations: Default::default(),
            placements: Default::default(),
            page_labels: Default::default(),
            fallback_pages: Default::default(),
            blocks: vec![Block {
                id: "pdf-b1-p000001-c0000010".into(),
                text: "Across a page".into(),
                heading: false,
                layout: Default::default(),
                size_ratio: 1.0,
                top_gap: 0.0,
                styles: Vec::new(),
                links: Vec::new(),
                sources: vec![
                    SourceRange {
                        page: 1,
                        start: 10,
                        end: 20,
                        top: 0.7,
                        bottom: 0.8,
                    },
                    SourceRange {
                        page: 2,
                        start: 5,
                        end: 15,
                        top: 0.2,
                        bottom: 0.3,
                    },
                ],
            }],
            warnings: Vec::new(),
        };
        let position = PdfReadingPosition {
            fingerprint: "a".repeat(64),
            page: 2,
            within: 0.25,
            horizontal: 0.0,
            zoom: position::PdfZoom::Scale(1.25),
        };
        let anchor = pdf_book_anchor(&conversion, &position);
        assert_eq!(anchor.row, 0);
        assert_eq!(anchor.fraction, 0.5);
        assert_eq!(pdf_source_range(&conversion, anchor).unwrap().page, 2);
        assert_eq!(
            pdf_source_range(
                &conversion,
                Anchor {
                    row: 0,
                    fraction: 0.0
                }
            )
            .unwrap()
            .page,
            1
        );
        assert_eq!(
            pdf_source_range(
                &conversion,
                Anchor {
                    row: 0,
                    fraction: 1.0
                }
            )
            .unwrap()
            .page,
            2
        );
    }

    #[test]
    fn structured_document_copy_and_link_focus_use_the_same_logical_items() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/book-structure/structured.html");
        let book = Arc::new(display_book(
            reader_document::load_html(&path).unwrap(),
            None,
            None,
        ));
        let mut reader = Reader {
            book: Some(book.clone()),
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 0,
            fraction: 0.0,
        });
        settle_pagination(&mut reader);
        let first = book
            .items
            .iter()
            .find(|item| item.text().is_some())
            .unwrap();
        let last = book
            .items
            .iter()
            .rfind(|item| item.text().is_some())
            .unwrap();
        reader.selection.begin(Endpoint {
            item_id: first.id().into(),
            byte_offset: 0,
        });
        reader.selection.extend(Endpoint {
            item_id: last.id().into(),
            byte_offset: last.text().unwrap().len(),
        });
        reader.selection.end_drag();
        let expected = book
            .items
            .iter()
            .filter_map(Item::text)
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            reader.selection.copy_text(&book.items).as_deref(),
            Some(expected.as_str())
        );
        assert!(expected.contains("3. Keep the important words."));
        assert!(expected.contains("<math><mfrac><mi>a</mi><mi>b</mi></mfrac></math>"));
        assert_eq!(book.images.len(), 1);
        let controls: Vec<_> = reader
            .controls()
            .filter(|c| matches!(c, Control::BookLink(_, _)))
            .collect();
        assert_eq!(controls, [Control::BookLink(1, 0)]);
        let _ = reader.activate(controls[0]);
        assert_eq!(reader.returns.len(), 1);
        assert_eq!(
            reader.saved_position().unwrap().1.item_id,
            book.anchors["note"]
        );
        let _ = reader.go_back();
        assert_eq!(reader.saved_position().unwrap().1.item_id, first.id());
    }

    #[test]
    fn links_and_back_preserve_passage_across_font_and_palette_changes() {
        let mut current = book("links");
        Arc::get_mut(&mut current)
            .unwrap()
            .anchors
            .insert("note".into(), "paragraph-9".into());
        let mut reader = Reader {
            book: Some(current),
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 3,
            fraction: 0.4,
        });
        settle_pagination(&mut reader);
        let _ = reader.follow_link("#note".into());
        assert_eq!(reader.saved_position().unwrap().1.item_id, "paragraph-9");
        assert_eq!(reader.returns.len(), 1);
        assert!(reader.controls().any(|c| c == Control::Back));
        let _ = update_inner(&mut reader, Message::Zoom(1.5));
        let _ = update_inner(&mut reader, Message::ToggleAppearance);
        let _ = reader.go_back();
        let restored = reader.saved_position().unwrap().1;
        assert_eq!(restored.item_id, "paragraph-3");
        assert!((restored.within - 0.4).abs() < 0.001);
        assert_eq!(restored.font_size, DEFAULT_FONT_SIZE);
        assert_eq!(reader.zoom, 1.5);
        assert!(reader.returns.is_empty());
        for href in ["#absent", "https://example.com", "other.html"] {
            let _ = reader.follow_link(href.into());
            assert!(reader.error.is_some());
            assert_eq!(reader.saved_position().unwrap().1.item_id, "paragraph-3");
            assert!(reader.returns.is_empty());
        }
    }

    #[test]
    fn failed_and_stale_navigation_do_not_consume_return_history() {
        let mut reader = Reader {
            book: Some(book("notes")),
            request: 4,
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 4,
            fraction: 0.2,
        });
        settle_pagination(&mut reader);
        reader.returns.push(reader.return_location().unwrap());
        reader.pending_navigation = Some(Navigation::Pop);
        let _ = update_inner(
            &mut reader,
            Message::Loaded {
                request: 3,
                result: Err("stale".into()),
            },
        );
        assert!(matches!(reader.pending_navigation, Some(Navigation::Pop)));
        let _ = update_inner(
            &mut reader,
            Message::Loaded {
                request: 4,
                result: Err("unavailable".into()),
            },
        );
        assert_eq!(reader.returns.len(), 1);
        assert!(reader.pending_navigation.is_none());
        assert_eq!(reader.saved_position().unwrap().1.item_id, "paragraph-4");
        reader.pending_navigation = Some(Navigation::Pop);
        let _ = update_inner(
            &mut reader,
            Message::Loaded {
                request: 4,
                result: Ok(LoadReply {
                    document: LoadedDocument::Reflow(book("returned")),
                    catalog: None,
                }),
            },
        );
        assert!(reader.returns.is_empty());
        reader.returns.push(reader.return_location().unwrap());
        let _ = update_inner(
            &mut reader,
            Message::Loaded {
                request: 4,
                result: Ok(LoadReply {
                    document: LoadedDocument::Reflow(book("different")),
                    catalog: None,
                }),
            },
        );
        assert!(reader.returns.is_empty());
    }

    #[test]
    fn appearance_changes_preserve_geometry_selection_and_content() {
        let mut reader = Reader {
            book: Some(book("appearance")),
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 4,
            fraction: 0.35,
        });
        settle_pagination(&mut reader);
        reader.selection.begin(Endpoint {
            item_id: "paragraph-4".into(),
            byte_offset: 0,
        });
        reader.selection.extend(Endpoint {
            item_id: "paragraph-4".into(),
            byte_offset: 20,
        });
        reader.selection.end_drag();
        let before = reader.saved_position().unwrap().1;
        let selected = reader
            .selection
            .copy_text(&reader.book.as_ref().unwrap().items);
        let generation = reader.generation;
        let offset = reader.offset;
        let _ = update_inner(&mut reader, Message::ToggleAppearance);
        assert_eq!(reader.appearance, Appearance::Dark);
        assert_eq!(reader.generation, generation);
        assert_eq!(reader.offset, offset);
        assert_eq!(reader.saved_position().unwrap().1.item_id, before.item_id);
        assert_eq!(
            reader
                .selection
                .copy_text(&reader.book.as_ref().unwrap().items),
            selected
        );
        assert!(reader.preferences_saving);
    }

    #[test]
    fn appearance_load_cannot_override_a_new_choice_and_writes_are_serialized() {
        let mut reader = Reader {
            preferences_loading: true,
            ..Reader::default()
        };
        let _ = update_inner(&mut reader, Message::ToggleAppearance);
        assert!(!reader.preferences_saving);
        let _ = update_inner(
            &mut reader,
            Message::PreferencesLoaded(Ok(Preferences::default())),
        );
        assert_eq!(reader.appearance, Appearance::Dark);
        assert!(reader.preferences_saving);
        let _ = update_inner(&mut reader, Message::ToggleAppearance);
        assert!(reader.preferences_dirty);
        let _ = update_inner(&mut reader, Message::PreferencesSaved(Ok(())));
        assert!(reader.preferences_saving);
        assert!(!reader.preferences_dirty);
        let _ = update_inner(&mut reader, Message::PreferencesSaved(Ok(())));
        assert!(!reader.preferences_saving);
        assert_eq!(reader.appearance, Appearance::Light);
    }

    #[test]
    fn unreadable_appearance_preferences_are_preserved_and_save_failures_are_visible() {
        let mut reader = Reader::default();
        let _ = update_inner(
            &mut reader,
            Message::PreferencesLoaded(Err("invalid data".into())),
        );
        let _ = update_inner(&mut reader, Message::ToggleAppearance);
        assert_eq!(reader.appearance, Appearance::Dark);
        assert!(!reader.preferences_writable);
        assert!(!reader.preferences_saving);
        assert!(
            reader
                .preferences_notice
                .as_ref()
                .unwrap()
                .contains("preserved")
        );
        reader.preferences_writable = true;
        let _ = reader.persist_preferences();
        let _ = update_inner(
            &mut reader,
            Message::PreferencesSaved(Err("disk full".into())),
        );
        assert!(
            reader
                .preferences_notice
                .as_ref()
                .unwrap()
                .contains("disk full")
        );
        assert!(!reader.preferences_saving);
    }

    #[test]
    fn font_and_width_changes_keep_the_content_anchor_and_size_limits() {
        let mut reader = Reader {
            book: Some(book("reflow")),
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 5,
            fraction: 0.4,
        });
        settle_pagination(&mut reader);
        for size in [24.0, 100.0, 0.0, 20.0] {
            let _ = update_inner(&mut reader, Message::Zoom(size / DEFAULT_FONT_SIZE));
            let position = reader.saved_position().unwrap().1;
            assert_eq!(position.item_id, "paragraph-5");
            assert!((position.within - 0.4).abs() < 0.001);
            assert_eq!(position.font_size, DEFAULT_FONT_SIZE);
        }
        let _ = update_inner(
            &mut reader,
            Message::Event(
                iced::Event::Window(window::Event::Resized(Size::new(540.0, 640.0))),
                window::Id::unique(),
            ),
        );
        assert_eq!(reader.width, book_map::TEXT);
        assert_eq!(reader.saved_position().unwrap().1.item_id, "paragraph-5");
        assert!(
            reader
                .controls()
                .any(|control| control == Control::ToggleAppearance)
        );
        let _ = update_inner(&mut reader, Message::ToggleToolbar);
        assert!(!reader.controls().any(|control| matches!(
            control,
            Control::ToggleAppearance | Control::FontUp | Control::FontDown
        )));
    }

    pub(super) fn book(title: &str) -> Arc<Book> {
        Arc::new(Book {
            path: PathBuf::from(format!("{title}.html")),
            title: title.into(),
            author: None,
            cover: false,
            fingerprint: format!("test-{title}"),
            items: (0..12)
                .map(|index| Item::Paragraph {
                    id: format!("paragraph-{index}"),
                    text: "A readable paragraph with enough words to wrap. ".repeat(50),
                    base_direction: BaseDirection::Ltr,
                    style_runs: Vec::new(),
                })
                .collect(),
            images: HashMap::new(),
            structure: HashMap::new(),
            anchors: HashMap::new(),
            page_breaks: Vec::new(),
            warnings: Vec::new(),
            restored: None,
            epub: None,
            pdf_source: None,
            contents: OnceLock::new(),
        })
    }

    fn searchable_reader() -> Reader {
        let mut source = Arc::try_unwrap(book("searchable")).unwrap();
        for (row, text) in [
            (2, "The Lighthouse stood alone."),
            (7, "lighthouse again, and LIGHTHOUSE once more."),
        ] {
            source.items[row] = Item::Paragraph {
                id: format!("paragraph-{row}"),
                text: text.into(),
                base_direction: BaseDirection::Ltr,
                style_runs: Vec::new(),
            };
        }
        let mut reader = Reader {
            book: Some(Arc::new(source)),
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 0,
            fraction: 0.0,
        });
        settle_pagination(&mut reader);
        reader
    }

    fn selected_item(reader: &Reader) -> Option<usize> {
        let items = &reader.book.as_ref().unwrap().items;
        reader
            .selection
            .bounds(items)
            .map(|bounds| bounds.start_item)
    }

    #[test]
    fn superseded_large_queries_are_cancelled_and_cannot_replace_new_results() {
        let mut reader = searchable_reader();
        reader.find = Some(Find {
            query: "lighthouse".into(),
            ..Find::default()
        });
        let corpus = SearchCorpus::Pdf(Arc::new(vec!["a".repeat(ASYNC_FIND_BYTES)]));
        let _ = reader.start_query(corpus, SearchOrigin::Pdf(0, 0));
        assert!(reader.find.as_ref().unwrap().searching);
        let old_generation = reader.query_generation;
        let cancelled = reader.query_task.as_ref().unwrap().cancel.clone();
        let _ = reader.search_document("lighthouse".into());
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(!reader.find.as_ref().unwrap().searching);
        assert_eq!(reader.find.as_ref().unwrap().count(), 3);
        let _ = reader.finish_query(
            old_generation,
            SearchResults {
                origin: SearchOrigin::Book(None, 0, 0),
                matches: Vec::new(),
                pdf_matches: Vec::new(),
            },
        );
        assert_eq!(reader.find.as_ref().unwrap().count(), 3);
        reader.close_find();
        let _ = reader.finish_query(
            reader.query_generation,
            SearchResults {
                origin: SearchOrigin::Book(None, 0, 0),
                matches: Vec::new(),
                pdf_matches: Vec::new(),
            },
        );
        assert!(reader.find.is_none());
    }

    #[test]
    fn closed_find_index_result_cannot_finish_a_reopened_job() {
        let mut reader = searchable_reader();
        let _ = reader.open_find();
        let old_generation = reader.find_generation;
        reader.close_find();
        let _ = reader.open_find();
        reader.find.as_mut().unwrap().indexing = true;
        let (_, handle) = Task::<Message>::none().abortable();
        let cancel = Arc::new(AtomicBool::new(false));
        reader.find_task = Some(FindJob {
            _handle: handle.abort_on_drop(),
            cancel: cancel.clone(),
        });
        let _ = update_inner(
            &mut reader,
            Message::FindIndexed {
                generation: old_generation,
                result: Err("old error".into()),
            },
        );
        assert!(reader.find_task.is_some());
        assert!(reader.find.as_ref().unwrap().indexing);
        assert!(!cancel.load(Ordering::Relaxed));
        reader.reset_find();
        assert!(cancel.load(Ordering::Relaxed));
    }

    #[test]
    fn blank_query_cancels_pending_search_without_leaving_searching_status() {
        let mut reader = searchable_reader();
        reader.find = Some(Find {
            query: "needle".into(),
            ..Find::default()
        });
        let _ = reader.start_query(
            SearchCorpus::Pdf(Arc::new(vec!["a".repeat(ASYNC_FIND_BYTES)])),
            SearchOrigin::Pdf(0, 0),
        );
        let cancel = reader.query_task.as_ref().unwrap().cancel.clone();
        let _ = reader.search_document(String::new());
        assert!(cancel.load(Ordering::Relaxed));
        assert!(reader.query_task.is_none());
        assert!(!reader.find.as_ref().unwrap().searching);
        assert_eq!(reader.find.as_ref().unwrap().count(), 0);
    }

    #[test]
    fn book_toolbar_controls_fit_the_supported_window_widths() {
        use iced::advanced::{
            Layout, layout,
            widget::{Id, Operation, Tree},
        };
        #[derive(Default)]
        struct FocusBounds(Option<iced::Rectangle>);
        impl Operation for FocusBounds {
            fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
                visit(self);
            }
            fn container(&mut self, id: Option<&Id>, bounds: iced::Rectangle) {
                if id == Some(&Id::new(ui::FOCUSED_CONTROL)) {
                    self.0 = Some(bounds);
                }
            }
        }
        let mut reader = searchable_reader();
        reader.returns.push(ReturnLocation {
            chapter: None,
            item_id: "paragraph-2".into(),
            within: 0.0,
        });
        let renderer = iced::Renderer::new(ui::SANS, iced::Pixels(13.0));
        for width in [540.0, 780.0, 899.0, 900.0, 1050.0, 1280.0] {
            reader.window_size.width = width;
            for control in [
                Control::Close,
                Control::Back,
                Control::Find,
                Control::ReadAloud,
                Control::FontDown,
                Control::FontUp,
                Control::ToggleAppearance,
            ] {
                reader.focused = Some(control);
                let mut toolbar = document_toolbar(&reader, true);
                let mut tree = Tree::new(&toolbar);
                let node = toolbar.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(Size::ZERO, Size::new(width, 800.0)),
                );
                let mut bounds = FocusBounds::default();
                toolbar.as_widget_mut().operate(
                    &mut tree,
                    Layout::new(&node),
                    &renderer,
                    &mut bounds,
                );
                let bounds = bounds.0.expect("focused toolbar control is present");
                assert!(
                    bounds.x >= 0.0 && bounds.x + bounds.width <= width + 0.5,
                    "{control:?} at {width}: {bounds:?}"
                );
            }
        }
    }

    #[test]
    fn find_steps_through_matches_highlights_them_and_wraps() {
        let mut reader = searchable_reader();
        let _ = update_inner(&mut reader, Message::FindOpen);
        assert!(reader.find.is_some());
        assert!(
            reader
                .controls()
                .any(|control| control == Control::FindInput)
        );
        let _ = update_inner(&mut reader, Message::FindChanged("LightHouse".into()));
        let find = reader.find.as_ref().unwrap();
        assert_eq!(find.matches.len(), 3);
        assert_eq!(find.current, Some(0));
        assert_eq!(selected_item(&reader), Some(2));
        for (expected, row) in [(1, 7), (2, 7), (0, 2)] {
            let _ = update_inner(&mut reader, Message::FindStep(true));
            assert_eq!(reader.find.as_ref().unwrap().current, Some(expected));
            assert_eq!(selected_item(&reader), Some(row));
        }
        let _ = update_inner(&mut reader, Message::FindStep(false));
        assert_eq!(reader.find.as_ref().unwrap().current, Some(2));
        let _ = update_inner(&mut reader, Message::FindChanged("no such text".into()));
        assert!(reader.find.as_ref().unwrap().matches.is_empty());
        assert!(reader.selection.endpoints().is_none());
        let _ = update_inner(&mut reader, Message::FindClose);
        assert!(reader.find.is_none());
        assert!(
            !reader
                .controls()
                .any(|control| control == Control::FindInput)
        );
    }

    pub(super) fn themed_reader(paragraphs: usize) -> Reader {
        let mut source = Arc::try_unwrap(book("themed")).unwrap();
        let sentence = "A readable paragraph with enough words to wrap over several lines. ";
        source.items = (0..paragraphs)
            .map(|index| Item::Paragraph {
                id: format!("paragraph-{index}"),
                text: sentence.repeat(6 + index % 40),
                base_direction: BaseDirection::Ltr,
                style_runs: Vec::new(),
            })
            .collect();
        let mut reader = Reader {
            book: Some(Arc::new(source)),
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 0,
            fraction: 0.0,
        });
        settle_pagination(&mut reader);
        reader
    }

    #[test]
    fn every_theme_keeps_the_same_pages_and_page_numbers() {
        let mut reader = themed_reader(60);
        let default_pages = reader.pages().to_vec();
        let total = reader.page_total();
        assert!(default_pages.len() > 10, "the fixture must span many pages");
        for index in 1..themes::THEMES.len() {
            let _ = update_inner(&mut reader, Message::SetTheme(index));
            settle_theme(&mut reader);
            let name = themes::THEMES[index].name;
            let pages = reader.pages();
            assert_eq!(pages.len(), default_pages.len(), "{name}: page count");
            assert_eq!(reader.page_total(), total, "{name}: total");
            assert_eq!(
                reader.heights.len(),
                reader.book.as_ref().unwrap().items.len()
            );
            for (adapted, original) in pages.iter().zip(&default_pages) {
                assert_eq!(adapted.number, original.number, "{name}");
                assert_eq!(adapted.label, original.label, "{name}");
                assert!(
                    adapted.height >= original.height - 0.01,
                    "{name}: sheet size"
                );
                if adapted.content.end - adapted.content.start > 0.5 {
                    // Each page starts in the same paragraph as in the default layout.
                    assert_eq!(
                        adapted.rows.start, original.rows.start,
                        "{name}: page {}",
                        adapted.number
                    );
                }
            }
            for pair in pages.windows(2) {
                assert_eq!(
                    pair[0].content.end, pair[1].content.start,
                    "{name}: contiguous"
                );
                assert!(pair[1].top > pair[0].top, "{name}: pages stack");
            }
            assert_eq!(pages[0].content.start, 0.0);
            assert_eq!(pages.last().unwrap().content.end, reader.heights.total());
        }
        // Back to the default theme restores the original layout exactly.
        let _ = update_inner(&mut reader, Message::SetTheme(0));
        settle_theme(&mut reader);
        let restored = reader.pages();
        assert_eq!(restored.len(), default_pages.len());
        assert!(
            restored
                .iter()
                .zip(&default_pages)
                .all(|(a, b)| a.top == b.top && a.content == b.content)
        );
    }

    #[test]
    fn changing_theme_keeps_the_reading_place() {
        let mut reader = themed_reader(60);
        let _ = reader.rebuild_geometry(Anchor {
            row: 30,
            fraction: 0.4,
        });
        settle_pagination(&mut reader);
        let before = reader.active_page().unwrap().number;
        let row_before = reader.anchor().row;
        for index in [1, 2, 3, 0] {
            let _ = update_inner(&mut reader, Message::SetTheme(index));
            settle_theme(&mut reader);
            assert_eq!(
                reader.anchor().row,
                row_before,
                "theme {index}: same paragraph"
            );
            assert_eq!(
                reader.active_page().unwrap().number,
                before,
                "theme {index}: same page"
            );
        }
    }

    #[test]
    fn a_theme_chosen_before_opening_a_book_lays_it_out_directly() {
        let default_total = themed_reader(40).page_total();
        for (index, theme) in themes::THEMES.iter().enumerate().skip(1) {
            let mut source = Arc::try_unwrap(book("startup")).unwrap();
            let sentence = "A readable paragraph with enough words to wrap over several lines. ";
            source.items = (0..40)
                .map(|i| Item::Paragraph {
                    id: format!("paragraph-{i}"),
                    text: sentence.repeat(6 + i % 40),
                    base_direction: BaseDirection::Ltr,
                    style_runs: Vec::new(),
                })
                .collect();
            let mut reader = Reader {
                theme,
                book: Some(Arc::new(source)),
                ..Reader::default()
            };
            let _ = reader.rebuild_geometry(Anchor {
                row: 0,
                fraction: 0.0,
            });
            settle_pagination(&mut reader);
            assert!(
                reader.pagination.is_none(),
                "{}: still preparing",
                theme.name
            );
            assert!(reader.section_layout().is_some(), "{index}");
            assert!(!reader.pages().is_empty(), "{}", theme.name);
            assert_eq!(
                reader.page_total(),
                default_total,
                "{}: same page count",
                theme.name
            );
        }
    }

    #[test]
    fn saved_theme_reaches_a_book_that_is_already_open() {
        let mut reader = themed_reader(40);
        let total = reader.page_total();
        let loaded = reader_document::preferences::Preferences {
            appearance: Appearance::Dark,
            window_controls: WindowControls::Mac,
            theme: "clear".into(),
            ..Default::default()
        };
        let _ = update_inner(&mut reader, Message::PreferencesLoaded(Ok(loaded)));
        assert_eq!(reader.theme.id, "clear");
        assert!(
            !reader.preferences_dirty,
            "loading a saved theme is not a change to save"
        );
        settle_theme(&mut reader);
        assert!(reader.section_layout().is_some());
        assert_eq!(reader.page_total(), total);
        // An unknown theme in the file means the default.
        let mut other = themed_reader(10);
        let unknown = reader_document::preferences::Preferences {
            theme: "from-a-newer-version".into(),
            ..Default::default()
        };
        let _ = update_inner(&mut other, Message::PreferencesLoaded(Ok(unknown)));
        assert_eq!(other.theme.id, themes::DEFAULT_ID);
    }

    #[test]
    fn choosing_a_theme_is_saved_and_unknown_choices_fall_back() {
        let mut reader = themed_reader(20);
        let _ = update_inner(&mut reader, Message::SetTheme(2));
        assert_eq!(reader.theme.id, "clear");
        assert!(
            reader.preferences_saving,
            "the choice is written to preferences"
        );
        let _ = update_inner(&mut reader, Message::SetTheme(999));
        assert_eq!(
            reader.theme.id,
            themes::THEMES.last().unwrap().id,
            "out of range picks the last"
        );
        assert_eq!(themes::find("no-such-theme").id, themes::DEFAULT_ID);
        // The window theme follows the choice and the light/dark mode.
        let light = ui::theme(Appearance::Light, reader.theme);
        let dark = ui::theme(Appearance::Dark, reader.theme);
        assert_ne!(ui::palette(&light).surface, ui::palette(&dark).surface);
        assert_eq!(
            ui::palette(&light).surface,
            reader.theme.palette(Appearance::Light).surface
        );
    }

    #[test]
    fn theme_controls_are_reachable_from_the_keyboard_and_activate_the_theme() {
        let mut reader = themed_reader(10);
        reader.show_settings = true;
        for index in 0..themes::THEMES.len() {
            assert!(
                reader
                    .controls()
                    .any(|c| c == Control::SettingsTheme(index))
            );
        }
        let _ = reader.activate(Control::SettingsTheme(1));
        assert_eq!(reader.theme.id, "soft");
    }

    /// Width of one line of text in a bundled reading face, shaped by the real renderer.
    fn line_width(family: Option<&'static str>, role: reader::FontRole) -> f32 {
        ui::load_test_fonts();
        let font = family.map_or(role.iced_font(), |family| {
            selection::with_reading_family(role.iced_font(), family)
        });
        selection::text_width("Hamburgefonstiv Quickly, naïve café", font, 20.0)
    }

    #[test]
    fn every_bundled_reading_face_resolves_to_its_own_font_file() {
        // A family name that matched no font file would silently fall back to one
        // default face, and then every weight of that family would measure the same.
        let mut regular = Vec::new();
        for theme in &themes::THEMES {
            if theme.family.is_none() && theme.id != themes::DEFAULT_ID {
                continue;
            }
            let widths: Vec<(&str, f32)> = [
                ("regular", reader::FontRole::EditorialRegular),
                ("medium", reader::FontRole::EditorialMedium),
                ("bold", reader::FontRole::EditorialBold),
                ("italic", reader::FontRole::EditorialItalic),
                ("bold italic", reader::FontRole::EditorialBoldItalic),
            ]
            .into_iter()
            .map(|(name, role)| (name, line_width(theme.family, role)))
            .collect();
            for (i, (name, width)) in widths.iter().enumerate() {
                assert!(*width > 50.0, "{} {name}: {width}", theme.name);
                for (other, other_width) in &widths[i + 1..] {
                    assert!(
                        (width - other_width).abs() > 0.05,
                        "{}: {name} and {other} measure alike ({width} px), so a face is not found",
                        theme.name
                    );
                }
            }
            regular.push((theme.name, widths[0].1));
        }
        // Different families look different, too.
        for (i, (name, width)) in regular.iter().enumerate() {
            for (other, other_width) in &regular[i + 1..] {
                assert!(
                    (width - other_width).abs() > 0.5,
                    "{name} and {other} share a face"
                );
            }
        }
    }

    #[test]
    fn ctrl_f_toggles_the_find_bar() {
        let press = |reader: &mut Reader, modifiers| {
            let event = iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Character("f".into()),
                modified_key: Key::Character("f".into()),
                physical_key: key::Physical::Code(key::Code::KeyF),
                location: keyboard::Location::Standard,
                modifiers,
                text: None,
                repeat: false,
            });
            let _ = update_inner(reader, Message::Event(event, window::Id::unique()));
        };
        let mut reader = searchable_reader();
        press(&mut reader, keyboard::Modifiers::CTRL);
        assert!(reader.find.is_some(), "the first press opens the bar");
        press(&mut reader, keyboard::Modifiers::CTRL);
        assert!(reader.find.is_none(), "the second press closes it");
        press(&mut reader, keyboard::Modifiers::CTRL);
        assert!(reader.find.is_some());
        // Ctrl+Shift+F is not Find.
        press(
            &mut reader,
            keyboard::Modifiers::CTRL | keyboard::Modifiers::SHIFT,
        );
        assert!(reader.find.is_some());
    }

    #[test]
    fn find_scrolls_to_the_match_not_just_the_top_of_its_page() {
        let mut source = Arc::try_unwrap(book("deep")).unwrap();
        // A marker in the middle of every long paragraph lands at varying depths on pages.
        let half = "A readable paragraph with enough words to wrap. ".repeat(25);
        for (row, item) in source.items.iter_mut().enumerate() {
            *item = Item::Paragraph {
                id: format!("paragraph-{row}"),
                text: format!("{half}zebra crossing {half}"),
                base_direction: BaseDirection::Ltr,
                style_runs: Vec::new(),
            };
        }
        let mut reader = Reader {
            book: Some(Arc::new(source)),
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 0,
            fraction: 0.0,
        });
        settle_pagination(&mut reader);
        let _ = update_inner(&mut reader, Message::FindOpen);
        let _ = update_inner(&mut reader, Message::FindChanged("zebra".into()));
        let count = reader.find.as_ref().unwrap().matches.len();
        assert_eq!(count, 12);
        let mut moved_past_page_top = false;
        for step in 0..count {
            let m = reader.find.as_ref().unwrap().matches
                [reader.find.as_ref().unwrap().current.unwrap()]
            .clone();
            let anchor = Anchor {
                row: m.item_index,
                fraction: m.fraction,
            };
            let (position, page) = reader.unclamped_offset_for(anchor).unwrap();
            assert!(
                position >= reader.offset && position <= reader.offset + reader.viewport,
                "match {step} at {position} must be inside the viewport starting at {}",
                reader.offset
            );
            moved_past_page_top |= reader.offset > page.top;
            // Stepping onto a match that is already in view must not scroll.
            let _ = update_inner(&mut reader, Message::FindStep(true));
        }
        assert!(
            moved_past_page_top,
            "at least one match lies below the top of its page"
        );
    }

    #[test]
    fn find_searches_pdf_text_highlights_the_match_and_closes_cleanly() {
        use crate::pdf_reader::tests::{complete, tall_pdf};
        let dll = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("pdfium.dll");
        if !dll.exists() {
            eprintln!("skipped: pdfium.dll is not beside the test executable");
            return;
        }
        let path = std::env::temp_dir().join(format!("simpl-app-find-{}.pdf", std::process::id()));
        std::fs::write(&path, tall_pdf()).unwrap();
        let document = complete(reader_pdf::open(path.clone())).unwrap();
        let _ = std::fs::remove_file(path);
        let (pdf, _) =
            pdf_reader::Reader::new(document.clone(), None, Size::new(1280.0, 800.0), 1.0);
        let mut reader = Reader {
            pdf: Some(pdf),
            ..Reader::default()
        };
        let _ = update_inner(&mut reader, Message::FindOpen);
        assert!(reader.find.as_ref().unwrap().indexing);
        assert!(
            reader
                .controls()
                .any(|control| control == Control::FindInput)
        );
        // Typing before the page text has been read finds nothing yet.
        let _ = update_inner(&mut reader, Message::FindChanged("lighthouse".into()));
        assert_eq!(reader.find.as_ref().unwrap().count(), 0);
        let text = complete(document.session.page_text(0)).unwrap();
        let generation = reader.find_generation;
        let _ = update_inner(
            &mut reader,
            Message::PdfFindIndexed {
                generation,
                result: Ok(PdfFindIndex {
                    document: document.id,
                    pages: Arc::new(vec![text]),
                    truncated: false,
                }),
            },
        );
        let find = reader.find.as_ref().unwrap();
        assert!(!find.indexing);
        assert_eq!(
            find.pdf_matches.len(),
            1,
            "the query typed earlier is re-run"
        );
        assert_eq!(find.current, Some(0));
        let selection = reader.pdf.as_ref().unwrap().selection().unwrap();
        assert_eq!(
            selection.focus.index - selection.anchor.index,
            "Lighthouse".len() - 1
        );
        let _ = update_inner(&mut reader, Message::FindStep(true));
        assert_eq!(reader.find.as_ref().unwrap().current, Some(0));
        let _ = update_inner(&mut reader, Message::FindClose);
        assert!(reader.find.is_none());
        assert!(reader.pdf.as_ref().unwrap().selection().is_none());
    }

    #[test]
    fn find_starts_at_the_reading_position_and_opening_another_book_closes_it() {
        let mut reader = searchable_reader();
        let _ = reader.rebuild_geometry(Anchor {
            row: 5,
            fraction: 0.0,
        });
        settle_pagination(&mut reader);
        let _ = update_inner(&mut reader, Message::FindOpen);
        let _ = update_inner(&mut reader, Message::FindChanged("lighthouse".into()));
        assert_eq!(selected_item(&reader), Some(7));
        let _ = update_inner(
            &mut reader,
            Message::Loaded {
                request: 0,
                result: Ok(LoadReply {
                    document: LoadedDocument::Reflow(book("another")),
                    catalog: None,
                }),
            },
        );
        assert!(reader.find.is_none());
        assert!(reader.selection.endpoints().is_none());
    }

    #[test]
    fn failed_library_write_stops_window_close_until_retry_or_discard() {
        for retry in [true, false] {
            let mut reader = Reader {
                preferences_loading: false,
                recent_loading: false,
                ..Default::default()
            };
            let _ = reader.shelf.update(shelf::Message::Loaded(Ok(Vec::new())));
            let _ = reader
                .shelf
                .update(shelf::Message::ShelvesLoaded(Ok(Default::default())));
            reader.pending_exit = true;
            let _ = update_inner(
                &mut reader,
                Message::Shelf(shelf::Message::Saved(Err("disk full".into()))),
            );
            assert!(!reader.pending_exit && !reader.exit_ready());
            assert!(matches!(reader.failed_close, Some(CloseAction::Window)));
            assert!(
                reader
                    .controls()
                    .any(|c| c == Control::Shelf(shelf::Control::RetrySave))
            );
            if retry {
                let _ = update_inner(
                    &mut reader,
                    Message::Shelf(shelf::Message::Activate(shelf::Control::RetrySave)),
                );
                assert!(reader.pending_exit && reader.failed_close.is_none());
                assert!(reader.shelf.saving);
                let _ = update_inner(&mut reader, Message::Shelf(shelf::Message::Saved(Ok(()))));
                assert!(reader.exit_ready() && !reader.shelf.has_failed_save());
            } else {
                let _ = update_inner(&mut reader, Message::CloseWithoutSaving);
                assert!(reader.exit_ready() && !reader.shelf.has_failed_save());
                assert!(reader.failed_close.is_none());
            }
        }
    }

    #[test]
    fn failed_save_can_be_retried_without_losing_the_reading_anchor() {
        let mut reader = Reader {
            book: Some(book("current")),
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 3,
            fraction: 0.4,
        });
        settle_pagination(&mut reader);
        let _ = update_inner(
            &mut reader,
            Message::Saved {
                action: CloseAction::Document,
                result: Err("storage unavailable".into()),
            },
        );
        let _ = reader.close(CloseAction::Document);
        assert!(reader.saving);
        assert!(reader.failed_close.is_none());
        let position = reader.saved_position().unwrap().1;
        assert_eq!(position.item_id, "paragraph-3");
        assert!((position.within - 0.4).abs() < 0.001);
    }

    #[test]
    fn dismissing_a_save_error_cancels_the_pending_close() {
        let mut reader = Reader {
            book: Some(book("current")),
            ..Reader::default()
        };
        let _ = update_inner(
            &mut reader,
            Message::Saved {
                action: CloseAction::Document,
                result: Err("storage unavailable".into()),
            },
        );
        let _ = update_inner(&mut reader, Message::DismissError);
        assert!(reader.failed_close.is_none());
        assert!(reader.error.is_none());
        assert_eq!(reader.book.as_ref().unwrap().title, "current");
    }

    #[test]
    fn obsolete_load_success_and_failure_cannot_replace_latest_book() {
        let mut reader = Reader {
            request: 2,
            ..Reader::default()
        };
        let _ = update_inner(
            &mut reader,
            Message::Loaded {
                request: 2,
                result: Ok(LoadReply {
                    document: LoadedDocument::Reflow(book("latest")),
                    catalog: None,
                }),
            },
        );
        let _ = update_inner(
            &mut reader,
            Message::Loaded {
                request: 1,
                result: Ok(LoadReply {
                    document: LoadedDocument::Reflow(book("old")),
                    catalog: None,
                }),
            },
        );
        assert_eq!(reader.book.as_ref().unwrap().title, "latest");
        let _ = update_inner(
            &mut reader,
            Message::Loaded {
                request: 1,
                result: Err("old error".into()),
            },
        );
        assert!(reader.error.is_none());
        assert_eq!(reader.book.as_ref().unwrap().title, "latest");
    }

    #[test]
    fn switching_reading_surfaces_releases_the_previous_keyboard_action() {
        let mut reader = Reader {
            focused: Some(Control::Shelf(shelf::Control::Resume(0))),
            ..Reader::default()
        };
        let _ = update_inner(
            &mut reader,
            Message::Loaded {
                request: 0,
                result: Ok(LoadReply {
                    document: LoadedDocument::Reflow(book("keyboard")),
                    catalog: None,
                }),
            },
        );
        assert!(
            reader.focused.is_none(),
            "Space must scroll, not reactivate a hidden shelf card"
        );
        reader.focused = Some(Control::Close);
        let _ = update_inner(
            &mut reader,
            Message::Saved {
                action: CloseAction::Document,
                result: Ok(()),
            },
        );
        assert!(
            reader.focused.is_none(),
            "Space on the shelf must resume, not close again"
        );
    }

    #[test]
    fn collapsing_reader_controls_preserves_position_and_removes_hidden_focus_targets() {
        let mut reader = Reader {
            book: Some(book("reading")),
            focused: Some(Control::FontUp),
            show_help: true,
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 3,
            fraction: 0.4,
        });
        settle_pagination(&mut reader);
        let original = reader.saved_position().unwrap().1;
        let _ = update_inner(&mut reader, Message::ToggleToolbar);
        assert!(!reader.toolbar_expanded);
        assert!(!reader.show_help);
        assert_eq!(reader.focused, Some(Control::ToggleToolbar));
        assert!(
            !reader
                .controls()
                .any(|control| control == Control::FontUp || control == Control::Close)
        );
        assert!(
            reader
                .controls()
                .any(|control| control == Control::ToggleToolbar)
        );
        let collapsed = reader.saved_position().unwrap().1;
        assert_eq!(collapsed.item_id, original.item_id);
        assert_eq!(collapsed.within, original.within);
        let _ = reader.activate(Control::ToggleToolbar);
        assert!(reader.toolbar_expanded);
        assert!(reader.controls().any(|control| control == Control::Close));
        assert_eq!(reader.saved_position().unwrap().1.item_id, original.item_id);
    }

    #[test]
    fn reflow_and_native_refinement_preserve_source_anchor_and_selection() {
        let mut reader = Reader {
            book: Some(book("reading")),
            viewport: 200.0,
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 0,
            fraction: 0.0,
        });
        settle_pagination(&mut reader);
        reader.pending_anchor = None;
        reader.offset = reader.offset_for(Anchor {
            row: 3,
            fraction: 0.4,
        });
        let start = Endpoint {
            item_id: "paragraph-3".into(),
            byte_offset: 2,
        };
        let end = Endpoint {
            item_id: "paragraph-3".into(),
            byte_offset: 10,
        };
        reader.selection.begin(start.clone());
        reader.selection.extend(end.clone());
        reader.selection.end_drag();
        let old_generation = reader.generation;
        let anchor = reader.anchor();
        reader.width = 460.0;
        reader.font_size = 28.0;
        let _ = reader.rebuild_geometry(anchor);
        settle_pagination(&mut reader);
        reader.measurements.lock().extend([
            (2, 1800.0, reader.width, reader.generation),
            (3, 1500.0, reader.width, reader.generation),
        ]);
        let _ = reader.refine_geometry();
        let (_, position) = reader.saved_position().unwrap();
        assert_eq!(position.item_id, "paragraph-3");
        assert!((position.within - 0.4).abs() < 0.001);
        assert_eq!(position.font_size, DEFAULT_FONT_SIZE);
        assert_eq!(reader.selection.endpoints(), Some((&start, &end)));
        let _ = update_inner(
            &mut reader,
            Message::Scroll {
                generation: old_generation,
                offset: 0.0,
                viewport: 200.0,
                page_top: 0.0,
            },
        );
        assert_eq!(reader.saved_position().unwrap().1.item_id, "paragraph-3");
    }

    #[test]
    fn completed_reflow_accepts_wheel_scroll_even_when_estimated_heights_are_exact() {
        let mut reader = Reader {
            book: Some(book("reading")),
            viewport: 200.0,
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 3,
            fraction: 0.4,
        });
        settle_pagination(&mut reader);
        let generation = reader.generation;
        let page = reader.active_page().unwrap();
        let offset = page.height - reader.viewport;
        let scroll = Message::Scroll {
            generation,
            page_top: reader.active_page().unwrap().top,
            offset,
            viewport: 200.0,
        };
        let _ = update_inner(&mut reader, Message::LayoutReady(generation - 1));
        let _ = update_inner(&mut reader, scroll.clone());
        assert_eq!(reader.saved_position().unwrap().1.item_id, "paragraph-3");

        let _ = update_inner(&mut reader, Message::LayoutReady(generation));
        let _ = update_inner(&mut reader, scroll);
        assert!((reader.local_offset() - offset).abs() < 0.01);
        assert_eq!(reader.active_page().unwrap().number, page.number);
    }

    #[test]
    fn failed_replacement_keeps_current_text_readable_and_copyable() {
        let mut reader = Reader {
            book: Some(book("current")),
            request: 3,
            ..Reader::default()
        };
        reader.selection.begin(Endpoint {
            item_id: "paragraph-0".into(),
            byte_offset: 0,
        });
        reader.selection.extend(Endpoint {
            item_id: "paragraph-0".into(),
            byte_offset: 10,
        });
        reader.selection.end_drag();
        let _ = update_inner(
            &mut reader,
            Message::Loaded {
                request: 3,
                result: Err("Cannot read missing.html".into()),
            },
        );
        assert_eq!(reader.book.as_ref().unwrap().title, "current");
        assert_eq!(
            reader
                .selection
                .copy_text(&reader.book.as_ref().unwrap().items)
                .as_deref(),
            Some("A readable")
        );
        assert!(reader.error.as_ref().unwrap().contains("missing.html"));
    }
}
