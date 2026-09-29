//! Separate illustrations from the scan backing an OCR text layer.
use crate::{Rect, RenderedPage, TextLayer};

pub(crate) fn scan_regions(raster: &RenderedPage, text: &TextLayer) -> Vec<Rect> {
    let w = raster.width as usize;
    let h = raster.height as usize;
    let mut ink = vec![false; w * h];
    for y in h / 50..h - h / 50 {
        for x in w / 50..w - w / 50 {
            let p = (y * w + x) * 4;
            let rgb = &raster.rgba[p..p + 3];
            ink[y * w + x] =
                (u32::from(rgb[0]) * 3 + u32::from(rgb[1]) * 6 + u32::from(rgb[2])) < 1250;
        }
    }
    // Remove recognized letters before looking for larger connected ink regions.
    for bounds in text.glyphs.iter().filter_map(|g| g.bounds) {
        let left = ((bounds.left * w as f32) as usize).saturating_sub(2).min(w);
        let right = ((bounds.right * w as f32).ceil() as usize + 2).min(w);
        let top = ((bounds.top * h as f32) as usize).saturating_sub(2).min(h);
        let bottom = ((bounds.bottom * h as f32).ceil() as usize + 2).min(h);
        for y in top..bottom {
            ink[y * w + left..y * w + right].fill(false);
        }
    }
    let cell = 8;
    let cols = w.div_ceil(cell);
    let rows = h.div_ceil(cell);
    let mut occupied = vec![false; cols * rows];
    for cy in 0..rows {
        for cx in 0..cols {
            let mut count = 0;
            for y in cy * cell..((cy + 1) * cell).min(h) {
                for x in cx * cell..((cx + 1) * cell).min(w) {
                    count += usize::from(ink[y * w + x]);
                }
            }
            occupied[cy * cols + cx] = count >= 4;
        }
    }
    let mut connected = occupied.clone();
    for y in 0..rows {
        for x in 0..cols {
            if occupied[y * cols + x] {
                for ny in y.saturating_sub(1)..=(y + 1).min(rows - 1) {
                    for nx in x.saturating_sub(1)..=(x + 1).min(cols - 1) {
                        connected[ny * cols + nx] = true;
                    }
                }
            }
        }
    }
    let mut regions = Vec::new();
    for start in 0..connected.len() {
        if !connected[start] {
            continue;
        }
        connected[start] = false;
        let mut queue = vec![start];
        let (mut left, mut right, mut top, mut bottom) = (cols, 0, rows, 0);
        while let Some(index) = queue.pop() {
            let (x, y) = (index % cols, index / cols);
            if occupied[index] {
                left = left.min(x);
                right = right.max(x + 1);
                top = top.min(y);
                bottom = bottom.max(y + 1);
            }
            for ny in y.saturating_sub(1)..=(y + 1).min(rows - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(cols - 1) {
                    let next = ny * cols + nx;
                    if connected[next] {
                        connected[next] = false;
                        queue.push(next);
                    }
                }
            }
        }
        if right <= left || bottom <= top {
            continue;
        }
        let (left, right, top, bottom) = (
            left * cell,
            (right * cell).min(w),
            top * cell,
            (bottom * cell).min(h),
        );
        let area = (right - left) * (bottom - top);
        if right - left < w / 8 || bottom - top < h / 12 || area < w * h / 60 {
            continue;
        }
        let dark: usize = (top..bottom)
            .map(|y| {
                ink[y * w + left..y * w + right]
                    .iter()
                    .filter(|p| **p)
                    .count()
            })
            .sum();
        if dark < 120 || dark * 40 < area {
            continue;
        }
        let (left, right, top, bottom) = precise_bounds(&ink, w, (left, right, top, bottom), cell);
        regions.push(Rect {
            left: left.saturating_sub(2) as f32 / w as f32,
            right: (right + 2).min(w) as f32 / w as f32,
            top: top.saturating_sub(2) as f32 / h as f32,
            bottom: (bottom + 2).min(h) as f32 / h as f32,
        });
    }
    regions.sort_by(|a, b| a.top.total_cmp(&b.top));
    regions
}

