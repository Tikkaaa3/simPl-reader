//! App-owned logical document selection for the disposable Iced reader PoC.
//!
//! Pointer positions are resolved by the visible native Iced paragraph. This
//! module owns only stable source coordinates and the fixture's copy policy;
//! no row widget or viewport geometry is retained here.

use std::ops::Range;

use iced::advanced::{
    Layout, Renderer as CoreRenderer, Widget, layout, mouse, renderer,
    text::{Paragraph as _, Renderer as TextRenderer, Span},
    widget::{Tree, tree},
};
use reader_document::{Endpoint, Item};
use unicode_segmentation::UnicodeSegmentation;

const LEADING_RLM: &str = "\u{200f}";

/// Selection anchor and focus in stable fixture text coordinates.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SelectionState {
    anchor: Option<Endpoint>,
    focus: Option<Endpoint>,
    dragging: bool,
}

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

impl SelectionState {
    /// Starts a fresh pointer selection at a native hit-test result.
    pub fn begin(&mut self, endpoint: Endpoint) {
        self.anchor = Some(endpoint.clone());
        self.focus = Some(endpoint);
        self.dragging = true;
    }

    /// Extends the active pointer selection to a native hit-test result.
    /// Returns `true` only when the logical focus changed.
    pub fn extend(&mut self, endpoint: Endpoint) -> bool {
        if !self.dragging || self.focus.as_ref() == Some(&endpoint) {
            return false;
        }
        self.focus = Some(endpoint);
        true
    }

    /// Stops pointer tracking while retaining the completed selection.
    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    /// Clears both selection endpoints and any active drag.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Whether a pointer selection is currently being dragged.
    #[must_use]
    pub const fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// Stable anchor and focus, if a pointer selection has started.
    #[must_use]
    pub fn endpoints(&self) -> Option<(&Endpoint, &Endpoint)> {
        Some((self.anchor.as_ref()?, self.focus.as_ref()?))
    }

    /// Normalizes and validates selection endpoints against the loaded source.
    #[must_use]
    pub fn bounds(&self, items: &[Item]) -> Option<SelectionBounds> {
        let (anchor, focus) = self.endpoints()?;
        let (anchor_item, anchor_text) = resolve(items, anchor)?;
        let (focus_item, focus_text) = resolve(items, focus)?;
        if !is_grapheme_boundary(anchor_text, anchor.byte_offset)
            || !is_grapheme_boundary(focus_text, focus.byte_offset)
        {
            return None;
        }

        let anchor_key = (anchor_item, anchor.byte_offset);
        let focus_key = (focus_item, focus.byte_offset);
        let (start_item, start_byte, end_item, end_byte) = if anchor_key <= focus_key {
            (
                anchor_item,
                anchor.byte_offset,
                focus_item,
                focus.byte_offset,
            )
        } else {
            (
                focus_item,
                focus.byte_offset,
                anchor_item,
                anchor.byte_offset,
            )
        };
        Some(SelectionBounds {
            start_item,
            start_byte,
            end_item,
            end_byte,
        })
    }

    /// Extracts selected plain text in logical source order.
    ///
    /// Images contribute neither text nor separators. Empty partial endpoint
    /// items are omitted, so no fabricated leading/trailing LF is produced.
    /// A collapsed selection resolves to `Some("")`; callers must not write
    /// that empty result to the clipboard.
    #[must_use]
    pub fn copy_text(&self, items: &[Item]) -> Option<String> {
        let bounds = self.bounds(items)?;
        if bounds.start_item == bounds.end_item && bounds.start_byte == bounds.end_byte {
            return Some(String::new());
        }

        let mut parts = Vec::new();
        for (index, item) in items
            .iter()
            .enumerate()
            .skip(bounds.start_item)
            .take(bounds.end_item - bounds.start_item + 1)
        {
            let Some(text) = item.text() else {
                continue;
            };
            if let Some(range) = bounds.range_for_item(index, text) {
                parts.push(&text[range]);
            }
        }
        Some(parts.join("\n"))
    }
}

