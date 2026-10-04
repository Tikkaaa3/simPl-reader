//! Row geometry of a laid-out section: one height per item and their running starts.
use std::ops::Range;

/// A stable item plus its position within the old width's geometry.
#[derive(Clone, Copy, Debug)]
pub struct ItemAnchor {
    row: usize,
    within: f32,
    at_end: bool,
}

/// One compact height per item; no rich text, widget or image is retained here.
#[derive(Debug)]
pub struct HeightIndex {
    heights: Vec<f32>,
    starts: Vec<f32>,
}

impl HeightIndex {
    pub fn new(heights: Vec<f32>) -> Self {
        let mut starts = Vec::with_capacity(heights.len() + 1);
        starts.push(0.0);
        for &height in &heights {
            starts.push(starts.last().copied().unwrap_or(0.0) + height);
        }
        Self { heights, starts }
    }

    pub fn anchor_at(&self, offset: f32) -> ItemAnchor {
        if self.is_empty() {
            return ItemAnchor {
                row: 0,
                within: 0.0,
                at_end: true,
            };
        }
        let position = offset.max(0.0).min(self.total());
        let row = self.window(position, 0.0, 0.0).start;
        ItemAnchor {
            row,
            within: position - self.start(row),
            at_end: position == self.total(),
        }
    }

    /// Clamp the intra-item offset when a wrapped paragraph becomes shorter.
    /// The largest representable float below its end remains in that item.
    pub fn resolve_anchor(&self, anchor: ItemAnchor) -> f32 {
        if self.is_empty() {
            return 0.0;
        }
        if anchor.at_end {
            return self.total();
        }
        let row = anchor.row.min(self.len() - 1);
        let end = self.start(row + 1);
        let last_inside = f32::from_bits(end.to_bits() - 1);
        (self.start(row) + anchor.within.max(0.0)).min(last_inside)
    }

    /// Refine one measured row and keep the current top visible item at the
    /// same intra-row position. A change above the anchor shifts the offset.
    pub fn refine(&mut self, item: usize, actual: f32, anchor_offset: f32) -> f32 {
        let anchor = self.anchor_at(anchor_offset);
        if item < self.len() && actual.is_finite() && actual > 0.0 {
            let delta = actual - self.heights[item];
            self.heights[item] = actual;
            for start in &mut self.starts[item + 1..] {
                *start += delta;
            }
        }
        self.resolve_anchor(anchor)
    }

    pub fn height(&self, item: usize) -> f32 {
        self.heights[item]
    }

    pub fn len(&self) -> usize {
        self.heights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heights.is_empty()
    }

    pub fn start(&self, index: usize) -> f32 {
        self.starts[index]
    }

    pub fn total(&self) -> f32 {
        self.starts.last().copied().unwrap_or(0.0)
    }

    /// Indices intersecting the viewport extended by `overscan` on either side.
    pub fn window(&self, offset: f32, viewport: f32, overscan: f32) -> Range<usize> {
        if self.heights.is_empty() {
            return 0..0;
        }
        let first_y = (offset - overscan).max(0.0);
        let last_y = (offset + viewport + overscan).max(first_y);
        let first = self
            .starts
            .partition_point(|&start| start <= first_y)
            .saturating_sub(1)
            .min(self.len() - 1);
        let end = self
            .starts
            .partition_point(|&start| start < last_y)
            .max(first + 1)
            .min(self.len());
        first..end
    }
}
