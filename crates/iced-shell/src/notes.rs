//! Bookmarks, highlights and notes as the reader uses them: which saved
//! highlights apply to the text that is loaded, how a passage is found again
//! when its item IDs changed, and the small pop-up menus that create them.
//!
//! Storage lives in `reader_document::annotations`; this module holds no
//! files and no widgets, so its rules are testable without a window.

use std::collections::HashMap;

use iced::widget::text_editor;
use iced::{Color, Point, Rectangle, Size};
use iced_shell::selection::SelectionBounds;
use reader_document::annotations::{
    Annotations, Bookmark, BookmarkPlace, Highlight, HighlightColor, PdfPoint, Place,
};
use reader_document::{Endpoint, Item};

/// A saved highlight resolved against the items currently loaded.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub id: u64,
    pub color: HighlightColor,
    pub note: bool,
    pub bounds: SelectionBounds,
}

/// A highlight's paint color. Translucent, so the text on top stays readable
/// on every reading theme.
pub fn tint(color: HighlightColor) -> Color {
    match color {
        HighlightColor::Yellow => Color::from_rgba8(0xf5, 0xc4, 0x0f, 0.42),
        HighlightColor::Green => Color::from_rgba8(0x3f, 0xc0, 0x6b, 0.38),
        HighlightColor::Blue => Color::from_rgba8(0x4a, 0x9e, 0xf5, 0.38),
        HighlightColor::Pink => Color::from_rgba8(0xf0, 0x5c, 0xa8, 0.36),
    }
}

/// The solid version of a highlight color, for swatches and note underlines.
pub fn solid(color: HighlightColor) -> Color {
    Color {
        a: 1.0,
        ..tint(color)
    }
}

/// Whether `c` belongs to a word that spaces delimit. Scripts written without
/// spaces (CJK, Thai and neighbors) are left as selected.
fn word_char(c: char) -> bool {
    let unspaced = matches!(c,
        '\u{0e00}'..='\u{0eff}'
        | '\u{1000}'..='\u{109f}'
        | '\u{1780}'..='\u{17ff}'
        | '\u{3000}'..='\u{9fff}'
        | '\u{ac00}'..='\u{d7af}'
        | '\u{f900}'..='\u{faff}'
        | '\u{ff00}'..='\u{ffef}'
        | '\u{20000}'..='\u{3134f}');
    !unspaced && (c.is_alphanumeric() || matches!(c, '\'' | '’'))
}

/// Widens a selection that starts or ends inside a word to the whole word, so a
/// highlight never cuts a word in half. Endpoints between words are kept.
pub fn snap_to_words(items: &[Item], bounds: SelectionBounds) -> SelectionBounds {
    let mut snapped = bounds;
    if let Some(text) = items.get(bounds.start_item).and_then(Item::text)
        && text.is_char_boundary(bounds.start_byte)
        && text[bounds.start_byte..]
            .chars()
            .next()
            .is_some_and(word_char)
    {
        snapped.start_byte = text[..bounds.start_byte]
            .char_indices()
            .rev()
            .take_while(|(_, c)| word_char(*c))
            .last()
            .map_or(bounds.start_byte, |(index, _)| index);
    }
    if let Some(text) = items.get(bounds.end_item).and_then(Item::text)
        && text.is_char_boundary(bounds.end_byte)
        && text[..bounds.end_byte]
            .chars()
            .next_back()
            .is_some_and(word_char)
    {
        snapped.end_byte = bounds.end_byte
            + text[bounds.end_byte..]
                .char_indices()
                .take_while(|(_, c)| word_char(*c))
                .last()
                .map_or(0, |(index, c)| index + c.len_utf8());
    }
    snapped
}

/// The selected text in reading order: what copying the selection would give.
pub fn text_of(items: &[Item], bounds: SelectionBounds) -> String {
    let mut parts = Vec::new();
    for index in bounds.start_item..=bounds.end_item.min(items.len().saturating_sub(1)) {
        let Some(text) = items.get(index).and_then(Item::text) else {
            continue;
        };
        if let Some(range) = bounds.range_for_item(index, text) {
            parts.push(&text[range]);
        }
    }
    parts.join("\n")
}

/// Every saved highlight of this chapter, found again in `items`.
///
/// A highlight is trusted only while the text under its anchor is still the
/// text that was highlighted. Otherwise the saved passage is searched for, so
/// a parser change that renumbers items does not lose highlights.
pub fn marks(items: &[Item], chapter: Option<&str>, notes: &Annotations) -> Vec<Mark> {
    let index: HashMap<&str, usize> = items
        .iter()
        .enumerate()
        .map(|(row, item)| (item.id(), row))
        .collect();
    notes
        .highlights
        .iter()
        .filter_map(|highlight| {
            let bounds = resolve(items, &index, chapter, highlight)?;
            Some(Mark {
                id: highlight.id,
                color: highlight.color,
                note: highlight.note.is_some(),
                bounds,
            })
        })
        .collect()
}

