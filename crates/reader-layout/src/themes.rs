//! Bundled reading themes: one choice sets the reading font, line and paragraph
//! spacing, and a light and a dark palette. Themes are plain data so more of
//! them (for example from a plugin) can be described the same way later.
use iced_core::Color;
use reader_document::preferences::Appearance;
use reader_document::reading::{Font, Options};

use crate::style::{BookStyle, MINIMAL};

/// Colors of one appearance of the reader (chrome and paper).
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub background: Color,
    pub surface: Color,
    pub raised: Color,
    pub lowest: Color,
    pub border: Color,
    pub text: Color,
    pub secondary: Color,
    pub muted: Color,
    pub accent: Color,
    pub danger: Color,
    pub control_border: Color,
}

/// Neutral surfaces with a restrained ice-blue accent.
pub const DARK: Palette = Palette {
    background: Color::from_rgb8(0x0c, 0x0c, 0x0c),
    surface: Color::from_rgb8(0x10, 0x10, 0x10),
    raised: Color::from_rgb8(0x16, 0x16, 0x16),
    lowest: Color::from_rgb8(0x0c, 0x0c, 0x0c),
    border: Color::from_rgb8(0x27, 0x27, 0x27),
    text: Color::from_rgb8(0xed, 0xed, 0xed),
    secondary: Color::from_rgb8(0xb5, 0xb5, 0xb5),
    muted: Color::from_rgb8(0x85, 0x85, 0x85),
    accent: Color::from_rgb8(0x58, 0xa6, 0xff),
    danger: Color::from_rgb8(0xff, 0xb4, 0xab),
    control_border: Color::from_rgb8(0x2a, 0x2a, 0x2a),
};

pub const LIGHT: Palette = Palette {
    background: Color::from_rgb8(0xff, 0xff, 0xff),
    surface: Color::from_rgb8(0xf6, 0xf6, 0xf6),
    raised: Color::from_rgb8(0xff, 0xff, 0xff),
    lowest: Color::from_rgb8(0xff, 0xff, 0xff),
    border: Color::from_rgb8(0xe5, 0xe5, 0xe5),
    text: Color::from_rgb8(0x10, 0x10, 0x10),
    secondary: Color::from_rgb8(0x52, 0x52, 0x52),
    muted: Color::from_rgb8(0x73, 0x73, 0x73),
    accent: Color::from_rgb8(0x09, 0x69, 0xda),
    danger: Color::from_rgb8(0xa3, 0x2b, 0x2b),
    control_border: Color::from_rgb8(0xe2, 0xe2, 0xe2),
};

pub const DEFAULT_ID: &str = "default";

#[derive(Debug)]
pub struct ReadingTheme {
    pub id: &'static str,
    pub name: &'static str,
    pub summary: &'static str,
    /// Bundled family used for document text; `None` keeps the default, Literata.
    pub family: Option<&'static str>,
    pub style: BookStyle,
    pub light: Palette,
    pub dark: Palette,
}

impl ReadingTheme {
    pub fn palette(&self, appearance: Appearance) -> Palette {
        match appearance {
            Appearance::Light => self.light,
            Appearance::Dark => self.dark,
        }
    }
}

