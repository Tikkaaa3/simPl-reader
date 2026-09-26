//! The normal, fixture-independent local HTML, PDF and EPUB reader.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{pdf_reader, ui};
use iced::keyboard::{self, Key, key};
use iced::widget::{column, container, image, row, scrollable, text};
use iced::{Element, Font, Length, Size, Subscription, Task, mouse, window};
use iced_shell::{reader, selection, virtual_reader};
use reader_document::position::{self, EpubReadingPosition, PdfReadingPosition, ReadingPosition};
use reader_document::recent::{self, DocumentKind, Entry};
use reader_document::{BaseDirection, Endpoint, Item};

const DEFAULT_FONT_SIZE: f32 = 18.0;
const MIN_FONT_SIZE: f32 = 12.0;
const MAX_FONT_SIZE: f32 = 36.0;
const GAP: f32 = 14.0;
const OVERSCAN: f32 = 600.0;
const CONTENT_ROW_HEIGHT: f32 = 34.0;
const RECENT_ROW_HEIGHT: f32 = 52.0;
const RECENT_ROW_SPACING: f32 = 8.0;

#[derive(Debug)]
struct DisplayImage {
    handle: image::Handle,
    width: u32,
    height: u32,
}

#[derive(Debug)]
struct EpubChapter {
    document: Arc<reader_document::epub::Epub>,
    index: usize,
    anchors: HashMap<String, String>,
}

#[derive(Debug)]
struct Book {
    path: PathBuf,
    title: String,
    fingerprint: String,
    items: Vec<Item>,
    images: HashMap<String, DisplayImage>,
    warnings: Vec<String>,
    restored: Option<ReadingPosition>,
    epub: Option<EpubChapter>,
}

fn load_book(
    path: PathBuf,
    locate: Option<&Entry>,
    carried: Option<&SavedPosition>,
) -> Result<Book, String> {
    let mut document = reader_document::load_html(&path)?;
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
    Ok(display_book(document, restored, None))
}

fn display_book(
    document: reader_document::Document,
    restored: Option<ReadingPosition>,
    epub: Option<EpubChapter>,
) -> Book {
    let reader_document::Document {
        path,
        title,
        fingerprint,
        items,
        images,
        warnings,
    } = document;
    let images = images
        .into_iter()
        .map(|(key, asset)| {
            let image = DisplayImage {
                handle: image::Handle::from_rgba(asset.width, asset.height, asset.rgba),
                width: asset.width,
                height: asset.height,
            };
            (key, image)
        })
        .collect();
    Book {
        path,
        title: epub
            .as_ref()
            .map_or(title, |chapter| chapter.document.title.clone()),
        fingerprint,
        items,
        images,
        warnings,
        restored,
        epub,
    }
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
    let index = restored.and_then(|position| {
        document
            .chapters
            .iter()
            .position(|chapter| chapter.href == position.chapter)
    });
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
    book.warnings.extend(warnings);
    Ok(book)
}

fn load_epub_chapter(
    document: Arc<reader_document::epub::Epub>,
    index: usize,
    restored: Option<ReadingPosition>,
) -> Result<Book, String> {
    let mut chapter = document.load_chapter(index)?;
    chapter
        .document
        .warnings
        .extend(document.warnings.iter().cloned());
    Ok(display_book(
        chapter.document,
        restored,
        Some(EpubChapter {
            document,
            index,
            anchors: chapter.anchors,
        }),
    ))
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

enum SavedPosition {
    Html(PathBuf, ReadingPosition),
    Pdf(PathBuf, PdfReadingPosition),
    Epub(PathBuf, EpubReadingPosition),
}

impl SavedPosition {
    fn save(&self) -> Result<(), String> {
        match self {
            Self::Html(path, _) | Self::Pdf(path, _) | Self::Epub(path, _) => self.save_at(path),
        }
    }

    fn save_at(&self, path: &Path) -> Result<(), String> {
        match self {
            Self::Html(_, position) => position::save(path, position),
            Self::Pdf(_, position) => position::save_pdf(path, position),
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
            "Unsupported document format: {}. Open an HTML, XHTML, PDF or EPUB file.",
            path.display()
        )),
    }
}