/// The connected-cell search finds illustrations quickly, but its 8-pixel
/// boundaries can absorb captions and page numbers. A dark rectangular frame
/// provides stronger evidence for the actual edges of a scanned picture.
fn precise_bounds(
    ink: &[bool],
    width: usize,
    (left, right, top, bottom): (usize, usize, usize, usize),
    cell: usize,
) -> (usize, usize, usize, usize) {
    let mut rows = vec![0_usize; bottom - top];
    let mut cols = vec![0_usize; right - left];
    for y in top..bottom {
        for x in left..right {
            if ink[y * width + x] {
                rows[y - top] += 1;
                cols[x - left] += 1;
            }
        }
    }
    let span = |counts: &[usize], threshold: usize| {
        let first = counts.iter().position(|&count| count >= threshold)?;
        let last = counts.iter().rposition(|&count| count >= threshold)? + 1;
        Some((first, last))
    };
    let region_width = right - left;
    let region_height = bottom - top;
    let frame_rows = span(&rows, region_width * 55 / 100);
    let frame_cols = span(&cols, region_height * 55 / 100);
    if let (Some((frame_top, frame_bottom)), Some((frame_left, frame_right))) =
        (frame_rows, frame_cols)
        && frame_bottom - frame_top >= region_height * 65 / 100
        && frame_right - frame_left >= region_width * 65 / 100
    {
        return (
            left + frame_left,
            left + frame_right,
            top + frame_top,
            top + frame_bottom,
        );
    }
    // Unframed art still benefits from pixel-level bounds, provided none of
    // its faint outer area would be cut by more than one connected cell.
    if let (Some((ink_top, ink_bottom)), Some((ink_left, ink_right))) =
        (span(&rows, 2), span(&cols, 2))
        && ink_left <= cell + 2
        && region_width - ink_right <= cell + 2
        && ink_top <= cell + 2
        && region_height - ink_bottom <= cell + 2
    {
        return (
            left + ink_left,
            left + ink_right,
            top + ink_top,
            top + ink_bottom,
        );
    }
    (left, right, top, bottom)
}

/// The central extent of a text-filled page, ignoring a few stray glyphs.
pub(crate) fn text_extent(text: &TextLayer) -> Option<Rect> {
    let bounds: Vec<Rect> = text.glyphs.iter().filter_map(|g| g.bounds).collect();
    if bounds.len() < 300 {
        return None;
    }
    let trimmed = |value: fn(&Rect) -> f32, low: bool| {
        let mut values: Vec<f32> = bounds.iter().map(value).collect();
        values.sort_by(f32::total_cmp);
        let skip = values.len() / 20;
        if low {
            values[skip]
        } else {
            values[values.len() - 1 - skip]
        }
    };
    Some(Rect {
        left: trimmed(|r| r.left, true),
        top: trimmed(|r| r.top, true),
        right: trimmed(|r| r.right, false),
        bottom: trimmed(|r| r.bottom, false),
    })
}

/// Whether the text layer leaves room for a region that `scan_regions` could report.
/// `block` is the book's typical text block; free space in the outer margins around
/// it is not considered. Without one, every page is treated as a candidate.
pub(crate) fn may_contain_illustration(text: &TextLayer, block: Option<Rect>) -> bool {
    const COLS: usize = 64;
    const ROWS: usize = 96;
    // The smallest reportable region is 1/8 of the width by 1/12 of the height.
    const WIDE: usize = COLS / 8;
    const TALL: usize = ROWS / 12;
    let Some(mut area) = block else {
        return true;
    };
    if let Some(page) = text_extent(text) {
        area = Rect {
            left: area.left.min(page.left),
            top: area.top.min(page.top),
            right: area.right.max(page.right),
            bottom: area.bottom.max(page.bottom),
        };
    }
    let mut covered = vec![0_u32; (COLS + 1) * (ROWS + 1)];
    let cell = |v: f32, n: usize| ((v.clamp(0.0, 1.0) * n as f32) as usize).min(n - 1);
    for rect in text.glyphs.iter().filter_map(|g| g.bounds) {
        for y in cell(rect.top, ROWS)..=cell(rect.bottom, ROWS) {
            for x in cell(rect.left, COLS)..=cell(rect.right, COLS) {
                covered[(y + 1) * (COLS + 1) + x + 1] = 1;
            }
        }
    }
    for y in 1..=ROWS {
        for x in 1..=COLS {
            let i = y * (COLS + 1) + x;
            covered[i] += covered[i - 1] + covered[i - COLS - 1] - covered[i - COLS - 2];
        }
    }
    let sum = |x: usize, y: usize| {
        let at = |x: usize, y: usize| covered[y * (COLS + 1) + x];
        at(x + WIDE, y + TALL) + at(x, y) - at(x, y + TALL) - at(x + WIDE, y)
    };
    let span = |from: f32, to: f32, n: usize, size: usize| {
        let first = cell(from, n);
        let last = (cell(to, n) + 1).max(first + size).min(n);
        last.saturating_sub(size).min(first)..=last - size
    };
    // A window mostly free of recognized text could hold an illustration.
    let (xs, ys) = (
        span(area.left, area.right, COLS, WIDE),
        span(area.top, area.bottom, ROWS, TALL),
    );
    ys.clone()
        .any(|y| xs.clone().any(|x| sum(x, y) as usize * 4 <= WIDE * TALL))
}

