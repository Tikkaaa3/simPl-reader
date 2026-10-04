//! Bounded native viewport geometry shared by document and fixture readers.

use iced::{
    Element, Length, Point, Rectangle, Size, Theme,
    advanced::{Layout, Widget, layout, mouse, renderer, widget::Tree},
};
use parking_lot::Mutex;
use std::{
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

/// Native layout corrections. Unchanged rows do not keep the application redrawing.
pub type Measurements = Arc<Mutex<Vec<(usize, f32, f32, u64)>>>;

/// Native measurement sink; diagnostic counters are gated separately.
pub struct LayoutReports {
    pub measurements: Measurements,
    pub generation: u64,
    pub counters: Option<(Arc<AtomicUsize>, Arc<AtomicUsize>)>,
}

/// Single logical scroll content with only a bounded subset of native children.
/// All children retain their original item order and are measured by Iced's
/// actual text/image layout; the remaining extent is metadata, not widgets.
pub struct VisibleRows<'a, Message> {
    first: usize,
    geometry: Vec<(f32, f32)>,
    total: f32,
    width: f32,
    gap: f32,
    item_count: usize,
    rows: Vec<Element<'a, Message>>,
    reports: LayoutReports,
}

#[derive(Debug)]
struct VisibleRowsState {
    first: usize,
}

impl<'a, Message> VisibleRows<'a, Message> {
    pub fn new(
        range: Range<usize>,
        index: &HeightIndex,
        width: f32,
        gap: f32,
        rows: Vec<Element<'a, Message>>,
        reports: LayoutReports,
    ) -> Self {
        Self {
            first: range.start,
            geometry: range
                .map(|row| (index.start(row), index.height(row)))
                .collect(),
            total: index.total(),
            width,
            gap,
            item_count: index.len(),
            rows,
            reports,
        }
    }
    /// Render a page's portion of the global index, retaining global row identities.
    pub fn within(self, scope: Range<usize>, index: &HeightIndex) -> Self {
        self.slice(index.start(scope.start)..index.start(scope.end))
    }

    /// Clip a fragment of a paragraph while keeping its complete native text layout.
    pub fn slice(mut self, content: Range<f32>) -> Self {
        let start = content.start;
        for (top, _) in &mut self.geometry {
            *top -= start;
        }
        self.total = content.end - start;
        self
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for VisibleRows<'_, Message> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.width), Length::Fixed(self.total))
    }