/// Projects the pinned native paragraph hit-test byte offset into the
/// workload's logical source coordinates.
///
/// Iced 0.14's pinned `iced_graphics::text::Paragraph` forwards Cosmic Text's
/// byte-index cursor. Its hit-test returns grapheme-edge positions; this
/// adapter independently validates UTF-8 and extended-grapheme boundaries and
/// removes only the reader adapter's single leading RLM when present.
#[must_use]
pub fn map_native_hit(
    items: &[Item],
    item_id: &str,
    mapped_text: &str,
    native_byte_offset: usize,
    leading_rlm: bool,
) -> Option<Endpoint> {
    let source = items.iter().find(|item| item.id() == item_id)?.text()?;
    project_native_hit(
        item_id,
        source,
        mapped_text,
        native_byte_offset,
        leading_rlm,
    )
}

fn project_native_hit(
    item_id: &str,
    source: &str,
    mapped_text: &str,
    native_byte_offset: usize,
    leading_rlm: bool,
) -> Option<Endpoint> {
    let logical_mapped = if leading_rlm {
        mapped_text.strip_prefix(LEADING_RLM)?
    } else {
        mapped_text
    };
    if logical_mapped != source
        || native_byte_offset > mapped_text.len()
        || !mapped_text.is_char_boundary(native_byte_offset)
    {
        return None;
    }

    let byte_offset = if leading_rlm {
        native_byte_offset.saturating_sub(LEADING_RLM.len())
    } else {
        native_byte_offset
    };
    if !is_grapheme_boundary(source, byte_offset) {
        return None;
    }

    Some(Endpoint {
        item_id: item_id.to_owned(),
        byte_offset,
    })
}

fn resolve<'a>(items: &'a [Item], endpoint: &Endpoint) -> Option<(usize, &'a str)> {
    let (index, item) = items
        .iter()
        .enumerate()
        .find(|(_, item)| item.id() == endpoint.item_id)?;
    let text = item.text()?;
    (endpoint.byte_offset <= text.len() && text.is_char_boundary(endpoint.byte_offset))
        .then_some((index, text))
}

fn is_grapheme_boundary(text: &str, byte_offset: usize) -> bool {
    text.is_char_boundary(byte_offset)
        && (byte_offset == text.len()
            || text
                .grapheme_indices(true)
                .any(|(start, _)| start == byte_offset))
}

/// Inputs for one active selectable paragraph, including its visible,
/// logical-byte selection range and the opt-in native hit observer gate.
pub struct SelectableParagraphConfig {
    /// Stable workload text-item ID.
    pub item_id: String,
    /// Original logical Unicode source text.
    pub logical_text: String,
    /// Existing reader adapter text/style mapping.
    pub mapped: crate::reader::MappedParagraph,
    /// Shared recipe font size.
    pub font_size: f32,
    /// Shared recipe line height.
    pub line_height: f32,
    /// Logical source byte range to highlight in this paragraph.
    pub selection: Option<Range<usize>>,
    /// Whether an active drag should keep receiving native pointer motion.
    pub dragging: bool,
    /// Whether the opt-in evidence run should observe native pointer hits.
    pub track_hit_test: bool,
}

/// Builds a renderer-backed, selectable paragraph element from the same
/// mapped text and font-role runs used by the existing reader adapter.
pub fn selectable_text<Message: 'static>(
    config: SelectableParagraphConfig,
    on_press: impl Fn(Endpoint) -> Message + 'static,
    on_move: impl Fn(Endpoint, iced::Point) -> Message + 'static,
) -> iced::Element<'static, Message> {
    let SelectableParagraphConfig {
        item_id,
        logical_text,
        mapped,
        font_size,
        line_height,
        selection,
        dragging,
        track_hit_test,
    } = config;
    let leading_rlm = mapped.text.starts_with(LEADING_RLM)
        && mapped.text.strip_prefix(LEADING_RLM) == Some(logical_text.as_str());
    let selection = selection.filter(|range| {
        range.start <= range.end
            && range.end <= logical_text.len()
            && is_grapheme_boundary(&logical_text, range.start)
            && is_grapheme_boundary(&logical_text, range.end)
    });
    let mapped_selection = selection.as_ref().map(|range| {
        let prefix = usize::from(leading_rlm) * LEADING_RLM.len();
        range.start + prefix..range.end + prefix
    });
    let spans = mapped_span_keys(&mapped, selection, leading_rlm);
    iced::Element::new(SelectableParagraph {
        item_id,
        logical_text,
        mapped_text: mapped.text,
        leading_rlm,
        mapped_selection,
        spans,
        font_size,
        line_height,
        dragging,
        track_hit_test,
        on_press: Box::new(on_press),
        on_move: Box::new(on_move),
    })
}