fn resolve(
    items: &[Item],
    index: &HashMap<&str, usize>,
    chapter: Option<&str>,
    highlight: &Highlight,
) -> Option<SelectionBounds> {
    let Place::Reflow {
        chapter: saved,
        from,
        to,
    } = &highlight.place
    else {
        return None;
    };
    if saved.as_deref() != chapter {
        return None;
    }
    let start = index.get(from.item_id.as_str()).copied();
    if let (Some(start_item), Some(end_item)) = (start, index.get(to.item_id.as_str()).copied()) {
        let bounds = SelectionBounds {
            start_item,
            start_byte: from.byte,
            end_item,
            end_byte: to.byte,
        };
        if (start_item, from.byte) < (end_item, to.byte)
            && text_of(items, bounds) == highlight.quote
        {
            return Some(bounds);
        }
    }
    locate(items, &highlight.quote, start)
}

/// Finds `quote` in the text items. With several candidates the one at or
/// after `hint` (the item it used to be in) wins; an ambiguous passage with
/// no hint is left unresolved rather than highlighted in the wrong place.
pub fn locate(items: &[Item], quote: &str, hint: Option<usize>) -> Option<SelectionBounds> {
    if quote.is_empty() {
        return None;
    }
    let lines: Vec<&str> = quote.split('\n').collect();
    let texts: Vec<(usize, &str)> = items
        .iter()
        .enumerate()
        .filter_map(|(row, item)| Some((row, item.text()?)))
        .collect();
    let mut found = Vec::new();
    for (row, text) in &texts {
        for (start, _) in text.match_indices(quote) {
            found.push(SelectionBounds {
                start_item: *row,
                start_byte: start,
                end_item: *row,
                end_byte: start + quote.len(),
            });
        }
    }
    if lines.len() > 1 {
        let (first, rest) = lines.split_first()?;
        let (last, middle) = rest.split_last()?;
        for (position, (row, text)) in texts.iter().enumerate() {
            if !text.ends_with(first) {
                continue;
            }
            let following = texts.get(position + 1..position + 1 + rest.len());
            let Some(following) = following else { continue };
            let inner_matches = following
                .iter()
                .zip(middle)
                .all(|((_, text), line)| text == line);
            let (end_row, end_text) = following[following.len() - 1];
            if inner_matches && end_text.starts_with(last) {
                found.push(SelectionBounds {
                    start_item: *row,
                    start_byte: text.len() - first.len(),
                    end_item: end_row,
                    end_byte: last.len(),
                });
            }
        }
    }
    match found.len() {
        0 => None,
        1 => found.pop(),
        _ => {
            let hint = hint?;
            found.into_iter().find(|bounds| bounds.start_item >= hint)
        }
    }
}

