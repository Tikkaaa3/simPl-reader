//! Android selection and annotation adapters over the desktop's source schema.
use crate::{CoreError, OpenBook, PdfDocument, ReaderLocation, complete};
use reader_document::{Item, annotation_logic as logic, annotations as store};
use std::sync::Mutex;
use unicode_segmentation::UnicodeSegmentation;

static WRITES: Mutex<()> = Mutex::new(());
const MAX_SELECTION_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AnnotationColor {
    Yellow,
    Green,
    Blue,
    Pink,
}
impl From<AnnotationColor> for store::HighlightColor {
    fn from(c: AnnotationColor) -> Self {
        match c {
            AnnotationColor::Yellow => Self::Yellow,
            AnnotationColor::Green => Self::Green,
            AnnotationColor::Blue => Self::Blue,
            AnnotationColor::Pink => Self::Pink,
        }
    }
}
impl From<store::HighlightColor> for AnnotationColor {
    fn from(c: store::HighlightColor) -> Self {
        match c {
            store::HighlightColor::Yellow => Self::Yellow,
            store::HighlightColor::Green => Self::Green,
            store::HighlightColor::Blue => Self::Blue,
            store::HighlightColor::Pink => Self::Pink,
        }
    }
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct AnnotationEntry {
    pub id: u64,
    pub page: String,
    pub page_number: u32,
    pub quote: String,
    pub color: Option<AnnotationColor>,
    pub note: Option<String>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct AnnotationCollection {
    pub bookmarks: Vec<AnnotationEntry>,
    pub highlights: Vec<AnnotationEntry>,
}
fn collection(data: &store::Annotations) -> AnnotationCollection {
    AnnotationCollection {
        bookmarks: data
            .bookmarks
            .iter()
            .map(|b| AnnotationEntry {
                id: b.id,
                page: b.page.clone(),
                quote: b.excerpt.clone(),
                color: None,
                note: None,
                page_number: match &b.place {
                    store::BookmarkPlace::Reflow { page_number, .. } => page_number + 1,
                    store::BookmarkPlace::Pdf { page, .. } => page + 1,
                },
            })
            .collect(),
        highlights: data
            .highlights
            .iter()
            .map(|h| AnnotationEntry {
                id: h.id,
                page: h.page.clone(),
                quote: h.quote.clone(),
                color: Some(h.color.into()),
                note: h.note.clone(),
                page_number: match h.place {
                    store::Place::Pdf { from, .. } => from.page + 1,
                    _ => 0,
                },
            })
            .collect(),
    }
}
fn mutate<T>(
    fingerprint: &str,
    edit: impl FnOnce(&mut store::Annotations) -> Result<T, String>,
) -> Result<T, CoreError> {
    let _guard = WRITES
        .lock()
        .map_err(|_| "Annotation storage is unavailable".to_owned())?;
    let mut data = store::load(fingerprint)?;
    let result = edit(&mut data)?;
    store::save(&data)?;
    Ok(result)
}
#[uniffi::export]
pub fn load_annotations(fingerprint: String) -> Result<AnnotationCollection, CoreError> {
    Ok(collection(&store::load(&fingerprint)?))
}
#[uniffi::export]
pub fn remove_annotation(
    fingerprint: String,
    id: u64,
    bookmark: bool,
) -> Result<AnnotationCollection, CoreError> {
    mutate(&fingerprint, |data| {
        let removed = if bookmark {
            data.remove_bookmark(id)
        } else {
            data.remove_highlight(id)
        };
        if !removed {
            return Err("This annotation no longer exists".into());
        }
        Ok(collection(data))
    })
}
#[uniffi::export]
pub fn edit_annotation(
    fingerprint: String,
    id: u64,
    color: AnnotationColor,
    note: String,
) -> Result<AnnotationCollection, CoreError> {
    mutate(&fingerprint, |data| {
        if !data.set_color(id, color.into()) || !data.set_note(id, &note)? {
            return Err("This highlight no longer exists".into());
        }
        Ok(collection(data))
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, uniffi::Record)]
/// Zero-based section/row and source UTF-8 byte boundary. Selection ends are exclusive.
pub struct SourcePoint {
    pub section: u32,
    pub row: u32,
    pub byte: u32,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ReflowSelection {
    pub from: SourcePoint,
    pub to: SourcePoint,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ReaderMark {
    pub id: u64,
    pub section: u32,
    pub row: u32,
    pub start_byte: u32,
    pub end_byte: u32,
    pub color: AnnotationColor,
    pub note: bool,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct AnnotationTarget {
    pub location: ReaderLocation,
    pub selection: Option<ReflowSelection>,
}

fn preview(text: &str) -> String {
    preview_parts([text])
}
fn preview_parts<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    let mut result = String::new();
    let mut count = 0;
    for word in parts.into_iter().flat_map(str::split_whitespace) {
        if !result.is_empty() {
            result.push(' ');
            count += 1;
        }
        for character in word.chars() {
            if count == 200 {
                return result;
            }
            result.push(character);
            count += 1;
        }
        if count == 200 {
            break;
        }
    }
    result
}
fn selection_size(items: &[Item], bounds: logic::SelectionBounds, limit: usize) -> usize {
    let mut bytes: usize = 0;
    for row in bounds.start_item..=bounds.end_item {
        if let Some(text) = items.get(row).and_then(Item::text)
            && let Some(range) = bounds.range_for_item(row, text)
        {
            bytes = bytes
                .saturating_add(range.len())
                .saturating_add(usize::from(bytes > 0));
            if bytes > limit {
                break;
            }
        }
    }
    bytes
}
fn boundary(text: &str, byte: usize) -> bool {
    byte == text.len() || text.grapheme_indices(true).any(|(i, _)| i == byte)
}

impl OpenBook {
    fn source_location(
        &self,
        section: usize,
        row: usize,
        byte: usize,
    ) -> Result<ReaderLocation, CoreError> {
        let book = self.section(section)?;
        let y = reader_layout::measure::source_y(&book, row, byte)?;
        let height = self.atlas.sections[section].heights[row];
        self.locate(section, row, (y / height).clamp(0.0, 0.999999))
    }
    fn validate_point(&self, point: SourcePoint) -> Result<(), CoreError> {
        if point.section as usize >= self.atlas.sections.len() {
            return Err("Selection is outside the reading order".to_owned().into());
        }
        let book = self.section(point.section as usize)?;
        let text = book
            .items
            .get(point.row as usize)
            .and_then(Item::text)
            .ok_or("Select a text passage".to_owned())?;
        if point.byte as usize > text.len() || !boundary(text, point.byte as usize) {
            return Err("Selection splits a Unicode grapheme".to_owned().into());
        }
        Ok(())
    }
    fn selected_parts(
        &self,
        selected: &ReflowSelection,
        snap: bool,
    ) -> Result<Vec<(usize, logic::SelectionBounds)>, CoreError> {
        let (from, to) = if selected.from <= selected.to {
            (selected.from, selected.to)
        } else {
            (selected.to, selected.from)
        };
        self.validate_point(from)?;
        self.validate_point(to)?;
        if from == to {
            return Err("Select a text passage".to_owned().into());
        }
        let mut parts = Vec::new();
        let mut bytes: usize = 0;
        for section in from.section..=to.section {
            let book = self.section(section as usize)?;
            let first = book.items.iter().position(|i| i.text().is_some());
            let last = book.items.iter().rposition(|i| i.text().is_some());
            let (Some(first), Some(last)) = (first, last) else {
                continue;
            };
            let start_row = if section == from.section {
                from.row as usize
            } else {
                first
            };
            let end_row = if section == to.section {
                to.row as usize
            } else {
                last
            };
            let bounds = logic::SelectionBounds {
                start_item: start_row,
                start_byte: if section == from.section {
                    from.byte as usize
                } else {
                    0
                },
                end_item: end_row,
                end_byte: if section == to.section {
                    to.byte as usize
                } else {
                    book.items[end_row].text().unwrap().len()
                },
            };
            let bounds = if snap {
                logic::snap_to_words(&book.items, bounds)
            } else {
                bounds
            };
            let size = selection_size(&book.items, bounds, MAX_SELECTION_BYTES);
            bytes = bytes
                .saturating_add(size)
                .saturating_add(usize::from(bytes > 0 && size > 0));
            if bytes > MAX_SELECTION_BYTES {
                return Err("Select at most 1 MiB of text at once".to_owned().into());
            }
            if size > 0 {
                parts.push((section as usize, bounds));
            }
        }
        Ok(parts)
    }
    fn place(
        &self,
        section: usize,
        bounds: logic::SelectionBounds,
    ) -> Result<store::Place, CoreError> {
        let book = self.section(section)?;
        let point = |row: usize, byte: usize| store::ReflowPoint {
            item_id: book.items[row].id().into(),
            byte,
        };
        Ok(store::Place::Reflow {
            chapter: book
                .epub
                .as_ref()
                .and_then(|e| e.document.section(section).map(|s| s.href.clone())),
            from: point(bounds.start_item, bounds.start_byte),
            to: point(bounds.end_item, bounds.end_byte),
        })
    }
    fn selection_of(section: usize, bounds: logic::SelectionBounds) -> ReflowSelection {
        ReflowSelection {
            from: SourcePoint {
                section: section as u32,
                row: bounds.start_item as u32,
                byte: bounds.start_byte as u32,
            },
            to: SourcePoint {
                section: section as u32,
                row: bounds.end_item as u32,
                byte: bounds.end_byte as u32,
            },
        }
    }
    fn section_for(&self, chapter: Option<&str>) -> Result<usize, CoreError> {
        match (&self.book.epub, chapter) {
            (Some(epub), Some(href)) => epub
                .document
                .section_index(href)
                .ok_or_else(|| "Annotation chapter is unavailable".to_owned().into()),
            (None, None) => Ok(0),
            _ => Err("Annotation is for another document mode".to_owned().into()),
        }
    }
}
#[uniffi::export]
impl OpenBook {
    pub fn selection_word(&self, point: SourcePoint) -> Result<ReflowSelection, CoreError> {
        self.validate_point(point)?;
        let book = self.section(point.section as usize)?;
        let text = book.items[point.row as usize].text().unwrap();
        let start = point.byte as usize;
        if let Some(range) = reader_core::word_translation::word_range(text, start) {
            return Ok(ReflowSelection {
                from: SourcePoint {
                    byte: range.start as u32,
                    ..point
                },
                to: SourcePoint {
                    byte: range.end as u32,
                    ..point
                },
            });
        }
        let end = text[start..]
            .graphemes(true)
            .next()
            .map_or(start, |g| start + g.len());
        if start == end {
            return Err("Select a text passage".to_owned().into());
        }
        let bounds = logic::snap_to_words(
            &book.items,
            logic::SelectionBounds {
                start_item: point.row as usize,
                start_byte: start,
                end_item: point.row as usize,
                end_byte: end,
            },
        );
        Ok(Self::selection_of(point.section as usize, bounds))
    }
    pub fn selection_text(&self, selection: ReflowSelection) -> Result<String, CoreError> {
        let mut quote = String::new();
        for (section, bounds) in self.selected_parts(&selection, false)? {
            let book = self.section(section)?;
            let part = logic::text_of(&book.items, bounds);
            if quote.len() + part.len() + 1 > MAX_SELECTION_BYTES {
                return Err("Select at most 1 MiB of text at once".to_owned().into());
            }
            if !quote.is_empty() {
                quote.push('\n');
            }
            quote.push_str(&part);
        }
        Ok(quote)
    }
    pub fn reader_marks(&self, section: u32) -> Result<Vec<ReaderMark>, CoreError> {
        let book = self.section(section as usize)?;
        let data = store::load(&self.book.fingerprint)?;
        let chapter = book.epub.as_ref().and_then(|e| {
            e.document
                .section(section as usize)
                .map(|s| s.href.as_str())
        });
        Ok(logic::marks(&book.items, chapter, &data)
            .iter()
            .flat_map(|mark| {
                (mark.bounds.start_item..=mark.bounds.end_item).filter_map(|row| {
                    let range = mark
                        .bounds
                        .range_for_item(row, book.items.get(row)?.text()?)?;
                    Some(ReaderMark {
                        id: mark.id,
                        section,
                        row: row as u32,
                        start_byte: range.start as u32,
                        end_byte: range.end as u32,
                        color: mark.color.into(),
                        note: mark.note,
                    })
                })
            })
            .collect())
    }
    pub fn highlight_selection(
        &self,
        selection: ReflowSelection,
        color: AnnotationColor,
        note: Option<String>,
    ) -> Result<Vec<u64>, CoreError> {
        let parts = self.selected_parts(&selection, true)?;
        mutate(&self.book.fingerprint, |data| {
            let mut ids = Vec::new();
            for (section, bounds) in parts {
                let book = self.section(section).map_err(|e| e.to_string())?;
                let chapter = book
                    .epub
                    .as_ref()
                    .and_then(|e| e.document.section(section).map(|s| s.href.as_str()));
                let marks = logic::marks(&book.items, chapter, data);
                let plan = logic::merge_reflow(&marks, color.into(), bounds);
                if plan.noted > 1
                    && note.is_none()
                    && plan.bounds == bounds
                    && let Some(id) = plan.ids.first()
                {
                    ids.push(*id);
                    continue;
                }
                let selected = if plan.noted > 1 { bounds } else { plan.bounds };
                let place = self.place(section, selected).map_err(|e| e.to_string())?;
                if selection_size(&book.items, selected, store::MAX_QUOTE_BYTES)
                    > store::MAX_QUOTE_BYTES
                {
                    return Err("Select at most about 2048 characters to highlight at once".into());
                }
                let quote = logic::text_of(&book.items, selected);
                if quote.trim().is_empty() {
                    continue;
                }
                let label = self
                    .source_location(section, selected.start_item, selected.start_byte)
                    .map_err(|e| e.to_string())?
                    .page;
                let page = self.atlas.sections[section]
                    .pages
                    .iter()
                    .find(|p| p.number + 1 == label)
                    .map_or(label.to_string(), |p| p.label.clone());
                let id = if plan.noted > 1 {
                    data.add_highlight(place, color.into(), page, quote)?
                } else {
                    data.merge_highlights(&plan.ids, place, color.into(), page, quote)?
                };
                if let Some(note) = &note {
                    data.set_note(id, note)?;
                }
                ids.push(id);
            }
            if ids.is_empty() {
                return Err("Select a text passage".into());
            }
            Ok(ids)
        })
    }
    pub fn toggle_bookmark(&self, page: u32) -> Result<AnnotationCollection, CoreError> {
        let fragments = self.page(page)?;
        let fragment = fragments.first().ok_or("Page is unavailable".to_owned())?;
        let row = fragment.rows.first().ok_or("Page is empty".to_owned())?;
        let within = fragment
            .layout
            .start_cut
            .as_ref()
            .filter(|c| c.row == row.index)
            .map_or(0.0, |c| c.line as f32 / c.lines.max(1) as f32);
        let book = self.section(fragment.section as usize)?;
        let chapter = book.epub.as_ref().and_then(|e| {
            e.document
                .section(fragment.section as usize)
                .map(|s| s.href.clone())
        });
        mutate(&self.book.fingerprint, |data| {
            if let Some(id) = data.bookmarks.iter().find(|b| matches!(b.place, store::BookmarkPlace::Reflow { page_number, .. } if page_number + 1 == page)).map(|b| b.id) { data.remove_bookmark(id); }
            else { data.add_bookmark(store::BookmarkPlace::Reflow { chapter, item_id: row.id.clone(), within, page_number: page - 1 },
                fragment.layout.label.clone(), preview_parts(fragment.rows.iter().filter_map(|r| r.text.as_deref())))?; }
            Ok(collection(data))
        })
    }
    pub fn annotation_target(
        &self,
        id: u64,
        bookmark: bool,
    ) -> Result<AnnotationTarget, CoreError> {
        let data = store::load(&self.book.fingerprint)?;
        if bookmark {
            let b = data
                .bookmark(id)
                .ok_or("This bookmark no longer exists".to_owned())?;
            let store::BookmarkPlace::Reflow {
                chapter,
                item_id,
                within,
                page_number,
            } = &b.place
            else {
                return Err("Bookmark is for another document mode".to_owned().into());
            };
            let section = self.section_for(chapter.as_deref())?;
            let book = self.section(section)?;
            let row = book
                .items
                .iter()
                .position(|i| i.id() == item_id)
                .ok_or("Bookmark passage is unavailable".to_owned())?;
            let mut location = self.locate(section, row, *within)?;
            if (*page_number as usize) < self.atlas.total {
                location.page = *page_number + 1;
            }
            Ok(AnnotationTarget {
                location,
                selection: None,
            })
        } else {
            let h = data
                .highlight(id)
                .ok_or("This highlight no longer exists".to_owned())?;
            let store::Place::Reflow { chapter, .. } = &h.place else {
                return Err("Highlight is for another document mode".to_owned().into());
            };
            let section = self.section_for(chapter.as_deref())?;
            let book = self.section(section)?;
            let bounds = logic::marks(&book.items, chapter.as_deref(), &data)
                .into_iter()
                .find(|m| m.id == id)
                .ok_or("Highlighted passage is unavailable".to_owned())?
                .bounds;
            Ok(AnnotationTarget {
                location: self.source_location(section, bounds.start_item, bounds.start_byte)?,
                selection: Some(Self::selection_of(section, bounds)),
            })
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, uniffi::Record)]
/// One-based source PDF page and zero-based inclusive glyph ordinal.
pub struct PdfSelectionPoint {
    pub page: u32,
    pub index: u32,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct PdfSelection {
    pub from: PdfSelectionPoint,
    pub to: PdfSelectionPoint,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct PdfMark {
    pub id: u64,
    pub from: PdfSelectionPoint,
    pub to: PdfSelectionPoint,
    pub color: AnnotationColor,
    pub note: bool,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct PdfAnnotationTarget {
    pub page: u32,
    pub within: f32,
    pub selection: Option<PdfSelection>,
}
impl PdfDocument {
    fn pdf_selection(&self, selection: &PdfSelection) -> Result<reader_pdf::Selection, CoreError> {
        let (from, to) = if selection.from <= selection.to {
            (selection.from, selection.to)
        } else {
            (selection.to, selection.from)
        };
        Ok(reader_pdf::Selection {
            anchor: reader_pdf::TextPoint {
                page: self.page_index(from.page)?,
                index: from.index as usize,
            },
            focus: reader_pdf::TextPoint {
                page: self.page_index(to.page)?,
                index: to.index as usize,
            },
        })
    }
}
#[uniffi::export]
impl PdfDocument {
    pub fn selection_text(&self, selection: PdfSelection) -> Result<String, CoreError> {
        Ok(complete(
            self.document.session.copy(self.pdf_selection(&selection)?),
        )?)
    }
    pub fn pdf_marks(&self) -> Result<Vec<PdfMark>, CoreError> {
        Ok(store::load(&self.document.fingerprint)?
            .highlights
            .iter()
            .filter_map(|h| {
                let store::Place::Pdf { from, to } = h.place else {
                    return None;
                };
                Some(PdfMark {
                    id: h.id,
                    from: PdfSelectionPoint {
                        page: from.page + 1,
                        index: u32::try_from(from.index).ok()?,
                    },
                    to: PdfSelectionPoint {
                        page: to.page + 1,
                        index: u32::try_from(to.index).ok()?,
                    },
                    color: h.color.into(),
                    note: h.note.is_some(),
                })
            })
            .collect())
    }
    pub fn highlight_selection(
        &self,
        selection: PdfSelection,
        color: AnnotationColor,
        note: Option<String>,
    ) -> Result<u64, CoreError> {
        let selected = self.pdf_selection(&selection)?;
        mutate(&self.document.fingerprint, |data| {
            let from = store::PdfPoint {
                page: selected.anchor.page,
                index: selected.anchor.index,
            };
            let to = store::PdfPoint {
                page: selected.focus.page,
                index: selected.focus.index,
            };
            let plan = logic::merge_pdf(data, color.into(), from, to);
            let (from, to, ids) = if plan.noted > 1 {
                (from, to, Vec::new())
            } else {
                (plan.from, plan.to, plan.ids)
            };
            let quote = complete(self.document.session.copy(reader_pdf::Selection {
                anchor: reader_pdf::TextPoint {
                    page: from.page,
                    index: from.index,
                },
                focus: reader_pdf::TextPoint {
                    page: to.page,
                    index: to.index,
                },
            }))?;
            if quote.trim().is_empty() {
                return Err("Select a text passage".into());
            }
            let id = data.merge_highlights(
                &ids,
                store::Place::Pdf { from, to },
                color.into(),
                (from.page + 1).to_string(),
                quote,
            )?;
            if let Some(note) = note {
                data.set_note(id, &note)?;
            }
            Ok(id)
        })
    }
    pub fn toggle_bookmark(&self, page: u32) -> Result<AnnotationCollection, CoreError> {
        let index = self.page_index(page)?;
        let excerpt = preview(&complete(self.document.session.page_text(index))?);
        mutate(&self.document.fingerprint, |data| {
            if let Some(id) = data
                .bookmarks
                .iter()
                .find(
                    |b| matches!(b.place, store::BookmarkPlace::Pdf { page, .. } if page == index),
                )
                .map(|b| b.id)
            {
                data.remove_bookmark(id);
            } else {
                data.add_bookmark(
                    store::BookmarkPlace::Pdf {
                        page: index,
                        within: 0.0,
                    },
                    page.to_string(),
                    excerpt,
                )?;
            }
            Ok(collection(data))
        })
    }
    pub fn annotation_target(
        &self,
        id: u64,
        bookmark: bool,
    ) -> Result<PdfAnnotationTarget, CoreError> {
        let data = store::load(&self.document.fingerprint)?;
        if bookmark {
            let b = data
                .bookmark(id)
                .ok_or("This bookmark no longer exists".to_owned())?;
            let store::BookmarkPlace::Pdf { page, within } = b.place else {
                return Err("Bookmark is for another document mode".to_owned().into());
            };
            self.page_index(page + 1)?;
            Ok(PdfAnnotationTarget {
                page: page + 1,
                within,
                selection: None,
            })
        } else {
            let h = data
                .highlight(id)
                .ok_or("This highlight no longer exists".to_owned())?;
            let store::Place::Pdf { from, to } = h.place else {
                return Err("Highlight is for another document mode".to_owned().into());
            };
            self.page_index(from.page + 1)?;
            self.page_index(to.page + 1)?;
            let layer = self.text(from.page + 1, 600)?;
            let first = layer
                .glyphs
                .get(from.index)
                .ok_or("Highlighted glyph is unavailable".to_owned())?;
            let within = first.bounds.as_ref().map_or(0.0, |r| r.top);
            let end_count = if from.page == to.page {
                layer.glyphs.len()
            } else {
                self.text(to.page + 1, 600)?.glyphs.len()
            };
            if to.index >= end_count {
                return Err("Highlighted glyph is unavailable".to_owned().into());
            }
            Ok(PdfAnnotationTarget {
                page: from.page + 1,
                within,
                selection: Some(PdfSelection {
                    from: PdfSelectionPoint {
                        page: from.page + 1,
                        index: from.index as u32,
                    },
                    to: PdfSelectionPoint {
                        page: to.page + 1,
                        index: to.index as u32,
                    },
                }),
            })
        }
    }
}
