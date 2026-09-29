//! In-book text search over reflow items. Matching is case-insensitive and
//! reports logical UTF-8 byte ranges, so a match can be shown with the same
//! selection highlight the reader already draws.

use reader_document::{Endpoint, Item};
use unicode_segmentation::UnicodeSegmentation;

/// Enough for any realistic reading search; keeps the result list bounded.
pub const MAX_MATCHES: usize = 1000;

#[derive(Clone, Debug, PartialEq)]
pub struct Match {
    /// EPUB section index; `None` for single-section documents.
    pub chapter: Option<usize>,
    /// Row of the item inside its section.
    pub item_index: usize,
    pub item_id: String,
    pub start: usize,
    pub end: usize,
    /// Position of the match inside its item, 0.0 at the top and 1.0 at the end.
    pub fraction: f32,
}

impl Match {
    pub fn endpoints(&self) -> (Endpoint, Endpoint) {
        (
            Endpoint {
                item_id: self.item_id.clone(),
                byte_offset: self.start,
            },
            Endpoint {
                item_id: self.item_id.clone(),
                byte_offset: self.end,
            },
        )
    }
}

/// Lower-cased query characters with whitespace runs collapsed to one space, or
/// `None` when there is nothing to look for.
pub fn needle(query: &str) -> Option<Vec<char>> {
    if query.trim().is_empty() {
        return None;
    }
    let mut needle = Vec::new();
    for c in query.chars().flat_map(char::to_lowercase) {
        if c.is_whitespace() {
            if needle.last() != Some(&' ') {
                needle.push(' ');
            }
        } else {
            needle.push(c);
        }
    }
    Some(needle)
}

/// Appends the non-overlapping matches in `items`, stopping at `MAX_MATCHES`.
pub fn search_items(
    items: &[Item],
    chapter: Option<usize>,
    needle: &[char],
    matches: &mut Vec<Match>,
) {
    for (item_index, item) in items.iter().enumerate() {
        if matches.len() >= MAX_MATCHES {
            return;
        }
        let Some(text) = item.text() else {
            continue;
        };
        for hit in scan(text, needle) {
            if matches.len() >= MAX_MATCHES {
                return;
            }
            let (start, end) = snap_to_graphemes(text, hit.start, hit.end);
            matches.push(Match {
                chapter,
                item_index,
                item_id: item.id().to_owned(),
                start,
                end,
                fraction: start as f32 / text.len().max(1) as f32,
            });
        }
    }
}

/// A match in a PDF page. Indices count glyphs, which is what `TextPoint` addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PdfMatch {
    pub page: u32,
    pub first: usize,
    /// Inclusive, like the focus of a selection.
    pub last: usize,
}

/// Appends the matches in each page's text (one glyph per character), in page order.
pub fn search_pages(pages: &[String], needle: &[char], matches: &mut Vec<PdfMatch>) {
    for (page, text) in pages.iter().enumerate() {
        for hit in scan(text, needle) {
            if matches.len() >= MAX_MATCHES {
                return;
            }
            matches.push(PdfMatch {
                page: page as u32,
                first: hit.first,
                last: hit.last,
            });
        }
    }
}

/// One folded (lower-cased, whitespace-collapsed) character and where it came from.
struct Folded {
    first: usize,
    last: usize,
    start: usize,
    end: usize,
    c: char,
}

/// A match as byte offsets and as character indices (the last one inclusive).
struct Hit {
    start: usize,
    end: usize,
    first: usize,
    last: usize,
}

fn scan(text: &str, needle: &[char]) -> Vec<Hit> {
    let Some(first) = needle.first() else {
        return Vec::new();
    };
    let mut folded: Vec<Folded> = Vec::with_capacity(text.len());
    for (index, (start, source)) in text.char_indices().enumerate() {
        let end = start + source.len_utf8();
        if source.is_whitespace() {
            // A run of blanks and line breaks reads as one space.
            match folded.last_mut() {
                Some(previous) if previous.c == ' ' && previous.end == start => {
                    previous.end = end;
                    previous.last = index;
                }
                _ => folded.push(Folded {
                    first: index,
                    last: index,
                    start,
                    end,
                    c: ' ',
                }),
            }
        } else {
            for c in source.to_lowercase() {
                folded.push(Folded {
                    first: index,
                    last: index,
                    start,
                    end,
                    c,
                });
            }
        }
    }
    let mut found = Vec::new();
    let mut index = 0;
    while index + needle.len() <= folded.len() {
        if folded[index].c == *first
            && folded[index..index + needle.len()]
                .iter()
                .map(|folded| &folded.c)
                .eq(needle)
        {
            let last = &folded[index + needle.len() - 1];
            found.push(Hit {
                start: folded[index].start,
                end: last.end,
                first: folded[index].first,
                last: last.last,
            });
            index += needle.len();
        } else {
            index += 1;
        }
    }
    found
}

