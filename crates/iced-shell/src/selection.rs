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
    /// Byte offset of this visual segment in the complete logical item.
    pub item_offset: usize,
    pub alignment: iced::advanced::text::Alignment,
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
    pub links: Vec<reader_document::Link>,
    pub focused_link: Option<usize>,
    /// Bundled family that replaces the default reading face (Literata), if any.
    pub font_family: Option<&'static str>,
    /// Saved highlights, oldest first, in logical source bytes of the whole item.
    pub marks: Vec<TextMark>,
}

/// A saved highlight painted behind the text of one paragraph.
#[derive(Clone, Debug, PartialEq)]
pub struct TextMark {
    pub range: Range<usize>,
    pub tint: iced::Color,
    /// A note is attached: the highlight also gets a solid underline.
    pub note: bool,
}

/// Builds a renderer-backed, selectable paragraph element from the same
/// mapped text and font-role runs used by the existing reader adapter.
pub fn selectable_text<Message: 'static>(
    config: SelectableParagraphConfig,
    on_press: impl Fn(Endpoint) -> Message + 'static,
    on_move: impl Fn(Endpoint, iced::Point) -> Message + 'static,
    on_link: Option<fn(String) -> Message>,
    on_context: Option<fn(Endpoint) -> Message>,
) -> iced::Element<'static, Message> {
    let SelectableParagraphConfig {
        item_id,
        logical_text,
        mapped,
        item_offset,
        alignment,
        font_size,
        line_height,
        selection,
        dragging,
        track_hit_test,
        mut links,
        focused_link,
        font_family,
        marks,
    } = config;
    let leading_rlm = mapped.text.starts_with(LEADING_RLM)
        && mapped.text.strip_prefix(LEADING_RLM) == Some(logical_text.as_str());
    let selection = selection.and_then(|range| {
        let start = range.start.max(item_offset).checked_sub(item_offset)?;
        let end = range
            .end
            .min(item_offset + logical_text.len())
            .checked_sub(item_offset)?;
        (start < end
            && is_grapheme_boundary(&logical_text, start)
            && is_grapheme_boundary(&logical_text, end))
        .then_some(start..end)
    });
    let mapped_selection = selection.as_ref().map(|range| {
        let prefix = usize::from(leading_rlm) * LEADING_RLM.len();
        range.start + prefix..range.end + prefix
    });
    let mark_prefix = usize::from(leading_rlm) * LEADING_RLM.len();
    let marks: Vec<TextMark> = marks
        .into_iter()
        .filter_map(|mark| {
            let start = mark.range.start.max(item_offset).checked_sub(item_offset)?;
            let end = mark
                .range
                .end
                .min(item_offset + logical_text.len())
                .checked_sub(item_offset)?;
            (start < end
                && is_grapheme_boundary(&logical_text, start)
                && is_grapheme_boundary(&logical_text, end))
            .then(|| TextMark {
                range: start + mark_prefix..end + mark_prefix,
                ..mark
            })
        })
        .collect();
    links.retain_mut(|link| {
        if link.start_byte < item_offset || link.end_byte > item_offset + logical_text.len() {
            return false;
        }
        link.start_byte -= item_offset;
        link.end_byte -= item_offset;
        link.start_byte < link.end_byte
            && logical_text.is_char_boundary(link.start_byte)
            && logical_text.is_char_boundary(link.end_byte)
    });
    // Shaping ignores the selection, so dragging never reshapes the paragraph;
    // the highlight is drawn separately from the native glyph bounds.
    let mut spans = mapped_span_keys(&mapped, None, leading_rlm);
    if let Some(family) = font_family {
        for span in &mut spans {
            span.font = with_reading_family(span.font, family);
        }
    }
    iced::Element::new(SelectableParagraph {
        item_id,
        logical_text,
        item_offset,
        alignment,
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
        links,
        focused_link,
        on_link,
        on_context,
        marks,
    })
}