    fn children(&self) -> Vec<Tree> {
        self.rows.iter().map(Tree::new).collect()
    }
    fn tag(&self) -> iced::advanced::widget::tree::Tag {
        iced::advanced::widget::tree::Tag::of::<VisibleRowsState>()
    }
    fn state(&self) -> iced::advanced::widget::tree::State {
        iced::advanced::widget::tree::State::new(VisibleRowsState { first: self.first })
    }
    fn diff(&self, tree: &mut Tree) {
        // The visible window moves by a row or two during scrolling. Preserve
        // the paragraph state of each retained source row; index-wise diffing
        // would reshape every paragraph whenever the first row changed.
        let state = tree.state.downcast_mut::<VisibleRowsState>();
        let old_first = state.first;
        let mut old: Vec<_> = std::mem::take(&mut tree.children)
            .into_iter()
            .map(Some)
            .collect();
        tree.children = self
            .rows
            .iter()
            .enumerate()
            .map(|(local, widget)| {
                let row = self.first + local;
                let mut child = row
                    .checked_sub(old_first)
                    .and_then(|index| old.get_mut(index))
                    .and_then(Option::take)
                    .unwrap_or_else(|| Tree::new(widget));
                child.diff(widget);
                child
            })
            .collect();
        state.first = self.first;
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        _limits: &layout::Limits,
    ) -> layout::Node {
        if let Some((last, peak)) = &self.reports.counters {
            last.store(self.rows.len(), Ordering::Relaxed);
            peak.fetch_max(self.rows.len(), Ordering::Relaxed);
        }
        // Only the most recent native layout pass is useful: a scrollbar
        // negotiation can layout twice before the app consumes the report.
        self.reports
            .measurements
            .lock()
            .retain(|(row, _, _, _)| *row < self.first || *row >= self.first + self.rows.len());
        let limits = layout::Limits::new(Size::ZERO, Size::new(self.width, f32::INFINITY));
        let mut nodes = Vec::with_capacity(self.rows.len());
        for (local, (row, state)) in self.rows.iter_mut().zip(&mut tree.children).enumerate() {
            let node = row.as_widget_mut().layout(state, renderer, &limits);
            let height = node.size().height
                + if self.first + local + 1 == self.item_count {
                    0.0
                } else {
                    self.gap
                };
            let (start, expected_height) = self.geometry[local];
            if (height - expected_height).abs() > 0.01 {
                self.reports.measurements.lock().push((
                    self.first + local,
                    height,
                    self.width,
                    self.reports.generation,
                ));
            }
            nodes.push(node.move_to(Point::new(0.0, start)));
        }
        layout::Node::with_children(Size::new(self.width, self.total), nodes)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut iced::advanced::Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let Some(viewport) = viewport.intersection(&layout.bounds()) else {
            return;
        };
        let viewport = &viewport;
        for ((row, state), row_layout) in self
            .rows
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            if row_layout.bounds().intersects(viewport) {
                row.as_widget_mut().update(
                    state, event, row_layout, cursor, renderer, clipboard, shell, viewport,
                );
            }
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let Some(viewport) = viewport.intersection(&layout.bounds()) else {
            return mouse::Interaction::default();
        };
        if !cursor.is_over(viewport) {
            return mouse::Interaction::default();
        }
        let viewport = &viewport;
        self.rows
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((row, state), row_layout)| {
                row.as_widget()
                    .mouse_interaction(state, row_layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn iced::advanced::widget::Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((row, state), row_layout) in self
                .rows
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                row.as_widget_mut()
                    .operate(state, row_layout, renderer, operation);
            }
        });
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let Some(viewport) = viewport.intersection(&layout.bounds()) else {
            return;
        };
        renderer.with_layer(viewport, |renderer| {
            let viewport = &viewport;
            for ((row, state), row_layout) in
                self.rows.iter().zip(&tree.children).zip(layout.children())
            {
                if row_layout.bounds().intersection(viewport).is_some() {
                    row.as_widget()
                        .draw(state, renderer, theme, style, row_layout, cursor, viewport);
                }
            }
        });
    }
}

impl<'a, Message: 'a> From<VisibleRows<'a, Message>> for Element<'a, Message> {
    fn from(value: VisibleRows<'a, Message>) -> Self {
        Element::new(value)
    }
}

/// Count completed row Element constructions at the same seam used by the
/// native reader view. Counting the selected source range before this call
/// would miss an accidental eager build.
pub fn construct_rows<'a, T, Message: 'a>(
    items: impl IntoIterator<Item = T>,
    mut render: impl FnMut(T) -> Element<'a, Message>,
    counter: Option<&AtomicUsize>,
) -> Vec<Element<'a, Message>> {
    items
        .into_iter()
        .map(|item| {
            let element = render(item);
            if let Some(counter) = counter {
                counter.fetch_add(1, Ordering::Relaxed);
            }
            element
        })
        .collect()
}

/// Diagnostics' finite upper bound for the 600-DIP cap with 600 DIP of
/// overscan on each side. This is a guardrail, not a performance budget.
pub const ROW_GUARD: usize = 128;

#[must_use]
pub fn bounded_active_rows(count: usize) -> bool {
    count <= ROW_GUARD
}

pub use reader_layout::index::{HeightIndex, ItemAnchor};

/// Fixture-reader geometry estimates for a [`HeightIndex`].
pub trait WorkloadHeights {
    /// Cheap initial geometry only. Never shapes offscreen text. Native row
    /// layout replaces these guesses when the row enters the window.
    fn for_workload(work: &reader_workload::Workload, width: u32) -> Self;
}

