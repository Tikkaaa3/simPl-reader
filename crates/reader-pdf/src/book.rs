//! Conservative, bounded PDF prose reconstruction. No PDFium handles escape the worker.
use crate::{Rect, TextLayer};
use std::collections::{HashMap, HashSet};

#[path = "book_images.rs"]
mod images;
pub use images::crop;
pub(crate) use images::{may_contain_illustration, scan_regions, text_extent};

pub const VERSION: u32 = 12;
pub const MAX_PAGES: usize = 5_000;
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_LINES: usize = 1_000_000;

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
    #[serde(default)]
    pub layout: BlockLayout,
    /// Size relative to the page's body text, clamped again by the presenter.
    #[serde(default = "unit_scale")]
    pub size_ratio: f32,
    /// Additional top space as a fraction of the source page height.
    #[serde(default)]
    pub top_gap: f32,
    #[serde(default)]
    pub styles: Vec<TextStyleRun>,
    #[serde(default)]
    pub links: Vec<TextLink>,
    pub sources: Vec<SourceRange>,
}
fn unit_scale() -> f32 {
    1.0
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum BlockLayout {
    #[default]
    Flow,
    Centered,
    Right,
    Inset {
        indent: f32,
    },
    List {
        indent: f32,
    },
    Toc {
        number_start: usize,
        indent: f32,
    },
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextStyleRun {
    pub start: usize,
    pub end: usize,
    pub bold: bool,
    pub italic: bool,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextLink {
    pub start: usize,
    pub end: usize,
    pub href: String,
}
#[derive(Clone, Debug)]
pub(crate) struct SourceLink {
    pub start: usize,
    pub end: usize,
    pub href: String,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Conversion {
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub page_labels: HashMap<u32, String>,
    #[serde(default)]
    pub fallback_pages: HashSet<u32>,
    pub illustrations: HashMap<String, Rect>,
    /// Horizontal placement of each illustration within the text measure.
    #[serde(default)]
    pub placements: HashMap<String, Placement>,
    pub warnings: Vec<String>,
}
/// An illustration's left edge and width as fractions of the text measure it
/// sits in: its column on multi-column pages, otherwise the book's text block.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Placement {
    pub offset: f32,
    pub width: f32,
}
#[derive(Debug)]
struct Line {
    text: String,
    source: SourceRange,
    left: f32,
    right: f32,
    height: f32,
    font_size: f32,
    styles: Vec<TextStyleRun>,
    links: Vec<TextLink>,
}

#[derive(Default)]
pub(crate) struct Builder {
    pages: Vec<Vec<Line>>,
    bytes: usize,
    lines: usize,
    unavailable: HashMap<u32, String>,
    illustrations: HashMap<u32, Vec<Rect>>,
    page_labels: HashMap<u32, String>,
    links: HashMap<u32, Vec<SourceLink>>,
}

impl Builder {
    pub(crate) fn page_label(&mut self, page: u32, label: Option<&str>) {
        if let Some(label) = label.map(str::trim).filter(|s| !s.is_empty()) {
            self.page_labels.insert(page, label.to_owned());
        }
    }
    pub(crate) fn illustrations(&mut self, page: u32, regions: Vec<Rect>) {
        self.illustrations.insert(page, regions);
    }
    pub(crate) fn links(&mut self, page: u32, links: Vec<SourceLink>) {
        self.links.insert(page, links);
    }

    pub(crate) fn push(&mut self, page: u32, layer: TextLayer) -> Result<(), String> {
        self.bytes = self
            .bytes
            .checked_add(layer.text.len())
            .ok_or("PDF Book text size overflow")?;
        if self.bytes > MAX_BYTES {
            return Err("PDF Book exceeds the 64 MiB text limit".into());
        }
        if self.pages.len() == MAX_PAGES {
            return Err("PDF Book exceeds the 5,000 page limit".into());
        }
        let lines = match lines(
            page,
            &layer,
            self.illustrations.get(&page).map_or(&[], Vec::as_slice),
            self.links.get(&page).map_or(&[], Vec::as_slice),
        ) {
            Ok(lines) => lines,
            Err(error) => {
                self.unavailable.insert(page, error);
                Vec::new()
            }
        };
        self.lines += lines.len();
        if self.lines > MAX_LINES {
            return Err("PDF Book exceeds the 1,000,000 line limit".into());
        }
        self.pages.push(lines);
        Ok(())
    }
    pub(crate) fn finish(self, canceled: impl Fn() -> bool) -> Result<Conversion, String> {
        let has_pdf_labels = !self.page_labels.is_empty();
        let printed_labels = printed_page_numbers(&self.pages, &self.page_labels);
        let mut page_labels = self.page_labels;
        let mut repeated: HashMap<(bool, String), usize> = HashMap::new();
        let mut words = HashSet::new();
        let mut document_sizes: Vec<f32> = self
            .pages
            .iter()
            .flat_map(|page| page.iter())
            .filter(|line| {
                line.text.chars().count() > 20
                    && line.source.top >= 0.1
                    && line.source.bottom <= 0.84
            })
            .map(|line| line.font_size)
            .filter(|size| *size > 0.0)
            .collect();
        document_sizes.sort_by(f32::total_cmp);
        let document_body_size = document_sizes
            .get(document_sizes.len() / 2)
            .copied()
            .unwrap_or(0.0);
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
        let mut placements = HashMap::new();
        let mut fallback_pages = HashSet::new();
        let measure = text_measure(&self.pages);
        let mut previous_complex_contents = false;
        for (page_index, page) in self.pages.into_iter().enumerate() {
            if canceled() {
                return Err("PDF Book conversion canceled".into());
            }
            let mut page: Vec<_> = page
                .into_iter()
                .filter(|line| {
                    if printed_labels
                        .get(&(page_index as u32))
                        .is_some_and(|(start, _)| *start == line.source.start)
                    {
                        return false;
                    }
                    let remove = (line.height < 0.004
                        && !line.text.chars().any(char::is_alphanumeric))
                        || margin_key(line).is_some_and(|key| {
                            repeated.get(&key).copied().unwrap_or(0) >= threshold
                        });
                    removed += usize::from(remove);
                    !remove
                })
                .collect();
            if let Some((_, label)) = printed_labels.get(&(page_index as u32)) {
                page_labels.insert(page_index as u32, label.clone());
            }
            // An explicit PDF label sequence can intentionally leave its opening
            // leaves unnumbered. Do not invent physical numbers for those leaves.
            if has_pdf_labels {
                page_labels.entry(page_index as u32).or_default();
            }
            removed += drop_margin_marks(&mut page);
            // OCR streams may append marginal fragments out of vertical order.
            // Restore spatial line order before checking for overlapping columns.
            page.sort_by(|a, b| {
                a.source
                    .top
                    .total_cmp(&b.source.top)
                    .then_with(|| a.left.total_cmp(&b.left))
            });
            let table_like = looks_like_table(&page);
            let gutter = gutter(&page);
            let (mut page, mut columns): (Vec<Line>, Vec<u8>) =
                reading_order(page, gutter).into_iter().unzip();
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
            let range_count = page
                .iter()
                .filter(|line| has_page_range(&line.text))
                .count();
            let has_contents_title = page.iter().any(|line| is_contents_title(&line.text));
            let complex_contents =
                range_count >= 3 && (has_contents_title || previous_complex_contents);
            previous_complex_contents = complex_contents;
            let fallback = order_error
                || table_like
                || complex_contents
                || self.unavailable.contains_key(&(page_index as u32));
            if fallback {
                layout_pages += 1;
                fallback_pages.insert(page_index as u32);
                page.clear();
                columns.clear();
            }
            // Paragraph geometry is measured against each line's own column.
            let mut extents = [(1.0_f32, 0.0_f32); 3];
            for (line, column) in page.iter().zip(&columns) {
                let extent = &mut extents[usize::from(*column)];
                extent.0 = extent.0.min(line.left);
                extent.1 = extent.1.max(line.right);
            }
            let full_page = [Rect {
                left: 0.0,
                top: 0.0,
                right: 1.0,
                bottom: 1.0,
            }];
            let regions: &[Rect] = if fallback {
                &full_page
            } else {
                self.illustrations
                    .get(&(page_index as u32))
                    .map_or(&[], Vec::as_slice)
            };
            let mut figures: Vec<(Block, u8)> = regions
                .iter()
                .enumerate()
                .map(|(index, rect)| {
                    let id = if fallback {
                        format!("pdf-b{VERSION}-p{page_index:06}-source")
                    } else {
                        format!("pdf-b{VERSION}-p{page_index:06}-image{index}")
                    };
                    illustrations.insert(id.clone(), *rect);
                    let column = match gutter {
                        Some(x) if rect.right <= x + 0.02 => 1,
                        Some(x) if rect.left >= x - 0.02 => 2,
                        _ => 0,
                    };
                    let (left, right) = match extents[usize::from(column)] {
                        (left, right) if column != 0 && right - left > 0.1 => (left, right),
                        _ => measure,
                    };
                    let width = ((rect.right - rect.left) / (right - left)).clamp(0.05, 1.0);
                    let offset = ((rect.left - left) / (right - left)).clamp(0.0, 1.0 - width);
                    placements.insert(id.clone(), Placement { offset, width });
                    let block = Block {
                        id,
                        text: String::new(),
                        heading: false,
                        layout: BlockLayout::Flow,
                        size_ratio: 1.0,
                        top_gap: 0.0,
                        styles: Vec::new(),
                        links: Vec::new(),
                        sources: vec![SourceRange {
                            page: page_index as u32,
                            start: 0,
                            end: 0,
                            top: rect.top,
                            bottom: rect.bottom,
                        }],
                    };
                    (block, column)
                })
                .collect();
            figures.sort_by(|a, b| a.0.sources[0].top.total_cmp(&b.0.sources[0].top));
            if page.is_empty() {
                empty_pages += 1;
                if figures.is_empty() {
                    blocks.push(Block {
                        id: format!("pdf-b{VERSION}-p{page_index:06}-empty"),
                        text: if order_error || self.unavailable.contains_key(&(page_index as u32)) {
                            "This page could not be reconstructed. Open Document to see the original.".into()
                        } else { String::new() },
                        heading: false,
                        layout: BlockLayout::Flow,
                        size_ratio: 1.0,
                        top_gap: 0.0,
                        styles: Vec::new(),
                        links: Vec::new(),
                        sources: vec![SourceRange {
                            page: page_index as u32,
                            start: 0,
                            end: 0,
                            top: 0.0,
                            bottom: 1.0,
                        }],
                    });
                }
                blocks.extend(figures.into_iter().map(|(block, _)| block));
                continue;
            }
            let mut heights: Vec<_> = page.iter().map(|l| l.height).collect();
            heights.sort_by(f32::total_cmp);
            let body_height = heights[heights.len() / 2];
            let mut font_sizes: Vec<_> = page
                .iter()
                .map(|l| l.font_size)
                .filter(|s| *s > 0.0)
                .collect();
            font_sizes.sort_by(f32::total_cmp);
            let body_size = if font_sizes.len() >= 5 {
                font_sizes[font_sizes.len() / 2]
            } else {
                document_body_size
            };
            let narrow_lines = page.len() >= 6
                && page
                    .iter()
                    .zip(&columns)
                    .filter(|(l, c)| {
                        let (left, right) = extents[usize::from(**c)];
                        l.right - l.left < if **c == 0 { 0.5 } else { (right - left) * 0.6 }
                    })
                    .count()
                    * 4
                    >= page.len() * 3;
            // In column layouts (dictionaries, encyclopedias), entries separated by a
            // little extra leading start new blocks even without indentation. Single
            // column scans are excluded: OCR line bounds are too noisy for this.
            let mut gaps: Vec<f32> = page
                .windows(2)
                .zip(columns.windows(2))
                .filter(|(_, c)| c[0] == c[1])
                .map(|(l, _)| l[1].source.top - l[0].source.bottom)
                .filter(|gap| gap.abs() < body_height)
                .collect();
            gaps.sort_by(f32::total_cmp);
            let spacing = (gutter.is_some() && gaps.len() >= 8)
                .then(|| gaps[gaps.len() / 4] + body_height * 0.3);
            let mut texts: Vec<(Block, u8)> = Vec::new();
            if let Some(toc) = toc_blocks(&page, &columns, measure, body_size, body_height) {
                texts = toc;
            } else {
                let mut previous: Option<(&Line, u8)> = None;
                let mut current: Option<(Block, u8)> = None;
                for (index, (line, &column)) in page.iter().zip(&columns).enumerate() {
                    let ratio = if body_size > 0.0 && line.font_size > 0.0 {
                        line.font_size / body_size
                    } else {
                        line.height / body_height
                    };
                    let centered_title = previous.is_none()
                        && centered(line)
                        && line.text.chars().count() <= 80
                        && page.get(1).is_some_and(|next| {
                            next.source.top - line.source.bottom > body_height * 1.5
                        });
                    let heading =
                        (ratio > 1.3 || centered_title) && line.text.chars().count() <= 120;
                    let (left, right) = extents[usize::from(column)];
                    let local_measure = if column == 0 && gutter.is_none() && page.len() <= 8 {
                        measure
                    } else {
                        (left, right)
                    };
                    let layout = line_layout(
                        line,
                        local_measure,
                        heading,
                        page.len() <= 8,
                        gutter.is_none(),
                        (previous.map(|(prev, _)| prev), page.get(index + 1)),
                        body_height,
                    );
                    let start = previous.is_none_or(|(prev, prev_column)| {
                        if (line.source.top + line.source.bottom
                            - prev.source.top
                            - prev.source.bottom)
                            .abs()
                            < body_height
                        {
                            return false;
                        }
                        let (left, _) = extents[usize::from(column)];
                        let (prev_left, prev_right) = extents[usize::from(prev_column)];
                        narrow_lines
                            || heading
                            || current
                                .as_ref()
                                .is_some_and(|(block, _)| block.layout != layout)
                            || !matches!(&layout, BlockLayout::Flow | BlockLayout::Inset { .. })
                            || current.as_ref().is_some_and(|(b, _)| b.heading)
                            || line.source.top - prev.source.bottom > body_height * 0.9
                            || spacing.is_some_and(|limit| {
                                column == prev_column
                                    && line.source.top - prev.source.bottom > limit
                            })
                            || (line.left - left > prev.left - prev_left + 0.025
                                && (column != prev_column
                                    || line.source.top >= prev.source.bottom - body_height * 0.3))
                            || is_list(&line.text)
                            || numbered_entry(&line.text)
                            || (prev.right < prev_right - (prev_right - prev_left) * 0.2
                                && (ends_sentence(&prev.text)
                                    || line.text.chars().next().is_some_and(char::is_uppercase)))
                    });
                    if start && let Some(block) = current.take() {
                        texts.push(block);
                    }
                    let (block, _) = current.get_or_insert_with(|| {
                        let block = Block {
                            id: format!(
                                "pdf-b{VERSION}-p{:06}-c{:07}",
                                line.source.page, line.source.start
                            ),
                            text: String::new(),
                            heading,
                            layout,
                            size_ratio: if heading { ratio.clamp(1.1, 2.0) } else { 1.0 },
                            top_gap: if previous.is_none()
                                && (page.len() <= 4 || centered_title)
                                && centered(line)
                            {
                                (line.source.top - 0.07).clamp(0.0, 0.55)
                            } else {
                                0.0
                            },
                            styles: Vec::new(),
                            links: Vec::new(),
                            sources: Vec::new(),
                        };
                        (block, column)
                    });
                    if !block.text.is_empty() {
                        let joined_word = join_hyphen(&mut block.text, &line.text, &words);
                        for style in &mut block.styles {
                            style.end = style.end.min(block.text.len());
                        }
                        block.styles.retain(|style| style.start < style.end);
                        for link in &mut block.links {
                            link.end = link.end.min(block.text.len());
                        }
                        block.links.retain(|link| link.start < link.end);
                        joined += usize::from(joined_word);
                        if !joined_word {
                            block.text.push(' ');
                        }
                    }
                    let offset = block.text.len();
                    block.text.push_str(&line.text);
                    append_styles(&mut block.styles, &line.styles, offset);
                    append_links(&mut block.links, &line.links, offset);
                    block.sources.push(line.source.clone());
                    previous = Some((line, column));
                }
                if let Some(block) = current {
                    texts.push(block);
                }
            }
            // Place each illustration before the first following text in its column.
            let mut slots: Vec<Vec<Block>> = vec![Vec::new(); texts.len() + 1];
            for (figure, column) in figures {
                let top = figure.sources[0].top;
                let slot = texts
                    .iter()
                    .position(|(text, text_column)| {
                        text.sources[0].top >= top
                            && (column == 0 || *text_column == 0 || *text_column == column)
                    })
                    .or_else(|| {
                        (column != 0)
                            .then(|| texts.iter().rposition(|(_, c)| *c == column))
                            .flatten()
                            .map(|index| index + 1)
                    })
                    .unwrap_or(texts.len());
                slots[slot].push(figure);
            }
            for ((text, _), slot) in texts.into_iter().zip(&mut slots) {
                blocks.append(slot);
                blocks.push(text);
            }
            if let Some(slot) = slots.last_mut() {
                blocks.append(slot);
            }
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
        if layout_pages > 0 {
            warnings.push(format!("{layout_pages} source pages are shown as original images in Book; use Document for selectable text on those pages."));
        }
        Ok(Conversion {
            blocks,
            page_labels,
            fallback_pages,
            warnings,
            illustrations,
            placements,
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

fn numbered_entry(text: &str) -> bool {
    let Some((prefix, rest)) = text.split_once('#') else {
        return false;
    };
    let prefix = prefix.trim();
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    !prefix.is_empty()
        && prefix.len() <= 20
        && prefix
            .chars()
            .all(|c| c.is_alphabetic() || c.is_whitespace())
        && (1..=5).contains(&digits)
        && rest[digits..].starts_with([' ', ':', '.', '—', '–'])
}

fn centered(line: &Line) -> bool {
    ((line.left + line.right) * 0.5 - 0.5).abs() < 0.09
}

fn right_aligned(line: &Line, measure: (f32, f32)) -> bool {
    let width = measure.1 - measure.0;
    width > 0.1 && line.left > measure.0 + width * 0.55 && line.right >= measure.1 - 0.08
}

fn line_layout(
    line: &Line,
    measure: (f32, f32),
    heading: bool,
    sparse_page: bool,
    single_column: bool,
    neighbors: (Option<&Line>, Option<&Line>),
    body_height: f32,
) -> BlockLayout {
    let (previous, next) = neighbors;
    let width = (measure.1 - measure.0).max(0.1);
    let indent = ((line.left - measure.0) / width).clamp(0.0, 0.2);
    if is_list(&line.text) || numbered_entry(&line.text) {
        return BlockLayout::List { indent };
    }
    if !single_column {
        return BlockLayout::Flow;
    }
    let short = line.text.chars().count() <= 80 && line.right - line.left < width * 0.78;
    let interior = line.left > measure.0 + width * 0.08 && line.right < measure.1 - width * 0.08;
    let centered_in_measure =
        ((line.left + line.right - measure.0 - measure.1) * 0.5).abs() < width * 0.09;
    let separated = previous
        .is_none_or(|prev| line.source.top - prev.source.bottom > body_height * 1.5)
        || next.is_none_or(|next| next.source.top - line.source.bottom > body_height * 1.5);
    let center_neighbor = previous.is_some_and(centered) || next.is_some_and(centered);
    if centered(line)
        && (heading
            || (short
                && interior
                && centered_in_measure
                && (sparse_page || separated || center_neighbor)))
    {
        return BlockLayout::Centered;
    }
    if short && right_aligned(line, measure) {
        return BlockLayout::Right;
    }
    if !sparse_page && line.left > measure.0 + width * 0.1 && line.right < measure.1 - 0.04 {
        return BlockLayout::Inset { indent };
    }
    BlockLayout::Flow
}

fn is_contents_title(text: &str) -> bool {
    matches!(
        text.trim().to_ascii_lowercase().as_str(),
        "contents" | "table of contents" | "içindekiler" | "sommaire" | "inhalt"
    )
}

fn has_page_range(text: &str) -> bool {
    text.split_whitespace().any(|word| {
        let word = word.trim_matches(|c: char| !c.is_ascii_digit() && c != '-');
        word.split_once('-').is_some_and(|(start, end)| {
            !start.is_empty()
                && !end.is_empty()
                && start.bytes().all(|c| c.is_ascii_digit())
                && end.bytes().all(|c| c.is_ascii_digit())
        })
    })
}

fn append_styles(target: &mut Vec<TextStyleRun>, source: &[TextStyleRun], offset: usize) {
    for style in source {
        let start = offset + style.start;
        let end = offset + style.end;
        if let Some(last) = target.last_mut()
            && last.end == start
            && last.bold == style.bold
            && last.italic == style.italic
        {
            last.end = end;
        } else {
            target.push(TextStyleRun {
                start,
                end,
                bold: style.bold,
                italic: style.italic,
            });
        }
    }
}

fn append_links(target: &mut Vec<TextLink>, source: &[TextLink], offset: usize) {
    for link in source {
        target.push(TextLink {
            start: offset + link.start,
            end: offset + link.end,
            href: link.href.clone(),
        });
    }
}

/// A TOC row is one selectable logical item with a separately positioned folio.
/// Only pages with repeated right-edge folios (or an explicit Contents title)
/// take this path; ordinary prose is never split merely for being short.
fn toc_blocks(
    page: &[Line],
    columns: &[u8],
    measure: (f32, f32),
    body_size: f32,
    body_height: f32,
) -> Option<Vec<(Block, u8)>> {
    let pairs: Vec<(usize, usize)> = (1..page.len())
        .filter_map(|number| {
            let title = number - 1;
            let a = &page[title];
            let b = &page[number];
            let same_row = ((a.source.top + a.source.bottom) - (b.source.top + b.source.bottom))
                .abs()
                < a.height.max(b.height);
            (same_row
                && columns[title] == columns[number]
                && b.left > 0.62
                && b.left > a.right + 0.04
                && a.text.chars().count() > 2
                && page_number_value(&b.text).is_some())
            .then_some((title, number))
        })
        .collect();
    let has_title = page.iter().any(|line| {
        matches!(
            line.text.trim().to_ascii_lowercase().as_str(),
            "contents" | "table of contents" | "içindekiler" | "sommaire" | "inhalt"
        )
    });
    if pairs.len() < if has_title { 2 } else { 3 } {
        return None;
    }
    let numbers: HashSet<_> = pairs.iter().map(|(_, number)| *number).collect();
    let paired: HashMap<_, _> = pairs.into_iter().collect();
    let mut result = Vec::new();
    for (index, line) in page.iter().enumerate() {
        if numbers.contains(&index) {
            continue;
        }
        let ratio = if body_size > 0.0 && line.font_size > 0.0 {
            line.font_size / body_size
        } else {
            line.height / body_height
        };
        let title = matches!(
            line.text.trim().to_ascii_lowercase().as_str(),
            "contents" | "table of contents" | "içindekiler" | "sommaire" | "inhalt"
        );
        let mut block = Block {
            id: format!(
                "pdf-b{VERSION}-p{:06}-c{:07}",
                line.source.page, line.source.start
            ),
            text: line.text.clone(),
            heading: title || ratio > 1.3,
            layout: if title || (ratio > 1.3 && centered(line)) {
                BlockLayout::Centered
            } else {
                BlockLayout::Flow
            },
            size_ratio: if title || ratio > 1.3 {
                ratio.clamp(1.1, 2.0)
            } else {
                1.0
            },
            top_gap: 0.0,
            styles: line.styles.clone(),
            links: line.links.clone(),
            sources: vec![line.source.clone()],
        };
        if let Some(&number) = paired.get(&index) {
            let folio = &page[number];
            let number_start = block.text.len() + 1;
            block.text.push(' ');
            block.text.push_str(&folio.text);
            append_styles(&mut block.styles, &folio.styles, number_start);
            append_links(&mut block.links, &folio.links, number_start);
            block.sources.push(folio.source.clone());
            block.heading = false;
            block.size_ratio = 1.0;
            block.layout = BlockLayout::Toc {
                number_start,
                indent: ((line.left - measure.0) / (measure.1 - measure.0).max(0.1))
                    .clamp(0.0, 0.2),
            };
        }
        result.push((block, columns[index]));
    }
    Some(result)
}
/// The book's usual text block: the median horizontal extent of text-filled pages.
fn text_measure(pages: &[Vec<Line>]) -> (f32, f32) {
    let mut lefts = Vec::new();
    let mut rights = Vec::new();
    for page in pages {
        let body: Vec<_> = page
            .iter()
            .filter(|l| l.text.chars().count() > 12)
            .collect();
        if body.len() >= 5 {
            lefts.push(body.iter().map(|l| l.left).fold(1.0, f32::min));
            rights.push(body.iter().map(|l| l.right).fold(0.0, f32::max));
        }
    }
    if lefts.is_empty() {
        return (0.0, 1.0);
    }
    lefts.sort_by(f32::total_cmp);
    rights.sort_by(f32::total_cmp);
    let (left, right) = (lefts[lefts.len() / 2], rights[rights.len() / 2]);
    if right - left > 0.1 {
        (left, right)
    } else {
        (0.0, 1.0)
    }
}

/// Removes thumb-index letters printed in the margin beside the text block.
fn drop_margin_marks(page: &mut Vec<Line>) -> usize {
    let body = page.iter().filter(|l| l.text.chars().count() > 12);
    let (left, right) = body.fold((1.0_f32, 0.0_f32), |(left, right), line| {
        (left.min(line.left), right.max(line.right))
    });
    if left >= right {
        return 0;
    }
    let before = page.len();
    page.retain(|line| {
        let outside = line.right < left - 0.01 || line.left > right + 0.01;
        let mark = line.text.chars().count() <= 2 && line.text.chars().all(char::is_uppercase);
        !(outside && mark)
    });
    before - page.len()
}

/// A vertical gap that separates two columns of body lines on this page.
fn gutter(lines: &[Line]) -> Option<f32> {
    let body: Vec<_> = lines
        .iter()
        .filter(|l| l.text.chars().count() > 12)
        .collect();
    if body.len() < 8 {
        return None;
    }
    let mut best = 0;
    let mut candidates = Vec::new();
    for step in 0..=100 {
        let x = 0.25 + step as f32 * 0.005;
        let (mut left, mut right, mut crossing) = (0, 0, 0);
        for line in &body {
            if line.right <= x {
                left += 1;
            } else if line.left >= x {
                right += 1;
            } else {
                crossing += 1;
            }
        }
        if crossing * 10 > body.len() || left < 3 || right < 3 {
            continue;
        }
        let balanced = left.min(right);
        if balanced > best {
            best = balanced;
            candidates.clear();
        }
        if balanced == best {
            candidates.push(x);
        }
    }
    candidates.get(candidates.len() / 2).copied()
}

fn looks_like_table(lines: &[Line]) -> bool {
    let mut row_sizes = Vec::new();
    let mut current = 0;
    let mut mid = 0.0;
    let mut height = 0.0_f32;
    for line in lines {
        let next_mid = (line.source.top + line.source.bottom) * 0.5;
        if current > 0 && (next_mid - mid).abs() > height.max(line.height) * 0.5 {
            row_sizes.push(current);
            current = 0;
        }
        current += 1;
        mid = next_mid;
        height = line.height;
    }
    if current > 0 {
        row_sizes.push(current);
    }
    row_sizes.iter().filter(|&&count| count >= 3).count() >= 3
        && row_sizes.iter().filter(|&&count| count >= 3).sum::<usize>() * 2 >= lines.len()
}

/// Groups lines on one baseline and orders each row from left to right.
fn rows(lines: Vec<Line>, column: u8, ordered: &mut Vec<(Line, u8)>) {
    let mut row: Vec<Line> = Vec::new();
    for line in lines {
        if row.first().is_some_and(|first| {
            let middle = (first.source.top + first.source.bottom) * 0.5;
            let next = (line.source.top + line.source.bottom) * 0.5;
            (next - middle).abs() > first.height.max(line.height) * 0.5
        }) {
            row.sort_by(|a, b| a.left.total_cmp(&b.left));
            ordered.extend(row.drain(..).map(|line| (line, column)));
        }
        row.push(line);
    }
    row.sort_by(|a, b| a.left.total_cmp(&b.left));
    ordered.extend(row.into_iter().map(|line| (line, column)));
}

/// Reading order for lines sorted by top: the left column, then the right column.
/// Lines crossing the gutter span both columns and separate stacked sections.
/// Column 0 is the whole page; columns 1 and 2 are the left and right columns.
fn reading_order(lines: Vec<Line>, gutter: Option<f32>) -> Vec<(Line, u8)> {
    let mut ordered = Vec::with_capacity(lines.len());
    let Some(x) = gutter else {
        rows(lines, 0, &mut ordered);
        return ordered;
    };
    let (mut left, mut right) = (Vec::new(), Vec::new());
    for line in lines {
        if line.right <= x {
            left.push(line);
        } else if line.left >= x {
            right.push(line);
        } else {
            rows(std::mem::take(&mut left), 1, &mut ordered);
            rows(std::mem::take(&mut right), 2, &mut ordered);
            ordered.push((line, 0));
        }
    }
    rows(left, 1, &mut ordered);
    rows(right, 2, &mut ordered);
    ordered
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

/// A printed folio is page furniture only when its geometry and the PDF's
/// page label or neighbouring folios support that interpretation.
fn printed_page_numbers(
    pages: &[Vec<Line>],
    pdf_labels: &HashMap<u32, String>,
) -> HashMap<u32, (usize, String)> {
    let candidates: Vec<Option<(&Line, u32)>> = pages
        .iter()
        .map(|page| {
            page.iter().find_map(|line| {
                let centered = ((line.left + line.right) * 0.5 - 0.5).abs() < 0.13;
                (line.source.top >= 0.84 && centered && line.right - line.left < 0.18)
                    .then(|| page_number_value(&line.text).map(|value| (line, value)))
                    .flatten()
            })
        })
        .collect();
    let mut result = HashMap::new();
    for (page, candidate) in candidates.iter().enumerate() {
        let Some((line, value)) = candidate else {
            continue;
        };
        let labelled = pdf_labels
            .get(&(page as u32))
            .is_some_and(|label| label.trim().eq_ignore_ascii_case(line.text.trim()));
        let sequential = page
            .checked_sub(1)
            .and_then(|p| candidates[p])
            .is_some_and(|(_, previous)| previous.checked_add(1) == Some(*value))
            || candidates
                .get(page + 1)
                .copied()
                .flatten()
                .is_some_and(|(_, next)| value.checked_add(1) == Some(next));
        if labelled || sequential {
            result.insert(page as u32, (line.source.start, line.text.clone()));
        }
    }
    result
}

fn page_number_value(text: &str) -> Option<u32> {
    let text = text.trim();
    if let Ok(number) = text.parse::<u32>() {
        return (number > 0 && number <= 100_000).then_some(number);
    }
    if text.is_empty() || text.len() > 12 || !text.bytes().all(|b| b"ivxlcdmIVXLCDM".contains(&b)) {
        return None;
    }
    let upper = text.to_ascii_uppercase();
    let value = |c| match c {
        b'I' => 1,
        b'V' => 5,
        b'X' => 10,
        b'L' => 50,
        b'C' => 100,
        b'D' => 500,
        b'M' => 1000,
        _ => 0,
    };
    let mut total = 0;
    let mut previous = 0;
    for c in upper.bytes().rev() {
        let current = value(c);
        total += if current < previous {
            -current
        } else {
            current
        };
        previous = current;
    }
    if !(1..=3999).contains(&total) {
        return None;
    }
    let mut number = total;
    let mut canonical = String::new();
    for (amount, letters) in [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ] {
        while number >= amount {
            canonical.push_str(letters);
            number -= amount;
        }
    }
    (canonical == upper).then_some(total as u32)
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

fn lines(
    page: u32,
    layer: &TextLayer,
    illustrations: &[Rect],
    source_links: &[SourceLink],
) -> Result<Vec<Line>, String> {
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
                let styles = text_styles(layer, from, from + text.len());
                let links = source_links
                    .iter()
                    .filter_map(|link| {
                        let start = link.start.max(from);
                        let end = link.end.min(from + text.len());
                        (start < end).then(|| TextLink {
                            start: start - from,
                            end: end - from,
                            href: link.href.clone(),
                        })
                    })
                    .collect();
                let mut sizes: Vec<f32> =
                    layer.glyphs[layer.glyphs.partition_point(|g| g.end <= from)
                        ..layer
                            .glyphs
                            .partition_point(|g| g.start < from + text.len())]
                        .iter()
                        .filter_map(|g| g.style.map(|s| s.size))
                        .filter(|size| size.is_finite() && *size > 0.0)
                        .collect();
                sizes.sort_by(f32::total_cmp);
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
                    font_size: sizes.get(sizes.len() / 2).copied().unwrap_or(0.0),
                    styles,
                    links,
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

fn text_styles(layer: &TextLayer, from: usize, to: usize) -> Vec<TextStyleRun> {
    let first = layer.glyphs.partition_point(|g| g.end <= from);
    let last = layer.glyphs.partition_point(|g| g.start < to);
    let mut runs: Vec<TextStyleRun> = Vec::new();
    for glyph in &layer.glyphs[first..last] {
        let Some(style) = glyph.style.filter(|s| s.bold || s.italic) else {
            continue;
        };
        let start = glyph.start.max(from) - from;
        let end = glyph.end.min(to) - from;
        if let Some(last) = runs.last_mut()
            && last.end == start
            && last.bold == style.bold
            && last.italic == style.italic
        {
            last.end = end;
        } else {
            runs.push(TextStyleRun {
                start,
                end,
                bold: style.bold,
                italic: style.italic,
            });
        }
    }
    runs
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
                    style: None,
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
        let result = builder.finish(|| false).unwrap();
        assert!(result.fallback_pages.contains(&0));
        assert!(result.illustrations.contains_key(&result.blocks[0].id));
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
        let result = builder.finish(|| false).unwrap();
        assert!(result.fallback_pages.contains(&0));
        assert!(result.illustrations.contains_key(&result.blocks[0].id));
        let mut input = layer(&[("Meaningful text", 0.1, 0.2)]);
        input.glyphs[0].bounds = None;
        let mut invalid = Builder::default();
        invalid.push(0, input).unwrap();
        let result = invalid.finish(|| false).unwrap();
        assert!(result.fallback_pages.contains(&0));
        assert!(result.illustrations.contains_key(&result.blocks[0].id));
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
    fn column(texts: &[&str], left: f32, tops: &[f32]) -> Vec<(String, f32, f32)> {
        texts
            .iter()
            .zip(tops)
            .map(|(text, top)| ((*text).to_owned(), left, *top))
            .collect()
    }

    fn page(rows: &[(String, f32, f32)]) -> TextLayer {
        let rows: Vec<_> = rows
            .iter()
            .map(|(t, l, top)| (t.as_str(), *l, *top))
            .collect();
        layer(&rows)
    }

    #[test]
    fn two_column_pages_read_the_left_column_before_the_right() {
        let tops = [0.1, 0.12, 0.14, 0.16, 0.18];
        let left = [
            "la0 words fill this column line",
            "la1 words fill this column line",
            "la2 words fill this column line",
            "la3 words fill this column line",
            "la4 words fill this column line",
        ];
        let right = [
            "rb0 words fill this column line",
            "rb1 words fill this column line",
            "rb2 words fill this column line",
            "rb3 words fill this column line",
            "rb4 words fill this column line",
        ];
        // Interleave rows the way a naive top-to-bottom sort would see them.
        let mut rows = Vec::new();
        for (l, r) in column(&left, 0.1, &tops)
            .into_iter()
            .zip(column(&right, 0.52, &tops))
        {
            rows.push(l);
            rows.push(r);
        }
        let mut builder = Builder::default();
        builder.push(0, page(&rows)).unwrap();
        let result = builder.finish(|| false).unwrap();
        assert_eq!(result.blocks.len(), 1);
        let expected: Vec<_> = left.iter().chain(&right).copied().collect();
        assert_eq!(result.blocks[0].text, expected.join(" "));
    }

    #[test]
    fn column_entries_split_on_extra_leading() {
        let text = ["entry words fill this column line"; 5];
        let mut rows = column(&text, 0.1, &[0.1, 0.12, 0.146, 0.166, 0.186]);
        rows.extend(column(&text, 0.52, &[0.1, 0.12, 0.14, 0.166, 0.186]));
        let mut builder = Builder::default();
        builder.push(0, page(&rows)).unwrap();
        let result = builder.finish(|| false).unwrap();
        let lines: Vec<_> = result.blocks.iter().map(|b| b.sources.len()).collect();
        // Left entry, an entry continuing into the right column, then a right entry.
        assert_eq!(lines, [2, 6, 2]);
    }

    #[test]
    fn column_illustrations_stay_in_their_column() {
        let tops = [0.1, 0.12, 0.14, 0.16, 0.18];
        let mut left = column(&["left words fill this column line"; 5], 0.1, &tops);
        left[4].0 = "left column ends here.".into();
        let right = column(&["Right words fill this column line"; 5], 0.52, &tops);
        let mut builder = Builder::default();
        let figure = Rect {
            left: 0.1,
            right: 0.36,
            top: 0.22,
            bottom: 0.4,
        };
        builder.illustrations(0, vec![figure]);
        builder.push(0, page(&[left, right].concat())).unwrap();
        let result = builder.finish(|| false).unwrap();
        assert_eq!(result.blocks.len(), 3);
        assert!(result.blocks[0].text.starts_with("left"));
        assert!(result.illustrations.contains_key(&result.blocks[1].id));
        assert!(result.blocks[2].text.starts_with("Right"));
    }

    #[test]
    fn illustrations_keep_their_size_and_position_in_the_text_measure() {
        let text = ["A body line that spans the whole text block"; 6];
        let mut builder = Builder::default();
        // Text block from 0.1 to about 0.44; a figure in its right half.
        let figure = Rect {
            left: 0.27,
            right: 0.44,
            top: 0.5,
            bottom: 0.7,
        };
        builder.illustrations(0, vec![figure]);
        builder
            .push(
                0,
                page(&column(&text, 0.1, &[0.1, 0.12, 0.14, 0.16, 0.18, 0.2])),
            )
            .unwrap();
        let result = builder.finish(|| false).unwrap();
        let id = result.illustrations.keys().next().unwrap();
        let placement = result.placements[id];
        assert!((placement.width - 0.5).abs() < 0.02, "{placement:?}");
        assert!((placement.offset - 0.5).abs() < 0.02, "{placement:?}");
    }

    #[test]
    fn thumb_index_letters_are_dropped_but_margin_numbers_are_kept() {
        let mut builder = Builder::default();
        builder
            .push(
                0,
                layer(&[
                    ("A contents entry that is long", 0.1, 0.2),
                    ("H", 0.02, 0.3),
                    ("Another contents entry here", 0.1, 0.4),
                    ("12", 0.9, 0.4),
                ]),
            )
            .unwrap();
        let text: Vec<_> = builder
            .finish(|| false)
            .unwrap()
            .blocks
            .into_iter()
            .map(|b| b.text)
            .collect();
        assert!(!text.iter().any(|t| t.split(' ').any(|w| w == "H")));
        assert!(text.iter().any(|t| t.contains("12")));
    }

    #[test]
    fn consecutive_roman_folios_move_from_body_to_page_labels() {
        let mut builder = Builder::default();
        for (page, folio) in ["i", "ii", "iii"].into_iter().enumerate() {
            builder
                .push(
                    page as u32,
                    layer(&[
                        ("The chapter body remains selectable.", 0.1, 0.2),
                        (folio, 0.49, 0.92),
                    ]),
                )
                .unwrap();
        }
        let result = builder.finish(|| false).unwrap();
        assert_eq!(result.blocks.len(), 3);
        for (page, folio) in ["i", "ii", "iii"].into_iter().enumerate() {
            assert_eq!(
                result.page_labels.get(&(page as u32)).map(String::as_str),
                Some(folio)
            );
            assert_eq!(
                result.blocks[page].text,
                "The chapter body remains selectable."
            );
        }
        assert_eq!(page_number_value("IC"), None);
    }

    #[test]
    fn explicit_page_labels_leave_unlabelled_front_matter_blank() {
        let mut builder = Builder::default();
        builder.page_label(2, Some("1"));
        for page in 0..3 {
            builder
                .push(page, layer(&[("A readable page of prose.", 0.1, 0.2)]))
                .unwrap();
        }
        let result = builder.finish(|| false).unwrap();
        assert_eq!(result.page_labels.get(&0).map(String::as_str), Some(""));
        assert_eq!(result.page_labels.get(&1).map(String::as_str), Some(""));
        assert_eq!(result.page_labels.get(&2).map(String::as_str), Some("1"));
    }

    #[test]
    fn complex_contents_and_its_continuation_use_source_pages() {
        let mut builder = Builder::default();
        builder
            .push(
                0,
                layer(&[
                    ("Contents", 0.46, 0.16),
                    ("Fantasy 3-20", 0.15, 0.30),
                    ("Drawing and Fantasy 21-52", 0.15, 0.34),
                    ("Making Things 67-84", 0.15, 0.38),
                ]),
            )
            .unwrap();
        builder
            .push(
                1,
                layer(&[
                    ("Enactment 137-158", 0.15, 0.20),
                    ("Play Therapy 159-179", 0.15, 0.24),
                    ("The Therapy Process 181-204", 0.15, 0.28),
                ]),
            )
            .unwrap();
        builder
            .push(2, layer(&[("The next chapter begins here.", 0.1, 0.2)]))
            .unwrap();
        let result = builder.finish(|| false).unwrap();
        assert!(result.fallback_pages.contains(&0));
        assert!(result.fallback_pages.contains(&1));
        assert!(!result.fallback_pages.contains(&2));
    }

    #[test]
    fn centered_foreword_and_right_signature_keep_alignment() {
        let mut builder = Builder::default();
        builder
            .push(
                0,
                layer(&[
                    ("Foreword", 0.46, 0.16),
                    (
                        "The opening paragraph has enough words to fill the body measure.",
                        0.10,
                        0.25,
                    ),
                    (
                        "The second paragraph continues the discussion in ordinary prose.",
                        0.10,
                        0.31,
                    ),
                    (
                        "The third paragraph fills the same width with normal text.",
                        0.10,
                        0.37,
                    ),
                    (
                        "The fourth paragraph fills the same width with normal text.",
                        0.10,
                        0.43,
                    ),
                    (
                        "The fifth paragraph fills the same width with normal text.",
                        0.10,
                        0.49,
                    ),
                    ("Barry Stevens", 0.72, 0.60),
                    ("June 1978", 0.76, 0.63),
                ]),
            )
            .unwrap();
        let result = builder.finish(|| false).unwrap();
        assert_eq!(result.blocks[0].layout, BlockLayout::Centered);
        assert!(result.blocks[0].top_gap > 0.0);
        assert!(
            result
                .blocks
                .iter()
                .any(|b| b.text == "Barry Stevens" && b.layout == BlockLayout::Right)
        );
        assert!(
            result
                .blocks
                .iter()
                .any(|b| b.text == "June 1978" && b.layout == BlockLayout::Right)
        );
    }

    #[test]
    fn sparse_centered_imprint_keeps_every_line_centered() {
        let lines = [
            "The Library",
            "of the",
            "CLAREMONT",
            "SCHOOL OF THEOLOGY",
            "1325 North College Avenue",
            "Claremont, CA 91711-3199",
            "1/800-626-7820",
        ];
        let rows: Vec<_> = lines
            .iter()
            .enumerate()
            .map(|(index, text)| {
                (
                    *text,
                    0.5 - text.chars().count() as f32 * 0.004,
                    0.2 + index as f32 * 0.05,
                )
            })
            .collect();
        let mut builder = Builder::default();
        builder.push(0, layer(&rows)).unwrap();
        let result = builder.finish(|| false).unwrap();
        assert_eq!(result.blocks.len(), lines.len());
        assert!(
            result
                .blocks
                .iter()
                .all(|block| block.layout == BlockLayout::Centered)
        );
    }

    #[test]
    fn centered_and_right_lines_inside_prose_do_not_merge_with_body() {
        let body = "Ordinary prose stays at the left edge of the readable text measure and continues all the way across this line.";
        let center = "A centered interlude";
        let rows = [
            (body, 0.10, 0.10),
            (body, 0.10, 0.14),
            (body, 0.10, 0.18),
            (center, 0.5 - center.len() as f32 * 0.004, 0.25),
            (body, 0.10, 0.32),
            (body, 0.10, 0.36),
            (
                "A right-hand note",
                0.10 + body.len() as f32 * 0.008 - 17.0 * 0.008,
                0.44,
            ),
            (body, 0.10, 0.51),
            ("An indented note", 0.22, 0.58),
            (body, 0.10, 0.65),
            (body, 0.10, 0.69),
        ];
        let mut builder = Builder::default();
        builder.push(0, layer(&rows)).unwrap();
        let result = builder.finish(|| false).unwrap();
        assert!(
            result
                .blocks
                .iter()
                .any(|block| { block.text == center && block.layout == BlockLayout::Centered })
        );
        assert!(result.blocks.iter().any(|block| {
            block.text == "A right-hand note" && block.layout == BlockLayout::Right
        }));
        assert!(result.blocks.iter().any(|block| {
            block.text == "An indented note"
                && matches!(block.layout, BlockLayout::Inset { indent } if indent > 0.1)
        }));
        assert!(
            result
                .blocks
                .iter()
                .any(|block| { block.text.starts_with(body) && block.layout == BlockLayout::Flow })
        );
    }

    #[test]
    fn sparse_title_page_keeps_left_aligned_subtitle_and_author() {
        let title = "Windows to Our Children";
        let mut builder = Builder::default();
        builder
            .push(
                0,
                layer(&[
                    (title, 0.5 - title.len() as f32 * 0.004, 0.2),
                    ("A gestalt therapy approach to children", 0.10, 0.42),
                    ("Violet Oaklander PhD", 0.10, 0.62),
                ]),
            )
            .unwrap();
        let result = builder.finish(|| false).unwrap();
        assert_eq!(result.blocks[0].layout, BlockLayout::Centered);
        assert_eq!(result.blocks[1].layout, BlockLayout::Flow);
        assert_eq!(result.blocks[2].layout, BlockLayout::Flow);
    }

    #[test]
    fn contents_rows_keep_indented_titles_and_separate_right_folios() {
        let mut builder = Builder::default();
        builder
            .push(
                0,
                layer(&[
                    ("Contents", 0.47, 0.12),
                    ("Introduction", 0.14, 0.24),
                    ("vii", 0.84, 0.24),
                    ("Chapter One", 0.18, 0.30),
                    ("1", 0.85, 0.30),
                    ("Chapter Two", 0.18, 0.36),
                    ("25", 0.84, 0.36),
                ]),
            )
            .unwrap();
        let result = builder.finish(|| false).unwrap();
        assert_eq!(result.blocks.len(), 4);
        assert_eq!(result.blocks[0].layout, BlockLayout::Centered);
        for block in &result.blocks[1..] {
            let BlockLayout::Toc { number_start, .. } = block.layout else {
                panic!("TOC row lost its two-column structure: {block:?}");
            };
            assert!(
                block.text[number_start..]
                    .chars()
                    .all(|c| c.is_ascii_digit() || "ivxlcdm".contains(c))
            );
            assert_eq!(block.sources.len(), 2);
        }
        assert!(matches!(result.blocks[2].layout, BlockLayout::Toc { indent, .. } if indent > 0.0));
    }

    #[test]
    fn repeated_numbered_entries_stay_on_distinct_lines_and_keep_inline_style() {
        let mut input = layer(&[
            ("Myth #38 The first claim", 0.1, 0.2),
            ("Myth #39 The second claim", 0.1, 0.24),
        ]);
        for glyph in &mut input.glyphs {
            if glyph.start < 4 {
                glyph.style = Some(crate::GlyphStyle {
                    size: 12.0,
                    bold: true,
                    italic: false,
                });
            }
        }
        let mut builder = Builder::default();
        builder.push(0, input).unwrap();
        let result = builder.finish(|| false).unwrap();
        assert_eq!(result.blocks.len(), 2);
        assert!(
            result
                .blocks
                .iter()
                .all(|b| matches!(b.layout, BlockLayout::List { .. }))
        );
        assert_eq!(
            result.blocks[0].styles[0],
            TextStyleRun {
                start: 0,
                end: 4,
                bold: true,
                italic: false
            }
        );
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
