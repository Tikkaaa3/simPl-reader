//! Bundled reading themes: one choice sets the reading font, line and paragraph
//! spacing, and a light and a dark palette. Themes are plain data so more of
//! them (for example from a plugin) can be described the same way later.
use std::sync::LazyLock;

use iced::{Color, Theme};
use reader_document::preferences::Appearance;

use crate::book_style::{BookStyle, MINIMAL};
use crate::ui::{self, Palette};

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
    danger: u32,
    button_bg: u32,
    button_text: u32,
    button_hover: u32,
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
        accent: rgb(text),
        danger: rgb(danger),
        button_bg: rgb(button_bg),
        button_text: rgb(button_text),
        button_hover: rgb(button_hover),
        control_border: rgb(control_border),
    }
}

/// The order here is the order shown in the settings panel; the first is the default.
pub static THEMES: [ReadingTheme; 4] = [
    // The look simPl always had.
    ReadingTheme {
        id: DEFAULT_ID,
        name: "Default",
        summary: "Literata, balanced spacing",
        family: None,
        style: MINIMAL,
        light: ui::LIGHT,
        dark: ui::DARK,
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
            0xe9e5dc, 0xf4f0e6, 0xf8f5ee, 0xd8d2c4, 0x2b2924, 0x57534b, 0x757064, 0xa3372b,
            0x3a3730, 0xf4f0e6, 0x25231e, 0xd4cebf,
        ),
        dark: palette(
            0x1a1917, 0x22211e, 0x2a2926, 0x36342f, 0xd8d4ca, 0xa8a499, 0x7f7b71, 0xe8a49b,
            0xd8d4ca, 0x22211e, 0xece8de, 0x3b3934,
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
            0xeceff3, 0xf7f8fa, 0xffffff, 0xd6dae1, 0x151a21, 0x434a57, 0x667080, 0xb3261e,
            0x1f2530, 0xf4f6f9, 0x0d1117, 0xcfd4dc,
        ),
        dark: palette(
            0x111317, 0x171a1f, 0x1e2229, 0x2b3038, 0xe4e7ec, 0xaeb4bf, 0x8390a0, 0xffb4ab,
            0xe4e7ec, 0x171a1f, 0xffffff, 0x30353d,
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
            0xe6e6e3, 0xf3f3f0, 0xfafaf8, 0xd3d3ce, 0x1f1f1d, 0x50504c, 0x6f6f69, 0xa32e2e,
            0x2a2a28, 0xf3f3f0, 0x181816, 0xcdcdc8,
        ),
        dark: palette(
            0x151516, 0x1c1c1e, 0x242426, 0x323235, 0xdcdcda, 0xa9a9a5, 0x80807c, 0xf0a8a0,
            0xdcdcda, 0x1c1c1e, 0xeeeeec, 0x37373a,
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

#[cfg(test)]
pub fn index_of(theme: &ReadingTheme) -> usize {
    THEMES
        .iter()
        .position(|candidate| candidate.id == theme.id)
        .unwrap_or(0)
}

fn theme_name(theme: &ReadingTheme, appearance: Appearance) -> String {
    format!(
        "simPl {} {}",
        theme.name,
        match appearance {
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        }
    )
}

/// One iced theme per (reading theme, appearance), named so the palette can be
/// found again from the iced theme alone.
static ICED_THEMES: LazyLock<Vec<(String, Palette, Theme)>> = LazyLock::new(|| {
    THEMES
        .iter()
        .flat_map(|theme| {
            [Appearance::Light, Appearance::Dark]
                .into_iter()
                .map(|appearance| {
                    let name = theme_name(theme, appearance);
                    let colors = theme.palette(appearance);
                    let iced = ui::make_theme(colors, &name);
                    (name, colors, iced)
                })
        })
        .collect()
});

pub fn iced_theme(theme: &ReadingTheme, appearance: Appearance) -> Theme {
    let name = theme_name(theme, appearance);
    ICED_THEMES
        .iter()
        .find(|(candidate, _, _)| *candidate == name)
        .map_or_else(|| ICED_THEMES[0].2.clone(), |(_, _, iced)| iced.clone())
}

/// The palette of an iced theme made by [`iced_theme`], if it is one of ours.
pub fn palette_of(theme: &Theme) -> Option<Palette> {
    // Our themes are shared, so the same allocation identifies them without comparing names.
    let Theme::Custom(custom) = theme else {
        return None;
    };
    ICED_THEMES
        .iter()
        .find(|(_, _, iced)| matches!(iced, Theme::Custom(ours) if std::sync::Arc::ptr_eq(ours, custom)))
        .map(|(_, colors, _)| *colors)
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
    fn the_first_theme_is_the_look_simpl_always_had() {
        let default = default_theme();
        assert_eq!(default.id, DEFAULT_ID);
        assert_eq!(default.family, None);
        assert_eq!(default.style.line_height, MINIMAL.line_height);
        assert_eq!(default.style.paragraph_gap_em, MINIMAL.paragraph_gap_em);
        assert_eq!(default.light.background, ui::LIGHT.background);
        assert_eq!(default.dark.surface, ui::DARK.surface);
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
                let button = contrast(colors.button_text, colors.button_bg);
                assert!(
                    button >= 7.0,
                    "{} {appearance:?}: button {button:.2}",
                    theme.name
                );
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

    /// Every font file we embed must ship with its license text, or the package would
    /// carry a font without the notice its license requires.
    #[test]
    fn every_bundled_font_file_has_a_license_notice_that_the_package_ships() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let notices = [
            ("Geist", "Geist-OFL.txt"),
            ("Inter", "Inter-OFL.txt"),
            ("Literata", "Literata-OFL.txt"),
            ("Spectral", "Spectral-OFL.txt"),
            ("FiraSans", "FiraSans-OFL.txt"),
            ("MaterialSymbols", "Material-Symbols-LICENSE.txt"),
        ];
        let package = std::fs::read_to_string(root.join("scripts/package.ps1")).unwrap();
        let sources =
            std::fs::read_to_string(root.join("assets/licenses/Typeface-SOURCES.txt")).unwrap();
        let mut seen = 0;
        for entry in std::fs::read_dir(root.join("assets/fonts")).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            let Some((family, notice)) =
                notices.iter().find(|(prefix, _)| name.starts_with(prefix))
            else {
                panic!(
                    "{name} has no license notice: list it here, in assets/licenses and in scripts/package.ps1"
                );
            };
            let text = std::fs::read_to_string(root.join("assets/licenses").join(notice))
                .unwrap_or_else(|_| panic!("{notice} is missing"));
            assert!(
                text.contains("SIL OPEN FONT LICENSE") || text.contains("Apache License"),
                "{notice} does not look like a license text"
            );
            assert!(
                package.contains(notice),
                "scripts/package.ps1 does not ship {notice}"
            );
            assert!(
                sources.contains(family) || sources.contains("Material"),
                "{family} is not in Typeface-SOURCES.txt"
            );
            seen += 1;
        }
        assert!(
            seen >= 16,
            "expected to find the bundled fonts, found {seen}"
        );
    }

    #[test]
    fn iced_themes_map_back_to_their_palette() {
        for theme in &THEMES {
            for appearance in [Appearance::Light, Appearance::Dark] {
                let found = palette_of(&iced_theme(theme, appearance)).expect("our theme");
                assert_eq!(found.surface, theme.palette(appearance).surface);
            }
        }
        assert!(palette_of(&Theme::Dark).is_none());
    }
}