/// Selection endpoints must sit on grapheme boundaries to be drawn.
fn snap_to_graphemes(text: &str, start: usize, end: usize) -> (usize, usize) {
    let (mut snapped_start, mut snapped_end) = (start, end);
    for (offset, grapheme) in text.grapheme_indices(true) {
        let grapheme_end = offset + grapheme.len();
        if offset <= start && start < grapheme_end {
            snapped_start = offset;
        }
        if offset < end && end <= grapheme_end {
            snapped_end = grapheme_end;
            break;
        }
    }
    (snapped_start, snapped_end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reader_document::BaseDirection;

    fn paragraph(id: &str, text: &str) -> Item {
        Item::Paragraph {
            id: id.into(),
            text: text.into(),
            base_direction: BaseDirection::Ltr,
            style_runs: Vec::new(),
        }
    }

    fn find(items: &[Item], query: &str) -> Vec<Match> {
        let mut matches = Vec::new();
        search_items(items, Some(2), &needle(query).unwrap(), &mut matches);
        matches
    }

    #[test]
    fn blank_queries_search_for_nothing() {
        assert!(needle("").is_none());
        assert!(needle("  \t").is_none());
        assert!(needle(" a ").is_some());
    }

    #[test]
    fn matches_ignore_case_and_report_source_byte_ranges() {
        let items = [
            paragraph("a", "Alpha beta ALPHA"),
            Item::Image {
                id: "img".into(),
                asset_path: "x.png".into(),
            },
            paragraph("b", "no hit"),
            paragraph("c", "alphaalpha"),
        ];
        let matches = find(&items, "Alpha");
        let ranges: Vec<_> = matches
            .iter()
            .map(|m| (m.item_id.as_str(), m.start, m.end))
            .collect();
        assert_eq!(
            ranges,
            [("a", 0, 5), ("a", 11, 16), ("c", 0, 5), ("c", 5, 10)]
        );
        assert!(matches.iter().all(|m| m.chapter == Some(2)));
        assert_eq!(matches[3].item_index, 3);
        assert_eq!(matches[0].fraction, 0.0);
        assert!(matches[1].fraction > 0.5);
    }

    #[test]
    fn multibyte_text_uses_source_offsets_on_grapheme_boundaries() {
        // "İ" lower-cases to two characters, and "e\u{301}" is one grapheme.
        let text = "İstanbul e\u{301}cole";
        let items = [paragraph("a", text)];
        let matches = find(&items, "stanbul");
        assert_eq!(&text[matches[0].start..matches[0].end], "stanbul");
        let matches = find(&items, "e");
        let widened = &text[matches[0].start..matches[0].end];
        assert_eq!(widened, "e\u{301}");
        assert!(text.is_char_boundary(matches[0].start) && text.is_char_boundary(matches[0].end));
    }

    #[test]
    fn line_breaks_and_runs_of_blanks_match_a_single_space() {
        let text = "the old\r\n  lighthouse";
        let items = [paragraph("a", text)];
        let matches = find(&items, "old   lighthouse");
        assert_eq!(matches.len(), 1);
        assert_eq!(
            &text[matches[0].start..matches[0].end],
            "old\r\n  lighthouse"
        );
    }

    #[test]
    fn pdf_matches_use_glyph_indices_per_page() {
        let pages = vec![
            "no hit".to_owned(),
            "See the Keeper.\r\nkeeper again".to_owned(),
        ];
        let mut matches = Vec::new();
        search_pages(&pages, &needle("keeper").unwrap(), &mut matches);
        assert_eq!(
            matches,
            [
                PdfMatch {
                    page: 1,
                    first: 8,
                    last: 13
                },
                PdfMatch {
                    page: 1,
                    first: 17,
                    last: 22
                },
            ]
        );
        let mut across = Vec::new();
        search_pages(&pages, &needle("keeper. keeper").unwrap(), &mut across);
        assert_eq!(
            across,
            [PdfMatch {
                page: 1,
                first: 8,
                last: 22
            }]
        );
    }

    #[test]
    fn results_are_bounded() {
        let items: Vec<_> = (0..MAX_MATCHES + 50)
            .map(|i| paragraph(&i.to_string(), "a"))
            .collect();
        assert_eq!(find(&items, "a").len(), MAX_MATCHES);
    }

    #[test]
    fn endpoints_cover_the_match() {
        let items = [paragraph("p", "one two")];
        let (from, to) = find(&items, "two")[0].endpoints();
        assert_eq!((from.item_id.as_str(), from.byte_offset), ("p", 4));
        assert_eq!(to.byte_offset, 7);
    }
}