const fn rgb(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

#[allow(clippy::too_many_arguments)]
const fn palette(
    background: u32,
    surface: u32,
    raised: u32,
    border: u32,
    text: u32,
    secondary: u32,
    muted: u32,
    accent: u32,
    danger: u32,
    control_border: u32,
) -> Palette {
    Palette {
        background: rgb(background),
        surface: rgb(surface),
        raised: rgb(raised),
        lowest: rgb(background),
        border: rgb(border),
        text: rgb(text),
        secondary: rgb(secondary),
        muted: rgb(muted),
        accent: rgb(accent),
        danger: rgb(danger),
        control_border: rgb(control_border),
    }
}

/// The order here is the order shown in the settings panel; the first is the default.
pub static THEMES: [ReadingTheme; 4] = [
    // The look simPl always had, with an ice-blue accent.
    ReadingTheme {
        id: DEFAULT_ID,
        name: "Default",
        summary: "Literata, balanced spacing",
        family: None,
        style: MINIMAL,
        light: LIGHT,
        dark: DARK,
    },
    // Warm, muted paper and charcoal; a slightly airier serif.
    ReadingTheme {
        id: "soft",
        name: "Soft",
        summary: "Spectral, warm and airy",
        family: Some("Spectral"),
        style: BookStyle {
            default_size: 20.0,
            line_height: 1.7,
            column_em: 36.0,
            paragraph_gap_em: 0.85,
            heading_scales: [1.55, 1.28, 1.1],
        },
        light: palette(
            0xe9e5dc, 0xf4f0e6, 0xf8f5ee, 0xd8d2c4, 0x2b2924, 0x57534b, 0x757064, 0x9a4a24,
            0xa3372b, 0xd4cebf,
        ),
        dark: palette(
            0x1a1917, 0x22211e, 0x2a2926, 0x36342f, 0xd8d4ca, 0xa8a499, 0x7f7b71, 0xe0a47a,
            0xe8a49b, 0x3b3934,
        ),
    },
    // A clear humanist sans with generous spacing and cool, quiet colors.
    ReadingTheme {
        id: "clear",
        name: "Clear",
        summary: "Fira Sans, roomy and cool",
        family: Some("Fira Sans"),
        style: BookStyle {
            default_size: 20.0,
            line_height: 1.75,
            column_em: 36.0,
            paragraph_gap_em: 0.9,
            heading_scales: [1.5, 1.25, 1.1],
        },
        light: palette(
            0xeceff3, 0xf7f8fa, 0xffffff, 0xd6dae1, 0x151a21, 0x434a57, 0x667080, 0x1a64d6,
            0xb3261e, 0xcfd4dc,
        ),
        dark: palette(
            0x111317, 0x171a1f, 0x1e2229, 0x2b3038, 0xe4e7ec, 0xaeb4bf, 0x8390a0, 0x6cb6ff,
            0xffb4ab, 0x30353d,
        ),
    },
    // Denser text with neutral colors for long reading sessions.
    ReadingTheme {
        id: "compact",
        name: "Compact",
        summary: "Literata, tighter spacing",
        family: None,
        style: BookStyle {
            default_size: 20.0,
            line_height: 1.45,
            column_em: 36.0,
            paragraph_gap_em: 0.55,
            heading_scales: [1.5, 1.25, 1.08],
        },
        light: palette(
            0xe6e6e3, 0xf3f3f0, 0xfafaf8, 0xd3d3ce, 0x1f1f1d, 0x50504c, 0x6f6f69, 0x2c6b5a,
            0xa32e2e, 0xcdcdc8,
        ),
        dark: palette(
            0x151516, 0x1c1c1e, 0x242426, 0x323235, 0xdcdcda, 0xa9a9a5, 0x80807c, 0x7cc4ae,
            0xf0a8a0, 0x37373a,
        ),
    },
];

pub fn default_theme() -> &'static ReadingTheme {
    &THEMES[0]
}

/// The theme with this id; an unknown or missing id (for example from a newer
/// version's preferences) falls back to the default.
pub fn find(id: &str) -> &'static ReadingTheme {
    THEMES
        .iter()
        .find(|theme| theme.id == id)
        .unwrap_or(&THEMES[0])
}

pub fn index_of(theme: &ReadingTheme) -> usize {
    THEMES
        .iter()
        .position(|candidate| candidate.id == theme.id)
        .unwrap_or(0)
}

