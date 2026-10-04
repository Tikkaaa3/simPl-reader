//! Cancellable background layout jobs. Page numbers are one-based at this API.
use crate::{CoreError, complete, count};
use reader_document::{
    Item,
    reading::{Font, Options},
};
use reader_layout::{
    atlas::{self, Atlas, Section},
    book::{self, Book},
    index::HeightIndex,
    measure, themes,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex, OnceLock, Weak,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LayoutStatus {
    Running,
    Complete,
    Cancelled,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ReadingFont {
    Theme,
    Literata,
    Spectral,
    FiraSans,
}

#[derive(Clone, Copy, Debug, uniffi::Record)]
pub struct LayoutOptions {
    pub font: ReadingFont,
    pub size: u16,
    pub margin: u16,
    /// Zero uses the theme spacing; other values are percentages of font size.
    pub spacing: u16,
}
impl From<LayoutOptions> for Options {
    fn from(value: LayoutOptions) -> Self {
        Self {
            font: match value.font {
                ReadingFont::Theme => Font::Theme,
                ReadingFont::Literata => Font::Literata,
                ReadingFont::Spectral => Font::Spectral,
                ReadingFont::FiraSans => Font::FiraSans,
            },
            size: value.size,
            margin: value.margin,
            spacing: value.spacing,
        }
        .validated()
    }
}

/// A split inside a paragraph. Compose maps `line / lines` to its own line grid.
#[derive(Clone, Debug, uniffi::Record)]
pub struct ParagraphCut {
    pub row: u32,
    pub line: u32,
    pub lines: u32,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PageLayout {
    pub number: u32,
    pub label: String,
    pub row_start: u32,
    pub row_end: u32,
    pub content_start: f32,
    pub content_end: f32,
    pub top: f32,
    pub height: f32,
    pub start_cut: Option<ParagraphCut>,
    pub end_cut: Option<ParagraphCut>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct SectionLayout {
    pub index: u32,
    pub heights: Vec<f32>,
    pub pages: Vec<PageLayout>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct BookAtlas {
    pub fingerprint: String,
    pub total: u32,
    pub source_pages: bool,
    pub sections: Vec<SectionLayout>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum RowKind {
    Heading,
    Paragraph,
    Image,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TextStyle {
    Bold,
    Italic,
    BoldItalic,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct TextRun {
    pub start_byte: u32,
    pub end_byte: u32,
    pub style: TextStyle,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct BookRow {
    pub index: u32,
    pub id: String,
    pub kind: RowKind,
    pub text: Option<String>,
    pub heading_level: Option<u8>,
    pub right_to_left: bool,
    pub styles: Vec<TextRun>,
    pub image_asset: Option<String>,
    pub semantics: crate::RowSemantics,
    pub presentation: crate::RowPresentation,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct PageContent {
    pub section: u32,
    pub layout: PageLayout,
    pub rows: Vec<BookRow>,
}

pub(crate) struct Job<T> {
    pub(crate) cancel: AtomicBool,
    result: Mutex<Option<Result<Option<T>, CoreError>>>,
}
impl<T: Clone> Job<T> {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            cancel: AtomicBool::new(false),
            result: Mutex::new(None),
        })
    }
    pub(crate) fn status(&self) -> LayoutStatus {
        match self.result.lock().expect("layout result").as_ref() {
            Some(Ok(Some(_))) => LayoutStatus::Complete,
            Some(Ok(None)) => LayoutStatus::Cancelled,
            Some(Err(_)) => LayoutStatus::Failed,
            None => LayoutStatus::Running,
        }
    }
    pub(crate) fn result(&self) -> Result<Option<T>, CoreError> {
        self.result
            .lock()
            .expect("layout result")
            .clone()
            .unwrap_or(Ok(None))
    }
    pub(crate) fn run(
        self: &Arc<Self>,
        work: impl FnOnce(&AtomicBool) -> Result<Option<T>, CoreError> + Send + 'static,
    ) -> Result<(), CoreError>
    where
        T: Send + 'static,
    {
        let job = self.clone();
        std::thread::Builder::new()
            .name("reader-layout".into())
            .spawn(move || {
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&job.cancel)))
                        .unwrap_or_else(|_| Err("Book layout worker failed".to_owned().into()));
                let result = if job.cancel.load(Ordering::Relaxed) {
                    Ok(None)
                } else {
                    result
                };
                *job.result.lock().expect("layout result") = Some(result);
            })
            .map_err(|e| CoreError::from(format!("Cannot start book layout: {e}")))?;
        Ok(())
    }
}

#[derive(uniffi::Object)]
pub struct OpenBookTask {
    job: Arc<Job<Arc<OpenBook>>>,
    progress: Arc<reader_pdf::BookProgress>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct BookPreparation {
    pub completed: u32,
    pub total: u32,
    pub cached: bool,
}
#[uniffi::export]
impl OpenBookTask {
    pub fn progress(&self) -> BookPreparation {
        BookPreparation {
            completed: self.progress.completed.load(Ordering::Relaxed),
            total: self.progress.total.load(Ordering::Relaxed),
            cached: self.progress.cached.load(Ordering::Relaxed),
        }
    }
    pub fn status(&self) -> LayoutStatus {
        self.job.status()
    }
    /// Returns None while running or cancelled; failures are returned as errors.
    pub fn result(&self) -> Result<Option<Arc<OpenBook>>, CoreError> {
        self.job.result()
    }
    pub fn cancel(&self) {
        self.job.cancel.store(true, Ordering::Relaxed);
        self.progress.cancel.store(true, Ordering::Relaxed);
    }
}
impl Drop for OpenBookTask {
    fn drop(&mut self) {
        self.job.cancel.store(true, Ordering::Relaxed);
        self.progress.cancel.store(true, Ordering::Relaxed);
    }
}

type OpenBooks = Mutex<HashMap<String, Weak<OpenBook>>>;
static OPEN_BOOKS: OnceLock<OpenBooks> = OnceLock::new();

#[derive(uniffi::Object)]
pub struct OpenBook {
    pub(crate) epoch: u64,
    pub(crate) book: Arc<Book>,
    pub(crate) atlas: Atlas,
    sections: Mutex<HashMap<usize, Arc<Book>>>,
}

/// Start opening and measuring a managed HTML/EPUB or PDF Book. The worker
/// checks cancellation between items/sections and physical PDF conversion pages.
#[uniffi::export]
pub fn open_book(path: String) -> Result<Arc<OpenBookTask>, CoreError> {
    start_book(path, true)
}
fn start_book(path: String, cache: bool) -> Result<Arc<OpenBookTask>, CoreError> {
    let job = Job::new();
    let progress = Arc::new(reader_pdf::BookProgress::default());
    let work_progress = progress.clone();
    let epoch = crate::backup::epoch();
    job.run(move |cancel| {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let path = PathBuf::from(path);
        let book = Arc::new(
            if path
                .extension()
                .is_some_and(|s| s.eq_ignore_ascii_case("pdf"))
            {
                let document = complete(reader_pdf::open(path))?;
                let conversion =
                    complete(document.session.book_with_progress(Some(work_progress)))?;
                let warnings = conversion.warnings.clone();
                let original = reader_document::position::load_pdf(&document.path)?
                    .filter(|p| p.fingerprint == document.fingerprint)
                    .unwrap_or(reader_document::position::PdfReadingPosition {
                        fingerprint: document.fingerprint.clone(),
                        page: 0,
                        within: 0.0,
                        horizontal: 0.0,
                        zoom: reader_document::position::PdfZoom::FitWidth,
                    });
                book::pdf_book(document, conversion, original, None, warnings)?
            } else {
                book::open(&path)?
            },
        );
        let Some(atlas) = atlas::build_with_cache(book.clone(), cancel, cache)? else {
            return Ok(None);
        };
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let opened = Arc::new(OpenBook {
            epoch,
            book,
            atlas,
            sections: Mutex::new(HashMap::new()),
        });
        let mut books = OPEN_BOOKS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .expect("open books");
        books.retain(|_, book| book.strong_count() > 0);
        books.insert(opened.book.fingerprint.clone(), Arc::downgrade(&opened));
        Ok(Some(opened))
    })?;
    Ok(Arc::new(OpenBookTask { job, progress }))
}

/// Look up a live book by its source fingerprint. Holding OpenBook retains it.
#[uniffi::export]
pub fn atlas(fingerprint: String) -> Result<BookAtlas, CoreError> {
    let book = OPEN_BOOKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("open books")
        .get(&fingerprint)
        .and_then(Weak::upgrade)
        .ok_or_else(|| CoreError::from("Book is not open".to_owned()))?;
    book.atlas()
}

pub(crate) fn section_book(book: &Arc<Book>, section: usize) -> Result<Arc<Book>, CoreError> {
    match &book.epub {
        Some(epub) if section != epub.index => Ok(Arc::new(book::load_epub_chapter(
            epub.document.clone(),
            section,
            None,
        )?)),
        _ if section == 0 || book.epub.as_ref().is_some_and(|e| e.index == section) => {
            Ok(book.clone())
        }
        _ => Err("Section is out of range".to_owned().into()),
    }
}

pub(crate) fn paragraph_cut(
    book: &Book,
    index: &HeightIndex,
    offset: f32,
    theme: &'static themes::ReadingTheme,
    options: Options,
) -> Option<ParagraphCut> {
    if offset <= 0.0 || offset >= index.total() {
        return None;
    }
    let row = index.window(offset, 0.0, 0.0).start;
    let within = offset - index.start(row);
    if within < 0.5 {
        return None;
    }
    let item = book.items.get(row)?;
    item.text()?;
    let style = themes::effective_style(theme, book.pdf_source.is_some(), options);
    let body = options.size as f32;
    let width = atlas::PAPER - options.margin as f32 * 2.0;
    let line = measure::styled_item_size(&style, book, row, item, body) * style.line_height;
    let padding = style.block_padding(item, book.structure.get(item.id()), body, width);
    let gap = if row + 1 == index.len() {
        0.0
    } else {
        style.gap(body)
    };
    let lines = ((index.height(row) - gap - padding.top - padding.bottom).max(line) / line)
        .round()
        .max(1.0) as usize;
    let boundary = ((within - padding.top) / line)
        .round()
        .clamp(0.0, lines as f32) as usize;
    Some(ParagraphCut {
        row: count(row),
        line: count(boundary),
        lines: count(lines),
    })
}
fn page_layout(
    book: &Book,
    geometry: &HeightIndex,
    page: &reader_layout::pages::Page,
    theme: &'static themes::ReadingTheme,
    options: Options,
) -> PageLayout {
    PageLayout {
        number: page.number + 1,
        label: page.label.clone(),
        row_start: count(page.rows.start),
        row_end: count(page.rows.end),
        content_start: page.content.start,
        content_end: page.content.end,
        top: page.top,
        height: page.height,
        start_cut: paragraph_cut(book, geometry, page.content.start, theme, options),
        end_cut: paragraph_cut(book, geometry, page.content.end, theme, options),
    }
}
fn describe(
    book: &Arc<Book>,
    atlas: &Atlas,
    sections: &[Section],
    theme: &'static themes::ReadingTheme,
    options: Options,
) -> Result<BookAtlas, CoreError> {
    let sections = sections
        .iter()
        .enumerate()
        .map(|(index, section)| {
            let book = section_book(book, index)?;
            let geometry = HeightIndex::new(section.heights.clone());
            Ok(SectionLayout {
                index: count(index),
                heights: section.heights.clone(),
                pages: section
                    .pages
                    .iter()
                    .map(|page| page_layout(&book, &geometry, page, theme, options))
                    .collect(),
            })
        })
        .collect::<Result<Vec<_>, CoreError>>()?;
    Ok(BookAtlas {
        fingerprint: book.fingerprint.clone(),
        total: count(atlas.total),
        source_pages: atlas.source_pages,
        sections,
    })
}
pub(crate) fn book_row(
    book: &Book,
    index: usize,
    item: &Item,
    theme: &'static themes::ReadingTheme,
    options: Options,
) -> BookRow {
    let (kind, heading_level, right_to_left, styles, image_asset) = match item {
        Item::Heading { level, .. } => (RowKind::Heading, Some(*level), false, Vec::new(), None),
        Item::Image { asset_path, .. } => (
            RowKind::Image,
            None,
            false,
            Vec::new(),
            Some(asset_path.clone()),
        ),
        Item::Paragraph {
            base_direction,
            style_runs,
            ..
        } => (
            RowKind::Paragraph,
            None,
            *base_direction == reader_document::BaseDirection::Rtl,
            style_runs
                .iter()
                .map(|run| TextRun {
                    start_byte: count(run.start_byte),
                    end_byte: count(run.end_byte),
                    style: match run.style {
                        reader_document::InlineStyle::Bold => TextStyle::Bold,
                        reader_document::InlineStyle::Italic => TextStyle::Italic,
                        reader_document::InlineStyle::BoldItalic => TextStyle::BoldItalic,
                    },
                })
                .collect(),
            None,
        ),
    };
    BookRow {
        index: count(index),
        id: item.id().to_owned(),
        kind,
        text: item.text().map(str::to_owned),
        heading_level,
        right_to_left,
        styles,
        image_asset,
        semantics: crate::reader::semantics(book, item),
        presentation: crate::reader::presentation(book, index, item, theme, options),
    }
}
fn page_content(
    book: &OpenBook,
    atlas: &Atlas,
    sections: &[Section],
    number: u32,
    theme: &'static themes::ReadingTheme,
    options: Options,
) -> Result<Vec<PageContent>, CoreError> {
    if number == 0 || number as usize > atlas.total {
        return Err("Page is out of range".to_owned().into());
    }
    let mut result = Vec::new();
    // A publisher page can continue across EPUB chapter boundaries.
    for (index, section) in sections.iter().enumerate() {
        if !section.pages.iter().any(|page| page.number + 1 == number) {
            continue;
        }
        let geometry = HeightIndex::new(section.heights.clone());
        for page in section.pages.iter().filter(|p| p.number + 1 == number) {
            let book = book.section(index)?;
            result.push(PageContent {
                section: count(index),
                layout: page_layout(&book, &geometry, page, theme, options),
                rows: book.items[page.rows.clone()]
                    .iter()
                    .enumerate()
                    .map(|(row, item)| book_row(&book, row + page.rows.start, item, theme, options))
                    .collect(),
            });
        }
    }
    Ok(result)
}

#[uniffi::export]
impl OpenBook {
    pub fn fingerprint(&self) -> String {
        self.book.fingerprint.clone()
    }
    pub fn atlas(&self) -> Result<BookAtlas, CoreError> {
        describe(
            &self.book,
            &self.atlas,
            &self.atlas.sections,
            themes::default_theme(),
            Options::default(),
        )
    }
    /// Exact serialized canonical atlas, useful for cross-platform parity checks.
    pub fn atlas_json(&self) -> Result<String, CoreError> {
        serde_json::to_string(&self.atlas).map_err(|e| CoreError::from(e.to_string()))
    }
    pub fn page(&self, number: u32) -> Result<Vec<PageContent>, CoreError> {
        page_content(
            self,
            &self.atlas,
            &self.atlas.sections,
            number,
            themes::default_theme(),
            Options::default(),
        )
    }
    pub fn adapt(
        self: Arc<Self>,
        theme: String,
        options: LayoutOptions,
    ) -> Result<Arc<AdaptBookTask>, CoreError> {
        let theme = themes::find(&theme);
        let options = if self.book.pdf_source.is_some() {
            Options::default()
        } else {
            options.into()
        };
        let job = Job::new();
        job.run(move |cancel| {
            let mut sections = Vec::with_capacity(self.atlas.sections.len());
            for (index, section) in self.atlas.sections.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    return Ok(None);
                }
                if self.book.pdf_source.is_some() {
                    sections.push(section.clone());
                    continue;
                }
                let current = self.section(index)?;
                let Some(adapted) =
                    atlas::adapt_section_with(current, section, theme, options, cancel)?
                else {
                    return Ok(None);
                };
                sections.push(adapted);
            }
            Ok(Some(Arc::new(AdaptedBook {
                book: self,
                sections,
                theme,
                options,
            })))
        })?;
        Ok(Arc::new(AdaptBookTask { job }))
    }
}

impl OpenBook {
    /// Bound decoded section assets independently of the size of the book.
    pub(crate) fn section(&self, index: usize) -> Result<Arc<Book>, CoreError> {
        let mut cached = self
            .sections
            .lock()
            .map_err(|_| CoreError::from("Book section cache is unavailable".to_owned()))?;
        if let Some(book) = cached.get(&index) {
            return Ok(book.clone());
        }
        let book = section_book(&self.book, index)?;
        if cached.len() >= 3 {
            cached.clear();
        }
        cached.insert(index, book.clone());
        Ok(book)
    }
}

#[derive(uniffi::Object)]
pub struct AdaptedBook {
    pub(crate) book: Arc<OpenBook>,
    pub(crate) sections: Vec<Section>,
    pub(crate) theme: &'static themes::ReadingTheme,
    pub(crate) options: Options,
}
#[uniffi::export]
impl AdaptedBook {
    pub fn atlas(&self) -> Result<BookAtlas, CoreError> {
        describe(
            &self.book.book,
            &self.book.atlas,
            &self.sections,
            self.theme,
            self.options,
        )
    }
    pub fn page(&self, number: u32) -> Result<Vec<PageContent>, CoreError> {
        page_content(
            &self.book,
            &self.book.atlas,
            &self.sections,
            number,
            self.theme,
            self.options,
        )
    }
}
#[derive(uniffi::Object)]
pub struct AdaptBookTask {
    job: Arc<Job<Arc<AdaptedBook>>>,
}
#[uniffi::export]
impl AdaptBookTask {
    pub fn status(&self) -> LayoutStatus {
        self.job.status()
    }
    pub fn result(&self) -> Result<Option<Arc<AdaptedBook>>, CoreError> {
        self.job.result()
    }
    pub fn cancel(&self) {
        self.job.cancel.store(true, Ordering::Relaxed);
    }
}
impl Drop for AdaptBookTask {
    fn drop(&mut self) {
        self.job.cancel.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait(mut status: impl FnMut() -> LayoutStatus) -> LayoutStatus {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let value = status();
            if value != LayoutStatus::Running {
                return value;
            }
            assert!(Instant::now() < deadline, "layout worker did not finish");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn open_page_lookup_and_adaptation_keep_canonical_page_identity() {
        let scratch = std::env::temp_dir().join(format!("simpl-ffi-layout-{}", std::process::id()));
        let fixtures = reader_layout::golden::write_all(&scratch).unwrap();
        let fixture = fixtures.iter().find(|f| f.name == "chapters-epub").unwrap();
        let task = start_book(fixture.path.to_string_lossy().into_owned(), false).unwrap();
        assert_eq!(wait(|| task.status()), LayoutStatus::Complete);
        let book = task.result().unwrap().unwrap();
        let canonical = book.atlas().unwrap();
        assert!(canonical.total > 2);
        assert_eq!(atlas(book.fingerprint()).unwrap().total, canonical.total);
        assert!(book.page(0).is_err());
        assert!(book.page(canonical.total + 1).is_err());
        let first = book.page(1).unwrap();
        assert!(!first[0].rows.is_empty());
        assert_eq!(first[0].layout.label, "1");
        assert_eq!(first[0].rows[0].index, first[0].layout.row_start);
        assert!(
            canonical
                .sections
                .iter()
                .flat_map(|s| &s.pages)
                .any(|p| p.start_cut.is_some() || p.end_cut.is_some())
        );
        let task = book
            .clone()
            .adapt(
                "soft".into(),
                LayoutOptions {
                    font: ReadingFont::FiraSans,
                    size: 26,
                    margin: 64,
                    spacing: 180,
                },
            )
            .unwrap();
        assert_eq!(wait(|| task.status()), LayoutStatus::Complete);
        let adapted = task.result().unwrap().unwrap();
        let atlas = adapted.atlas().unwrap();
        assert_eq!(atlas.total, canonical.total);
        for (before, after) in canonical.sections.iter().zip(&atlas.sections) {
            assert_eq!(
                before
                    .pages
                    .iter()
                    .map(|p| (&p.label, p.number))
                    .collect::<Vec<_>>(),
                after
                    .pages
                    .iter()
                    .map(|p| (&p.label, p.number))
                    .collect::<Vec<_>>()
            );
        }
        assert_ne!(atlas.sections[0].heights, canonical.sections[0].heights);
        assert_eq!(adapted.page(1).unwrap()[0].rows[0].id, first[0].rows[0].id);
        std::fs::remove_dir_all(scratch).unwrap();
    }
    #[test]
    fn publisher_page_fragments_are_returned_from_every_section() {
        let scratch =
            std::env::temp_dir().join(format!("simpl-ffi-source-pages-{}", std::process::id()));
        let fixtures = reader_layout::golden::write_all(&scratch).unwrap();
        let fixture = fixtures.iter().find(|f| f.name == "paged-epub").unwrap();
        let task = start_book(fixture.path.to_string_lossy().into_owned(), false).unwrap();
        assert_eq!(wait(|| task.status()), LayoutStatus::Complete);
        let book = task.result().unwrap().unwrap();
        let atlas = book.atlas().unwrap();
        assert!(atlas.source_pages);
        for number in 1..=atlas.total {
            let expected = atlas
                .sections
                .iter()
                .flat_map(|s| &s.pages)
                .filter(|p| p.number == number)
                .count();
            let fragments = book.page(number).unwrap();
            assert_eq!(fragments.len(), expected);
            assert!(fragments.iter().all(|p| p.layout.number == number));
        }
        std::fs::remove_dir_all(scratch).unwrap();
    }
    #[test]
    fn background_failure_and_cooperative_cancellation_have_terminal_states() {
        let failed = start_book("missing-layout-fixture.html".into(), false).unwrap();
        assert_eq!(wait(|| failed.status()), LayoutStatus::Failed);
        assert!(failed.result().is_err());
        let gate = Arc::new(std::sync::Barrier::new(2));
        let worker_gate = gate.clone();
        let job = Job::new();
        job.run(move |cancel| {
            worker_gate.wait();
            Ok(if cancel.load(Ordering::Relaxed) {
                None
            } else {
                Some(42)
            })
        })
        .unwrap();
        assert_eq!(job.status(), LayoutStatus::Running);
        job.cancel.store(true, Ordering::Relaxed);
        gate.wait();
        assert_eq!(wait(|| job.status()), LayoutStatus::Cancelled);
        assert_eq!(job.result().unwrap(), None);
    }
}