impl WorkloadHeights for HeightIndex {
    fn for_workload(work: &reader_workload::Workload, width: u32) -> Self {
        let recipe = &reader_workload::LAYOUT_RECIPE;
        let count = work.total_item_count();
        let heights = work
            .items()
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let gap = if i + 1 == count {
                    0.0
                } else {
                    recipe.paragraph_gap_dip
                };
                let estimated = match item {
                    reader_document::Item::Heading { text, .. } => {
                        let lines = (text.chars().count() as f32 * 15.0 / width as f32)
                            .ceil()
                            .max(1.0);
                        lines * recipe.heading_line_height_dip
                    }
                    reader_document::Item::Paragraph { text, .. } => {
                        let lines = (text.chars().count() as f32 * 9.0 / width as f32)
                            .ceil()
                            .max(1.0);
                        lines * recipe.body_line_height_dip
                    }
                    reader_document::Item::Image { .. } => recipe.image_display_size_dip.1 as f32,
                };
                estimated + gap
            })
            .collect();
        HeightIndex::new(heights)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_height_window_includes_only_intersecting_rows_and_finite_overscan() {
        let index = HeightIndex::new(vec![20.0, 100.0, 40.0, 200.0, 30.0]);
        assert_eq!(index.window(125.0, 50.0, 0.0), 2..4);
        assert_eq!(index.window(125.0, 50.0, 30.0), 1..4);
        assert_eq!(index.window(0.0, 10.0, 0.0), 0..1);
        assert_eq!(index.window(370.0, 50.0, 0.0), 4..5);
        assert_eq!(index.window(100_000.0, 50.0, 0.0), 4..5);
        assert_eq!(HeightIndex::new(vec![]).window(0.0, 50.0, 30.0), 0..0);
    }

    #[test]
    fn eager_negative_control_is_detected_for_both_workload_sizes() {
        let built = AtomicUsize::new(0);
        let small = reader_workload::workload(reader_workload::WorkloadSize::Small);
        let large = reader_workload::workload(reader_workload::WorkloadSize::Large);
        for work in [&small, &large] {
            built.store(0, Ordering::Relaxed);
            let index = HeightIndex::for_workload(work, 480);
            let active = index.window(0.0, 600.0, 600.0);
            let rows: Vec<Element<'static, ()>> = construct_rows(
                work.items()[active].iter(),
                |_| iced::widget::text("row").into(),
                Some(&built),
            );
            assert!(bounded_active_rows(built.load(Ordering::Relaxed)));
            assert_eq!(built.load(Ordering::Relaxed), rows.len());
            let eager: Vec<Element<'static, ()>> = construct_rows(
                work.items().iter(),
                |_| iced::widget::text("row").into(),
                Some(&built),
            );
            assert_eq!(built.load(Ordering::Relaxed), rows.len() + eager.len());
            assert!(
                !bounded_active_rows(built.load(Ordering::Relaxed)),
                "original eager construction must trip the real built counter"
            );
        }
    }

    #[test]
    fn index_window_bound_is_independent_of_fixture_length() {
        let small = reader_workload::workload(reader_workload::WorkloadSize::Small);
        let large = reader_workload::workload(reader_workload::WorkloadSize::Large);
        for work in [&small, &large] {
            let index = HeightIndex::for_workload(work, 480);
            for offset in [0.0, index.total() / 2.0, index.total()] {
                assert!(bounded_active_rows(
                    index.window(offset, 600.0, 600.0).len()
                ));
            }
        }
    }

    #[test]
    fn fixture_index_is_variable_and_width_specific_without_retaining_rows() {
        let work = reader_workload::workload(reader_workload::WorkloadSize::Small);
        let wide = HeightIndex::for_workload(&work, 800);
        let narrow = HeightIndex::for_workload(&work, 480);
        assert_eq!(wide.len(), 1051);
        assert_eq!(narrow.len(), 1051);
        assert_ne!(wide.total(), narrow.total());
        assert!(wide.window(0.0, 600.0, 600.0).len() < 1051);
        let large = reader_workload::workload(reader_workload::WorkloadSize::Large);
        let index = HeightIndex::for_workload(&large, 800);
        assert_eq!(index.len(), large.total_item_count());
        assert!(index.window(0.0, 600.0, 600.0).len() < 100);
    }

    #[test]
    fn native_tree_evicts_rows_and_rebuilds_after_content_close() {
        let index = HeightIndex::new(vec![39.0; 200]);
        let reports: Measurements = Default::default();
        let make = |range: Range<usize>| {
            let rows: Vec<Element<'static, ()>> = range
                .clone()
                .map(|row| iced::widget::button(iced::widget::text(format!("row {row}"))).into())
                .collect();
            VisibleRows::new(
                range,
                &index,
                480.0,
                12.0,
                rows,
                LayoutReports {
                    measurements: reports.clone(),
                    generation: 0,
                    counters: None,
                },
            )
        };
        let first = make(0..22);
        let mut tree = Tree::new(&first as &dyn Widget<(), Theme, iced::Renderer>);
        assert_eq!(tree.children.len(), 22);
        let retained = match &tree.children[5].state {
            iced::advanced::widget::tree::State::Some(state) => {
                &**state as *const dyn std::any::Any as *const ()
            }
            _ => panic!("button row should own state"),
        };
        let shifted = make(1..23);
        tree.diff(&shifted as &dyn Widget<(), Theme, iced::Renderer>);
        let moved = match &tree.children[4].state {
            iced::advanced::widget::tree::State::Some(state) => {
                &**state as *const dyn std::any::Any as *const ()
            }
            _ => panic!("button row should keep state"),
        };
        assert_eq!(
            retained, moved,
            "scrolling must retain the overlapping row state"
        );
        let far = make(190..200);
        tree.diff(&far as &dyn Widget<(), Theme, iced::Renderer>);
        assert_eq!(tree.children.len(), 10);
        let closed = make(0..0);
        tree.diff(&closed as &dyn Widget<(), Theme, iced::Renderer>);
        assert!(tree.children.is_empty());
        tree.diff(&first as &dyn Widget<(), Theme, iced::Renderer>);
        assert_eq!(tree.children.len(), 22);
    }

    #[test]
    fn tall_row_and_large_jump_keep_a_single_contiguous_range() {
        let mut index = HeightIndex::new(vec![40.0, 2000.0, 30.0, 40.0]);
        assert_eq!(index.window(1200.0, 200.0, 0.0), 1..2);
        assert_eq!(index.window(2050.0, 100.0, 0.0), 2..4);
        assert_eq!(index.window(index.total(), 100.0, 0.0), 3..4);
        assert_eq!(index.refine(1, 2500.0, 2050.0), 2550.0);
        assert_eq!(index.window(2550.0, 100.0, 0.0), 2..4);
    }

    #[test]
    fn width_rebuild_resets_old_measurements_and_preserves_id_anchor() {
        let work = reader_workload::workload(reader_workload::WorkloadSize::Small);
        let mut wide = HeightIndex::for_workload(&work, 800);
        let anchor = 100;
        let within = 15.0;
        wide.refine(anchor, 310.0, wide.start(anchor));
        let narrow = HeightIndex::for_workload(&work, 480);
        let offset = narrow.start(anchor) + within;
        assert_eq!(narrow.window(offset, 0.0, 0.0).start, anchor);
        assert_ne!(wide.total(), narrow.total());
    }

    #[test]
    fn shrinking_anchor_row_never_moves_into_next_row() {
        let mut index = HeightIndex::new(vec![20.0, 200.0, 40.0]);
        let offset = index.refine(1, 30.0, 120.0);
        assert!((20.0..50.0).contains(&offset), "offset={offset}");
        assert_eq!(index.window(offset, 0.0, 0.0).start, 1);
        assert_eq!(index.refine(0, 25.0, index.total()), index.total());
    }

    #[test]
    fn tall_to_short_width_change_preserves_the_top_item() {
        let narrow = HeightIndex::new(vec![39.0, 350.0, 39.0]);
        let wide = HeightIndex::new(vec![39.0, 66.0, 39.0]);
        let offset = wide.resolve_anchor(narrow.anchor_at(39.0 + 250.0));
        assert_eq!(wide.window(offset, 0.0, 0.0).start, 1);
        assert!((39.0..105.0).contains(&offset));
        assert_eq!(wide.resolve_anchor(narrow.anchor_at(39.0)), 39.0);
    }

    #[test]
    fn refinement_preserves_item_anchor_and_releases_compact_index() {
        let mut index = HeightIndex::new(vec![20.0, 100.0, 40.0]);
        assert_eq!(index.refine(0, 50.0, 130.0), 160.0);
        assert_eq!(index.start(2), 150.0);
        assert_eq!(index.refine(2, 90.0, 160.0), 160.0);
        assert_eq!(index.total(), 240.0);
        drop(index);
        let index = HeightIndex::new(Vec::new());
        assert_eq!(index.window(500.0, 600.0, 100.0), 0..0);
    }
}
