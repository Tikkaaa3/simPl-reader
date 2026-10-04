//! The layout rule of one reading paragraph. The desktop's selectable paragraph
//! widget and the window-free measurement both build their text with these
//! functions, so a canonical page atlas measures exactly what the reader draws.
use iced_core::alignment;
use iced_core::layout::{self, Limits, Node};
use iced_core::text::{Alignment, LineHeight, Shaping, Span, Wrapping};
use iced_core::{Font, Length, Pixels, Size};

use crate::text::MappedParagraph;

/// The paragraph-level font; every span names its own, so this is only a default.
const PARAGRAPH_FONT: Font = Font::with_name("Noto Sans");

/// Swaps the default reading face (Literata) for another bundled family, keeping
/// weight and style; monospace and other faces are left alone.
pub fn with_reading_family(font: Font, family: &'static str) -> Font {
    if font.family == iced_core::font::Family::Name("Literata") {
        Font {
            family: iced_core::font::Family::Name(family),
            ..font
        }
    } else {
        font
    }
}

/// The shaped runs of a mapped paragraph: its text and font per run.
pub fn runs(mapped: &MappedParagraph, family: Option<&'static str>) -> Vec<(String, Font)> {
    mapped
        .runs
        .iter()
        .map(|run| {
            let font = run.role.iced_font();
            (
                mapped.text[run.bytes.clone()].to_owned(),
                family.map_or(font, |family| with_reading_family(font, family)),
            )
        })
        .collect()
}

/// Rich-text spans for runs of `(text, font)` at the paragraph's size and line height.
pub fn spans<'a>(
    runs: impl IntoIterator<Item = (&'a str, Font)>,
    font_size: f32,
    line_height: f32,
) -> Vec<Span<'static, (), Font>> {
    runs.into_iter()
        .map(|(text, font)| {
            Span::new(text.to_owned())
                .font(font)
                .size(font_size)
                .line_height(LineHeight::Absolute(Pixels(line_height)))
        })
        .collect()
}

/// The text request for a paragraph's content within `bounds`.
pub fn text<C>(content: C, bounds: Size, font_size: f32, line_height: f32) -> iced_core::Text<C> {
    iced_core::Text {
        content,
        bounds,
        size: Pixels(font_size),
        line_height: LineHeight::Absolute(Pixels(line_height)),
        font: PARAGRAPH_FONT,
        align_x: Alignment::Default,
        align_y: alignment::Vertical::Top,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::WordOrGlyph,
    }
}

/// Centered and right-aligned paragraphs shrink to their text; others fill.
pub fn width(alignment: Alignment) -> Length {
    if matches!(alignment, Alignment::Center | Alignment::Right) {
        Length::Shrink
    } else {
        Length::Fill
    }
}

/// The paragraph's node: `shape` lays the text out within the available
/// bounds and returns its size.
pub fn node(limits: &Limits, alignment: Alignment, shape: impl FnOnce(Size) -> Size) -> Node {
    let bounds = limits.max();
    let limits = Limits::new(Size::ZERO, bounds);
    layout::sized(&limits, width(alignment), Length::Shrink, |_| shape(bounds))
}