/// The topmost highlight whose text contains the given position, if any.
pub fn mark_at(marks: &[Mark], items: &[Item], endpoint: &Endpoint) -> Option<u64> {
    let row = items
        .iter()
        .position(|item| item.id() == endpoint.item_id)?;
    let point = (row, endpoint.byte_offset);
    marks
        .iter()
        .rev()
        .find(|mark| {
            (mark.bounds.start_item, mark.bounds.start_byte) <= point
                && point < (mark.bounds.end_item, mark.bounds.end_byte)
        })
        .map(|mark| mark.id)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReflowMerge {
    pub bounds: SelectionBounds,
    pub ids: Vec<u64>,
    pub noted: usize,
}

/// The connected same-color passage, including highlights reached through
/// another overlap. Other colors remain independent annotations.
pub fn merge_reflow(
    marks: &[Mark],
    color: HighlightColor,
    selected: SelectionBounds,
) -> ReflowMerge {
    let mut plan = ReflowMerge {
        bounds: selected,
        ids: Vec::new(),
        noted: 0,
    };
    loop {
        let mut grew = false;
        for mark in marks.iter().filter(|mark| mark.color == color) {
            if plan.ids.contains(&mark.id) {
                continue;
            }
            let a = plan.bounds;
            let b = mark.bounds;
            if (a.start_item, a.start_byte) <= (b.end_item, b.end_byte)
                && (b.start_item, b.start_byte) <= (a.end_item, a.end_byte)
            {
                plan.ids.push(mark.id);
                plan.noted += usize::from(mark.note);
                let start = (a.start_item, a.start_byte).min((b.start_item, b.start_byte));
                let end = (a.end_item, a.end_byte).max((b.end_item, b.end_byte));
                plan.bounds = SelectionBounds {
                    start_item: start.0,
                    start_byte: start.1,
                    end_item: end.0,
                    end_byte: end.1,
                };
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    plan
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfMerge {
    pub from: PdfPoint,
    pub to: PdfPoint,
    pub ids: Vec<u64>,
    pub noted: usize,
}

pub fn merge_pdf(
    data: &Annotations,
    color: HighlightColor,
    from: PdfPoint,
    to: PdfPoint,
) -> PdfMerge {
    let mut plan = PdfMerge {
        from,
        to,
        ids: Vec::new(),
        noted: 0,
    };
    loop {
        let mut grew = false;
        for highlight in data
            .highlights
            .iter()
            .filter(|highlight| highlight.color == color)
        {
            let Place::Pdf { from, to } = highlight.place else {
                continue;
            };
            if plan.ids.contains(&highlight.id) {
                continue;
            }
            let touches = from <= plan.to && plan.from <= to
                || (plan.to.page == from.page && plan.to.index.checked_add(1) == Some(from.index))
                || (to.page == plan.from.page && to.index.checked_add(1) == Some(plan.from.index));
            if touches {
                plan.ids.push(highlight.id);
                plan.noted += usize::from(highlight.note.is_some());
                plan.from = plan.from.min(from);
                plan.to = plan.to.max(to);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    plan
}

/// Combines equal-color spans before painting, so their shared area is drawn
/// once. Different colors retain their separate translucent layers.
pub fn coalesce_colored_ranges<T: Copy + PartialEq>(
    spans: Vec<(std::ops::Range<usize>, T, bool)>,
) -> Vec<(std::ops::Range<usize>, T, bool)> {
    let mut result = Vec::new();
    for (range, color, note) in spans {
        if range.start >= range.end {
            continue;
        }
        result.push((range, color, note));
    }
    let mut index = 0;
    while index < result.len() {
        let mut other = index + 1;
        while other < result.len() {
            if result[index].1 == result[other].1
                && result[index].0.start <= result[other].0.end
                && result[other].0.start <= result[index].0.end
            {
                let merged = result.remove(other);
                result[index].0.start = result[index].0.start.min(merged.0.start);
                result[index].0.end = result[index].0.end.max(merged.0.end);
                result[index].2 |= merged.2;
                other = index + 1;
            } else {
                other += 1;
            }
        }
        index += 1;
    }
    result
}

/// Highlighted byte ranges of one item, oldest first so newer colors paint on top.
pub fn ranges_for(
    marks: &[Mark],
    item_index: usize,
    text: &str,
) -> Vec<(std::ops::Range<usize>, HighlightColor, bool)> {
    coalesce_colored_ranges(
        marks
            .iter()
            .filter_map(|mark| {
                mark.bounds
                    .range_for_item(item_index, text)
                    .map(|range| (range, mark.color, mark.note))
            })
            .collect(),
    )
}

/// A short single-line preview for lists.
pub fn preview(text: &str, limit: usize) -> String {
    let mut words = text.split_whitespace();
    let mut result = String::new();
    for word in words.by_ref() {
        let extra = usize::from(!result.is_empty());
        if result.chars().count() + word.chars().count() + extra > limit {
            if result.is_empty() {
                result.extend(word.chars().take(limit));
            }
            result.push('…');
            return result;
        }
        if extra == 1 {
            result.push(' ');
        }
        result.push_str(word);
    }
    result
}

/// Pdf highlight ranges that touch one page, as inclusive glyph indices.
/// `glyphs` is the number of glyphs on that page.
pub fn pdf_range_on_page(
    from: PdfPoint,
    to: PdfPoint,
    page: u32,
    glyphs: usize,
) -> Option<std::ops::Range<usize>> {
    if page < from.page || page > to.page || glyphs == 0 {
        return None;
    }
    let first = if page == from.page { from.index } else { 0 };
    let end = if page == to.page {
        to.index.saturating_add(1)
    } else {
        glyphs
    };
    let (first, end) = (first.min(glyphs), end.min(glyphs));
    (first < end).then_some(first..end)
}

// ---------------------------------------------------------------- pop-ups

/// What a pop-up acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The text that is selected now.
    Selection,
    /// A saved highlight.
    Highlight(u64),
    /// The page in view, with nothing selected.
    Page,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupItem {
    Color(HighlightColor),
    Note,
    Copy,
    RemoveHighlight,
    ToggleBookmark,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Popup {
    /// Where the pointer was, in window coordinates.
    pub at: Point,
    pub target: Target,
    /// Opened with the right button: also offers the page actions.
    pub menu: bool,
}

pub const POPUP_WIDTH: f32 = 232.0;
pub const POPUP_PADDING: f32 = 6.0;
pub const POPUP_SPACING: f32 = 2.0;
pub const POPUP_ROW: f32 = 32.0;
/// A hairline with breathing room between groups of the menu.
pub const POPUP_SEPARATOR: f32 = 9.0;

impl Popup {
    pub fn entries(&self) -> Vec<PopupItem> {
        let mut entries = Vec::new();
        if !matches!(self.target, Target::Page) {
            entries.extend(HighlightColor::ALL.map(PopupItem::Color));
            entries.push(PopupItem::Note);
            entries.push(PopupItem::Copy);
        }
        if self.menu || matches!(self.target, Target::Page) {
            entries.push(PopupItem::ToggleBookmark);
        }
        if matches!(self.target, Target::Highlight(_)) {
            entries.push(PopupItem::RemoveHighlight);
        }
        entries
    }

    /// Separators in drawing order: after the colors, and before removing a highlight.
    pub fn separated(entries: &[PopupItem]) -> (bool, bool) {
        let actions = entries
            .iter()
            .filter(|entry| !matches!(entry, PopupItem::Color(_)))
            .count();
        let colors = entries.len() > actions;
        let removal = entries.contains(&PopupItem::RemoveHighlight);
        (colors && actions > 0, removal && actions > 1)
    }

    pub fn size(&self) -> Size {
        let entries = self.entries();
        let colors = entries
            .iter()
            .any(|entry| matches!(entry, PopupItem::Color(_)));
        let rows = entries
            .iter()
            .filter(|entry| !matches!(entry, PopupItem::Color(_)))
            .count()
            + usize::from(colors);
        let (after_colors, before_removal) = Self::separated(&entries);
        let separators = usize::from(after_colors) + usize::from(before_removal);
        let children = rows + separators;
        Size::new(
            POPUP_WIDTH,
            POPUP_PADDING * 2.0
                + rows as f32 * POPUP_ROW
                + separators as f32 * POPUP_SEPARATOR
                + children.saturating_sub(1) as f32 * POPUP_SPACING,
        )
    }

    /// Top-left corner: just below and right of the pointer, moved to stay
    /// inside the window (above the pointer when there is no room below).
    pub fn origin(&self, window: Size) -> Point {
        let size = self.size();
        let margin = 8.0;
        let x = self
            .at
            .x
            .min(window.width - size.width - margin)
            .max(margin);
        let below = self.at.y + 12.0;
        let y = if below + size.height + margin <= window.height {
            below
        } else {
            (self.at.y - size.height - 8.0).max(margin)
        };
        Point::new(x, y)
    }

    pub fn bounds(&self, window: Size) -> Rectangle {
        Rectangle::new(self.origin(window), self.size())
    }
}

// ----------------------------------------------------------------- state

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListTab {
    #[default]
    Bookmarks,
    Highlights,
}

/// A row of the bookmarks and notes list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Bookmark(u64),
    Highlight(u64),
}

/// What a note-related control does.
#[derive(Clone, Debug)]
pub enum Action {
    Popup(PopupItem),
    ToggleBookmark,
    ToggleList,
    ToggleNote(u64),
    EditNote(u64),
    Editing(text_editor::Action),
    SaveNote,
    CancelNote,
    DeleteNote,
    Tab(ListTab),
    Go(Entry),
    Remove(Entry),
}

/// Keyboard-focusable note controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Popup(PopupItem),
    ToggleList,
    Tab(ListTab),
    Go(Entry),
    Edit(u64),
    ToggleNote(u64),
    Remove(Entry),
}

impl Focus {
    pub fn action(self) -> Action {
        match self {
            Self::Popup(item) => Action::Popup(item),
            Self::ToggleList => Action::ToggleList,
            Self::Tab(tab) => Action::Tab(tab),
            Self::Go(entry) => Action::Go(entry),
            Self::Edit(id) => Action::EditNote(id),
            Self::ToggleNote(id) => Action::ToggleNote(id),
            Self::Remove(entry) => Action::Remove(entry),
        }
    }
}

/// The note being written.
#[derive(Debug)]
pub struct Editor {
    pub id: u64,
    pub content: text_editor::Content,
    /// The highlight already had a note, so it can be deleted.
    pub existing: bool,
}

/// What the reader knows about the open book's annotations.
#[derive(Debug, Default)]
pub struct Notes {
    /// Loaded records; `None` before the load finishes or when it failed.
    pub data: Option<Annotations>,
    /// The fingerprint whose records are loaded or loading.
    pub book: Option<String>,
    /// Why the records cannot be used (an unreadable file is never overwritten).
    pub blocked: Option<String>,
    pub marks: Vec<Mark>,
    pub popup: Option<Popup>,
    pub editor: Option<Editor>,
    pub list: Option<ListTab>,
    /// The note whose full text is open in the sidebar.
    pub expanded_note: Option<u64>,
    /// The highlight under the last left press, to open it when released without a drag.
    pub pressed: Option<u64>,
    /// Highlight to reveal after its EPUB chapter finishes loading.
    pub pending_reveal: Option<u64>,
    /// The color used by the next highlight made without choosing one.
    pub color: HighlightColor,
    /// A write is running. Kept across books so a late reply still matches.
    pub saving: bool,
    /// Snapshots waiting to be written, oldest first, at most one per book.
    queue: Vec<Annotations>,
    in_flight: Option<Annotations>,
    pub failed_save: bool,
}

impl Notes {
    /// Forgets everything about the previous book. Writes already started or
    /// queued for it are kept, so its last change is never dropped.
    pub fn reset(&mut self, fingerprint: Option<&str>) {
        *self = Self {
            book: fingerprint.map(str::to_owned),
            color: self.color,
            saving: self.saving,
            queue: std::mem::take(&mut self.queue),
            in_flight: self.in_flight.take(),
            failed_save: self.failed_save,
            ..Self::default()
        };
    }

    /// Queues the current records to be written.
    pub fn changed(&mut self) {
        let Some(data) = self.data.clone() else {
            return;
        };
        self.failed_save = false;
        match self
            .queue
            .iter_mut()
            .find(|queued| queued.fingerprint == data.fingerprint)
        {
            Some(queued) => *queued = data,
            None => self.queue.push(data),
        }
    }

    pub fn close_transients(&mut self) {
        self.popup = None;
        self.pressed = None;
    }

    /// The newest unsaved snapshot of a book, used if it is reopened before
    /// the background write finishes.
    pub fn pending_for(&self, fingerprint: &str) -> Option<Annotations> {
        self.queue
            .iter()
            .find(|data| data.fingerprint.eq_ignore_ascii_case(fingerprint))
            .or_else(|| {
                self.in_flight
                    .as_ref()
                    .filter(|data| data.fingerprint.eq_ignore_ascii_case(fingerprint))
            })
            .cloned()
    }

    pub fn is_bookmarked_reflow(&self, page_number: u32) -> Option<&Bookmark> {
        self.data.as_ref()?.bookmarks.iter().find(|bookmark| {
            matches!(&bookmark.place, BookmarkPlace::Reflow { page_number: n, .. } if *n == page_number)
        })
    }

    pub fn is_bookmarked_pdf(&self, page: u32) -> Option<&Bookmark> {
        self.data.as_ref()?.bookmarks.iter().find(
            |bookmark| matches!(&bookmark.place, BookmarkPlace::Pdf { page: p, .. } if *p == page),
        )
    }

    /// The next snapshot to write, unless a write is already running.
    pub fn take_pending_save(&mut self) -> Option<Annotations> {
        if self.saving || self.failed_save || self.queue.is_empty() {
            return None;
        }
        self.saving = true;
        let data = self.queue.remove(0);
        self.in_flight = Some(data.clone());
        Some(data)
    }

    pub fn finish_save(&mut self, success: bool) {
        self.saving = false;
        if let Some(data) = self.in_flight.take()
            && !success
        {
            if !self
                .queue
                .iter()
                .any(|queued| queued.fingerprint == data.fingerprint)
            {
                self.queue.insert(0, data);
            }
            self.failed_save = true;
        }
    }

    pub fn discard_failed_save(&mut self) {
        self.queue.clear();
        self.failed_save = false;
    }

    /// Nothing is waiting to be written, so the window may close.
    pub fn settled(&self) -> bool {
        !self.saving && self.queue.is_empty() && self.in_flight.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reader_document::BaseDirection;
    use reader_document::annotations::ReflowPoint;

    #[test]
    fn highlights_snap_to_whole_words() {
        let items = [
            paragraph("a", "A quieter page"),
            paragraph("b", "Don’t split it"),
            paragraph("c", "静かな頁です"),
        ];
        let bounds = |item, start, end| SelectionBounds {
            start_item: item,
            start_byte: start,
            end_item: item,
            end_byte: end,
        };
        let snapped = snap_to_words(&items, bounds(0, 0, 12));
        assert_eq!(text_of(&items, snapped), "A quieter page");
        let snapped = snap_to_words(&items, bounds(0, 4, 7));
        assert_eq!(text_of(&items, snapped), "quieter");
        // Endpoints already between words are kept.
        let snapped = snap_to_words(&items, bounds(0, 1, 10));
        assert_eq!(snapped, bounds(0, 1, 10));
        let snapped = snap_to_words(&items, bounds(1, 1, 3));
        assert_eq!(text_of(&items, snapped), "Don’t");
        let unspaced = bounds(2, 3, 9);
        assert_eq!(snap_to_words(&items, unspaced), unspaced);
    }

    fn paragraph(id: &str, text: &str) -> Item {
        Item::Paragraph {
            id: id.into(),
            text: text.into(),
            base_direction: BaseDirection::Ltr,
            style_runs: Vec::new(),
        }
    }

    fn fingerprint() -> String {
        "ab".repeat(32)
    }

    fn place(chapter: Option<&str>, from: (&str, usize), to: (&str, usize)) -> Place {
        Place::Reflow {
            chapter: chapter.map(str::to_owned),
            from: ReflowPoint {
                item_id: from.0.into(),
                byte: from.1,
            },
            to: ReflowPoint {
                item_id: to.0.into(),
                byte: to.1,
            },
        }
    }

    fn book() -> Vec<Item> {
        vec![
            paragraph("item-000001", "The old lighthouse stood alone."),
            Item::Image {
                id: "item-000002".into(),
                asset_path: "map.png".into(),
            },
            paragraph("item-000003", "Its keeper counted the ships."),
            paragraph("item-000004", "Nobody came."),
        ]
    }

    fn saved(notes: &mut Annotations, place: Place, quote: &str) -> u64 {
        notes
            .add_highlight(place, HighlightColor::Yellow, "1".into(), quote.into())
            .unwrap()
    }

    #[test]
    fn a_highlight_resolves_where_it_was_made() {
        let items = book();
        let mut notes = Annotations::new(&fingerprint()).unwrap();
        let id = saved(
            &mut notes,
            place(None, ("item-000001", 4), ("item-000001", 18)),
            "old lighthouse",
        );
        let found = marks(&items, None, &notes);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, id);
        assert_eq!(
            found[0].bounds,
            SelectionBounds {
                start_item: 0,
                start_byte: 4,
                end_item: 0,
                end_byte: 18
            }
        );
        assert_eq!(ranges_for(&found, 0, "x".repeat(40).as_str()).len(), 1);
        assert!(ranges_for(&found, 2, "Its keeper counted the ships.").is_empty());
    }

    #[test]
    fn a_highlight_can_span_items_and_skips_images() {
        let items = book();
        let mut notes = Annotations::new(&fingerprint()).unwrap();
        let quote = "lighthouse stood alone.\nIts keeper";
        saved(
            &mut notes,
            place(None, ("item-000001", 8), ("item-000003", 10)),
            quote,
        );
        let found = marks(&items, None, &notes);
        assert_eq!(found.len(), 1);
        assert_eq!(text_of(&items, found[0].bounds), quote);
        let bounds = found[0].bounds;
        assert!(bounds.range_for_item(1, "").is_none());
        assert_eq!(
            bounds.range_for_item(2, items[2].text().unwrap()),
            Some(0..10)
        );
    }

    #[test]
    fn renumbered_items_are_found_again_by_their_text() {
        // The book was parsed again and every item ID changed.
        let items: Vec<Item> = book()
            .into_iter()
            .map(|item| match item {
                Item::Paragraph {
                    id,
                    text,
                    base_direction,
                    style_runs,
                } => Item::Paragraph {
                    id: id.replace("item-", "block-"),
                    text,
                    base_direction,
                    style_runs,
                },
                other => other,
            })
            .collect();
        let mut notes = Annotations::new(&fingerprint()).unwrap();
        saved(
            &mut notes,
            place(None, ("item-000003", 4), ("item-000003", 10)),
            "keeper",
        );
        saved(
            &mut notes,
            place(None, ("item-000001", 8), ("item-000003", 10)),
            "lighthouse stood alone.\nIts keeper",
        );
        let found = marks(&items, None, &notes);
        assert_eq!(found.len(), 2);
        assert_eq!(text_of(&items, found[0].bounds), "keeper");
        assert_eq!(
            text_of(&items, found[1].bounds),
            "lighthouse stood alone.\nIts keeper"
        );
    }

    #[test]
    fn changed_text_and_other_chapters_do_not_resolve() {
        let items = book();
        let mut notes = Annotations::new(&fingerprint()).unwrap();
        saved(
            &mut notes,
            place(None, ("item-000004", 0), ("item-000004", 6)),
            "Gone!!",
        );
        saved(
            &mut notes,
            place(Some("ch2.xhtml"), ("item-000001", 4), ("item-000001", 18)),
            "old lighthouse",
        );
        assert!(marks(&items, None, &notes).is_empty());
        assert_eq!(marks(&items, Some("ch2.xhtml"), &notes).len(), 1);
    }

    #[test]
    fn an_ambiguous_passage_prefers_its_old_item_and_never_guesses() {
        let items = vec![
            paragraph("a", "the sea"),
            paragraph("b", "and the sea again"),
        ];
        assert_eq!(locate(&items, "the sea", Some(1)).unwrap().start_item, 1);
        assert!(locate(&items, "the sea", None).is_none());
        assert!(locate(&items, "", Some(0)).is_none());
        // Unique text needs no hint.
        assert_eq!(locate(&items, "again", None).unwrap().start_byte, 12);
    }

    #[test]
    fn same_color_spans_merge_and_other_colors_remain_independent() {
        let saved = SelectionBounds {
            start_item: 0,
            start_byte: 4,
            end_item: 1,
            end_byte: 8,
        };
        let marks = vec![Mark {
            id: 1,
            color: HighlightColor::Yellow,
            note: false,
            bounds: saved,
        }];
        let candidate = |start_item, start_byte, end_item, end_byte| SelectionBounds {
            start_item,
            start_byte,
            end_item,
            end_byte,
        };
        let selected = candidate(0, 5, 1, 12);
        let merged = merge_reflow(&marks, HighlightColor::Yellow, selected);
        assert_eq!(merged.bounds, candidate(0, 4, 1, 12));
        assert_eq!(merged.ids, [1]);
        assert!(
            merge_reflow(&marks, HighlightColor::Blue, selected)
                .ids
                .is_empty()
        );
        let ink = coalesce_colored_ranges(vec![
            (0..5, HighlightColor::Yellow, false),
            (3..8, HighlightColor::Yellow, true),
            (4..6, HighlightColor::Blue, false),
        ]);
        assert_eq!(
            ink,
            [
                (0..8, HighlightColor::Yellow, true),
                (4..6, HighlightColor::Blue, false)
            ]
        );

        let mut data = Annotations::new(&fingerprint()).unwrap();
        let first = PdfPoint { page: 2, index: 5 };
        let last = PdfPoint { page: 2, index: 9 };
        data.add_highlight(
            Place::Pdf {
                from: first,
                to: last,
            },
            HighlightColor::Yellow,
            "3".into(),
            "passage".into(),
        )
        .unwrap();
        let selected_from = PdfPoint { page: 2, index: 8 };
        let selected_to = PdfPoint { page: 2, index: 12 };
        let merged = merge_pdf(&data, HighlightColor::Yellow, selected_from, selected_to);
        assert_eq!(
            (merged.from, merged.to, merged.ids.len()),
            (first, selected_to, 1)
        );
        assert!(
            merge_pdf(&data, HighlightColor::Pink, selected_from, selected_to)
                .ids
                .is_empty()
        );
    }

    #[test]
    fn clicking_finds_the_topmost_highlight() {
        let items = book();
        let mut notes = Annotations::new(&fingerprint()).unwrap();
        let wide = saved(
            &mut notes,
            place(None, ("item-000001", 0), ("item-000001", 18)),
            "The old lighthouse",
        );
        let narrow = saved(
            &mut notes,
            place(None, ("item-000001", 4), ("item-000001", 7)),
            "old",
        );
        let found = marks(&items, None, &notes);
        let at = |offset| Endpoint {
            item_id: "item-000001".into(),
            byte_offset: offset,
        };
        assert_eq!(mark_at(&found, &items, &at(5)), Some(narrow));
        assert_eq!(mark_at(&found, &items, &at(12)), Some(wide));
        assert_eq!(mark_at(&found, &items, &at(25)), None);
        assert_eq!(
            mark_at(
                &found,
                &items,
                &Endpoint {
                    item_id: "missing".into(),
                    byte_offset: 0
                }
            ),
            None
        );
    }

    #[test]
    fn previews_cut_at_words_and_pdf_ranges_clip_to_pages() {
        assert_eq!(preview("  a  few\nwords ", 40), "a few words");
        assert_eq!(preview("one two three", 8), "one two…");
        assert_eq!(preview("extraordinarily", 5), "extra…");
        let from = PdfPoint { page: 2, index: 5 };
        let to = PdfPoint { page: 4, index: 1 };
        assert_eq!(pdf_range_on_page(from, to, 2, 10), Some(5..10));
        assert_eq!(pdf_range_on_page(from, to, 3, 10), Some(0..10));
        assert_eq!(pdf_range_on_page(from, to, 4, 10), Some(0..2));
        assert_eq!(pdf_range_on_page(from, to, 5, 10), None);
        assert_eq!(pdf_range_on_page(from, to, 2, 3), None);
    }

    #[test]
    fn popups_offer_what_fits_their_target_and_stay_inside_the_window() {
        let window = Size::new(800.0, 600.0);
        let selection = Popup {
            at: Point::new(100.0, 100.0),
            target: Target::Selection,
            menu: false,
        };
        assert_eq!(
            selection.entries(),
            [
                PopupItem::Color(HighlightColor::Yellow),
                PopupItem::Color(HighlightColor::Green),
                PopupItem::Color(HighlightColor::Blue),
                PopupItem::Color(HighlightColor::Pink),
                PopupItem::Note,
                PopupItem::Copy,
            ]
        );
        let menu = Popup {
            menu: true,
            target: Target::Highlight(3),
            ..selection.clone()
        };
        assert!(menu.entries().contains(&PopupItem::RemoveHighlight));
        assert!(menu.entries().contains(&PopupItem::ToggleBookmark));
        assert_eq!(menu.entries().last(), Some(&PopupItem::RemoveHighlight));
        let page = Popup {
            target: Target::Page,
            ..selection.clone()
        };
        assert_eq!(page.entries(), [PopupItem::ToggleBookmark]);
        assert!(menu.size().height > selection.size().height);

        for at in [
            Point::new(5.0, 5.0),
            Point::new(795.0, 595.0),
            Point::new(400.0, 590.0),
        ] {
            let popup = Popup { at, ..menu.clone() };
            let bounds = popup.bounds(window);
            assert!(bounds.x >= 0.0 && bounds.y >= 0.0, "{at:?}");
            assert!(bounds.x + bounds.width <= window.width, "{at:?}");
            assert!(bounds.y + bounds.height <= window.height, "{at:?}");
        }
        // Near the bottom the menu opens above the pointer.
        let low = Popup {
            at: Point::new(400.0, 590.0),
            ..menu
        };
        assert!(low.bounds(window).y < 590.0);
    }

    #[test]
    fn saves_are_serialized_per_book_and_survive_switching_books() {
        let mut notes = Notes::default();
        assert!(notes.settled());
        notes.data = Some(Annotations::new(&fingerprint()).unwrap());
        notes.changed();
        assert!(!notes.settled());
        let first = notes.take_pending_save().unwrap();
        assert_eq!(first.fingerprint, fingerprint());
        // A change made while saving waits, and later changes replace it.
        notes
            .data
            .as_mut()
            .unwrap()
            .add_bookmark(
                BookmarkPlace::Pdf {
                    page: 2,
                    within: 0.0,
                },
                "3".into(),
                String::new(),
            )
            .unwrap();
        notes.changed();
        notes.changed();
        assert!(notes.take_pending_save().is_none());
        assert!(!notes.settled());
        // Switching books keeps the write that is still owed to the old one.
        notes.reset(Some(&"cd".repeat(32)));
        assert!(notes.data.is_none() && notes.saving);
        assert!(!notes.settled());
        assert_eq!(
            notes.pending_for(&fingerprint()).unwrap().bookmarks.len(),
            1
        );
        notes.finish_save(true);
        assert_eq!(
            notes.take_pending_save().unwrap().fingerprint,
            fingerprint()
        );
        notes.finish_save(true);
        assert!(notes.take_pending_save().is_none());
        assert!(notes.settled());
        // Records that never loaded are never written.
        notes.changed();
        assert!(notes.settled());
    }

    #[test]
    fn failed_save_keeps_the_latest_snapshot_for_an_explicit_retry() {
        let mut notes = Notes {
            data: Some(Annotations::new(&fingerprint()).unwrap()),
            ..Notes::default()
        };
        notes.changed();
        let first = notes.take_pending_save().unwrap();
        notes.changed();
        notes.finish_save(false);
        assert!(notes.failed_save);
        assert!(!notes.settled());
        assert!(notes.take_pending_save().is_none());
        notes.failed_save = false;
        assert_eq!(
            notes.take_pending_save().unwrap().fingerprint,
            first.fingerprint
        );
        notes.finish_save(true);
        assert!(notes.settled());
    }

    #[test]
    fn a_line_break_inside_one_item_remains_findable_after_renumbering() {
        let items = vec![paragraph("new-id", "Before\nAfter the break")];
        let found = locate(&items, "Before\nAfter", None).unwrap();
        assert_eq!(found.start_item, 0);
        assert_eq!(found.end_item, 0);
        assert_eq!(text_of(&items, found), "Before\nAfter");
    }
}
