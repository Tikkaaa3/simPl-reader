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
}
