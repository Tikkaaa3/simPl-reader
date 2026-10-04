//! Bundled reading themes. The theme data lives in `reader-layout` (shared with
//! the Android app); this module maps each theme to an iced theme.
use std::sync::LazyLock;

use iced::Theme;
use reader_document::preferences::Appearance;

use crate::ui::{self, Palette};
pub use reader_layout::themes::{DEFAULT_ID, ReadingTheme, THEMES, default_theme, find};

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
