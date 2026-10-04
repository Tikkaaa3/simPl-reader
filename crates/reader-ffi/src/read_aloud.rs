//! Bounded source-text streaming and speech coordinates for Android TTS.
use crate::{AdaptedBook, CoreError, OpenBook, PdfDocument, SourcePoint, count};
use reader_core::read_aloud::{self as speech, Cursor};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, uniffi::Record)]
pub struct SpeechChunk {
    pub text: String,
    pub source: SourcePoint,
    /// One-based PDF page, or zero for reflow/passage speech.
    pub pdf_page: u32,
    pub language: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct SpeechRange {
    pub from: SourcePoint,
    pub to: SourcePoint,
    pub pdf_page: u32,
}

#[uniffi::export]
pub fn speech_range(chunk: SpeechChunk, start: u32, end: u32) -> SpeechRange {
    let range = speech::word_bytes(&chunk.text, start, end.saturating_sub(start));
    SpeechRange {
        from: SourcePoint {
            byte: chunk.source.byte.saturating_add(count(range.start)),
            ..chunk.source
        },
        to: SourcePoint {
            byte: chunk.source.byte.saturating_add(count(range.end)),
            ..chunk.source
        },
        pdf_page: chunk.pdf_page,
    }
}

/// A speech plan owns its native document independently of the reader route.
#[derive(uniffi::Object)]
pub struct SpeechPlan {
    source: Source,
    cursor: Mutex<Cursor>,
    /// Only the current PDF page; reflow text remains borrowed from its section.
    pdf_text: Mutex<Option<(usize, String)>>,
}
enum Source {
    Book(Arc<OpenBook>),
    Pdf(Arc<PdfDocument>),
    Passage(String),
}

#[uniffi::export]
pub fn speech_passage(text: String) -> Result<Arc<SpeechPlan>, CoreError> {
    if text.len() > 1024 * 1024 {
        return Err("This passage is too large to read aloud".to_owned().into());
    }
    Ok(Arc::new(SpeechPlan {
        source: Source::Passage(text),
        cursor: Mutex::new(Cursor::default()),
        pdf_text: Mutex::new(None),
    }))
}

#[uniffi::export]
impl OpenBook {
    pub fn speech_row(
        &self,
        point: SourcePoint,
        theme: String,
        options: crate::LayoutOptions,
    ) -> Result<crate::BookRow, CoreError> {
        let book = self.section(point.section as usize)?;
        let item = book
            .items
            .get(point.row as usize)
            .ok_or("Speech row is unavailable".to_owned())?;
        Ok(crate::layout::book_row(
            &book,
            point.row as usize,
            item,
            reader_layout::themes::find(&theme),
            options.into(),
        ))
    }
    pub fn speech_plan(self: Arc<Self>, point: SourcePoint) -> Result<Arc<SpeechPlan>, CoreError> {
        let book = self.section(point.section as usize)?;
        let item = book
            .items
            .get(point.row as usize)
            .ok_or("Speech row is unavailable".to_owned())?;
        let text = item.text().unwrap_or_default();
        if point.byte as usize > text.len() || !text.is_char_boundary(point.byte as usize) {
            return Err("Speech offset is unavailable".to_owned().into());
        }
        let cursor = Cursor {
            section: point.section as usize,
            row: point.row as usize,
            byte: speech::sentence_start(text, point.byte as usize),
        };
        Ok(Arc::new(SpeechPlan {
            source: Source::Book(self),
            cursor: Mutex::new(cursor),
            pdf_text: Mutex::new(None),
        }))
    }
}

#[uniffi::export]
impl PdfDocument {
    pub fn speech_plan(self: Arc<Self>, page: u32) -> Result<Arc<SpeechPlan>, CoreError> {
        self.page_index(page)?;
        if !self.document.can_copy {
            return Err(
                "This PDF does not permit copying its text, so it cannot be read aloud"
                    .to_owned()
                    .into(),
            );
        }
        Ok(Arc::new(SpeechPlan {
            source: Source::Pdf(self),
            cursor: Mutex::new(Cursor {
                section: page as usize - 1,
                ..Cursor::default()
            }),
            pdf_text: Mutex::new(None),
        }))
    }
}

#[uniffi::export]
impl SpeechPlan {
    /// Keep screen-off playback recoverable without keeping a reader ViewModel.
    /// Passages do not move the document's saved reading position.
    pub fn checkpoint(&self, range: SpeechRange) -> Result<(), CoreError> {
        match &self.source {
            Source::Book(book) => {
                let section = book.section(range.from.section as usize)?;
                let text = section
                    .items
                    .get(range.from.row as usize)
                    .and_then(reader_document::Item::text)
                    .unwrap_or_default();
                let within = (range.from.byte as f32 / text.len().max(1) as f32).clamp(0.0, 0.9999);
                let location =
                    book.locate(range.from.section as usize, range.from.row as usize, within)?;
                let size = book.reader_info()?.options.size;
                book.save_location(location, size)
            }
            Source::Pdf(pdf) => {
                let layer = pdf.text(range.pdf_page, 600)?;
                let within = layer
                    .glyphs
                    .iter()
                    .find(|g| g.end > range.from.byte)
                    .and_then(|g| g.bounds.as_ref())
                    .map_or(0.0, |r| r.top);
                let location = pdf.info()?.restored;
                pdf.save_location(crate::PdfLocation {
                    page: range.pdf_page,
                    within,
                    ..location
                })
            }
            Source::Passage(_) => Ok(()),
        }
    }
    /// Call off the UI thread. At most one paragraph/page is extracted per chunk;
    /// images and textless pages are skipped without rasterizing the PDF.
    pub fn next(&self) -> Result<Option<SpeechChunk>, CoreError> {
        let mut cursor = self
            .cursor
            .lock()
            .map_err(|_| "Speech plan is unavailable".to_owned())?;
        loop {
            let chunk = match &self.source {
                Source::Book(opened) => {
                    if cursor.section >= opened.atlas.sections.len() {
                        return Ok(None);
                    }
                    let book = opened.section(cursor.section)?;
                    if cursor.row >= book.items.len() {
                        cursor.section += 1;
                        cursor.row = 0;
                        cursor.byte = 0;
                        continue;
                    }
                    let text = book.items[cursor.row].text().unwrap_or_default();
                    if cursor.byte >= text.len() {
                        cursor.advance_row(book.items.len());
                        continue;
                    }
                    utterance(text, &mut cursor, false)
                }
                Source::Pdf(pdf) => {
                    if cursor.section >= pdf.document.pages.len() {
                        return Ok(None);
                    }
                    let mut cached = self
                        .pdf_text
                        .lock()
                        .map_err(|_| "Speech page is unavailable".to_owned())?;
                    if cached
                        .as_ref()
                        .is_none_or(|(page, _)| *page != cursor.section)
                    {
                        *cached = Some((
                            cursor.section,
                            crate::complete(pdf.document.session.page_text(cursor.section as u32))?,
                        ));
                    }
                    let text = &cached.as_ref().expect("cached PDF speech page").1;
                    if cursor.byte >= text.len() {
                        cursor.section += 1;
                        cursor.byte = 0;
                        continue;
                    }
                    utterance(text, &mut cursor, true)
                }
                Source::Passage(text) => {
                    if cursor.byte >= text.len() {
                        return Ok(None);
                    }
                    utterance(text, &mut cursor, false)
                }
            };
            if chunk.text.trim().is_empty() {
                continue;
            }
            return Ok(Some(chunk));
        }
    }
}

fn utterance(text: &str, cursor: &mut Cursor, pdf: bool) -> SpeechChunk {
    let start = cursor.byte;
    let end = speech::chunk_end(text, start, 3500);
    cursor.byte = end;
    SpeechChunk {
        text: text[start..end].to_owned(),
        source: SourcePoint {
            section: count(cursor.section),
            row: count(cursor.row),
            byte: count(start),
        },
        pdf_page: if pdf { count(cursor.section + 1) } else { 0 },
        language: speech::guess_language(text).map(str::to_owned),
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct SpeechSheet {
    pub page: u32,
    pub start_line: u32,
    pub end_line: u32,
}

#[uniffi::export]
impl AdaptedBook {
    /// Compose supplies its paragraph line grid. Canonical cuts map to that grid
    /// exactly as in measureRows, preserving split rows and publisher pages.
    pub fn speech_sheets(
        &self,
        point: SourcePoint,
        lines: u32,
    ) -> Result<Vec<SpeechSheet>, CoreError> {
        let section = self
            .sections
            .get(point.section as usize)
            .ok_or("Speech section is unavailable".to_owned())?;
        let pages = section
            .pages
            .iter()
            .filter(|p| p.rows.contains(&(point.row as usize)));
        let mut result = Vec::new();
        let book = self.book.section(point.section as usize)?;
        let geometry = reader_layout::index::HeightIndex::new(section.heights.clone());
        for page in pages {
            let cut = |cut: Option<crate::ParagraphCut>, default| -> u32 {
                cut.filter(|c| c.row == point.row).map_or(default, |c| {
                    (c.line as f64 * lines as f64 / c.lines.max(1) as f64).round() as u32
                })
            };
            result.push(SpeechSheet {
                page: page.number + 1,
                start_line: cut(
                    crate::layout::paragraph_cut(
                        &book,
                        &geometry,
                        page.content.start,
                        self.theme,
                        self.options,
                    ),
                    0,
                ),
                end_line: cut(
                    crate::layout::paragraph_cut(
                        &book,
                        &geometry,
                        page.content.end,
                        self.theme,
                        self.options,
                    ),
                    lines,
                ),
            });
        }
        Ok(result)
    }
}

#[uniffi::export]
pub fn speech_follow_page(line: u32, lines: u32, sheets: Vec<SpeechSheet>) -> Option<u32> {
    speech::follow_page(
        line as usize,
        lines as usize,
        &sheets
            .into_iter()
            .map(|s| (s.page, s.start_line as usize, s.end_line as usize))
            .collect::<Vec<_>>(),
    )
}
