//! Shared annotation resolution, word snapping, merge and paint rules.
//! These are behavior-preserving moves from the desktop; no UI dependencies.

use crate::annotations::{Annotations, Highlight, HighlightColor, PdfPoint, Place};
use crate::{Endpoint, Item};
use std::{collections::HashMap, ops::Range};

/// A normalized selection range using workload order and logical UTF-8 bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectionBounds {
    /// Ordered workload index of the first endpoint item.
    pub start_item: usize,
    /// UTF-8 byte offset in the first endpoint item.
    pub start_byte: usize,
    /// Ordered workload index of the last endpoint item.
    pub end_item: usize,
    /// UTF-8 byte offset in the last endpoint item.
    pub end_byte: usize,
}

impl SelectionBounds {
    /// Selected logical byte range for one item, or `None` for unselected
    /// text, collapsed ranges, images, and empty partial endpoints.
    #[must_use]
    pub fn range_for_item(self, item_index: usize, text: &str) -> Option<Range<usize>> {
        if self.start_item == self.end_item && self.start_byte == self.end_byte {
            return None;
        }
        if !(self.start_item..=self.end_item).contains(&item_index) {
            return None;
        }

        let start = if item_index == self.start_item {
            self.start_byte
        } else {
            0
        };
        let end = if item_index == self.end_item {
            self.end_byte
        } else {
            text.len()
        };
        if start >= end
            || end > text.len()
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            return None;
        }
        Some(start..end)
    }
}

/// A saved highlight resolved against the items currently loaded.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub id: u64,
    pub color: HighlightColor,
    pub note: bool,
    pub bounds: SelectionBounds,
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
    // Recover one location without retaining every repetition of a short quote.
    // An unhinted second candidate already proves the passage is ambiguous.
    let mut count = 0usize;
    let mut first_match = None;
    let mut hinted_match = None;
    let mut consider = |bounds: SelectionBounds| {
        count = (count + 1).min(2);
        first_match.get_or_insert(bounds);
        if hint.is_some_and(|hint| bounds.start_item >= hint) {
            hinted_match.get_or_insert(bounds);
        }
        if hinted_match.is_some() {
            Some(hinted_match)
        } else if hint.is_none() && count > 1 {
            Some(None)
        } else {
            None
        }
    };
    for (row, text) in &texts {
        for (start, _) in text.match_indices(quote) {
            if let Some(result) = consider(SelectionBounds {
                start_item: *row,
                start_byte: start,
                end_item: *row,
                end_byte: start + quote.len(),
            }) {
                return result;
            }
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
            if inner_matches
                && end_text.starts_with(last)
                && let Some(result) = consider(SelectionBounds {
                    start_item: *row,
                    start_byte: text.len() - first.len(),
                    end_item: end_row,
                    end_byte: last.len(),
                })
            {
                return result;
            }
        }
    }
    if count == 1 {
        first_match
    } else {
        hinted_match
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
    let (from, to, ids, noted) = connected_ranges(
        marks.iter().filter(|mark| mark.color == color).map(|mark| {
            (
                (mark.bounds.start_item, mark.bounds.start_byte),
                (mark.bounds.end_item, mark.bounds.end_byte),
                mark.id,
                mark.note,
            )
        }),
        (selected.start_item, selected.start_byte),
        (selected.end_item, selected.end_byte),
        |end, start| end >= start,
    );
    ReflowMerge {
        bounds: SelectionBounds {
            start_item: from.0,
            start_byte: from.1,
            end_item: to.0,
            end_byte: to.1,
        },
        ids,
        noted,
    }
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
    let (from, to, ids, noted) = connected_ranges(
        data.highlights
            .iter()
            .filter(|highlight| highlight.color == color)
            .filter_map(|highlight| {
                let Place::Pdf { from, to } = highlight.place else {
                    return None;
                };
                Some((from, to, highlight.id, highlight.note.is_some()))
            }),
        from,
        to,
        |end, start| {
            end >= start
                || (end.page == start.page && end.index.checked_add(1) == Some(start.index))
        },
    );
    PdfMerge {
        from,
        to,
        ids,
        noted,
    }
}

struct MergeRange<P> {
    from: P,
    to: P,
    id: Option<u64>,
    note: bool,
    order: usize,
    direct: bool,
}

/// Include the selection in the sorted intervals, then find its connected component.
/// The first directly touched ID stays first, preserving the retained annotation ID.
pub fn connected_ranges<P: Ord + Copy>(
    ranges: impl Iterator<Item = (P, P, u64, bool)>,
    from: P,
    to: P,
    touches: impl Fn(P, P) -> bool,
) -> (P, P, Vec<u64>, usize) {
    let mut ranges: Vec<_> = ranges
        .enumerate()
        .map(|(order, (start, end, id, note))| MergeRange {
            from: start,
            to: end,
            id: Some(id),
            note,
            order,
            direct: touches(to, start) && touches(end, from),
        })
        .collect();
    ranges.push(MergeRange {
        from,
        to,
        id: None,
        note: false,
        order: usize::MAX,
        direct: false,
    });
    ranges.sort_by_key(|range| (range.from, range.to));
    let (mut start, mut end) = (ranges[0].from, ranges[0].to);
    let mut selected = false;
    let mut members = Vec::new();
    for range in ranges {
        if !touches(end, range.from) {
            if selected {
                break;
            }
            members.clear();
            start = range.from;
            end = range.to;
        }
        end = end.max(range.to);
        if range.id.is_none() {
            selected = true;
        } else {
            members.push(range);
        }
    }
    members.sort_by_key(|range| (!range.direct, range.order));
    let noted = members.iter().filter(|range| range.note).count();
    (
        start,
        end,
        members.into_iter().filter_map(|range| range.id).collect(),
        noted,
    )
}

struct PaintRange {
    range: std::ops::Range<usize>,
    note: bool,
    order: usize,
}

struct ColorRanges<T> {
    color: T,
    ranges: Vec<PaintRange>,
}

/// Combines equal-color spans before painting, so their shared area is drawn
/// once. Different colors retain their separate translucent layers.
pub fn coalesce_colored_ranges<T: Copy + PartialEq>(
    spans: Vec<(std::ops::Range<usize>, T, bool)>,
) -> Vec<(std::ops::Range<usize>, T, bool)> {
    let mut colors: Vec<ColorRanges<T>> = Vec::new();
    for (order, (range, color, note)) in spans.into_iter().enumerate() {
        if range.start >= range.end {
            continue;
        }
        let index = colors
            .iter()
            .position(|group| group.color == color)
            .unwrap_or_else(|| {
                colors.push(ColorRanges {
                    color,
                    ranges: Vec::new(),
                });
                colors.len() - 1
            });
        colors[index].ranges.push(PaintRange { range, note, order });
    }
    let mut result = Vec::new();
    for mut group in colors {
        group
            .ranges
            .sort_by_key(|span| (span.range.start, span.range.end));
        let mut merged: Vec<PaintRange> = Vec::new();
        for span in group.ranges {
            if let Some(previous) = merged.last_mut()
                && span.range.start <= previous.range.end
            {
                previous.range.end = previous.range.end.max(span.range.end);
                previous.note |= span.note;
                previous.order = previous.order.min(span.order);
            } else {
                merged.push(span);
            }
        }
        result.extend(
            merged
                .into_iter()
                .map(|span| (span.order, span.range, group.color, span.note)),
        );
    }
    // Different colors keep their original painting order, including disjoint
    // components of one color that lie on either side of a newer color.
    result.sort_by_key(|span| span.0);
    result
        .into_iter()
        .map(|(_, range, color, note)| (range, color, note))
        .collect()
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