/// Spacing and heading scales for a book: the theme's, except that PDF Book
/// keeps the default typography its layout was built around.
pub fn style_for(theme: &'static ReadingTheme, pdf_book: bool) -> &'static BookStyle {
    if pdf_book { &MINIMAL } else { &theme.style }
}

/// The bundled family replacing Literata for this book's text, if the theme has one.
pub fn family_for(theme: &'static ReadingTheme, pdf_book: bool) -> Option<&'static str> {
    if pdf_book { None } else { theme.family }
}

/// The typography a book is laid out with under `theme` and the reader's options.
pub fn effective_style(
    theme: &'static ReadingTheme,
    pdf_book: bool,
    options: Options,
) -> BookStyle {
    let mut style = *style_for(theme, pdf_book);
    if !pdf_book && options.spacing != 0 {
        style.line_height = options.spacing as f32 / 100.0;
    }
    style
}

/// The reading family a book is laid out with (`None` = Literata).
pub fn effective_family(
    theme: &'static ReadingTheme,
    pdf_book: bool,
    options: Options,
) -> Option<&'static str> {
    if pdf_book {
        return None;
    }
    match options.font {
        Font::Theme => family_for(theme, pdf_book),
        Font::Literata => None,
        Font::Spectral => Some("Spectral"),
        Font::FiraSans => Some("Fira Sans"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear(channel: f32) -> f32 {
        if channel <= 0.03928 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }

    fn luminance(color: Color) -> f32 {
        0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
    }

    /// WCAG contrast ratio between two opaque colors.
    fn contrast(a: Color, b: Color) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn the_first_theme_is_simpls_own_look() {
        let default = default_theme();
        assert_eq!(default.id, DEFAULT_ID);
        assert_eq!(default.family, None);
        assert_eq!(default.style.line_height, MINIMAL.line_height);
        assert_eq!(default.style.paragraph_gap_em, MINIMAL.paragraph_gap_em);
        assert_eq!(default.light.background, LIGHT.background);
        assert_eq!(default.dark.surface, DARK.surface);
    }

    #[test]
    fn ids_are_unique_and_unknown_ids_fall_back_to_the_default() {
        for (index, theme) in THEMES.iter().enumerate() {
            assert_eq!(THEMES.iter().filter(|t| t.id == theme.id).count(), 1);
            assert_eq!(index_of(find(theme.id)), index);
        }
        assert_eq!(find("no-such-theme").id, DEFAULT_ID);
        assert_eq!(find("").id, DEFAULT_ID);
    }

    #[test]
    fn every_palette_keeps_text_readable() {
        for theme in &THEMES {
            for appearance in [Appearance::Light, Appearance::Dark] {
                let colors = theme.palette(appearance);
                for (surface, name) in [(colors.surface, "paper"), (colors.background, "desk")] {
                    let text = contrast(colors.text, surface);
                    let secondary = contrast(colors.secondary, surface);
                    let muted = contrast(colors.muted, surface);
                    let label = format!("{} {appearance:?} on {name}", theme.name);
                    assert!(text >= 7.0, "{label}: text {text:.2}");
                    assert!(secondary >= 4.5, "{label}: secondary {secondary:.2}");
                    assert!(muted >= 3.0, "{label}: muted {muted:.2}");
                }
                for (surface, name) in [(colors.surface, "paper"), (colors.background, "desk")] {
                    let accent = contrast(colors.accent, surface);
                    assert!(
                        accent >= 3.0,
                        "{} {appearance:?}: accent on {name} {accent:.2}",
                        theme.name
                    );
                }
                let danger = contrast(colors.danger, colors.surface);
                assert!(
                    danger >= 3.0,
                    "{} {appearance:?}: danger {danger:.2}",
                    theme.name
                );
            }
        }
    }

    #[test]
    fn light_themes_are_light_and_dark_themes_are_dark() {
        for theme in &THEMES {
            assert!(luminance(theme.light.surface) > 0.5, "{}", theme.name);
            assert!(luminance(theme.dark.surface) < 0.05, "{}", theme.name);
        }
    }

    #[test]
    fn spacing_stays_within_comfortable_bounds() {
        for theme in &THEMES {
            assert!(
                (1.4..=1.9).contains(&theme.style.line_height),
                "{}",
                theme.name
            );
            assert!(
                (0.4..=1.0).contains(&theme.style.paragraph_gap_em),
                "{}",
                theme.name
            );
            assert_eq!(
                theme.style.default_size, MINIMAL.default_size,
                "{}",
                theme.name
            );
        }
    }
}