impl LoadedDocument {
    fn recent_entry(&self) -> Entry {
        match self {
            Self::Reflow(book) => Entry {
                path: book.path.clone(),
                title: book.title.clone(),
                fingerprint: book.fingerprint.clone(),
                kind: if book.epub.is_some() {
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
) -> Result<LoadedDocument, String> {
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
        Ok(LoadedDocument::Pdf {
            document,
            restored,
            warnings,
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
        Ok(LoadedDocument::Reflow(Arc::new(book)))
    }
}

fn forward_pdf(document: u64, task: Task<pdf_reader::Message>) -> Task<Message> {
    task.map(move |message| Message::Pdf { document, message })
}

#[derive(Clone, Copy, Debug)]
struct Anchor {
    row: usize,
    fraction: f32,
}

#[derive(Debug)]
struct Reader {
    book: Option<Arc<Book>>,
    pdf: Option<pdf_reader::Reader>,
    pdf_warnings: Vec<String>,
    window_size: Size,
    scale_factor: f64,
    opening_task: Option<iced::task::Handle>,
    opening: Option<PathBuf>,
    recent: Vec<Entry>,
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
    show_help: bool,
    focused: Option<Control>,
    focus_generation: u64,
    focus_pending: bool,
    error: Option<String>,
    dialog_open: bool,
    show_warnings: bool,
    show_contents: bool,
    contents_offset: f32,
    request: u64,
    generation: u64,
    font_size: f32,
    width: f32,
    viewport: f32,
    offset: f32,
    heights: virtual_reader::HeightIndex,
    measurements: virtual_reader::Measurements,
    pending_anchor: Option<Anchor>,
    selection: selection::SelectionState,
    saving: bool,
    failed_close: Option<CloseAction>,
    window: Option<window::Id>,
}

impl Default for Reader {
    fn default() -> Self {
        Self {
            book: None,
            pdf: None,
            pdf_warnings: Vec::new(),
            window_size: Size::new(1000.0, 720.0),
            scale_factor: 1.0,
            opening_task: None,
            opening: None,
            recent: Vec::new(),
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
            focused: None,
            focus_generation: 0,
            focus_pending: false,
            error: None,
            dialog_open: false,
            show_warnings: false,
            show_contents: false,
            contents_offset: 0.0,
            request: 0,
            generation: 0,
            font_size: DEFAULT_FONT_SIZE,
            width: content_width(1000.0),
            viewport: 620.0,
            offset: 0.0,
            heights: virtual_reader::HeightIndex::new(Vec::new()),
            measurements: Default::default(),
            pending_anchor: None,
            selection: Default::default(),
            saving: false,
            failed_close: None,
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
    Open,
    Recent,
    Close,
    Help,
    FontDown,
    FontUp,
    Copy,
    Pdf(pdf_reader::FocusControl),
    PreviousChapter,
    NextChapter,
    Contents,
    ContentsEntry(usize),
    DismissError,
    LocateMissing,
    CloseWithoutSaving,
    Warnings,
    ResetRecent,
    HideRecent,
    OpenRecent(usize),
    LocateRecent(usize),
    RemoveRecent(usize),
    HideHelp,
}

#[derive(Clone, Debug)]
enum Message {
    OpenDialog,
    RecentLoaded(Result<Vec<Entry>, String>),
    RecentSaved(Result<(), String>),
    ToggleRecent,
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
        result: Result<LoadedDocument, String>,
    },
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
    FontSize(f32),
    SelectStart(Endpoint),
    SelectMove(Endpoint),
    Copy,
    LayoutReady(u64),
    Scroll {
        generation: u64,
        offset: f32,
        viewport: f32,
    },
    Close(CloseAction),
    Saved {
        action: CloseAction,
        result: Result<(), String>,
    },
    CloseWithoutSaving,
    ToggleWarnings,
    Event(iced::Event, window::Id),
}

fn content_width(window_width: f32) -> f32 {
    (window_width - 64.0).clamp(160.0, 960.0)
}

fn image_size(image: &DisplayImage, width: f32) -> Size {
    let scale = (width / image.width.max(1) as f32).min(1.0);
    Size::new(image.width as f32 * scale, image.height as f32 * scale)
}

fn text_size(item: &Item, body: f32) -> f32 {
    match item {
        Item::Heading { level, .. } => {
            body * match level {
                1 => 1.7,
                2 => 1.45,
                _ => 1.2,
            }
        }
        _ => body,
    }
}

impl Reader {
    fn interactive(&self) -> bool {
        self.opening.is_none() && !self.saving && !self.dialog_open
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
        let row = self.heights.window(self.offset, 0.0, 0.0).start;
        Anchor {
            row,
            fraction: ((self.offset - self.heights.start(row)) / self.heights.height(row).max(1.0))
                .clamp(0.0, 1.0),
        }
    }

    fn offset_for(&self, anchor: Anchor) -> f32 {
        if self.heights.is_empty() {
            return 0.0;
        }
        let row = anchor.row.min(self.heights.len() - 1);
        (self.heights.start(row) + anchor.fraction * self.heights.height(row))
            .min((self.heights.total() - self.viewport).max(0.0))
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
                        chapter: chapter.document.chapters[chapter.index].href.clone(),
                        item_id: position.item_id,
                        within: position.within,
                        font_size: position.font_size,
                    },
                ))
            } else {
                Some(SavedPosition::Html(path, position))
            }
        }
    }

    fn cancel_open(&mut self) {
        if let Some(task) = self.opening_task.take() {
            task.abort();
        }
        self.request = self.request.wrapping_add(1);
        self.opening = None;
        self.pending_locate = None;
        self.recent_open = None;
    }

    fn clear_content(&mut self) {
        self.book = None;
        self.pdf = None;
        self.pdf_warnings.clear();
        self.selection.clear();
        self.heights = virtual_reader::HeightIndex::new(Vec::new());
        self.measurements.lock().clear();
        self.pending_anchor = None;
        self.offset = 0.0;
        self.generation = self.generation.wrapping_add(1);
        self.show_warnings = false;
        self.show_contents = false;
        self.contents_offset = 0.0;
    }

    fn rebuild_geometry(&mut self, anchor: Anchor) -> Task<Message> {
        self.generation = self.generation.wrapping_add(1);
        self.measurements.lock().clear();
        let heights = self
            .book
            .as_ref()
            .map(|book| {
                book.items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        let height = match item {
                            Item::Image { asset_path, .. } => book
                                .images
                                .get(asset_path)
                                .map_or(self.font_size * 1.5, |image| {
                                    image_size(image, self.width).height
                                }),
                            _ => {
                                let size = text_size(item, self.font_size);
                                let lines = item
                                    .text()
                                    .unwrap_or_default()
                                    .split('\n')
                                    .map(|line| {
                                        (line.chars().count() as f32 * size * 0.52 / self.width)
                                            .ceil()
                                            .max(1.0)
                                    })
                                    .sum::<f32>();
                                lines * size * 1.5
                            }
                        };
                        height
                            + if index + 1 == book.items.len() {
                                0.0
                            } else {
                                GAP
                            }
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.heights = virtual_reader::HeightIndex::new(heights);
        self.offset = self.offset_for(anchor);
        self.pending_anchor = Some(anchor);
        scroll_to(self.offset)
    }

    fn refine_geometry(&mut self) -> Task<Message> {
        if self.book.is_none() || self.opening.is_some() {
            return Task::none();
        }
        let anchor = self.anchor();
        let mut changed = false;
        let mut pending = self.measurements.lock();
        for (row, height, width, generation) in pending.drain(..) {
            if generation != self.generation || width != self.width || row >= self.heights.len() {
                continue;
            }
            if (self.heights.height(row) - height).abs() > 0.01 {
                self.heights.refine(row, height, self.offset);
                changed = true;
            }
        }
        drop(pending);
        // A native layout also acknowledges an exact estimate. Keeping the
        // anchor locked in that case would discard subsequent wheel scrolling.
        let restore = self.pending_anchor.take().is_some();
        if changed || restore {
            self.offset = self.offset_for(anchor);
            scroll_to(self.offset)
        } else {
            Task::none()
        }
    }

    fn open(&mut self, path: PathBuf, locate: Option<Entry>) -> Task<Message> {
        if self.saving || self.dialog_open || self.failed_close.is_some() {
            return Task::none();
        }
        let previous = self.document_position();
        self.cancel_open();
        let request = self.request;
        self.opening = Some(path.clone());
        self.pending_locate = locate.clone();
        self.recent_open = if locate.is_some() {
            None
        } else {
            self.recent.iter().find(|entry| entry.path == path).cloned()
        };
        self.error = None;
        self.missing_recent = None;
        self.selection.end_drag();
        let (task, handle) = Task::perform(load_document(path, previous, locate), move |result| {
            Message::Loaded { request, result }
        })
        .abortable();
        self.opening_task = Some(handle.abort_on_drop());
        task
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

    fn contents_range(&self) -> std::ops::Range<usize> {
        let Some(chapter) = self
            .book
            .as_ref()
            .and_then(|book| book.epub.as_ref())
            .filter(|_| self.show_contents)
        else {
            return 0..0;
        };
        let count = chapter.document.contents.len();
        let height = (self.window_size.height * 0.35).clamp(100.0, 240.0);
        let start = ((self.contents_offset / CONTENT_ROW_HEIGHT) as usize)
            .saturating_sub(1)
            .min(count);
        start..(start + (height / CONTENT_ROW_HEIGHT).ceil() as usize + 3).min(count)
    }

    fn controls(&self) -> impl Iterator<Item = Control> + Clone + '_ {
        let book = self.book.as_ref();
        let chapter = book.and_then(|book| book.epub.as_ref());
        let history = self.show_recent || (book.is_none() && self.pdf.is_none());
        let pdf_page = self.pdf.as_ref().map(pdf_reader::Reader::page_index);
        let primary = [
            Some(Control::Open),
            Some(Control::Recent),
            (book.is_some() || self.pdf.is_some()).then_some(Control::Close),
            Some(Control::Help),
            book.is_some_and(|_| self.font_size > MIN_FONT_SIZE)
                .then_some(Control::FontDown),
            book.is_some_and(|_| self.font_size < MAX_FONT_SIZE)
                .then_some(Control::FontUp),
            book.is_some_and(|_| self.selection.endpoints().is_some())
                .then_some(Control::Copy),
            pdf_page
                .filter(|page| *page > 0)
                .map(|_| Control::Pdf(pdf_reader::FocusControl::Previous)),
            self.pdf
                .as_ref()
                .map(|_| Control::Pdf(pdf_reader::FocusControl::Page)),
            pdf_page
                .filter(|page| {
                    self.pdf
                        .as_ref()
                        .is_some_and(|pdf| *page + 1 < pdf.document().pages.len())
                })
                .map(|_| Control::Pdf(pdf_reader::FocusControl::Next)),
            self.pdf
                .as_ref()
                .map(|_| Control::Pdf(pdf_reader::FocusControl::ZoomOut)),
            self.pdf
                .as_ref()
                .map(|_| Control::Pdf(pdf_reader::FocusControl::ZoomIn)),
            self.pdf
                .as_ref()
                .map(|_| Control::Pdf(pdf_reader::FocusControl::ActualSize)),
            self.pdf
                .as_ref()
                .map(|_| Control::Pdf(pdf_reader::FocusControl::FitWidth)),
            self.pdf
                .as_ref()
                .filter(|pdf| pdf.copy_ready())
                .map(|_| Control::Pdf(pdf_reader::FocusControl::Copy)),
            chapter
                .filter(|chapter| chapter.index > 0)
                .map(|_| Control::PreviousChapter),
            chapter
                .filter(|chapter| chapter.index + 1 < chapter.document.chapters.len())
                .map(|_| Control::NextChapter),
            chapter.map(|_| Control::Contents),
        ];
        let secondary = [
            self.error.as_ref().map(|_| Control::DismissError),
            self.missing_recent.as_ref().map(|_| Control::LocateMissing),
            self.failed_close.map(|_| Control::CloseWithoutSaving),
            self.history_corrupt.then_some(Control::ResetRecent),
            (book.is_some_and(|book| !book.warnings.is_empty()) || !self.pdf_warnings.is_empty())
                .then_some(Control::Warnings),
            (history && (book.is_some() || self.pdf.is_some())).then_some(Control::HideRecent),
        ];
        primary
            .into_iter()
            .flatten()
            .chain(self.contents_range().map(Control::ContentsEntry))
            .chain(secondary.into_iter().flatten())
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
            .chain(self.show_help.then_some(Control::HideHelp))
    }

    fn activate(&mut self, control: Control) -> Task<Message> {
        if !self.controls().any(|visible| visible == control) {
            return Task::none();
        }
        let message = match control {
            Control::Open => Message::OpenDialog,
            Control::Recent | Control::HideRecent => Message::ToggleRecent,
            Control::Close => Message::Close(CloseAction::Document),
            Control::Help | Control::HideHelp => Message::ToggleHelp,
            Control::FontDown => Message::FontSize(self.font_size - 2.0),
            Control::FontUp => Message::FontSize(self.font_size + 2.0),
            Control::Copy => Message::Copy,
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
                    pdf_reader::FocusControl::Copy => pdf_reader::Message::Copy,
                    pdf_reader::FocusControl::Page => unreachable!(),
                };
                return update_inner(self, Message::Pdf { document, message });
            }
            Control::PreviousChapter => return self.adjacent_chapter(false),
            Control::NextChapter => return self.adjacent_chapter(true),
            Control::Contents => Message::ToggleContents,
            Control::ContentsEntry(index) => {
                let Some(entry) = self
                    .book
                    .as_ref()
                    .and_then(|book| book.epub.as_ref())
                    .and_then(|chapter| chapter.document.contents.get(index))
                else {
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
            Control::Warnings => Message::ToggleWarnings,
            Control::ResetRecent => Message::ResetRecent,
            Control::OpenRecent(index) => Message::OpenRecent(index),
            Control::LocateRecent(index) => Message::LocateRecent(index),
            Control::RemoveRecent(index) => Message::RemoveRecent(index),
        };
        update_inner(self, message)
    }

    fn open_chapter(&mut self, index: usize, fragment: Option<String>) -> Task<Message> {
        if !self.interactive() {
            return Task::none();
        }
        let Some(book) = &self.book else {
            return Task::none();
        };
        let Some(chapter) = &book.epub else {
            return Task::none();
        };
        if index >= chapter.document.chapters.len() {
            return Task::none();
        }
        if index == chapter.index {
            let row = fragment
                .as_ref()
                .and_then(|fragment| chapter.anchors.get(fragment))
                .and_then(|id| book.items.iter().position(|item| item.id() == id));
            if fragment.is_some() && row.is_none() {
                self.error = Some("The contents target is absent from this chapter.".into());
                return Task::none();
            }
            self.show_contents = false;
            self.selection.clear();
            self.error = None;
            return self.rebuild_geometry(Anchor {
                row: row.unwrap_or(0),
                fraction: 0.0,
            });
        }
        let document = chapter.document.clone();
        let path = book.path.clone();
        let font_size = self.font_size;
        let previous = self.document_position();
        self.cancel_open();
        let request = self.request;
        self.opening = Some(path);
        self.error = None;
        self.failed_close = None;
        self.selection.end_drag();
        let (task, handle) = Task::perform(
            async move {
                let warning = previous.and_then(|position| position.save().err());
                let mut book = load_epub_chapter(document, index, None)?;
                let target = fragment
                    .as_ref()
                    .and_then(|fragment| book.epub.as_ref()?.anchors.get(fragment));
                if fragment.is_some() && target.is_none() {
                    book.warnings
                        .push("The contents target is absent; showing the chapter start.".into());
                }
                book.restored = book.items.first().map(|first| ReadingPosition {
                    fingerprint: book.fingerprint.clone(),
                    item_id: target.cloned().unwrap_or_else(|| first.id().to_owned()),
                    within: 0.0,
                    font_size,
                });
                if let Some(error) = warning {
                    book.warnings.push(format!(
                        "Could not save the previous reading position: {error}"
                    ));
                }
                Ok(LoadedDocument::Reflow(Arc::new(book)))
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
        let index = if next {
            chapter.index.checked_add(1)
        } else {
            chapter.index.checked_sub(1)
        };
        index.map_or_else(Task::none, |index| self.open_chapter(index, None))
    }

    fn finish_close(&mut self, action: CloseAction) -> Task<Message> {
        self.failed_close = None;
        match action {
            CloseAction::Window => {
                if self.recent_loading
                    || self.recent_saving
                    || (self.recent_dirty && !self.history_corrupt)
                {
                    self.pending_exit = true;
                    self.persist_recent()
                } else {
                    iced::exit()
                }
            }
            CloseAction::Document => {
                self.cancel_open();
                self.clear_content();
                self.error = None;
                Task::none()
            }
        }
    }

    fn close(&mut self, action: CloseAction) -> Task<Message> {
        if self.saving || self.dialog_open {
            return Task::none();
        }
        self.failed_close = None;
        self.cancel_open();
        self.selection.end_drag();
        if let Some(position) = self.document_position() {
            self.saving = true;
            Task::perform(async move { position.save() }, move |result| {
                Message::Saved { action, result }
            })
        } else {
            self.finish_close(action)
        }
    }

    fn jump(&mut self, offset: f32) -> Task<Message> {
        if !self.interactive() || self.book.is_none() {
            return Task::none();
        }
        self.pending_anchor = None;
        self.offset = offset.clamp(0.0, (self.heights.total() - self.viewport).max(0.0));
        scroll_to(self.offset)
    }
}

fn update(reader: &mut Reader, message: Message) -> Task<Message> {
    let previous_focus = reader.focused;
    let previous_panels = (reader.show_recent, reader.show_help, reader.show_contents);
    let resized = matches!(
        &message,
        Message::Event(iced::Event::Window(window::Event::Resized(_)), _)
    );
    let task = update_inner(reader, message);
    let panels_changed =
        previous_panels != (reader.show_recent, reader.show_help, reader.show_contents);
    if reader.focused != previous_focus || resized || panels_changed {
        reader.focus_generation = reader.focus_generation.wrapping_add(1);
        reader.focus_pending = reader.focused.is_some();
    }
    task
}

fn update_inner(reader: &mut Reader, message: Message) -> Task<Message> {
    match message {
        Message::FocusReady(generation) if generation == reader.focus_generation => {
            reader.focus_pending = false;
            ui::reveal_focus()
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
            if reader.pending_exit && !reader.recent_saving {
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
            if reader.pending_exit && !reader.recent_saving {
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
        Message::ToggleRecent if reader.interactive() => {
            reader.show_recent = !reader.show_recent;
            reader.focused =
                if reader.show_recent || (reader.book.is_none() && reader.pdf.is_none()) {
                    Some(if reader.recent.is_empty() {
                        Control::Recent
                    } else {
                        Control::OpenRecent(0)
                    })
                } else {
                    Some(Control::Recent)
                };
            restore_viewport(reader)
        }
        Message::ToggleHelp if reader.interactive() => {
            reader.show_help = !reader.show_help;
            reader.focused = Some(if reader.show_help {
                Control::HideHelp
            } else {
                Control::Help
            });
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
            reader.dialog_locate = Some(entry);
            reader.dialog_open = true;
            reader.selection.end_drag();
            Task::perform(
                async { crate::platform::open_document_dialog(true) },
                Message::DialogChosen,
            )
        }
        Message::LocateMissing if reader.interactive() => {
            let Some(entry) = reader.missing_recent.clone() else {
                return Task::none();
            };
            reader.dialog_locate = Some(entry);
            reader.dialog_open = true;
            Task::perform(
                async { crate::platform::open_document_dialog(true) },
                Message::DialogChosen,
            )
        }
        Message::RemoveRecent(index) if reader.interactive() => {
            let Some(entry) = reader.recent.get(index).cloned() else {
                return Task::none();
            };
            recent::remove(&mut reader.recent, &entry.path);
            reader.recent_dirty = true;
            reader.persist_recent()
        }
        Message::DismissError => {
            reader.error = None;
            reader.missing_recent = None;
            reader.failed_close = None;
            restore_viewport(reader)
        }
        Message::OpenDialog if reader.interactive() => {
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
                Ok(Some(path)) => reader.open(path, locate),
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
            reader.opening = None;
            reader.opening_task = None;
            let relocated = reader.pending_locate.take();
            let recent_open = reader.recent_open.take();
            let history_task = result.as_ref().ok().map_or_else(Task::none, |document| {
                reader.missing_recent = None;
                reader.remember_recent(document.recent_entry(), relocated.as_ref())
            });
            let content_task = match result {
                Ok(LoadedDocument::Reflow(book)) => {
                    reader.clear_content();
                    reader.font_size = book
                        .restored
                        .as_ref()
                        .map_or(DEFAULT_FONT_SIZE, |position| {
                            position.font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
                        });
                    let anchor = book
                        .restored
                        .as_ref()
                        .and_then(|position| {
                            book.items
                                .iter()
                                .position(|item| item.id() == position.item_id)
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
                    reader.show_warnings = false;
                    reader.selection.clear();
                    reader.rebuild_geometry(anchor)
                }
                Ok(LoadedDocument::Pdf {
                    document,
                    restored,
                    warnings,
                }) => {
                    reader.clear_content();
                    let id = document.id;
                    let (pdf, task) = pdf_reader::Reader::new(
                        document,
                        restored,
                        reader.window_size,
                        reader.scale_factor,
                    );
                    reader.pdf = Some(pdf);
                    reader.pdf_warnings = warnings;
                    reader.error = None;
                    forward_pdf(id, task)
                }
                Err(error) => {
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
            Task::batch([content_task, history_task])
        }
        Message::Pdf { document, message } => {
            if let Some(pdf) = &mut reader.pdf
                && pdf.document().id == document
            {
                forward_pdf(document, pdf.update(message))
            } else {
                Task::none()
            }
        }
        Message::EpubChapter { index, fragment } => reader.open_chapter(index, fragment),
        Message::ToggleContents if reader.interactive() => {
            if reader.book.as_ref().is_some_and(|book| book.epub.is_some()) {
                reader.show_contents = !reader.show_contents;
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
            if let Some(pdf) = &mut reader.pdf {
                forward_pdf(pdf.document().id, pdf.resize(reader.window_size, scale))
            } else {
                Task::none()
            }
        }
        Message::FontSize(size) if reader.interactive() && reader.book.is_some() => {
            let anchor = reader.anchor();
            reader.font_size = size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
            reader.rebuild_geometry(anchor)
        }
        Message::SelectStart(endpoint) if reader.interactive() => {
            reader.selection.begin(endpoint);
            Task::none()
        }
        Message::SelectMove(endpoint) if reader.interactive() => {
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
        } if generation == reader.generation => {
            reader.viewport = viewport;
            if reader.pending_anchor.is_none() {
                reader.offset = offset;
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
        Message::CloseWithoutSaving => reader
            .failed_close
            .map_or_else(Task::none, |action| reader.finish_close(action)),
        Message::ToggleWarnings if reader.interactive() => {
            reader.show_warnings = !reader.show_warnings;
            restore_viewport(reader)
        }
        Message::Event(event, id) => {
            if let iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                repeat,
                ..
            }) = &event
                && reader.interactive()
            {
                match key.as_ref() {
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
                        let count = reader
                            .book
                            .as_ref()
                            .and_then(|book| book.epub.as_ref())
                            .map_or(0, |chapter| chapter.document.contents.len());
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
                    Key::Named(key::Named::F1) if !repeat => {
                        return update_inner(reader, Message::ToggleHelp);
                    }
                    Key::Named(key::Named::Escape) if !repeat => {
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
                        if reader.show_warnings {
                            reader.show_warnings = false;
                            return restore_viewport(reader);
                        }
                        if reader.error.is_some() {
                            return update_inner(reader, Message::DismissError);
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
                            && reader.pdf.is_some() =>
                    {
                        reader.focused = Some(Control::Pdf(pdf_reader::FocusControl::Page));
                        return iced::widget::operation::focus(pdf_reader::page_input_id());
                    }
                    _ => {}
                }
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
                }) if modifiers.control() && (value.eq_ignore_ascii_case("o") || value.eq_ignore_ascii_case("w") || value.eq_ignore_ascii_case("r") || value.eq_ignore_ascii_case("l")));
            if pdf_input && !global_shortcut {
                let accepted = reader.interactive() || !matches!(event, iced::Event::Keyboard(_));
                if let Some(pdf) = &mut reader.pdf {
                    return if accepted {
                        forward_pdf(pdf.document().id, pdf.event(&event))
                    } else {
                        Task::none()
                    };
                }
            }
            match event {
                iced::Event::Window(window::Event::Opened { size, .. }) => {
                    reader.window = Some(id);
                    reader.window_size = size;
                    reader.width = content_width(size.width);
                    reader.viewport = (size.height - 100.0).max(100.0);
                    let task = if let Some(pdf) = &mut reader.pdf {
                        forward_pdf(pdf.document().id, pdf.resize(size, reader.scale_factor))
                    } else if reader.book.is_some() {
                        reader.rebuild_geometry(reader.anchor())
                    } else {
                        Task::none()
                    };
                    Task::batch([
                        task,
                        window::scale_factor(id).map(|scale| Message::ScaleFactor(scale.into())),
                    ])
                }
                iced::Event::Window(window::Event::Resized(size)) => {
                    let anchor = reader.anchor();
                    let width = content_width(size.width);
                    let width_changed = (width - reader.width).abs() > 0.5;
                    reader.width = width;
                    reader.viewport =
                        (reader.viewport + size.height - reader.window_size.height).max(1.0);
                    reader.window_size = size;
                    let task = if let Some(pdf) = &mut reader.pdf {
                        forward_pdf(pdf.document().id, pdf.resize(size, reader.scale_factor))
                    } else if width_changed && reader.book.is_some() {
                        reader.rebuild_geometry(anchor)
                    } else {
                        Task::none()
                    };
                    Task::batch([
                        task,
                        window::scale_factor(id).map(|scale| Message::ScaleFactor(scale.into())),
                    ])
                }
                iced::Event::Window(window::Event::CloseRequested) => {
                    reader.close(CloseAction::Window)
                }
                iced::Event::Window(window::Event::FileDropped(path)) => reader.open(path, None),
                iced::Event::Window(window::Event::Unfocused)
                | iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    reader.selection.end_drag();
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
                                "+" | "=" => {
                                    update_inner(reader, Message::FontSize(reader.font_size + 2.0))
                                }
                                "-" => {
                                    update_inner(reader, Message::FontSize(reader.font_size - 2.0))
                                }
                                "0" => update_inner(reader, Message::FontSize(DEFAULT_FONT_SIZE)),
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
                        Key::Named(key::Named::PageDown | key::Named::Space) => {
                            reader.jump(reader.offset + reader.viewport * 0.9)
                        }
                        Key::Named(key::Named::PageUp) => {
                            reader.jump(reader.offset - reader.viewport * 0.9)
                        }
                        Key::Named(key::Named::Home) if modifiers.control() => reader.jump(0.0),
                        Key::Named(key::Named::End) if modifiers.control() => {
                            reader.jump(reader.heights.total())
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
        scroll_to(reader.offset)
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

fn render_item(
    reader: &Reader,
    book: &Book,
    index: usize,
    item: &Item,
    bounds: Option<selection::SelectionBounds>,
) -> Element<'static, Message> {
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
    let mapped = match reader::map_document_paragraph(logical, direction, styles) {
        Ok(mapped) => mapped,
        Err(error) => return text(format!("Cannot display this paragraph: {error}")).into(),
    };
    let size = text_size(item, reader.font_size);
    selection::selectable_text(
        selection::SelectableParagraphConfig {
            item_id: item.id().to_owned(),
            logical_text: logical.to_owned(),
            mapped,
            font_size: size,
            line_height: size * 1.5,
            selection: bounds.and_then(|bounds| bounds.range_for_item(index, logical)),
            dragging: reader.selection.is_dragging(),
            track_hit_test: false,
        },
        Message::SelectStart,
        |endpoint, _| Message::SelectMove(endpoint),
    )
}

fn control_button<'a>(
    reader: &Reader,
    control: Control,
    label: impl Into<Element<'a, Message>>,
    message: Option<Message>,
) -> Element<'a, Message> {
    toned_button(
        reader,
        control,
        label,
        message,
        ui::ButtonTone::Quiet,
        false,
    )
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
    let button = iced::widget::button(label)
        .padding([7, 11])
        .on_press_maybe(message)
        .style(move |_, status| ui::button_style(status, tone, focused, selected))
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
    if has_document {
        heading = heading.push(control_button(
            reader,
            Control::HideRecent,
            text("Hide").size(13),
            active.then_some(Message::ToggleRecent),
        ));
    }
    let mut history = column![heading].spacing(10);
    if reader.recent_loading {
        history = history.push(text("Loading your library…").size(13).color(ui::MUTED));
    } else if reader.recent.is_empty() {
        history = history.push(
            text("Your recent documents will appear here after you open a file.")
                .size(13)
                .color(ui::MUTED),
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
                container(text(kind).size(10).font(ui::SEMIBOLD).color(ui::ACCENT))
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
                            .color(ui::MUTED)
                            .wrapping(iced::widget::text::Wrapping::None),
                    ]
                    .spacing(2),
                )
                .width(Length::Fill)
                .clip(true),
                control_button(
                    reader,
                    Control::OpenRecent(index),
                    text("Open").size(12),
                    active.then_some(Message::OpenRecent(index)),
                ),
                toned_button(
                    reader,
                    Control::LocateRecent(index),
                    text("Locate").size(12),
                    active.then_some(Message::LocateRecent(index)),
                    ui::ButtonTone::Subtle,
                    false,
                ),
                toned_button(
                    reader,
                    Control::RemoveRecent(index),
                    text("Remove").size(12),
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

fn view(reader: &Reader) -> Element<'_, Message> {
    let active = reader.interactive();
    let loaded = (reader.book.is_some() || reader.pdf.is_some()) && active;
    let has_document = reader.book.is_some() || reader.pdf.is_some();
    let current_title = reader
        .book
        .as_ref()
        .map(|book| book.title.as_str())
        .or_else(|| reader.pdf.as_ref().map(|pdf| pdf.document().title.as_str()));
    let brand = row![
        text("simPl").size(22).font(ui::SEMIBOLD).color(ui::ACCENT),
        container(
            text(current_title.unwrap_or("A quieter place to read"))
                .size(13)
                .color(ui::MUTED)
                .wrapping(iced::widget::text::Wrapping::None)
        )
        .width(Length::Fill)
        .clip(true),
        text("LOCAL READER").size(10).color(ui::MUTED),
    ]
    .spacing(16)
    .align_y(iced::Alignment::Center);
    let mut toolbar = row![
        toned_button(
            reader,
            Control::Open,
            text("Open file").size(13).font(ui::SEMIBOLD),
            active.then_some(Message::OpenDialog),
            ui::ButtonTone::Primary,
            false,
        ),
        control_button(
            reader,
            Control::Recent,
            text("Recent").size(13),
            active.then_some(Message::ToggleRecent)
        ),
    ]
    .spacing(7)
    .align_y(iced::Alignment::Center);
    if has_document {
        toolbar = toolbar.push(control_button(
            reader,
            Control::Close,
            text("Close").size(13),
            loaded.then_some(Message::Close(CloseAction::Document)),
        ));
    }
    toolbar = toolbar
        .push(control_button(
            reader,
            Control::Help,
            text("Keys").size(13),
            active.then_some(Message::ToggleHelp),
        ))
        .push(iced::widget::Space::new().width(Length::Fill));
    if reader.book.is_some() {
        toolbar = toolbar
            .push(control_button(
                reader,
                Control::FontDown,
                text("A−").size(13),
                (loaded && reader.font_size > MIN_FONT_SIZE)
                    .then_some(Message::FontSize(reader.font_size - 2.0)),
            ))
            .push(
                text(format!("{:.0}", reader.font_size))
                    .size(12)
                    .color(ui::MUTED),
            )
            .push(control_button(
                reader,
                Control::FontUp,
                text("A+").size(13),
                (loaded && reader.font_size < MAX_FONT_SIZE)
                    .then_some(Message::FontSize(reader.font_size + 2.0)),
            ))
            .push(control_button(
                reader,
                Control::Copy,
                text("Copy").size(13),
                (loaded && reader.selection.endpoints().is_some()).then_some(Message::Copy),
            ));
    }
    // This slot is always present, so expanding any panel never replaces the
    // reading scrollable's widget-tree position or its native scroll state.
    let mut auxiliary = column![].spacing(8);
    if let Some(chapter) = reader.book.as_ref().and_then(|book| book.epub.as_ref()) {
        let count = chapter.document.chapters.len();
        let navigation = row![
            text("CHAPTER").size(10).color(ui::MUTED),
            text(format!("{:02} / {:02}", chapter.index + 1, count))
                .size(13)
                .font(ui::SEMIBOLD),
            container(
                text(&chapter.document.chapters[chapter.index].title)
                    .size(13)
                    .color(ui::MUTED)
                    .wrapping(iced::widget::text::Wrapping::None)
            )
            .width(Length::Fill)
            .clip(true),
            control_button(
                reader,
                Control::PreviousChapter,
                text("←").size(13),
                (active && chapter.index > 0).then_some(Message::EpubChapter {
                    index: chapter.index - 1,
                    fragment: None,
                })
            ),
            control_button(
                reader,
                Control::NextChapter,
                text("→").size(13),
                (active && chapter.index + 1 < count).then_some(Message::EpubChapter {
                    index: chapter.index + 1,
                    fragment: None,
                })
            ),
            toned_button(
                reader,
                Control::Contents,
                text("Contents").size(13),
                active.then_some(Message::ToggleContents),
                ui::ButtonTone::Quiet,
                reader.show_contents,
            ),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);
        auxiliary = auxiliary.push(container(navigation).padding([3, 12]).style(ui::panel));
        if reader.show_contents {
            let contents = &chapter.document.contents;
            let height = (reader.window_size.height * 0.35).clamp(100.0, 240.0);
            let range = reader.contents_range();
            let start = range.start;
            let end = range.end;
            let mut entries =
                column![iced::widget::Space::new().height(start as f32 * CONTENT_ROW_HEIGHT)];
            for (relative, entry) in contents[start..end].iter().enumerate() {
                let label = text(&entry.label)
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
                        entry.chapter == chapter.index,
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
                iced::widget::Space::new()
                    .height((contents.len() - end) as f32 * CONTENT_ROW_HEIGHT),
            );
            auxiliary = auxiliary.push(
                container(
                    scrollable(entries)
                        .direction(ui::vertical_scrollbar())
                        .style(ui::scroll_style)
                        .id(iced::advanced::widget::Id::new("epub-contents"))
                        .height(height)
                        .on_scroll(|viewport| {
                            Message::ContentsScrolled(viewport.absolute_offset().y)
                        }),
                )
                .padding(8)
                .style(ui::panel),
            );
        }
    }
    if let Some(error) = &reader.error {
        let mut notice = row![
            text(error)
                .size(13)
                .width(Length::Fill)
                .wrapping(iced::advanced::text::Wrapping::WordOrGlyph)
                .color(ui::DANGER),
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
                text("Locate").size(12),
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
            .color(ui::DANGER)
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
    let warnings = reader
        .book
        .as_ref()
        .map_or(reader.pdf_warnings.as_slice(), |book| {
            book.warnings.as_slice()
        });
    if !warnings.is_empty() {
        let mut details = column![toned_button(
            reader,
            Control::Warnings,
            text(format!(
                "{} document {} · {}",
                warnings.len(),
                if warnings.len() == 1 {
                    "notice"
                } else {
                    "notices"
                },
                if reader.show_warnings { "Hide" } else { "View" }
            ))
            .size(12),
            active.then_some(Message::ToggleWarnings),
            ui::ButtonTone::Subtle,
            reader.show_warnings,
        )]
        .spacing(6);
        if reader.show_warnings {
            details = details.push(
                scrollable(
                    text(warnings.join("\n"))
                        .size(12)
                        .color(ui::MUTED)
                        .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
                )
                .direction(ui::vertical_scrollbar())
                .style(ui::scroll_style)
                .height(90),
            );
        }
        auxiliary = auxiliary.push(container(details).padding([5, 10]).style(ui::panel));
    }
    if reader.show_recent && has_document {
        auxiliary = auxiliary.push(recent_panel(reader, active, true));
    }
    if reader.show_help {
        let help = column![
            row![
                text("Keyboard shortcuts").size(16).font(ui::SEMIBOLD),
                iced::widget::Space::new().width(Length::Fill),
                control_button(reader, Control::HideHelp, text("Hide").size(12),
                    Some(Message::ToggleHelp)),
            ]
            .align_y(iced::Alignment::Center),
            text("Tab / Shift+Tab  ·  Controls    Enter / Space  ·  Activate    Escape  ·  Dismiss    F1  ·  Help")
                .size(12).color(ui::MUTED),
            text("Ctrl+O  Open    Ctrl+R  Recent    Ctrl+W  Close    Ctrl+C  Copy    Ctrl+A  Select all")
                .size(12).color(ui::MUTED),
            text("Page Up / Down  Read    Ctrl+Home / End  Ends    Ctrl+plus / minus / 0  Text size or PDF zoom")
                .size(12).color(ui::MUTED),
            text("EPUB  Ctrl+T contents · Ctrl+Page Up / Down chapters    PDF  Ctrl+L page · Ctrl+F fit width")
                .size(12).color(ui::MUTED),
        ]
        .spacing(6);
        let mut panel = container(help).padding(12).style(ui::panel);
        if reader.focused == Some(Control::HideHelp) {
            panel = panel.id(iced::advanced::widget::Id::new(ui::FOCUSED_CONTROL));
        }
        auxiliary = auxiliary.push(panel);
    }
    if reader.saving {
        auxiliary = auxiliary.push(text("Saving reading position…").size(12).color(ui::MUTED));
    } else if let Some(path) = &reader.opening {
        auxiliary = auxiliary.push(
            container(
                text(format!("Opening {}…", path.display()))
                    .size(12)
                    .color(ui::MUTED)
                    .wrapping(iced::widget::text::Wrapping::None),
            )
            .width(Length::Fill)
            .clip(true),
        );
    }
    let mut page = column![
        container(brand)
            .width(Length::Fill)
            .padding([10, 20])
            .style(ui::header),
        container(toolbar)
            .width(Length::Fill)
            .padding(iced::Padding {
                top: 5.0,
                right: 20.0,
                bottom: 8.0,
                left: 20.0
            }),
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
    ]
    .spacing(4);
    if let Some(pdf) = &reader.pdf {
        let document = pdf.document().id;
        page = page.push(
            pdf.view(reader.focused.and_then(|control| {
                if let Control::Pdf(control) = control {
                    Some(control)
                } else {
                    None
                }
            }))
            .map(move |message| Message::Pdf { document, message }),
        );
    } else if let Some(book) = &reader.book {
        let range = reader
            .heights
            .window(reader.offset, reader.viewport, OVERSCAN);
        let bounds = reader.selection.bounds(&book.items);
        let rows = range
            .clone()
            .map(|index| render_item(reader, book, index, &book.items[index], bounds))
            .collect();
        let blocks = virtual_reader::VisibleRows::new(
            range,
            &reader.heights,
            reader.width,
            GAP,
            rows,
            virtual_reader::LayoutReports {
                measurements: reader.measurements.clone(),
                generation: reader.generation,
                counters: None,
            },
        );
        let centered = container(container(blocks).width(reader.width)).center_x(Length::Fill);
        let generation = reader.generation;
        page = page.push(
            container(
                scrollable(centered)
                    .direction(ui::vertical_scrollbar())
                    .style(ui::scroll_style)
                    .id(scroll_id())
                    .height(Length::Fill)
                    .on_scroll(move |viewport| Message::Scroll {
                        generation,
                        offset: viewport.absolute_offset().y,
                        viewport: viewport.bounds().height,
                    }),
            )
            .padding([0, 24])
            .height(Length::Fill),
        );
    } else {
        let compact = reader.window_size.height < 500.0;
        let mut introduction = column![].spacing(7);
        if !compact {
            introduction = introduction.push(
                text("YOUR READING SPACE")
                    .size(10)
                    .font(ui::SEMIBOLD)
                    .color(ui::ACCENT),
            );
        }
        introduction = introduction
            .push(
                text("Settle into a good read.")
                    .size(if compact { 22 } else { 26 })
                    .font(ui::SEMIBOLD),
            )
            .push(
                text(if compact {
                    "HTML, PDF or EPUB. Drop a file here, or use Open file."
                } else {
                    "Open a local HTML, XHTML, PDF or EPUB document, or drop one into this window."
                })
                .size(if compact { 12 } else { 14 })
                .color(ui::MUTED)
                .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
            );
        if !compact {
            introduction = introduction.push(
                text("Private by design · No scripts or remote resources")
                    .size(11)
                    .color(ui::MUTED),
            );
        }
        let hero = container(introduction)
            .padding([if compact { 12 } else { 17 }, 18])
            .width(Length::Fill)
            .style(ui::panel);
        page = page.push(
            container(
                scrollable(column![hero, recent_panel(reader, active, false),].spacing(12))
                    .id(iced::advanced::widget::Id::new("welcome-panels"))
                    .direction(ui::vertical_scrollbar())
                    .style(ui::scroll_style)
                    .height(Length::Fill),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(iced::Padding {
                top: 8.0,
                right: 20.0,
                bottom: 12.0,
                left: 20.0,
            }),
        );
    }
    container(page)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn title(reader: &Reader) -> String {
    if let Some(pdf) = &reader.pdf {
        return format!("{} — simPl", pdf.document().title);
    }
    reader.book.as_ref().map_or_else(
        || "simPl".to_owned(),
        |book| format!("{} — simPl", book.title),
    )
}

fn subscription(reader: &Reader) -> Subscription<Message> {
    use iced_futures::subscription::{self, Event};

    #[derive(Hash)]
    struct HtmlEvents;

    let measurements = reader.measurements.clone();
    let generation = reader.generation;
    let pending = reader.pending_anchor.is_some();
    let active = reader.book.is_some() && reader.opening.is_none();
    let focus = reader.focus_pending.then_some(reader.focus_generation);
    subscription::filter_map(
        (HtmlEvents, generation, pending, active, focus),
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
                    || value.eq_ignore_ascii_case("l")))
                && !matches!(
                    &key,
                    Key::Named(key::Named::Tab | key::Named::F1 | key::Named::Escape)
                ) =>
            {
                None
            }
            Event::Interaction { event, window, .. } => Some(Message::Event(event, window)),
            Event::SystemThemeChanged(_) => None,
        },
    )
}

pub fn run(path: Option<PathBuf>, error: Option<String>) -> iced::Result {
    iced::application(
        move || {
            let mut reader = Reader {
                error: error.clone(),
                ..Reader::default()
            };
            let tasks = [
                Task::perform(async { recent::load() }, Message::RecentLoaded),
                path.clone()
                    .map_or_else(Task::none, |path| reader.open(path, None)),
            ];
            (reader, Task::batch(tasks))
        },
        update,
        view,
    )
    .title(title)
    .subscription(subscription)
    .theme(|_: &Reader| ui::theme())
    .settings(iced::Settings {
        default_font: Font::with_name("Segoe UI"),
        default_text_size: 13.into(),
        ..iced::Settings::default()
    })
    .window(window::Settings {
        size: Size::new(1000.0, 720.0),
        min_size: Some(Size::new(540.0, 360.0)),
        position: window::Position::Centered,
        exit_on_close_request: false,
        ..window::Settings::default()
    })
    .run()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(title: &str) -> Arc<Book> {
        Arc::new(Book {
            path: PathBuf::from(format!("{title}.html")),
            title: title.into(),
            fingerprint: "a".repeat(64),
            items: (0..12)
                .map(|index| Item::Paragraph {
                    id: format!("paragraph-{index}"),
                    text: "A readable paragraph with enough words to wrap. ".repeat(50),
                    base_direction: BaseDirection::Ltr,
                    style_runs: Vec::new(),
                })
                .collect(),
            images: HashMap::new(),
            warnings: Vec::new(),
            restored: None,
            epub: None,
        })
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
                result: Ok(LoadedDocument::Reflow(book("latest"))),
            },
        );
        let _ = update_inner(
            &mut reader,
            Message::Loaded {
                request: 1,
                result: Ok(LoadedDocument::Reflow(book("old"))),
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
        reader.pending_anchor = None;
        reader.offset = reader.heights.start(3) + reader.heights.height(3) * 0.4;
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
        reader.measurements.lock().extend([
            (2, 1800.0, reader.width, reader.generation),
            (3, 1500.0, reader.width, reader.generation),
        ]);
        let _ = reader.refine_geometry();
        let (_, position) = reader.saved_position().unwrap();
        assert_eq!(position.item_id, "paragraph-3");
        assert!((position.within - 0.4).abs() < 0.001);
        assert_eq!(position.font_size, 28.0);
        assert_eq!(reader.selection.endpoints(), Some((&start, &end)));
        let _ = update_inner(
            &mut reader,
            Message::Scroll {
                generation: old_generation,
                offset: 0.0,
                viewport: 200.0,
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
        let generation = reader.generation;
        let offset = reader.offset_for(Anchor {
            row: 5,
            fraction: 0.25,
        });
        let scroll = Message::Scroll {
            generation,
            offset,
            viewport: 200.0,
        };
        let _ = update_inner(&mut reader, Message::LayoutReady(generation - 1));
        let _ = update_inner(&mut reader, scroll.clone());
        assert_eq!(reader.saved_position().unwrap().1.item_id, "paragraph-3");

        let _ = update_inner(&mut reader, Message::LayoutReady(generation));
        let _ = update_inner(&mut reader, scroll);
        let position = reader.saved_position().unwrap().1;
        assert_eq!(position.item_id, "paragraph-5");
        assert!((position.within - 0.25).abs() < 0.001);
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
