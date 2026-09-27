//! Conservative, bounded PDF prose reconstruction. No PDFium handles escape the worker.
use crate::{Rect, TextLayer};
use std::collections::{HashMap, HashSet};

#[path = "book_images.rs"]
mod images;
pub use images::crop;
pub(crate) use images::scan_regions;

pub const VERSION: u32 = 5;
pub const MAX_PAGES: usize = 2_000;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_LINES: usize = 100_000;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SourceRange {
    pub page: u32,
    /// UTF-8 offsets in this page's original TextLayer::text (end exclusive).
    pub start: usize,
    pub end: usize,
    pub top: f32,
    pub bottom: f32,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Block {
    pub id: String,
    pub text: String,
    pub heading: bool,
    pub sources: Vec<SourceRange>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Conversion {
    pub blocks: Vec<Block>,
    pub illustrations: HashMap<String, Rect>,
    pub warnings: Vec<String>,
}
#[derive(Debug)]
struct Line {
    text: String,
    source: SourceRange,
    left: f32,
    right: f32,
    height: f32,
}

#[derive(Default)]
pub(crate) struct Builder {
    pages: Vec<Vec<Line>>,
    bytes: usize,
    lines: usize,
    unavailable: HashMap<u32, String>,
    illustrations: HashMap<u32, Vec<Rect>>,
}

impl Builder {
    pub(crate) fn illustrations(&mut self, page: u32, regions: Vec<Rect>) {
        self.illustrations.insert(page, regions);
    }

    pub(crate) fn push(&mut self, page: u32, layer: TextLayer) -> Result<(), String> {
        self.bytes = self
            .bytes
            .checked_add(layer.text.len())
            .ok_or("PDF Book text size overflow")?;
        if self.bytes > MAX_BYTES {
            return Err("PDF Book exceeds the 16 MiB text limit".into());
        }
        if self.pages.len() == MAX_PAGES {
            return Err("PDF Book exceeds the 2,000 page limit".into());
        }
        let lines = match lines(
            page,
            &layer,
            self.illustrations.get(&page).map_or(&[], Vec::as_slice),
        ) {
            Ok(lines) => lines,
            Err(error) => {
                self.unavailable.insert(page, error);
                Vec::new()
            }
        };
        self.lines += lines.len();
        if self.lines > MAX_LINES {
            return Err("PDF Book exceeds the 100,000 line limit".into());
        }
        self.pages.push(lines);
        Ok(())
    }
    pub(crate) fn finish(self, canceled: impl Fn() -> bool) -> Result<Conversion, String> {
        let mut repeated: HashMap<(bool, String), usize> = HashMap::new();
        let mut words = HashSet::new();
        for page in &self.pages {
            if canceled() {
                return Err("PDF Book conversion canceled".into());
            }
            let mut seen = HashSet::new();
            for line in page {
                if let Some(key) = margin_key(line) {
                    seen.insert(key);
                }
                for word in line.text.split(|c: char| !c.is_alphabetic() && c != '-') {
                    if (3..=64).contains(&word.len()) && words.len() < 100_000 {
                        words.insert(word.to_lowercase());
                    }
                }
            }
            for key in seen {
                *repeated.entry(key).or_default() += 1;
            }
        }
        let threshold = 3.max(self.pages.len().div_ceil(3));
        let mut blocks = Vec::new();
        let mut removed = 0;
        let mut joined = 0;
        let mut empty_pages = 0;
        let mut layout_pages = 0;
        let mut illustrations = HashMap::new();
        for (page_index, page) in self.pages.into_iter().enumerate() {
            if canceled() {
                return Err("PDF Book conversion canceled".into());
            }
            let mut page: Vec<_> = page
                .into_iter()
                .filter(|line| {
                    let remove = (line.height < 0.004
                        && !line.text.chars().any(char::is_alphanumeric))
                        || margin_key(line).is_some_and(|key| {
                            repeated.get(&key).copied().unwrap_or(0) >= threshold
                        });
                    removed += usize::from(remove);
                    !remove
                })
                .collect();
            // OCR streams may append marginal fragments out of vertical order.
            // Restore spatial line order before checking for overlapping columns.
            page.sort_by(|a, b| {
                a.source
                    .top
                    .total_cmp(&b.source.top)
                    .then_with(|| a.left.total_cmp(&b.left))
            });
            let mut ordered = Vec::with_capacity(page.len());
            let mut row: Vec<Line> = Vec::new();
            for line in page {
                if row.first().is_some_and(|first| {
                    let middle = (first.source.top + first.source.bottom) * 0.5;
                    let next = (line.source.top + line.source.bottom) * 0.5;
                    (next - middle).abs() > first.height.max(line.height) * 0.5
                }) {
                    row.sort_by(|a, b| a.left.total_cmp(&b.left));
                    ordered.append(&mut row);
                }
                row.push(line);
            }
            row.sort_by(|a, b| a.left.total_cmp(&b.left));
            ordered.append(&mut row);
            let mut page = ordered;
            let column_pairs = page
                .windows(2)
                .filter(|pair| {
                    let a = &pair[0];
                    let b = &pair[1];
                    (b.source.top - a.source.top).abs() < a.height.min(b.height) * 0.45
                        && b.left > a.right + 0.12
                        && a.text.len() > 6
                        && b.text.len() > 6
                })
                .count();
            let order_error = column_pairs >= 3 || (column_pairs > 0 && page.len() <= 4);
            if order_error || self.unavailable.contains_key(&(page_index as u32)) {
                layout_pages += 1;
                page.clear();
            }
            let first_block = blocks.len();
            let regions: &[Rect] = self
                .illustrations
                .get(&(page_index as u32))
                .map_or(&[], Vec::as_slice);
            for (index, rect) in regions.iter().enumerate() {
                let id = format!("pdf-b{VERSION}-p{page_index:06}-image{index}");
                illustrations.insert(id.clone(), *rect);
                blocks.push(Block {
                    id,
                    text: String::new(),
                    heading: false,
                    sources: vec![SourceRange {
                        page: page_index as u32,
                        start: 0,
                        end: 0,
                        top: rect.top,
                        bottom: rect.bottom,
                    }],
                });
            }
            if page.is_empty() {
                empty_pages += 1;
                if regions.is_empty() {
                    blocks.push(Block {
                        id: format!("pdf-b{VERSION}-p{page_index:06}-empty"),
                        text: if order_error || self.unavailable.contains_key(&(page_index as u32)) {
                            "This page could not be reconstructed. Open Document to see the original.".into()
                        } else { String::new() },
                        heading: false,
                        sources: vec![SourceRange {
                            page: page_index as u32,
                            start: 0,
                            end: 0,
                            top: 0.0,
                            bottom: 1.0,
                        }],
                    });
                }
                continue;
            }
            let mut heights: Vec<_> = page.iter().map(|l| l.height).collect();
            heights.sort_by(f32::total_cmp);
            let body_height = heights[heights.len() / 2];
            let right = page.iter().map(|l| l.right).fold(0.0, f32::max);
            let left = page.iter().map(|l| l.left).fold(1.0, f32::min);
            let narrow_lines = page.len() >= 6
                && page.iter().filter(|l| l.right - l.left < 0.5).count() * 4 >= page.len() * 3;
            let mut previous: Option<&Line> = None;
            let mut current: Option<Block> = None;
            for line in &page {
                let heading = line.height > body_height * 1.35 && line.text.chars().count() <= 120;
                let start = previous.is_none_or(|prev| {
                    if (line.source.top + line.source.bottom - prev.source.top - prev.source.bottom)
                        .abs()
                        < body_height
                    {
                        return false;
                    }
                    narrow_lines
                        || heading
                        || current.as_ref().is_some_and(|b| b.heading)
                        || line.source.top - prev.source.bottom > body_height * 0.9
                        || (line.left > prev.left + 0.025
                            && line.source.top >= prev.source.bottom - body_height * 0.3)
                        || is_list(&line.text)
                        || (prev.right < right - (right - left) * 0.2
                            && (ends_sentence(&prev.text)
                                || line.text.chars().next().is_some_and(char::is_uppercase)))
                });
                if start && let Some(block) = current.take() {
                    blocks.push(block);
                }
                let block = current.get_or_insert_with(|| Block {
                    id: format!(
                        "pdf-b{VERSION}-p{:06}-c{:07}",
                        line.source.page, line.source.start
                    ),
                    text: String::new(),
                    heading,
                    sources: Vec::new(),
                });
                if !block.text.is_empty() {
                    let joined_word = join_hyphen(&mut block.text, &line.text, &words);
                    joined += usize::from(joined_word);
                    if !joined_word {
                        block.text.push(' ');
                    }
                }
                block.text.push_str(&line.text);
                block.sources.push(line.source.clone());
                previous = Some(line);
            }
            if let Some(block) = current {
                blocks.push(block);
            }
            blocks[first_block..].sort_by(|a, b| a.sources[0].top.total_cmp(&b.sources[0].top));
        }
        let mut warnings = Vec::new();
        if removed > 0 {
            warnings.push(format!(
                "Omitted {removed} repeated margin lines; original pages retain them."
            ));
        }
        if joined > 0 {
            warnings.push(format!("Joined {joined} line-end hyphenations. Visible hyphens are retained unless the unhyphenated word also appears in this PDF."));
        }
        if empty_pages > 0 {
            warnings.push(format!("{empty_pages} source pages have no reconstructed prose ({layout_pages} with unsupported text layout)."));
        }
        Ok(Conversion {
            blocks,
            warnings,
            illustrations,
        })
    }
}

fn ends_sentence(text: &str) -> bool {
    text.trim_end_matches(['"', '\'', '”', '’', ')'])
        .ends_with(['.', '!', '?', ':'])
}
fn is_list(text: &str) -> bool {
    text.starts_with(['•', '*', '–'])
        || text.starts_with("- ")
        || text.split_once(['.', ')']).is_some_and(|(prefix, suffix)| {
            prefix.len() <= 4
                && !prefix.is_empty()
                && prefix.bytes().all(|c| c.is_ascii_digit())
                && suffix.starts_with(' ')
        })
}
fn margin_key(line: &Line) -> Option<(bool, String)> {
    let top = line.source.bottom < 0.12;
    if !top && line.source.top < 0.84 {
        return None;
    }
    if line.text.len() > 160 {
        return None;
    }
    let mut key = String::new();
    let mut digit = false;
    for c in line.text.chars().flat_map(char::to_lowercase) {
        if c.is_numeric() || c.is_control() {
            if !digit {
                key.push('#');
            }
            digit = true;
        } else {
            digit = false;
            key.push(c);
        }
    }
    Some((top, key))
}
fn join_hyphen(text: &mut String, next: &str, words: &HashSet<String>) -> bool {
    if !text.ends_with(['-', '\u{ad}']) || !next.chars().next().is_some_and(char::is_lowercase) {
        return false;
    }
    let prefix = text[..text.len() - text.chars().last().unwrap().len_utf8()]
        .rsplit(|c: char| !c.is_alphabetic())
        .next()
        .unwrap_or("");
    let suffix = next
        .split(|c: char| !c.is_alphabetic())
        .next()
        .unwrap_or("");
    if prefix.is_empty() || suffix.is_empty() {
        return false;
    }
    if text.ends_with('\u{ad}')
        || (words.contains(&format!("{prefix}{suffix}").to_lowercase())
            && !words.contains(&format!("{prefix}-{suffix}").to_lowercase()))
    {
        text.pop();
    }
    true
}

fn lines(page: u32, layer: &TextLayer, illustrations: &[Rect]) -> Result<Vec<Line>, String> {
    let nonspace = layer.text.chars().filter(|c| !c.is_whitespace()).count();
    let letters = layer.text.chars().filter(|c| c.is_alphabetic()).count();
    if nonspace > 200 && letters * 2 < nonspace {
        return Err(format!("Page {} has unreliable OCR text", page + 1));
    }
    let mut result = Vec::new();
    let mut start = None;
    let mut end = 0;
    let mut bounds: Option<Rect> = None;
    let mut previous: Option<Rect> = None;
    let mut missing = 0;
    let mut letters = 0;
    let mut glyph_heights = Vec::new();
    let finish = |result: &mut Vec<Line>,
                  start: &mut Option<usize>,
                  end: usize,
                  bounds: &mut Option<Rect>,
                  glyph_heights: &mut Vec<f32>| {
        if let (Some(from), Some(rect)) = (start.take(), bounds.take()) {
            let raw = &layer.text[from..end];
            let text = raw.trim();
            if !text.is_empty() {
                let from = from + raw.len() - raw.trim_start().len();
                glyph_heights.sort_by(f32::total_cmp);
                let height = glyph_heights
                    .get(glyph_heights.len() / 2)
                    .copied()
                    .unwrap_or(rect.bottom - rect.top);
                result.push(Line {
                    text: text.into(),
                    source: SourceRange {
                        page,
                        start: from,
                        end: from + text.len(),
                        top: rect.top,
                        bottom: rect.bottom,
                    },
                    left: rect.left,
                    right: rect.right,
                    height,
                });
            }
        }
        glyph_heights.clear();
    };
    for glyph in &layer.glyphs {
        let ch = &layer.text[glyph.start..glyph.end];
        if glyph.bounds.is_some_and(|r| {
            illustrations.iter().any(|image| {
                (r.left + r.right) * 0.5 >= image.left
                    && (r.left + r.right) * 0.5 <= image.right
                    && (r.top + r.bottom) * 0.5 >= image.top
                    && (r.top + r.bottom) * 0.5 <= image.bottom
            })
        }) {
            finish(
                &mut result,
                &mut start,
                end,
                &mut bounds,
                &mut glyph_heights,
            );
            previous = None;
            continue;
        }
        if ch == "\r" || ch == "\n" {
            finish(
                &mut result,
                &mut start,
                end,
                &mut bounds,
                &mut glyph_heights,
            );
            previous = None;
            continue;
        }
        if !ch.trim().is_empty() {
            letters += 1;
            missing += usize::from(glyph.bounds.is_none() || ch.contains('\u{fffd}'));
        }
        if let Some(rect) = glyph.bounds {
            if let Some(prev) = previous {
                let height = (prev.bottom - prev.top).max(rect.bottom - rect.top);
                if (rect.top - prev.top).abs() > height * 0.65 {
                    finish(
                        &mut result,
                        &mut start,
                        end,
                        &mut bounds,
                        &mut glyph_heights,
                    );
                } else if rect.left - prev.right > 0.12
                    && rect.top >= 0.12
                    && rect.bottom <= 0.84
                    && start.is_some_and(|s| glyph.start - s > 8)
                {
                    // Marginal line numbers and OCR fragments can be separated by
                    // a wide gap. Keep the segments; inspect complete rows later.
                    finish(
                        &mut result,
                        &mut start,
                        end,
                        &mut bounds,
                        &mut glyph_heights,
                    );
                }
            }
            glyph_heights.push(rect.bottom - rect.top);
            bounds = Some(bounds.map_or(rect, |b| Rect {
                left: b.left.min(rect.left),
                right: b.right.max(rect.right),
                top: b.top.min(rect.top),
                bottom: b.bottom.max(rect.bottom),
            }));
            previous = Some(rect);
        }
        start.get_or_insert(glyph.start);
        end = glyph.end;
    }
    finish(
        &mut result,
        &mut start,
        end,
        &mut bounds,
        &mut glyph_heights,
    );
    if letters > 0 && missing * 20 > letters {
        return Err(format!(
            "Page {} has unreliable text or geometry. Use Document mode.",
            page + 1
        ));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Glyph;
    fn layer(rows: &[(&str, f32, f32)]) -> TextLayer {
        let mut text = String::new();
        let mut glyphs = Vec::new();
        for (line, left, top) in rows {
            for (i, ch) in line.chars().chain(std::iter::once('\n')).enumerate() {
                let start = text.len();
                text.push(ch);
                glyphs.push(Glyph {
                    start,
                    end: text.len(),
                    bounds: (!ch.is_whitespace()).then_some(Rect {
                        left: left + i as f32 * 0.008,
                        right: left + (i + 1) as f32 * 0.008,
                        top: *top,
                        bottom: top + 0.018,
                    }),
                });
            }
        }
        TextLayer::new(text, glyphs)
    }
    #[test]
    fn illustrations_are_inserted_between_prose_without_duplicating_their_text() {
        let mut builder = Builder::default();
        builder.illustrations(
            0,
            vec![Rect {
                left: 0.1,
                right: 0.9,
                top: 0.3,
                bottom: 0.5,
            }],
        );
        builder
            .push(
                0,
                layer(&[
                    ("Text above the illustration.", 0.1, 0.2),
                    ("Lettering inside the illustration", 0.1, 0.4),
                    ("Caption below the illustration.", 0.1, 0.6),
                ]),
            )
            .unwrap();
        let result = builder.finish(|| false).unwrap();
        assert_eq!(result.blocks.len(), 3);
        assert!(result.blocks[0].text.starts_with("Text above"));
        assert!(result.illustrations.contains_key(&result.blocks[1].id));
        assert!(result.blocks[2].text.starts_with("Caption below"));
        assert!(result.blocks.iter().all(|b| b.sources[0].page == 0));
    }

    #[test]
    fn ocr_fragments_on_one_baseline_are_not_mistaken_for_columns() {
        let mut builder = Builder::default();
        builder
            .push(
                0,
                layer(&[
                    ("Reading the first fragment", 0.1, 0.2),
                    ("and the continuation.", 0.35, 0.202),
                ]),
            )
            .unwrap();
        let result = builder.finish(|| false).unwrap();
        assert_eq!(result.blocks.len(), 1);
        assert!(
            result.blocks[0]
                .text
                .contains("fragment and the continuation")
        );
    }

    #[test]
    fn unmapped_page_number_controls_match_repeated_margin_templates() {
        let mut builder = Builder::default();
        for (page, footer) in [
            "\u{18} Book title",
            "2 Book title",
            "3 Book title",
            "4 Book title",
        ]
        .into_iter()
        .enumerate()
        {
            builder
                .push(
                    page as u32,
                    layer(&[(footer, 0.1, 0.86), ("Body text remains intact.", 0.1, 0.2)]),
                )
                .unwrap();
        }
        let result = builder.finish(|| false).unwrap();
        assert_eq!(result.blocks.len(), 4);
        assert!(
            result
                .blocks
                .iter()
                .all(|b| b.text == "Body text remains intact.")
        );
    }

    #[test]
    fn interleaved_columns_keep_a_source_page_slot() {
        let mut builder = Builder::default();
        builder
            .push(
                0,
                layer(&[
                    ("Left column text", 0.1, 0.2),
                    ("Right column text", 0.6, 0.2),
                ]),
            )
            .unwrap();
        assert!(
            builder.finish(|| false).unwrap().blocks[0]
                .text
                .contains("could not be reconstructed")
        );
    }

    #[test]
    fn drop_caps_stay_prose_and_source_page_boundaries_are_preserved() {
        let mut first = layer(&[
            ("A normal opening sentence that stays prose", 0.1, 0.2),
            ("continues beside the drop cap.", 0.14, 0.226),
            ("This continues across the", 0.1, 0.76),
        ]);
        first.glyphs[0].bounds.as_mut().unwrap().bottom += 0.025;
        let mut builder = Builder::default();
        builder.push(0, first).unwrap();
        builder
            .push(1, layer(&[("page boundary and ends here.", 0.1, 0.15)]))
            .unwrap();
        let result = builder.finish(|| false).unwrap();
        assert!(!result.blocks[0].heading);
        assert!(result.blocks[0].text.contains("prose continues beside"));
        let last = result.blocks.last().unwrap();
        assert_eq!(last.text, "page boundary and ends here.");
        assert_eq!(last.sources.first().unwrap().page, 1);
        assert!(result.blocks.iter().all(|block| {
            block
                .sources
                .iter()
                .all(|source| source.page == block.sources[0].page)
        }));
        assert_eq!(last.sources.last().unwrap().page, 1);
    }
    #[test]
    fn short_date_line_stays_separate_from_the_letter_body() {
        let mut builder = Builder::default();
        builder
            .push(
                0,
                layer(&[
                    ("December 11, 17—", 0.1, 0.2),
                    (
                        "You will rejoice to hear that this letter has arrived.",
                        0.1,
                        0.226,
                    ),
                ]),
            )
            .unwrap();
        let result = builder.finish(|| false).unwrap();
        assert_eq!(result.blocks.len(), 2);
        assert_eq!(result.blocks[0].text, "December 11, 17—");
    }
    #[test]
    fn pages_with_only_repeated_headers_keep_source_page_slots() {
        let mut builder = Builder::default();
        for page in 0..4 {
            builder
                .push(page, layer(&[("A running header", 0.1, 0.03)]))
                .unwrap();
        }
        assert_eq!(builder.finish(|| false).unwrap().blocks.len(), 4);
    }

    #[test]
    fn repeated_margins_prose_and_exact_source_ranges() {
        let mut builder = Builder::default();
        let mut originals = Vec::new();
        for page in 0..4 {
            let input = layer(&[
                ("A running title", 0.1, 0.03),
                ("A paragraph continues on the following", 0.1, 0.2),
                ("line, with punctuation intact.", 0.1, 0.226),
                ("A new paragraph.", 0.13, 0.28),
                ("Page 12", 0.4, 0.96),
            ]);
            originals.push(input.text.clone());
            builder.push(page, input).unwrap();
        }
        let result = builder.finish(|| false).unwrap();
        assert_eq!(result.blocks.len(), 8);
        assert_eq!(
            result.blocks[0].text,
            "A paragraph continues on the following line, with punctuation intact."
        );
        assert!(result.warnings.iter().any(|w| w.contains("8 repeated")));
        for block in &result.blocks {
            for source in &block.sources {
                let original = &originals[source.page as usize][source.start..source.end];
                assert!(block.text.contains(original));
                assert!(source.top < source.bottom);
            }
        }
    }
    #[test]
    fn hyphen_cleanup_requires_evidence_and_keeps_compounds() {
        let words = HashSet::from(["reading".into(), "well-known".into()]);
        for (before, after, expected) in [
            ("read-", "ing", "reading"),
            ("well-", "known", "well-known"),
            ("un-", "seen", "un-seen"),
            ("soft\u{ad}", "ware", "software"),
        ] {
            let mut text = before.to_owned();
            assert!(join_hyphen(&mut text, after, &words));
            text.push_str(after);
            assert_eq!(text, expected);
        }
        let mut text = "word-".into();
        assert!(!join_hyphen(&mut text, "Uppercase", &words));
        assert_eq!(text, "word-");
    }
    #[test]
    fn preserves_columns_and_unreliable_text_but_rejects_canceled_results() {
        let mut builder = Builder::default();
        builder
            .push(
                0,
                layer(&[
                    ("Left column upper", 0.1, 0.2),
                    ("Left column lower", 0.1, 0.5),
                    ("Right column upper", 0.6, 0.2),
                ]),
            )
            .unwrap();
        assert!(
            builder.finish(|| false).unwrap().blocks[0]
                .text
                .contains("could not be reconstructed")
        );
        let mut input = layer(&[("Meaningful text", 0.1, 0.2)]);
        input.glyphs[0].bounds = None;
        let mut invalid = Builder::default();
        invalid.push(0, input).unwrap();
        assert!(
            invalid.finish(|| false).unwrap().blocks[0]
                .text
                .contains("could not be reconstructed")
        );
        let mut builder = Builder::default();
        builder.push(0, layer(&[("Text", 0.1, 0.2)])).unwrap();
        assert!(builder.finish(|| true).unwrap_err().contains("canceled"));
    }
    #[test]
    fn image_only_and_mixed_pages_retain_global_source_pages() {
        let mut builder = Builder::default();
        builder
            .push(0, TextLayer::new(String::new(), vec![]))
            .unwrap();
        assert!(builder.finish(|| false).unwrap().blocks[0].text.is_empty());
        let mut builder = Builder::default();
        for page in 0..3 {
            builder
                .push(page, layer(&[("Ordinary text", 0.1, 0.2)]))
                .unwrap();
        }
        builder
            .push(3, TextLayer::new(String::new(), vec![]))
            .unwrap();
        let result = builder.finish(|| false).unwrap();
        assert!(result.blocks[3].text.is_empty());
        assert_eq!(result.blocks.len(), 4);
        assert_eq!(result.blocks[3].sources[0].page, 3);
    }
    #[test]
    fn unique_marginal_text_is_not_silently_discarded() {
        let mut builder = Builder::default();
        builder
            .push(
                0,
                layer(&[
                    ("A meaningful dedication", 0.1, 0.03),
                    ("The body", 0.1, 0.2),
                ]),
            )
            .unwrap();
        let result = builder.finish(|| false).unwrap();
        assert!(result.blocks.iter().any(|b| b.text.contains("dedication")));
    }
    #[test]
    fn byte_and_page_limits_fail_without_truncating() {
        let mut builder = Builder {
            bytes: MAX_BYTES,
            ..Builder::default()
        };
        assert!(builder.push(0, layer(&[("text", 0.1, 0.2)])).is_err());
        let mut builder = Builder {
            pages: (0..MAX_PAGES).map(|_| Vec::new()).collect(),
            ..Builder::default()
        };
        assert!(
            builder
                .push(0, TextLayer::new(String::new(), vec![]))
                .is_err()
        );
    }
}