#[derive(Clone, Debug, PartialEq)]
struct SpanKey {
    text: String,
    font: iced::Font,
    selected: bool,
}

fn mapped_span_keys(
    mapped: &crate::reader::MappedParagraph,
    selection: Option<Range<usize>>,
    leading_rlm: bool,
) -> Vec<SpanKey> {
    let selection = selection.map(|range| {
        let prefix = usize::from(leading_rlm) * LEADING_RLM.len();
        range.start + prefix..range.end + prefix
    });
    let mut result = Vec::new();
    for run in &mapped.runs {
        let Some(selected) = selection.as_ref().filter(|selected| {
            selected.start < selected.end
                && selected.start < run.bytes.end
                && run.bytes.start < selected.end
        }) else {
            result.push(SpanKey {
                text: mapped.text[run.bytes.clone()].to_owned(),
                font: run.role.iced_font(),
                selected: false,
            });
            continue;
        };

        let overlap_start = run.bytes.start.max(selected.start);
        let overlap_end = run.bytes.end.min(selected.end);
        if run.bytes.start < overlap_start {
            result.push(SpanKey {
                text: mapped.text[run.bytes.start..overlap_start].to_owned(),
                font: run.role.iced_font(),
                selected: false,
            });
        }
        if overlap_start < overlap_end {
            result.push(SpanKey {
                text: mapped.text[overlap_start..overlap_end].to_owned(),
                font: run.role.iced_font(),
                selected: true,
            });
        }
        if overlap_end < run.bytes.end {
            result.push(SpanKey {
                text: mapped.text[overlap_end..run.bytes.end].to_owned(),
                font: run.role.iced_font(),
                selected: false,
            });
        }
    }
    result
}

fn selected_glyph_bounds(
    paragraph: &NativeParagraph,
    selection: Option<&Range<usize>>,
) -> Vec<iced::Rectangle> {
    let Some(selection) = selection else {
        return Vec::new();
    };

    let buffer = paragraph.buffer();
    let mut line_starts = Vec::with_capacity(buffer.lines.len());
    let mut line_start = 0;
    for line in &buffer.lines {
        line_starts.push(line_start);
        line_start += line.text().len() + line.ending().as_str().len();
    }

    let mut bounds = Vec::new();
    for run in buffer.layout_runs() {
        let Some(line_start) = line_starts.get(run.line_i).copied() else {
            continue;
        };
        for glyph in run.glyphs {
            let glyph_start = line_start + glyph.start;
            let glyph_end = line_start + glyph.end;
            if glyph_start >= selection.end || selection.start >= glyph_end {
                continue;
            }

            let height = glyph.line_height_opt.unwrap_or(run.line_height);
            if glyph.w <= 0.0 || height <= 0.0 {
                continue;
            }
            bounds.push(iced::Rectangle::new(
                iced::Point::new(glyph.x, run.line_top + glyph.y),
                iced::Size::new(glyph.w, height),
            ));
        }
    }
    bounds
}

const TEXT_COLOR: iced::Color = iced::Color::from_rgb8(0xd7, 0xdc, 0xe2);
const SELECTION_BACKGROUND: iced::Color = iced::Color::from_rgba8(0x3e, 0x79, 0xd8, 0.75);

