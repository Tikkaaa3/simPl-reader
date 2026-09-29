//! Source-preserving paper geometry over the existing virtual text index.
use crate::virtual_reader::HeightIndex;
use std::ops::Range;

pub const GAP: f32 = 24.0;
pub const TOP: f32 = 42.0;
pub const BOTTOM: f32 = 46.0;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Page {
    pub number: u32,
    #[serde(default)]
    pub label: String,
    pub rows: Range<usize>,
    pub content: Range<f32>,
    pub top: f32,
    pub height: f32,
}

pub fn layout(pages: impl Iterator<Item = u32>, index: &HeightIndex, width: f32) -> Vec<Page> {
    let mut result: Vec<Page> = Vec::new();
    for (row, number) in pages.enumerate() {
        if let Some(last) = result.last_mut()
            && last.number == number
        {
            last.rows.end = row + 1;
        } else {
            result.push(Page {
                number,
                label: (number + 1).to_string(),
                rows: row..row + 1,
                content: 0.0..0.0,
                top: 0.0,
                height: 0.0,
            });
        }
    }
    let mut top = GAP;
    for page in &mut result {
        page.top = top;
        page.content = index.start(page.rows.start)..index.start(page.rows.end);
        page.height = (index.start(page.rows.end) - index.start(page.rows.start) + TOP + BOTTOM)
            .max(width * 1.414);
        top += page.height + GAP;
    }
    result
}

/// Text line grid in the unchanged source paragraph. Images have no line grid.
#[derive(Clone, Copy)]
pub struct Lines {
    pub height: f32,
    pub top: f32,
}

/// Split long paragraphs at line boundaries, retaining their source row identities.
pub fn reflow(
    index: &HeightIndex,
    width: f32,
    chapter_starts: &[usize],
    lines: &[Option<Lines>],
) -> Vec<Page> {
    let capacity = (width * 1.414 - TOP - BOTTOM).max(128.0);
    let mut pages = Vec::new();
    let mut start = 0.0;
    let mut cursor = 0.0;
    let finish = |pages: &mut Vec<Page>, start: &mut f32, end: f32| {
        if end <= *start {
            return;
        }
        let top = pages.last().map_or(GAP, |p| p.top + p.height + GAP);
        pages.push(Page {
            number: pages.len() as u32,
            label: (pages.len() + 1).to_string(),
            rows: index.window(*start, end - *start, 0.0),
            content: *start..end,
            top,
            height: capacity.max(end - *start) + TOP + BOTTOM,
        });
        *start = end;
    };
    for row in 0..index.len() {
        if chapter_starts.binary_search(&row).is_ok() {
            finish(&mut pages, &mut start, cursor);
        }
        let end = index.start(row + 1);
        while cursor < end {
            let available = capacity - (cursor - start);
            if end - cursor <= available + 0.01 {
                cursor = end;
                break;
            }
            let grid = lines.get(row).copied().flatten();
            // Keep ordinary paragraphs and images together when they fit a sheet.
            if index.height(row) <= capacity || grid.is_none() {
                if cursor > start {
                    finish(&mut pages, &mut start, cursor);
                } else {
                    cursor = end;
                    finish(&mut pages, &mut start, cursor);
                }
                continue;
            }
            let grid = grid.unwrap();
            let origin = index.start(row) + grid.top;
            let cut = origin + ((cursor + available - origin) / grid.height).floor() * grid.height;
            if cut > cursor + 0.01 {
                cursor = cut.min(end);
            }
            finish(&mut pages, &mut start, cursor);
        }
    }
    finish(&mut pages, &mut start, cursor);
    pages
}

pub fn current(pages: &[Page], offset: f32) -> Option<&Page> {
    let end = pages.partition_point(|page| page.top <= offset + 1.0);
    pages.get(end.saturating_sub(1))
}

/// The sheet occupying the most visible space; ties prefer the leading sheet.
/// Unlike content anchors this accounts for a short final sheet below the viewport top.
#[cfg(test)]
pub fn visible(pages: &[Page], offset: f32, viewport: f32) -> Option<&Page> {
    let mut best = None;
    let mut largest = 0.0;
    for page in pages {
        let overlap =
            ((page.top + page.height).min(offset + viewport) - page.top.max(offset)).max(0.0);
        if overlap > largest + 0.1 {
            largest = overlap;
            best = Some(page);
        }
    }
    best.or_else(|| current(pages, offset))
}