/// Swaps the default reading face (Literata) for another bundled family, keeping
/// weight and style; monospace and other faces are left alone.
pub fn with_reading_family(font: iced::Font, family: &'static str) -> iced::Font {
    if font.family == iced::font::Family::Name("Literata") {
        iced::Font {
            family: iced::font::Family::Name(family),
            ..font
        }
    } else {
        font
    }
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

/// Joins touching glyph rectangles on one line, so a highlight is drawn as a
/// few quads per line rather than one per glyph (costly for CPU rendering).
fn merge_lines(mut glyphs: Vec<iced::Rectangle>) -> Vec<iced::Rectangle> {
    glyphs.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let mut lines: Vec<iced::Rectangle> = Vec::new();
    for glyph in glyphs {
        if let Some(last) = lines.last_mut()
            && (last.y - glyph.y).abs() < 0.5
            && (last.height - glyph.height).abs() < 0.5
            && glyph.x <= last.x + last.width + 0.5
        {
            last.width = (glyph.x + glyph.width).max(last.x + last.width) - last.x;
        } else {
            lines.push(glyph);
        }
    }
    lines
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

type NativeParagraph = <iced::Renderer as TextRenderer>::Paragraph;

/// Natural width of one unwrapped line, as the real text renderer shapes it.
pub fn text_width(text: &str, font: iced::Font, size: f32) -> f32 {
    let spans: Vec<Span<'static, (), iced::Font>> =
        vec![Span::new(text.to_owned()).font(font).size(size)];
    NativeParagraph::with_spans(iced::advanced::Text {
        content: spans.as_slice(),
        bounds: iced::Size::new(1_000_000.0, 1_000_000.0),
        size: iced::Pixels(size),
        line_height: iced::advanced::text::LineHeight::Absolute(iced::Pixels(size * 1.5)),
        font: iced::Font::with_name("Noto Sans"),
        align_x: iced::advanced::text::Alignment::Default,
        align_y: iced::alignment::Vertical::Top,
        shaping: iced::advanced::text::Shaping::Advanced,
        wrapping: iced::advanced::text::Wrapping::None,
    })
    .min_bounds()
    .width
}

fn native_spans(
    spans: &[SpanKey],
    font_size: f32,
    line_height: f32,
) -> Vec<Span<'static, (), iced::Font>> {
    spans
        .iter()
        .map(|key| {
            Span::new(key.text.clone())
                .font(key.font)
                .size(font_size)
                .line_height(iced::advanced::text::LineHeight::Absolute(iced::Pixels(
                    line_height,
                )))
        })
        .collect()
}

struct ParagraphState {
    paragraph: NativeParagraph,
    spans: Vec<SpanKey>,
    item_id: String,
    link_press: Option<(usize, iced::Point)>,
    link_dragged: bool,
    /// Last endpoint reported during a drag; unchanged hits are not re-sent.
    last_move: Option<Endpoint>,
}

struct SelectableParagraph<Message> {
    item_id: String,
    logical_text: String,
    item_offset: usize,
    alignment: iced::advanced::text::Alignment,
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
    links: Vec<reader_document::Link>,
    focused_link: Option<usize>,
    on_link: Option<fn(String) -> Message>,
    on_context: Option<fn(Endpoint) -> Message>,
    /// Highlights in mapped (shaped) byte coordinates.
    marks: Vec<TextMark>,
}

impl<Message> SelectableParagraph<Message> {
    fn link_bounds(&self, state: &ParagraphState, index: usize) -> Vec<iced::Rectangle> {
        let link = &self.links[index];
        let prefix = usize::from(self.leading_rlm) * LEADING_RLM.len();
        merge_lines(selected_glyph_bounds(
            &state.paragraph,
            Some(&(link.start_byte + prefix..link.end_byte + prefix)),
        ))
    }
    fn link_at(&self, state: &ParagraphState, point: iced::Point) -> Option<usize> {
        self.links.iter().enumerate().find_map(|(index, _)| {
            self.link_bounds(state, index)
                .iter()
                .any(|bounds| bounds.contains(point))
                .then_some(index)
        })
    }
}

impl<Message: 'static> Widget<Message, iced::Theme, iced::Renderer>
    for SelectableParagraph<Message>
{
    fn size(&self) -> iced::Size<iced::Length> {
        iced::Size::new(
            if matches!(
                self.alignment,
                iced::advanced::text::Alignment::Center | iced::advanced::text::Alignment::Right
            ) {
                iced::Length::Shrink
            } else {
                iced::Length::Fill
            },
            iced::Length::Shrink,
        )
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<ParagraphState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(ParagraphState {
            paragraph: NativeParagraph::default(),
            spans: Vec::new(),
            item_id: String::new(),
            link_press: None,
            link_dragged: false,
            last_move: None,
        })
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<ParagraphState>();
        if state.item_id != self.item_id {
            state.item_id.clone_from(&self.item_id);
            state.link_press = None;
        }
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
            wrapping: iced::advanced::text::Wrapping::WordOrGlyph,
        };

        let limits = layout::Limits::new(iced::Size::ZERO, bounds);
        layout::sized(&limits, self.size().width, iced::Length::Shrink, |_| {
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
                    wrapping: iced::advanced::text::Wrapping::WordOrGlyph,
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
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &iced::Rectangle,
    ) {
        if !layout.bounds().intersects(viewport) {
            return;
        }
        let state = tree.state.downcast_ref::<ParagraphState>();
        let translation = layout.position() - iced::Point::ORIGIN;
        for mark in &self.marks {
            for bounds in merge_lines(selected_glyph_bounds(&state.paragraph, Some(&mark.range))) {
                let bounds = bounds + translation;
                renderer.fill_quad(
                    renderer::Quad {
                        bounds,
                        ..Default::default()
                    },
                    iced::Background::Color(mark.tint),
                );
                if mark.note {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: iced::Rectangle {
                                y: bounds.y + bounds.height - 2.0,
                                height: 2.0,
                                ..bounds
                            },
                            ..Default::default()
                        },
                        iced::Background::Color(iced::Color {
                            a: 1.0,
                            ..mark.tint
                        }),
                    );
                }
            }
        }
        for bounds in merge_lines(selected_glyph_bounds(
            &state.paragraph,
            self.mapped_selection.as_ref(),
        )) {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: bounds + translation,
                    ..Default::default()
                },
                iced::Background::Color(theme.palette().primary.scale_alpha(0.25)),
            );
        }
        renderer.fill_paragraph(
            &state.paragraph,
            layout.position(),
            style.text_color,
            *viewport,
        );
        for index in 0..self.links.len() {
            for bounds in self.link_bounds(state, index) {
                let bounds = bounds + translation;
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: iced::Rectangle {
                            y: bounds.y + bounds.height - 1.0,
                            height: 1.0,
                            ..bounds
                        },
                        ..Default::default()
                    },
                    theme.palette().primary,
                );
                if self.focused_link == Some(index) {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds,
                            border: iced::Border {
                                color: theme.palette().primary,
                                width: 1.0,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        iced::Color::TRANSPARENT,
                    );
                }
            }
        }
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
        viewport: &iced::Rectangle,
    ) {
        let hit = |state: &ParagraphState| {
            let point = cursor.position()?;
            if !viewport.contains(point) {
                return None;
            }
            let position = cursor.position_in(layout.bounds())?;
            let native_offset = state.paragraph.hit_test(position)?.cursor();
            project_native_hit(
                &self.item_id,
                &self.logical_text,
                &self.mapped_text,
                native_offset,
                self.leading_rlm,
            )
            .map(|mut endpoint| {
                endpoint.byte_offset += self.item_offset;
                endpoint
            })
        };
        let point = cursor
            .position_in(layout.bounds())
            .filter(|_| cursor.position().is_some_and(|p| viewport.contains(p)));
        let state = tree.state.downcast_mut::<ParagraphState>();
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.link_press =
                    point.and_then(|point| self.link_at(state, point).map(|index| (index, point)));
                state.link_dragged = false;
                state.last_move = None;
                if let Some(endpoint) = hit(state) {
                    shell.publish((self.on_press)(endpoint));
                    shell.capture_event();
                }
            }
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if let Some(on_context) = self.on_context
                    && let Some(endpoint) = hit(state)
                {
                    shell.publish(on_context(endpoint));
                    shell.capture_event();
                }
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { position })
                if self.dragging || self.track_hit_test || state.link_press.is_some() =>
            {
                if let Some((_, start)) = state.link_press {
                    let moved = cursor
                        .position_in(layout.bounds())
                        .is_none_or(|point| point.distance(start) > 4.0);
                    state.link_dragged |= moved;
                }
                if let Some(endpoint) = hit(state)
                    && state.last_move.as_ref() != Some(&endpoint)
                {
                    state.last_move = Some(endpoint.clone());
                    shell.publish((self.on_move)(endpoint, *position));
                }
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some((index, start)) = state.link_press.take()
                    && !state.link_dragged
                    && point.is_some_and(|point| {
                        point.distance(start) <= 4.0 && self.link_at(state, point) == Some(index)
                    })
                    && let Some(on_link) = self.on_link
                {
                    shell.publish(on_link(self.links[index].href.clone()));
                    shell.capture_event();
                }
            }
            iced::Event::Window(iced::window::Event::Unfocused) => state.link_press = None,
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &iced::Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        if cursor
            .position()
            .is_some_and(|point| viewport.contains(point))
            && let Some(point) = cursor.position_in(layout.bounds())
        {
            if self.on_link.is_some()
                && self
                    .link_at(tree.state.downcast_ref::<ParagraphState>(), point)
                    .is_some()
            {
                return mouse::Interaction::Pointer;
            }
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
    #[test]
    #[ignore = "Native EPUB selection QA: SIMPL_PREVIEW_EPUB and SIMPL_PREVIEW_CHAPTER"]
    fn verify_epub_selection() {
        use iced::advanced::{Layout, Shell, layout, widget::Tree};
        use iced::{Event, Point, Rectangle, Size, mouse};
        use reader_document::{Endpoint, Item};
        #[derive(Debug)]
        enum Message {
            Start(Endpoint),
            Move(Endpoint),
        }
        let path = std::path::PathBuf::from(std::env::var_os("SIMPL_PREVIEW_EPUB").unwrap());
        let document = reader_document::epub::open(&path).unwrap();
        let index = std::env::var("SIMPL_PREVIEW_CHAPTER")
            .unwrap_or("5".into())
            .parse()
            .unwrap();
        let chapter = document.load_chapter(index).unwrap();
        for bytes in [
            include_bytes!("../../../assets/fonts/Literata-Regular.ttf").as_slice(),
            include_bytes!("../../../assets/fonts/Literata-Bold.ttf").as_slice(),
            include_bytes!("../../../assets/fonts/Literata-Italic.ttf").as_slice(),
        ] {
            iced::advanced::graphics::text::font_system()
                .write()
                .unwrap()
                .load_font(std::borrow::Cow::Borrowed(bytes));
        }
        let item = chapter
            .document
            .items
            .iter()
            .find(|item| matches!(item, Item::Paragraph { text, .. } if text.len() > 200))
            .unwrap();
        let Item::Paragraph {
            id,
            text,
            base_direction,
            style_runs,
        } = item
        else {
            unreachable!()
        };
        for width in [400.0, 720.0] {
            let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(20.0));
            let mut element = super::selectable_text(
                super::SelectableParagraphConfig {
                    item_id: id.clone(),
                    logical_text: text.clone(),
                    mapped: crate::reader::map_document_paragraph(
                        text,
                        *base_direction,
                        style_runs,
                    )
                    .unwrap(),
                    item_offset: 0,
                    alignment: iced::advanced::text::Alignment::Default,
                    font_size: 20.0,
                    line_height: 32.0,
                    selection: None,
                    dragging: true,
                    track_hit_test: false,
                    links: vec![],
                    focused_link: None,
                    font_family: None,
                    marks: Vec::new(),
                },
                Message::Start,
                |endpoint, _| Message::Move(endpoint),
                None,
                None,
            );
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::new(width, 10000.0)),
            );
            let state = tree.state.downcast_ref::<super::ParagraphState>();
            let space = text.find(' ').unwrap();
            let space_bounds =
                super::selected_glyph_bounds(&state.paragraph, Some(&(space..space + 1)));
            let all = super::selected_glyph_bounds(&state.paragraph, Some(&(0..text.len())));
            let start = space_bounds
                .first()
                .expect("space has native hit geometry")
                .center();
            let end = all.iter().find(|r| r.y > start.y + 32.0).unwrap().center();
            let viewport = Rectangle::with_size(node.size());
            let mut messages = Vec::new();
            for (event, point) in [
                (
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    start,
                ),
                (
                    Event::Mouse(mouse::Event::CursorMoved { position: end }),
                    end,
                ),
            ] {
                element.as_widget_mut().update(
                    &mut tree,
                    &event,
                    Layout::new(&node),
                    mouse::Cursor::Available(point),
                    &renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut Shell::new(&mut messages),
                    &viewport,
                );
            }
            let [Message::Start(anchor), Message::Move(focus)] = messages.as_slice() else {
                panic!(
                    "Whitespace press and multiline drag must reach the EPUB selection state: {messages:?}"
                );
            };
            assert!(anchor.byte_offset.abs_diff(space) <= 1);
            assert!(focus.byte_offset > anchor.byte_offset);
            let mut selection = super::SelectionState::default();
            selection.begin(anchor.clone());
            selection.extend(focus.clone());
            selection.end_drag();
            assert_eq!(
                selection.copy_text(&chapter.document.items).unwrap(),
                text[anchor.byte_offset..focus.byte_offset]
            );
            assert!(
                !super::selected_glyph_bounds(
                    &tree.state.downcast_ref::<super::ParagraphState>().paragraph,
                    Some(&(anchor.byte_offset..focus.byte_offset))
                )
                .is_empty()
            );
            // A point clipped out of the viewport must not steal the selection.
            messages.clear();
            element.as_widget_mut().update(
                &mut tree,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Layout::new(&node),
                mouse::Cursor::Available(start),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &Rectangle::new(Point::new(10000.0, 0.0), Size::new(10.0, 10.0)),
            );
            assert!(messages.is_empty());
        }
        println!(
            "EPUB whitespace press, multiline drag, logical copy and clipped hits passed: chapter {} of {}",
            index + 1,
            document.chapters.len()
        );
    }
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
            wrapping: iced::advanced::text::Wrapping::WordOrGlyph,
        });
        let prefix = usize::from(leading_rlm) * super::LEADING_RLM.len();
        let mapped_selection = selection.start + prefix..selection.end + prefix;
        Some(super::selected_glyph_bounds(
            &paragraph,
            Some(&mapped_selection),
        ))
    }

    #[test]
    fn native_link_click_is_distinct_from_drag_selection_and_clipped_content() {
        use iced::advanced::{Layout, Shell, layout, widget::Tree};
        use iced::{Event, Point, Rectangle, Size, mouse};
        #[derive(Debug)]
        enum Message {
            Start,
            Move,
            Link(String),
        }
        for (source, direction) in [
            ("é note and more", reader_document::BaseDirection::Ltr),
            ("ملاحظة عربية", reader_document::BaseDirection::Rtl),
        ] {
            let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(20.0));
            let mut element = super::selectable_text(
                super::SelectableParagraphConfig {
                    item_id: "p".into(),
                    logical_text: source.into(),
                    mapped: crate::reader::map_document_paragraph(source, direction, &[]).unwrap(),
                    item_offset: 0,
                    alignment: iced::advanced::text::Alignment::Default,
                    font_size: 20.0,
                    line_height: 32.0,
                    selection: None,
                    dragging: true,
                    track_hit_test: false,
                    links: vec![reader_document::Link {
                        start_byte: 0,
                        end_byte: source.len(),
                        href: "#note".into(),
                        kind: reader_document::LinkKind::Note,
                    }],
                    focused_link: None,
                    font_family: None,
                    marks: Vec::new(),
                },
                |_| Message::Start,
                |_, _| Message::Move,
                Some(Message::Link),
                None,
            );
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::new(400.0, 200.0)),
            );
            let state = tree.state.downcast_ref::<super::ParagraphState>();
            let prefix = if direction == reader_document::BaseDirection::Rtl {
                super::LEADING_RLM.len()
            } else {
                0
            };
            let glyphs = super::selected_glyph_bounds(
                &state.paragraph,
                Some(&(prefix..source.len() + prefix)),
            );
            let point = glyphs.first().expect("shaped link").center();
            let viewport = Rectangle::with_size(Size::new(400.0, 200.0));
            let mut messages = Vec::new();
            let mut dispatch = |event, point, viewport| {
                element.as_widget_mut().update(
                    &mut tree,
                    &event,
                    Layout::new(&node),
                    mouse::Cursor::Available(point),
                    &renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut Shell::new(&mut messages),
                    &viewport,
                );
            };
            let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
            let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
            dispatch(press.clone(), point, viewport);
            dispatch(release.clone(), point, viewport);
            let moved = Point::new(point.x + 30.0, point.y);
            dispatch(press.clone(), point, viewport);
            dispatch(
                Event::Mouse(mouse::Event::CursorMoved { position: moved }),
                moved,
                viewport,
            );
            dispatch(release.clone(), point, viewport);
            // Neither a cancelled press nor an off-viewport hit may activate a link.
            dispatch(press.clone(), point, viewport);
            dispatch(
                Event::Window(iced::window::Event::Unfocused),
                point,
                viewport,
            );
            dispatch(release.clone(), point, viewport);
            let clipped = Rectangle::new(Point::new(500.0, 500.0), Size::new(10.0, 10.0));
            dispatch(press, point, clipped);
            dispatch(release, point, clipped);
            let links: Vec<_> = messages
                .iter()
                .filter_map(|m| {
                    if let Message::Link(href) = m {
                        Some(href.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(links, ["#note"]);
            assert!(messages.iter().any(|m| matches!(m, Message::Move)));
        }
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
            wrapping: iced::advanced::text::Wrapping::WordOrGlyph,
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
