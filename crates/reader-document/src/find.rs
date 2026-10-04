//! In-book text search over reflow items. Matching is case-insensitive and
//! reports logical UTF-8 byte ranges, so a match can be shown with the same
//! selection highlight the reader already draws.

use crate::{Endpoint, Item};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
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
    search_items_cancellable(items, chapter, needle, matches, &AtomicBool::new(false));
}

pub fn search_items_cancellable(
    items: &[Item],
    chapter: Option<usize>,
    needle: &[char],
    matches: &mut Vec<Match>,
    cancel: &AtomicBool,
) {
    for (item_index, item) in items.iter().enumerate() {
        if matches.len() >= MAX_MATCHES || cancel.load(Ordering::Relaxed) {
            return;
        }
        let Some(text) = item.text() else {
            continue;
        };
        let mut graphemes = text.grapheme_indices(true).peekable();
        for hit in scan(
            text,
            needle,
            MAX_MATCHES.saturating_sub(matches.len()),
            cancel,
        ) {
            // Matches arrive in source order. Walk graphemes once rather than
            // rescanning the paragraph from its start for every highlight.
            while graphemes
                .peek()
                .is_some_and(|(offset, g)| offset + g.len() <= hit.start)
            {
                graphemes.next();
            }
            let start = graphemes.peek().map_or(hit.start, |(offset, _)| *offset);
            while graphemes
                .peek()
                .is_some_and(|(offset, g)| offset + g.len() < hit.end)
            {
                graphemes.next();
            }
            let end = graphemes
                .peek()
                .map_or(hit.end, |(offset, g)| offset + g.len());
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
    search_pages_cancellable(pages, needle, matches, &AtomicBool::new(false));
}

pub fn search_pages_cancellable(
    pages: &[String],
    needle: &[char],
    matches: &mut Vec<PdfMatch>,
    cancel: &AtomicBool,
) {
    for (page, text) in pages.iter().enumerate() {
        if matches.len() >= MAX_MATCHES || cancel.load(Ordering::Relaxed) {
            return;
        }
        for hit in scan(
            text,
            needle,
            MAX_MATCHES.saturating_sub(matches.len()),
            cancel,
        ) {
            matches.push(PdfMatch {
                page: page as u32,
                first: hit.first,
                last: hit.last,
            });
        }
    }
}

/// One folded (lower-cased, whitespace-collapsed) character and where it came from.
#[derive(Clone, Copy)]
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

fn scan(text: &str, needle: &[char], limit: usize, cancel: &AtomicBool) -> Vec<Hit> {
    if needle.is_empty() || limit == 0 {
        return Vec::new();
    }
    // KMP avoids quadratic comparisons for long repeated query prefixes.
    let mut prefix = vec![0; needle.len()];
    let mut matched = 0;
    for index in 1..needle.len() {
        while matched > 0 && needle[index] != needle[matched] {
            matched = prefix[matched - 1];
        }
        if needle[index] == needle[matched] {
            matched += 1;
        }
        prefix[index] = matched;
    }
    let mut source = text.char_indices().enumerate().peekable();
    let mut expansion: Option<(Folded, std::char::ToLowercase)> = None;
    let folded = std::iter::from_fn(move || {
        if let Some((origin, chars)) = &mut expansion
            && let Some(c) = chars.next()
        {
            return Some(Folded { c, ..*origin });
        }
        let (index, (start, c)) = source.next()?;
        let mut origin = Folded {
            first: index,
            last: index,
            start,
            end: start + c.len_utf8(),
            c,
        };
        if c.is_whitespace() {
            while source.peek().is_some_and(|(_, (_, c))| c.is_whitespace()) {
                let (last, (start, c)) = source.next().unwrap();
                origin.last = last;
                origin.end = start + c.len_utf8();
            }
            origin.c = ' ';
        } else {
            let mut chars = c.to_lowercase();
            origin.c = chars.next().unwrap();
            expansion = Some((origin, chars));
        }
        Some(origin)
    });
    let mut history = VecDeque::with_capacity(needle.len());
    let mut found = Vec::new();
    matched = 0;
    for (index, current) in folded.enumerate() {
        if index % 1024 == 0 && cancel.load(Ordering::Relaxed) {
            break;
        }
        if history.len() == needle.len() {
            history.pop_front();
        }
        history.push_back(current);
        while matched > 0 && current.c != needle[matched] {
            matched = prefix[matched - 1];
        }
        if current.c == needle[matched] {
            matched += 1;
        }
        if matched == needle.len() {
            let first = history.front().unwrap();
            found.push(Hit {
                start: first.start,
                end: current.end,
                first: first.first,
                last: current.last,
            });
            if found.len() == limit {
                break;
            }
            matched = 0; // Keep the existing non-overlapping-match behavior.
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BaseDirection;

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
        // U+0130 lower-cases to two characters, and "e\u{301}" is one grapheme.
        let text = "\u{130}ndx e\u{301}cole";
        let items = [paragraph("a", text)];
        let matches = find(&items, "ndx");
        assert_eq!(&text[matches[0].start..matches[0].end], "ndx");
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
    fn one_large_paragraph_stops_at_the_result_limit() {
        let text = "a ".repeat(MAX_MATCHES * 100);
        let matches = find(&[paragraph("large", &text)], "a");
        assert_eq!(matches.len(), MAX_MATCHES);
        assert_eq!(matches.last().unwrap().end, MAX_MATCHES * 2 - 1);
    }

    #[test]
    fn repeated_prefixes_and_unicode_expansions_keep_source_ranges() {
        let text = format!("{}b aaab İİ", "a".repeat(10_000));
        let matches = find(&[paragraph("prefix", &text)], "aaab");
        assert_eq!(matches.len(), 2);
        for hit in matches {
            assert_eq!(&text[hit.start..hit.end], "aaab");
        }
        let matches = find(&[paragraph("unicode", "İİ")], "i");
        assert_eq!(
            matches.iter().map(|m| (m.start, m.end)).collect::<Vec<_>>(),
            [(0, 2), (2, 4)]
        );
    }

    #[test]
    fn cancelled_search_does_not_scan_or_append_results() {
        let mut matches = Vec::new();
        search_items_cancellable(
            &[paragraph("large", &"a".repeat(100_000))],
            None,
            &needle("a").unwrap(),
            &mut matches,
            &AtomicBool::new(true),
        );
        assert!(matches.is_empty());
        assert!(
            scan(
                "aaa",
                &needle("a").unwrap(),
                MAX_MATCHES,
                &AtomicBool::new(true)
            )
            .is_empty()
        );
    }

    #[test]
    #[ignore = "Focused search timing: one MiB of repeated matches"]
    fn measure_large_paragraph_search() {
        let items = [paragraph("large", &"a ".repeat(524_288))];
        for _ in 0..3 {
            let start = std::time::Instant::now();
            let matches = find(&items, "a");
            assert_eq!(matches.len(), MAX_MATCHES);
            println!("search_us={}", start.elapsed().as_micros());
        }
    }

    #[test]
    fn endpoints_cover_the_match() {
        let items = [paragraph("p", "one two")];
        let (from, to) = find(&items, "two")[0].endpoints();
        assert_eq!((from.item_id.as_str(), from.byte_offset), ("p", 4));
        assert_eq!(to.byte_offset, 7);
    }
}