/// Leave enough trailing space to align even a short final sheet with the viewport top.
#[cfg(test)]
pub fn scroll_total(pages: &[Page], viewport: f32) -> f32 {
    total(pages).max(pages.last().map_or(0.0, |p| p.top + viewport))
}

#[cfg(test)]
pub fn total(pages: &[Page]) -> f32 {
    pages
        .last()
        .map_or(0.0, |page| page.top + page.height + GAP)
}

pub fn content_offset(pages: &[Page], _index: &HeightIndex, offset: f32) -> f32 {
    let Some(page) = current(pages, offset) else {
        return offset;
    };
    let start = page.content.start;
    let end = page.content.end;
    (start + (offset - page.top - TOP).max(0.0))
        .min(end - 0.01)
        .max(start)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_page_matches_linear_lookup_at_page_edges_and_gaps() {
        let index = HeightIndex::new(vec![20.0; 1000]);
        let pages = layout(0..1000, &index, 400.0);
        assert!(current(&[], 0.0).is_none());
        for page in &pages {
            for offset in [
                page.top - 2.0,
                page.top - 1.0,
                page.top,
                page.top + page.height,
                page.top + page.height + GAP,
            ] {
                let expected = pages
                    .iter()
                    .rev()
                    .find(|page| page.top <= offset + 1.0)
                    .or_else(|| pages.first());
                assert_eq!(
                    current(&pages, offset).map(|p| p.number),
                    expected.map(|p| p.number)
                );
            }
        }
    }

    #[test]
    fn short_last_sheet_is_current_at_the_old_scroll_limit_and_can_align_at_top() {
        let index = HeightIndex::new(vec![100.0, 100.0]);
        let pages = layout([0, 1].into_iter(), &index, 400.0);
        let viewport = 800.0;
        let old_limit = total(&pages) - viewport;
        assert_eq!(current(&pages, old_limit).unwrap().number, 0);
        assert_eq!(visible(&pages, old_limit, viewport).unwrap().number, 1);
        let limit = scroll_total(&pages, viewport) - viewport;
        assert!((limit - pages[1].top).abs() < 0.01);
        assert_eq!(visible(&pages, limit, viewport).unwrap().number, 1);
        assert_eq!(visible(&pages, pages[0].top, viewport).unwrap().number, 0);
        // In a very tall window, back navigation can still align each requested sheet.
        assert_eq!(visible(&pages, pages[0].top, 2000.0).unwrap().number, 0);
        assert!((scroll_total(&pages, 2000.0) - 2000.0 - pages[1].top).abs() < 0.01);
    }

    #[test]
    fn long_paragraphs_split_on_lines_without_losing_content() {
        let index = HeightIndex::new(vec![200.0, 200.0, 40.0, 9000.0, 100.0]);
        let grids = vec![
            Some(Lines {
                height: 20.0,
                top: 0.0
            });
            5
        ];
        let pages = reflow(&index, 400.0, &[2], &grids);
        assert!(pages.len() > 20);
        assert_eq!(pages[0].rows, 0..2);
        assert_eq!(pages[1].rows.start, 2);
        assert_eq!(pages.first().unwrap().content.start, 0.0);
        assert_eq!(pages.last().unwrap().content.end, index.total());
        for pair in pages.windows(2) {
            assert_eq!(pair[0].content.end, pair[1].content.start);
        }
        for page in &pages {
            assert!(page.height <= 400.0 * 1.414 + 0.01);
            assert_eq!(page.content.end % 20.0, 0.0);
            assert_eq!(content_offset(&pages, &index, page.top), page.content.start);
        }
    }

    #[test]
    fn sheets_preserve_source_pages_and_expand_without_clipping() {
        let index = HeightIndex::new(vec![200.0, 400.0, 1200.0]);
        let pages = layout([0, 0, 1].into_iter(), &index, 600.0);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].rows, 0..2);
        assert!(pages[0].height >= 600.0 * 1.414);
        assert_eq!(pages[1].height, 1200.0 + TOP + BOTTOM);
        assert_eq!(content_offset(&pages, &index, pages[1].top), 600.0);
        assert_eq!(current(&pages, pages[1].top).unwrap().number, 1);
        assert!(content_offset(&pages, &index, pages[0].top + pages[0].height) < 600.0);
    }
}