pub fn crop(raster: &RenderedPage, rect: Rect) -> Result<RenderedPage, String> {
    if [rect.left, rect.right, rect.top, rect.bottom]
        .iter()
        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        || rect.right <= rect.left
        || rect.bottom <= rect.top
    {
        return Err("Invalid PDF illustration bounds".into());
    }
    let x = (rect.left * raster.width as f32) as u32;
    let y = (rect.top * raster.height as f32) as u32;
    let right = (rect.right * raster.width as f32)
        .ceil()
        .min(raster.width as f32) as u32;
    let bottom = (rect.bottom * raster.height as f32)
        .ceil()
        .min(raster.height as f32) as u32;
    let width = right - x;
    let height = bottom - y;
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for row in y..bottom {
        let start = (row as usize * raster.width as usize + x as usize) * 4;
        rgba.extend_from_slice(&raster.rgba[start..start + width as usize * 4]);
    }
    Ok(RenderedPage {
        page: raster.page,
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Glyph;

    /// Rows of glyphs filling `left..right`, one text line every 0.02 of the page.
    fn text_rows(rows: impl Iterator<Item = f32>, left: f32, right: f32) -> TextLayer {
        let mut glyphs = Vec::new();
        for top in rows {
            let mut x = left;
            while x < right {
                glyphs.push(Glyph {
                    start: 0,
                    end: 0,
                    bounds: Some(Rect {
                        left: x,
                        right: x + 0.01,
                        top,
                        bottom: top + 0.02,
                    }),
                    style: None,
                });
                x += 0.01;
            }
        }
        TextLayer::new(String::new(), glyphs)
    }

    #[test]
    fn text_filled_pages_are_not_rasterized_but_open_space_is() {
        let block = Rect {
            left: 0.1,
            top: 0.08,
            right: 0.9,
            bottom: 0.92,
        };
        let lines = |from: f32, to: f32| {
            (0..)
                .map(move |i| from + i as f32 * 0.02)
                .take_while(move |t| *t < to)
        };
        // A full page of text; its outer margins do not count as open space.
        let full = text_rows(lines(0.08, 0.9), 0.1, 0.9);
        assert!(!may_contain_illustration(&full, Some(block)));
        // Until the book's text block is known, every page is scanned.
        assert!(may_contain_illustration(&full, None));
        // A chapter opening leaves the top of the text block free.
        let opening = text_rows(lines(0.4, 0.9), 0.1, 0.9);
        assert!(may_contain_illustration(&opening, Some(block)));
        // Text wrapped beside a figure on the left.
        let wrapped = text_rows(lines(0.08, 0.9), 0.45, 0.9);
        assert!(may_contain_illustration(&wrapped, Some(block)));
    }

    #[test]
    fn scan_background_and_ocr_text_do_not_become_a_full_page_image() {
        let mut raster = RenderedPage {
            page: 7,
            width: 400,
            height: 600,
            rgba: vec![255; 400 * 600 * 4],
        };
        for y in 80..320 {
            for x in 80..320 {
                raster.rgba[(y * 400 + x) * 4..(y * 400 + x) * 4 + 3].fill(0);
            }
        }
        for y in 450..470 {
            for x in 60..340 {
                raster.rgba[(y * 400 + x) * 4..(y * 400 + x) * 4 + 3].fill(0);
            }
        }
        let text = TextLayer::new(
            "Caption".into(),
            vec![Glyph {
                start: 0,
                end: 7,
                bounds: Some(Rect {
                    left: 0.15,
                    right: 0.85,
                    top: 0.75,
                    bottom: 470.0 / 600.0,
                }),
                style: None,
            }],
        );
        let regions = scan_regions(&raster, &text);
        assert_eq!(regions.len(), 1);
        assert!(regions[0].top > 0.1 && regions[0].bottom < 0.6);
        let image = crop(&raster, regions[0]).unwrap();
        assert_eq!(image.page, 7);
        assert!(image.width < raster.width && image.height < raster.height);
        assert_eq!(
            image.rgba.len(),
            image.width as usize * image.height as usize * 4
        );
        // Once the only remaining ink is recognized text, there is no illustration.
        for y in 80..320 {
            for x in 80..320 {
                raster.rgba[(y * 400 + x) * 4..(y * 400 + x) * 4 + 3].fill(255);
            }
        }
        assert!(scan_regions(&raster, &text).is_empty());
    }

    #[test]
    fn framed_scan_excludes_nearby_caption_and_page_number() {
        let mut raster = RenderedPage {
            page: 8,
            width: 400,
            height: 600,
            rgba: vec![255; 400 * 600 * 4],
        };
        let mut paint = |left: usize, right: usize, top: usize, bottom: usize| {
            for y in top..bottom {
                for x in left..right {
                    raster.rgba[(y * 400 + x) * 4..(y * 400 + x) * 4 + 3].fill(0);
                }
            }
        };
        paint(80, 320, 100, 108);
        paint(80, 320, 372, 380);
        paint(80, 88, 100, 380);
        paint(312, 320, 100, 380);
        paint(60, 68, 88, 98);
        for x in (72..328).step_by(12) {
            paint(x, x + 4, 390, 402);
        }
        let regions = scan_regions(&raster, &TextLayer::new(String::new(), Vec::new()));
        assert_eq!(regions.len(), 1);
        let image = regions[0];
        assert!((image.left * 400.0 - 80.0).abs() <= 3.0, "{image:?}");
        assert!((image.right * 400.0 - 320.0).abs() <= 3.0, "{image:?}");
        assert!((image.top * 600.0 - 100.0).abs() <= 3.0, "{image:?}");
        assert!((image.bottom * 600.0 - 380.0).abs() <= 3.0, "{image:?}");
    }
}