type NativeParagraph = <iced::Renderer as TextRenderer>::Paragraph;

fn native_spans(
    spans: &[SpanKey],
    font_size: f32,
    line_height: f32,
) -> Vec<Span<'static, (), iced::Font>> {
    spans
        .iter()
        .map(|key| {
            let mut span: Span<'static, (), iced::Font> = Span::new(key.text.clone())
                .font(key.font)
                .size(font_size)
                .line_height(iced::advanced::text::LineHeight::Absolute(iced::Pixels(
                    line_height,
                )))
                .color(TEXT_COLOR);
            if key.selected {
                span = span
                    .background(iced::Background::Color(SELECTION_BACKGROUND))
                    .border(iced::Border::default());
            }
            span
        })
        .collect()
}

struct ParagraphState {
    paragraph: NativeParagraph,
    spans: Vec<SpanKey>,
}

struct SelectableParagraph<Message> {
    item_id: String,
    logical_text: String,
    mapped_text: String,
    leading_rlm: bool,
    mapped_selection: Option<Range<usize>>,
    spans: Vec<SpanKey>,
    font_size: f32,
    line_height: f32,
    dragging: bool,
    track_hit_test: bool,
    on_press: Box<dyn Fn(Endpoint) -> Message>,
    on_move: Box<dyn Fn(Endpoint, iced::Point) -> Message>,
}

impl<Message: 'static> Widget<Message, iced::Theme, iced::Renderer>
    for SelectableParagraph<Message>
{
    fn size(&self) -> iced::Size<iced::Length> {
        iced::Size::new(iced::Length::Fill, iced::Length::Shrink)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<ParagraphState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(ParagraphState {
            paragraph: NativeParagraph::default(),
            spans: Vec::new(),
        })
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<ParagraphState>();
        let bounds = limits.max();
        let spans = native_spans(&self.spans, self.font_size, self.line_height);
        let text = iced::advanced::Text {
            content: spans.as_slice(),
            bounds,
            size: iced::Pixels(self.font_size),
            line_height: iced::advanced::text::LineHeight::Absolute(iced::Pixels(self.line_height)),
            font: iced::Font::with_name("Noto Sans"),
            align_x: iced::advanced::text::Alignment::Default,
            align_y: iced::alignment::Vertical::Top,
            shaping: iced::advanced::text::Shaping::Advanced,
            wrapping: iced::advanced::text::Wrapping::Word,
        };

        let limits = layout::Limits::new(iced::Size::ZERO, bounds);
        layout::sized(&limits, iced::Length::Fill, iced::Length::Shrink, |_| {
            if state.spans != self.spans {
                state.paragraph = NativeParagraph::with_spans(text);
                state.spans.clone_from(&self.spans);
            } else {
                match state.paragraph.compare(iced::advanced::Text {
                    content: (),
                    bounds,
                    size: iced::Pixels(self.font_size),
                    line_height: iced::advanced::text::LineHeight::Absolute(iced::Pixels(
                        self.line_height,
                    )),
                    font: iced::Font::with_name("Noto Sans"),
                    align_x: iced::advanced::text::Alignment::Default,
                    align_y: iced::alignment::Vertical::Top,
                    shaping: iced::advanced::text::Shaping::Advanced,
                    wrapping: iced::advanced::text::Wrapping::Word,
                }) {
                    iced::advanced::text::Difference::None => {}
                    iced::advanced::text::Difference::Bounds => {
                        state.paragraph.resize(bounds);
                    }
                    iced::advanced::text::Difference::Shape => {
                        state.paragraph = NativeParagraph::with_spans(text);
                    }
                }
            }
            state.paragraph.min_bounds()
        })
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &iced::Rectangle,
    ) {
        if !layout.bounds().intersects(viewport) {
            return;
        }
        let state = tree.state.downcast_ref::<ParagraphState>();
        let translation = layout.position() - iced::Point::ORIGIN;
        for bounds in selected_glyph_bounds(&state.paragraph, self.mapped_selection.as_ref()) {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: bounds + translation,
                    ..Default::default()
                },
                iced::Background::Color(SELECTION_BACKGROUND),
            );
        }
        renderer.fill_paragraph(&state.paragraph, layout.position(), TEXT_COLOR, *viewport);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut iced::advanced::Shell<'_, Message>,
        _viewport: &iced::Rectangle,
    ) {
        let hit = || {
            let position = cursor.position_in(layout.bounds())?;
            let state = tree.state.downcast_ref::<ParagraphState>();
            let native_offset = state.paragraph.hit_test(position)?.cursor();
            project_native_hit(
                &self.item_id,
                &self.logical_text,
                &self.mapped_text,
                native_offset,
                self.leading_rlm,
            )
        };
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(endpoint) = hit() {
                    shell.publish((self.on_press)(endpoint));
                    shell.capture_event();
                }
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { position })
                if self.dragging || self.track_hit_test =>
            {
                if let Some(endpoint) = hit() {
                    shell.publish((self.on_move)(endpoint, *position));
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &iced::Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
    }
}

