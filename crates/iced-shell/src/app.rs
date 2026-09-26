//! The normal, fixture-independent local HTML, PDF and EPUB reader.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::pdf_reader;
use iced::keyboard::{self, Key, key};
use iced::widget::{button, column, container, image, row, scrollable, text};
use iced::{Element, Font, Length, Size, Subscription, Task, Theme, mouse, window};
use iced_shell::{reader, selection, virtual_reader};
use reader_document::position::{self, EpubReadingPosition, PdfReadingPosition, ReadingPosition};
use reader_document::{BaseDirection, Endpoint, Item};

const DEFAULT_FONT_SIZE: f32 = 18.0;
const MIN_FONT_SIZE: f32 = 12.0;
const MAX_FONT_SIZE: f32 = 36.0;
const GAP: f32 = 14.0;
const OVERSCAN: f32 = 600.0;

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

fn load_book(path: PathBuf) -> Result<Book, String> {
    let mut document = reader_document::load_html(&path)?;
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

fn load_epub(path: PathBuf) -> Result<Book, String> {
    let document = Arc::new(reader_document::epub::open(&path)?);
    let mut warnings = Vec::new();
    let restored = match position::load_epub(&document.path) {
        Ok(position) => position.filter(|position| position.fingerprint == document.fingerprint),
        Err(error) => {
            warnings.push(format!("Could not restore the reading position: {error}"));
            None
        }
    };
    let index = restored.as_ref().and_then(|position| {
        document
            .chapters
            .iter()
            .position(|chapter| chapter.href == position.chapter)
    });
    let restored = index.and(restored).map(|position| ReadingPosition {
        fingerprint: position.fingerprint,
        item_id: position.item_id,
        within: position.within,
        font_size: position.font_size,
    });
    let mut book = load_epub_chapter(document, index.unwrap_or(0), restored)?;
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
    fn save(self) -> Result<(), String> {
        match self {
            Self::Html(path, position) => position::save(&path, &position),
            Self::Pdf(path, position) => position::save_pdf(&path, &position),
            Self::Epub(path, position) => position::save_epub(&path, &position),
        }
    }
}

async fn load_document(
    path: PathBuf,
    previous: Option<SavedPosition>,
) -> Result<LoadedDocument, String> {
    let save_warning = previous.and_then(|position| position.save().err());
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    {
        let document = reader_pdf::open(path).await?;
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
        let mut book = if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("epub"))
        {
            load_epub(path)?
        } else {
            load_book(path)?
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

#[derive(Clone, Debug)]
enum Message {
    OpenDialog,
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

    fn open(&mut self, path: PathBuf) -> Task<Message> {
        if self.saving {
            return Task::none();
        }
        let previous = self.document_position();
        self.cancel_open();
        let request = self.request;
        self.opening = Some(path.clone());
        self.error = None;
        self.failed_close = None;
        self.selection.end_drag();
        let (task, handle) = Task::perform(load_document(path, previous), move |result| {
            Message::Loaded { request, result }
        })
        .abortable();
        self.opening_task = Some(handle.abort_on_drop());
        task
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
            CloseAction::Window => iced::exit(),
            CloseAction::Document => {
                self.cancel_open();
                self.clear_content();
                self.error = None;
                Task::none()
            }
        }
    }

    fn close(&mut self, action: CloseAction) -> Task<Message> {
        if self.saving {
            return Task::none();
        }
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
    match message {
        Message::OpenDialog if reader.interactive() => {
            reader.dialog_open = true;
            reader.selection.end_drag();
            Task::perform(
                async { crate::platform::open_document_dialog() },
                Message::DialogChosen,
            )
        }
        Message::DialogChosen(result) => {
            reader.dialog_open = false;
            let task = match result {
                Ok(Some(path)) => reader.open(path),
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
            match result {
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
                    reader.error = Some(error);
                    // The loading view replaced the scroll widget. Restore its
                    // native offset as well as retaining the document model.
                    if reader.book.is_some() {
                        scroll_to(reader.offset)
                    } else {
                        Task::none()
                    }
                }
            }
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
                return Task::batch([
                    scroll_to(reader.offset),
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
        } if generation == reader.generation && reader.interactive() => {
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
        Message::ToggleWarnings => {
            reader.show_warnings = !reader.show_warnings;
            Task::none()
        }
        Message::Event(event, id) => {
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
                }) if modifiers.control() && (value.eq_ignore_ascii_case("o") || value.eq_ignore_ascii_case("w")));
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
                    reader.window_size = size;
                    reader.viewport = (size.height - 100.0).max(100.0);
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
                iced::Event::Window(window::Event::FileDropped(path)) => reader.open(path),
                iced::Event::Window(window::Event::Unfocused)
                | iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    reader.selection.end_drag();
                    Task::none()
                }
                iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    match key.as_ref() {
                        Key::Character(value) if modifiers.control() => {
                            match value.to_ascii_lowercase().as_str() {
                                "o" => update(reader, Message::OpenDialog),
                                "w" => reader.close(CloseAction::Document),
                                "c" => update(reader, Message::Copy),
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
                                    update(reader, Message::FontSize(reader.font_size + 2.0))
                                }
                                "-" => update(reader, Message::FontSize(reader.font_size - 2.0)),
                                "0" => update(reader, Message::FontSize(DEFAULT_FONT_SIZE)),
                                "t" => update(reader, Message::ToggleContents),
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
                                update(reader, Message::ToggleContents)
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

fn view(reader: &Reader) -> Element<'_, Message> {
    let active = reader.interactive();
    let loaded = (reader.book.is_some() || reader.pdf.is_some()) && active;
    let mut toolbar = row![
        button("Open...").on_press_maybe(active.then_some(Message::OpenDialog)),
        button("Close").on_press_maybe(loaded.then_some(Message::Close(CloseAction::Document))),
        iced::widget::Space::new().width(Length::Fill),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);
    if reader.pdf.is_none() {
        toolbar = toolbar
            .push(
                button("A-").on_press_maybe(
                    (loaded && reader.font_size > MIN_FONT_SIZE)
                        .then_some(Message::FontSize(reader.font_size - 2.0)),
                ),
            )
            .push(text(format!("{:.0} px", reader.font_size)).size(14))
            .push(
                button("A+").on_press_maybe(
                    (loaded && reader.font_size < MAX_FONT_SIZE)
                        .then_some(Message::FontSize(reader.font_size + 2.0)),
                ),
            )
            .push(button("Copy").on_press_maybe(
                (loaded && reader.selection.endpoints().is_some()).then_some(Message::Copy),
            ));
    }
    let mut page = column![container(toolbar).padding(12)].spacing(8);
    if let Some(chapter) = reader.book.as_ref().and_then(|book| book.epub.as_ref()) {
        let count = chapter.document.chapters.len();
        let navigation = row![
            button("Prev chapter").on_press_maybe((active && chapter.index > 0).then(|| {
                Message::EpubChapter {
                    index: chapter.index - 1,
                    fragment: None,
                }
            })),
            text(format!("Chapter {} / {}", chapter.index + 1, count)).size(14),
            button("Next chapter").on_press_maybe((active && chapter.index + 1 < count).then_some(
                Message::EpubChapter {
                    index: chapter.index + 1,
                    fragment: None
                }
            )),
            button(if reader.show_contents {
                "Hide contents"
            } else {
                "Contents"
            })
            .on_press_maybe(active.then_some(Message::ToggleContents)),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center);
        page = page.push(
            container(
                column![
                    navigation,
                    text(&chapter.document.chapters[chapter.index].title).size(15),
                    text("Selection and copy apply to the current chapter.").size(12),
                ]
                .spacing(6),
            )
            .padding([0, 20]),
        );
        if reader.show_contents {
            const ROW_HEIGHT: f32 = 34.0;
            let contents = &chapter.document.contents;
            let height = (reader.window_size.height * 0.35).clamp(100.0, 240.0);
            let start = ((reader.contents_offset / ROW_HEIGHT) as usize)
                .saturating_sub(1)
                .min(contents.len());
            let end = (start + (height / ROW_HEIGHT).ceil() as usize + 3).min(contents.len());
            let mut entries = column![iced::widget::Space::new().height(start as f32 * ROW_HEIGHT)];
            for entry in &contents[start..end] {
                let label = text(&entry.label)
                    .size(14)
                    .wrapping(iced::widget::text::Wrapping::None);
                entries = entries.push(
                    container(
                        button(label)
                            .height(ROW_HEIGHT)
                            .width(Length::Fill)
                            .on_press_maybe(active.then(|| Message::EpubChapter {
                                index: entry.chapter,
                                fragment: entry.fragment.clone(),
                            })),
                    )
                    .padding(iced::Padding {
                        left: entry.depth.min(8) as f32 * 14.0,
                        ..iced::Padding::default()
                    }),
                );
            }
            entries = entries.push(
                iced::widget::Space::new().height((contents.len() - end) as f32 * ROW_HEIGHT),
            );
            page = page.push(
                container(
                    scrollable(entries)
                        .id(iced::advanced::widget::Id::new("epub-contents"))
                        .height(height)
                        .on_scroll(|viewport| {
                            Message::ContentsScrolled(viewport.absolute_offset().y)
                        }),
                )
                .padding([0, 20]),
            );
        }
    }
    if let Some(error) = &reader.error {
        let mut notice = column![
            text(error)
                .size(14)
                .color(iced::Color::from_rgb8(255, 170, 145))
        ]
        .spacing(8);
        if reader.failed_close.is_some() {
            notice =
                notice.push(button("Close without saving").on_press(Message::CloseWithoutSaving));
        }
        page = page.push(container(notice).padding([0, 20]));
    }
    let warnings = reader
        .book
        .as_ref()
        .map_or(reader.pdf_warnings.as_slice(), |book| {
            book.warnings.as_slice()
        });
    if !warnings.is_empty() {
        page = page.push(
            container(
                button(
                    text(format!(
                        "{} document warning(s) — {}",
                        warnings.len(),
                        if reader.show_warnings {
                            "hide"
                        } else {
                            "details"
                        },
                    ))
                    .size(13),
                )
                .on_press(Message::ToggleWarnings),
            )
            .padding([0, 20]),
        );
        if reader.show_warnings {
            page = page.push(
                container(scrollable(text(warnings.join("\n")).size(13)).height(110))
                    .padding([0, 20]),
            );
        }
    }
    if reader.saving {
        page = page.push(container(text("Saving reading position...")).padding(24));
    } else if let Some(path) = &reader.opening {
        page = page.push(container(text(format!("Opening {}...", path.display()))).padding(24));
    } else if let Some(pdf) = &reader.pdf {
        let document = pdf.document().id;
        page = page.push(
            pdf.view()
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
        page = page.push(
            container(
                column![
                    text("simPl").size(30),
                    text("Open a local HTML, XHTML, PDF or EPUB document to start reading.")
                        .size(18),
                    text("Ctrl+O opens a file. You can also drop a file here.").size(14),
                    text("Local content only. Scripts and remote resources are not loaded.")
                        .size(14),
                ]
                .spacing(16),
            )
            .padding(24),
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
    subscription::filter_map(
        (HtmlEvents, generation, pending, active),
        move |event| match event {
            Event::Interaction {
                event: iced::Event::Window(window::Event::RedrawRequested(_)),
                ..
            } => {
                // listen_with excludes redraws; an unfiltered raw subscription
                // would redraw forever. Wake only for a new layout or correction.
                (active && (pending || !measurements.lock().is_empty()))
                    .then_some(Message::LayoutReady(generation))
            }
            Event::Interaction {
                event: iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }),
                status: iced::event::Status::Captured,
                ..
            } if !matches!(&key, Key::Character(value)
                if modifiers.control() && (value.eq_ignore_ascii_case("o") || value.eq_ignore_ascii_case("w"))) =>
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
            let task = path
                .clone()
                .map_or_else(Task::none, |path| reader.open(path));
            (reader, task)
        },
        update,
        view,
    )
    .title(title)
    .subscription(subscription)
    .theme(|_: &Reader| Theme::Dark)
    .default_font(Font::with_name("Segoe UI"))
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
    fn obsolete_load_success_and_failure_cannot_replace_latest_book() {
        let mut reader = Reader {
            request: 2,
            ..Reader::default()
        };
        let _ = update(
            &mut reader,
            Message::Loaded {
                request: 2,
                result: Ok(LoadedDocument::Reflow(book("latest"))),
            },
        );
        let _ = update(
            &mut reader,
            Message::Loaded {
                request: 1,
                result: Ok(LoadedDocument::Reflow(book("old"))),
            },
        );
        assert_eq!(reader.book.as_ref().unwrap().title, "latest");
        let _ = update(
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
        let _ = update(
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
        let _ = update(&mut reader, Message::LayoutReady(generation - 1));
        let _ = update(&mut reader, scroll.clone());
        assert_eq!(reader.saved_position().unwrap().1.item_id, "paragraph-3");

        let _ = update(&mut reader, Message::LayoutReady(generation));
        let _ = update(&mut reader, scroll);
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
        let _ = update(
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
