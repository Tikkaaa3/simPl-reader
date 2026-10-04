//! Bounded, cancellable search over the same source ranges as the desktop.
use crate::{
    CoreError, LayoutStatus, OpenBook, PdfDocument, PdfSelection, PdfSelectionPoint,
    ReaderLocation, ReflowSelection, SourcePoint, layout::Job,
};
use reader_document::find;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, uniffi::Record)]
pub struct SearchHit {
    pub excerpt: String,
    pub page: u32,
    pub within: f32,
    pub location: Option<ReaderLocation>,
    pub reflow: Option<ReflowSelection>,
    pub pdf: Option<PdfSelection>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// At least 1,000 matches; further results were not scanned.
    pub limited: bool,
}
#[derive(uniffi::Object)]
pub struct FindTask {
    job: Arc<Job<SearchResults>>,
}
#[uniffi::export]
impl FindTask {
    pub fn status(&self) -> LayoutStatus {
        self.job.status()
    }
    pub fn result(&self) -> Result<Option<SearchResults>, CoreError> {
        self.job.result()
    }
    pub fn cancel(&self) {
        self.job.cancel.store(true, Ordering::Relaxed);
    }
}
impl Drop for FindTask {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn start(
    query: String,
    work: impl FnOnce(&[char], &AtomicBool) -> Result<Vec<SearchHit>, CoreError> + Send + 'static,
) -> Result<Arc<FindTask>, CoreError> {
    if query.len() > 256 {
        return Err("Search is limited to 256 UTF-8 bytes".to_owned().into());
    }
    let job = Job::new();
    job.run(move |cancel| {
        let hits = match find::needle(&query) {
            Some(needle) => work(&needle, cancel)?,
            None => Vec::new(),
        };
        Ok(Some(SearchResults {
            limited: hits.len() == find::MAX_MATCHES,
            hits,
        }))
    })?;
    Ok(Arc::new(FindTask { job }))
}
fn excerpt(text: &str, byte: usize) -> String {
    let mut start = byte.saturating_sub(64).min(text.len());
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    let mut chars = text[start..].chars();
    let mut result: String = chars
        .by_ref()
        .take(160)
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    if start > 0 {
        result.insert(0, '…');
    }
    if chars.next().is_some() {
        result.push('…');
    }
    result
}
#[uniffi::export]
impl OpenBook {
    /// Chapter navigation follows the EPUB spine, including chapters absent from the TOC.
    pub fn adjacent_chapter(&self, section: u32, delta: i32) -> Result<ReaderLocation, CoreError> {
        let index = (i64::from(section) + i64::from(delta))
            .clamp(0, self.atlas.sections.len().saturating_sub(1) as i64)
            as usize;
        self.locate(index, 0, 0.0)
    }

    pub fn find(self: Arc<Self>, query: String) -> Result<Arc<FindTask>, CoreError> {
        start(query, move |needle, cancel| {
            let mut matches = Vec::new();
            let mut hits = Vec::new();
            for section in 0..self.atlas.sections.len() {
                if cancel.load(Ordering::Relaxed) || matches.len() == find::MAX_MATCHES {
                    break;
                }
                let book = self.section(section)?;
                let before = matches.len();
                find::search_items_cancellable(
                    &book.items,
                    Some(section),
                    needle,
                    &mut matches,
                    cancel,
                );
                for group in matches[before..].chunk_by(|a, b| a.item_index == b.item_index) {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let row = group[0].item_index;
                    let ys = reader_layout::measure::source_ys(
                        &book,
                        row,
                        &group.iter().map(|hit| hit.start).collect::<Vec<_>>(),
                    )?;
                    for (hit, y) in group.iter().zip(ys) {
                        let height = self.atlas.sections[section].heights[row];
                        let location =
                            self.locate(section, row, (y / height).clamp(0.0, 0.999999))?;
                        let from = SourcePoint {
                            section: section as u32,
                            row: hit.item_index as u32,
                            byte: hit.start as u32,
                        };
                        hits.push(SearchHit {
                            excerpt: excerpt(
                                book.items[hit.item_index].text().unwrap_or_default(),
                                hit.start,
                            ),
                            page: location.page,
                            within: location.within,
                            location: Some(location),
                            reflow: Some(ReflowSelection {
                                from,
                                to: SourcePoint {
                                    byte: hit.end as u32,
                                    ..from
                                },
                            }),
                            pdf: None,
                        });
                    }
                }
            }
            Ok(hits)
        })
    }
}
#[uniffi::export]
impl PdfDocument {
    pub fn find(self: Arc<Self>, query: String) -> Result<Arc<FindTask>, CoreError> {
        if !self.document.can_copy {
            return Err("This PDF does not allow text search".to_owned().into());
        }
        start(query, move |needle, cancel| {
            let mut hits = Vec::new();
            for page in 1..=self.document.pages.len() as u32 {
                if cancel.load(Ordering::Relaxed) || hits.len() == find::MAX_MATCHES {
                    break;
                }
                let layer = self.text(page, 720)?;
                let mut matches = Vec::new();
                find::search_pages_cancellable(
                    std::slice::from_ref(&layer.text),
                    needle,
                    &mut matches,
                    cancel,
                );
                for hit in matches.into_iter().take(find::MAX_MATCHES - hits.len()) {
                    let glyph = &layer.glyphs[hit.first];
                    hits.push(SearchHit {
                        excerpt: excerpt(&layer.text, glyph.start as usize),
                        page,
                        within: glyph.bounds.as_ref().map_or(0.0, |r| r.top),
                        location: None,
                        reflow: None,
                        pdf: Some(PdfSelection {
                            from: PdfSelectionPoint {
                                page,
                                index: hit.first as u32,
                            },
                            to: PdfSelectionPoint {
                                page,
                                index: hit.last as u32,
                            },
                        }),
                    });
                }
            }
            Ok(hits)
        })
    }
}