impl<Message: 'static> From<SelectableParagraph<Message>> for iced::Element<'static, Message> {
    fn from(widget: SelectableParagraph<Message>) -> Self {
        Self::new(widget)
    }
}

#[cfg(test)]
mod tests {
    use std::{ops::Range, path::PathBuf};

    use reader_document::{Endpoint, Item};
    use reader_workload::{Workload, WorkloadSize, selection_cases, workload};
    use unicode_segmentation::UnicodeSegmentation;

    use super::{SelectionState, map_native_hit, mapped_span_keys};

    fn golden_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/reader-workload/references/expected-copy")
            .join(format!("{name}.txt"))
    }

    fn materialize_visible_selection(
        workload: &Workload,
        active_rows: &Range<usize>,
        item_index: usize,
        bounds: super::SelectionBounds,
        width: f32,
    ) -> Option<Vec<iced::Rectangle>> {
        use iced::advanced::text::Paragraph as _;

        if !active_rows.contains(&item_index) {
            return None;
        }
        let reader_document::Item::Paragraph {
            text,
            base_direction,
            style_runs,
            ..
        } = workload.items().get(item_index)?
        else {
            return None;
        };
        let selection = bounds.range_for_item(item_index, text)?;
        let mapped = crate::reader::map_paragraph(text, *base_direction, style_runs).ok()?;
        let leading_rlm = mapped.text.starts_with(super::LEADING_RLM);
        let spans = super::mapped_span_keys(&mapped, Some(selection.clone()), leading_rlm);
        let native_spans = super::native_spans(&spans, 18.0, 27.0);
        let paragraph = super::NativeParagraph::with_spans(iced::advanced::Text {
            content: &native_spans,
            bounds: iced::Size::new(width, 600.0),
            size: iced::Pixels(18.0),
            line_height: iced::advanced::text::LineHeight::Absolute(iced::Pixels(27.0)),
            font: iced::Font::with_name("Noto Sans"),
            align_x: iced::advanced::text::Alignment::Default,
            align_y: iced::alignment::Vertical::Top,
            shaping: iced::advanced::text::Shaping::Advanced,
            wrapping: iced::advanced::text::Wrapping::Word,
        });
        let prefix = usize::from(leading_rlm) * super::LEADING_RLM.len();
        let mapped_selection = selection.start + prefix..selection.end + prefix;
        Some(super::selected_glyph_bounds(
            &paragraph,
            Some(&mapped_selection),
        ))
    }

    #[test]
    fn all_curated_selection_cases_match_independent_golden_bytes() {
        let workload = workload(WorkloadSize::Small);
        let mut selection = SelectionState::default();

        for case in selection_cases() {
            selection.begin(case.anchor.clone());
            selection.extend(case.focus.clone());
            selection.end_drag();
            let actual = selection
                .copy_text(workload.items())
                .unwrap_or_else(|| panic!("{} should resolve", case.name));
            let expected = std::fs::read(golden_path(case.name))
                .unwrap_or_else(|error| panic!("{} golden readable: {error}", case.name));
            assert_eq!(actual.as_bytes(), expected, "{}", case.name);
        }
    }

    #[test]
    fn selected_span_splitting_preserves_mapped_text_and_fixture_style_roles() {
        let workload = workload(WorkloadSize::Small);
        let reader_document::Item::Paragraph {
            text,
            base_direction,
            style_runs,
            ..
        } = workload.item_by_id("p-00009").expect("styled paragraph")
        else {
            panic!("p-00009 is a paragraph");
        };
        let mapped = crate::reader::map_paragraph(text, *base_direction, style_runs)
            .expect("fixture style ranges map");
        let spans = mapped_span_keys(&mapped, Some(22..56), false);
        assert_eq!(
            spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>(),
            mapped.text
        );
        assert_eq!(
            spans
                .iter()
                .filter(|span| span.selected)
                .map(|span| span.text.as_str())
                .collect::<String>(),
            &text[22..56]
        );
        assert!(
            spans
                .iter()
                .any(|span| span.selected && span.font.weight == iced::font::Weight::Bold)
        );
        assert!(
            spans
                .iter()
                .any(|span| span.selected && span.font.style == iced::font::Style::Italic)
        );
        assert!(spans.iter().any(|span| !span.selected));
    }

    #[test]
    fn rtl_selected_visual_fragments_cover_every_selected_native_glyph() {
        use iced::advanced::text::Paragraph as _;

        let workload = workload(WorkloadSize::Small);
        let reader_document::Item::Paragraph {
            text: source,
            base_direction,
            style_runs,
            ..
        } = workload.item_by_id("p-00002").expect("mixed RTL paragraph")
        else {
            panic!("p-00002 is a paragraph");
        };
        let mapped = crate::reader::map_paragraph(source, *base_direction, style_runs)
            .expect("fixture paragraph maps");
        let leading_rlm = mapped.text.starts_with(super::LEADING_RLM);
        let selected = 0..source.len();
        let spans = super::mapped_span_keys(&mapped, Some(selected.clone()), leading_rlm);
        let native_spans = super::native_spans(&spans, 18.0, 27.0);
        let paragraph = super::NativeParagraph::with_spans(iced::advanced::Text {
            content: &native_spans,
            bounds: iced::Size::new(480.0, 600.0),
            size: iced::Pixels(18.0),
            line_height: iced::advanced::text::LineHeight::Absolute(iced::Pixels(27.0)),
            font: iced::Font::with_name("Noto Sans"),
            align_x: iced::advanced::text::Alignment::Default,
            align_y: iced::alignment::Vertical::Top,
            shaping: iced::advanced::text::Shaping::Advanced,
            wrapping: iced::advanced::text::Wrapping::Word,
        });
        let prefix = usize::from(leading_rlm) * super::LEADING_RLM.len();
        let mapped_selection = selected.start + prefix..selected.end + prefix;
        let old_bounds = spans
            .iter()
            .enumerate()
            .filter(|(_, span)| span.selected)
            .flat_map(|(index, _)| paragraph.span_bounds(index))
            .collect::<Vec<_>>();
        let mut selected_glyphs = Vec::new();
        let mut uncovered_by_span_bounds = Vec::new();
        for run in paragraph.buffer().layout_runs() {
            for glyph in run.glyphs {
                if glyph.start >= mapped_selection.end
                    || mapped_selection.start >= glyph.end
                    || glyph.w <= 0.0
                {
                    continue;
                }
                selected_glyphs.push(glyph.start..glyph.end);
                let center = iced::Point::new(
                    glyph.x + glyph.w / 2.0,
                    run.line_top + glyph.y + glyph.line_height_opt.unwrap_or(run.line_height) / 2.0,
                );
                if !old_bounds.iter().any(|bounds| {
                    bounds.x <= center.x
                        && center.x <= bounds.x + bounds.width
                        && bounds.y <= center.y
                        && center.y <= bounds.y + bounds.height
                }) {
                    uncovered_by_span_bounds.push(glyph.start..glyph.end);
                }
            }
        }

        assert!(
            !selected_glyphs.is_empty(),
            "native paragraph has selected glyphs"
        );
        assert!(
            !uncovered_by_span_bounds.is_empty(),
            "the fixture must retain a selected visual fragment that span_bounds omits"
        );

        let actual_bounds = super::selected_glyph_bounds(&paragraph, Some(&mapped_selection));
        assert_eq!(
            actual_bounds.len(),
            selected_glyphs.len(),
            "the render seam should return one visible rectangle per selected native glyph"
        );
        let mut uncovered_by_actual = Vec::new();
        for run in paragraph.buffer().layout_runs() {
            for glyph in run.glyphs {
                if glyph.start >= mapped_selection.end
                    || mapped_selection.start >= glyph.end
                    || glyph.w <= 0.0
                {
                    continue;
                }
                let center = iced::Point::new(
                    glyph.x + glyph.w / 2.0,
                    run.line_top + glyph.y + glyph.line_height_opt.unwrap_or(run.line_height) / 2.0,
                );
                if !actual_bounds.iter().any(|bounds| {
                    bounds.x <= center.x
                        && center.x <= bounds.x + bounds.width
                        && bounds.y <= center.y
                        && center.y <= bounds.y + bounds.height
                }) {
                    uncovered_by_actual.push(glyph.start..glyph.end);
                }
            }
        }
        assert_eq!(
            uncovered_by_actual,
            [],
            "no selected visual fragment may be omitted"
        );
    }

    #[test]
    fn native_hit_projection_validates_utf8_graphemes_and_rtl_hint() {
        let workload = workload(WorkloadSize::Small);
        let combining = workload.item_by_id("p-00007").expect("combining case");
        let source = combining.text().expect("text item");
        let cluster = source
            .grapheme_indices(true)
            .find(|(_, grapheme)| grapheme.chars().count() > 1)
            .expect("curated decomposed grapheme");
        let inner_scalar_boundary = cluster.0 + cluster.1.chars().next().unwrap().len_utf8();
        assert!(source.is_char_boundary(inner_scalar_boundary));
        assert!(
            map_native_hit(
                workload.items(),
                "p-00007",
                source,
                inner_scalar_boundary,
                false
            )
            .is_none()
        );

        let rtl = workload.item_by_id("p-00002").expect("RTL case");
        let rtl_source = rtl.text().expect("text item");
        let mapped = format!("\u{200f}{rtl_source}");
        assert_eq!(
            map_native_hit(workload.items(), "p-00002", &mapped, 0, true)
                .expect("RLM start maps to source start"),
            Endpoint {
                item_id: "p-00002".into(),
                byte_offset: 0
            }
        );
        assert_eq!(
            map_native_hit(workload.items(), "p-00002", &mapped, 3, true)
                .expect("RLM end maps to source start")
                .byte_offset,
            0
        );
        assert_eq!(
            map_native_hit(workload.items(), "p-00002", &mapped, 5, true)
                .expect("first two-byte source scalar maps without RLM")
                .byte_offset,
            2
        );
        assert!(map_native_hit(workload.items(), "p-00002", &mapped, 1, true).is_none());
        assert!(map_native_hit(workload.items(), "p-00002", rtl_source, 0, true).is_none());
    }

    #[test]
    fn native_hit_projection_rejects_images_mismatched_text_and_out_of_range_offsets() {
        let workload = workload(WorkloadSize::Small);
        let image = workload
            .items()
            .iter()
            .find(|item| matches!(item, Item::Image { .. }))
            .expect("fixture image");
        assert!(map_native_hit(workload.items(), image.id(), "", 0, false).is_none());
        assert!(map_native_hit(workload.items(), "p-00001", "wrong source", 0, false).is_none());
        let text = workload.item_by_id("p-00001").unwrap().text().unwrap();
        assert!(map_native_hit(workload.items(), "p-00001", text, text.len() + 1, false).is_none());
    }

    #[test]
    fn stable_selection_rebuilds_native_highlight_after_visible_row_reentry() {
        let workload = workload(WorkloadSize::Small);
        let mut selection = SelectionState::default();
        let rtl_text = workload
            .item_by_id("p-00002")
            .expect("mixed RTL item")
            .text()
            .expect("paragraph text");
        selection.begin(Endpoint {
            item_id: "p-00002".into(),
            byte_offset: 0,
        });
        assert!(selection.extend(Endpoint {
            item_id: "p-00002".into(),
            byte_offset: rtl_text.len(),
        }));
        selection.end_drag();

        let initial = selection
            .bounds(workload.items())
            .expect("valid stable endpoints");
        let rtl_index = workload
            .items()
            .iter()
            .position(|item| item.id() == "p-00002")
            .expect("RTL item index");
        let initially_visible = 0..16;
        assert!(initially_visible.contains(&rtl_index));
        let first_render =
            materialize_visible_selection(&workload, &initially_visible, rtl_index, initial, 480.0)
                .expect("visible RTL row materializes its highlight");
        assert!(!first_render.is_empty());

        let evicted_rows = 4..22;
        assert!(!evicted_rows.contains(&rtl_index));
        assert!(
            materialize_visible_selection(
                &workload,
                &evicted_rows,
                rtl_index,
                selection
                    .bounds(workload.items())
                    .expect("selection survives eviction"),
                480.0,
            )
            .is_none()
        );
        assert_eq!(selection.bounds(workload.items()), Some(initial));
        assert_eq!(
            selection.copy_text(workload.items()).as_deref(),
            Some(rtl_text)
        );

        let reentered_rows = 0..16;
        assert!(reentered_rows.contains(&rtl_index));
        let rebuilt = materialize_visible_selection(
            &workload,
            &reentered_rows,
            rtl_index,
            selection
                .bounds(workload.items())
                .expect("stable source range"),
            480.0,
        )
        .expect("re-entered RTL row rematerializes its highlight");
        assert_eq!(rebuilt, first_render);

        let wide = materialize_visible_selection(
            &workload,
            &reentered_rows,
            rtl_index,
            selection
                .bounds(workload.items())
                .expect("stable source range at wide width"),
            800.0,
        )
        .expect("width change rematerializes RTL highlight");
        assert!(!wide.is_empty());
        assert_eq!(selection.bounds(workload.items()), Some(initial));
    }

    #[test]
    fn collapsed_and_empty_partial_selections_produce_no_fabricated_copy_text() {
        let workload = workload(WorkloadSize::Small);
        let mut selection = SelectionState::default();
        selection.begin(Endpoint {
            item_id: "p-00010".into(),
            byte_offset: 217,
        });
        assert_eq!(selection.copy_text(workload.items()).as_deref(), Some(""));

        selection.begin(Endpoint {
            item_id: "p-00005".into(),
            byte_offset: workload
                .item_by_id("p-00005")
                .unwrap()
                .text()
                .unwrap()
                .len(),
        });
        selection.extend(Endpoint {
            item_id: "p-00006".into(),
            byte_offset: 0,
        });
        assert_eq!(selection.copy_text(workload.items()).as_deref(), Some(""));
    }

    #[test]
    fn end_drag_keeps_logical_range_and_clear_releases_it_for_reload() {
        let workload = workload(WorkloadSize::Small);
        let mut selection = SelectionState::default();
        selection.begin(Endpoint {
            item_id: "p-00009".into(),
            byte_offset: 22,
        });
        assert!(selection.is_dragging());
        selection.extend(Endpoint {
            item_id: "p-00009".into(),
            byte_offset: 56,
        });
        selection.end_drag();
        assert!(!selection.is_dragging());
        assert_eq!(
            selection.copy_text(workload.items()).as_deref(),
            Some("bold emphasis and an italic aside ")
        );
        selection.clear();
        assert_eq!(selection.copy_text(workload.items()), None);
        assert!(!selection.is_dragging());
    }
}
