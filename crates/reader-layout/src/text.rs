//! Reading text as the shaper receives it: logical paragraphs mapped to font
//! runs (Literata and the bundled reading families, with platform fallback for
//! scripts they do not cover).
use reader_document::{BaseDirection, InlineStyle, StyleRun};

/// Controlled fixture face selected for one logical character run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FontRole {
    /// Noto Sans regular Latin face.
    LatinRegular,
    /// Noto Sans bold Latin face.
    LatinBold,
    /// Noto Sans italic Latin face.
    LatinItalic,
    /// Supplied regular Arabic face (no synthetic style).
    Arabic,
    /// Supplied regular Hebrew face (no synthetic style).
    Hebrew,
    /// Noto Sans JP variable face at the Iced weight corresponding to 400.
    Japanese,
    /// Native platform fallback for scripts absent from Literata.
    SystemRegular,
    SystemBold,
    SystemItalic,
    SystemBoldItalic,
    /// Bundled Literata faces for normal document text (not diagnostics).
    Code,
    CodeBold,
    CodeItalic,
    CodeBoldItalic,
    EditorialRegular,
    EditorialMedium,
    EditorialBold,
    EditorialItalic,
    EditorialBoldItalic,
}

impl FontRole {
    /// Iced-native font request; diagnostic Noto bytes come from the checked
    /// fixture, while normal document Literata bytes ship inside the shell.
    #[must_use]
    pub const fn iced_font(self) -> iced_core::Font {
        use iced_core::font::{Family, Font, Style, Weight};
        match self {
            Self::Code => Font::MONOSPACE,
            Self::CodeBold => Font {
                weight: Weight::Bold,
                ..Font::MONOSPACE
            },
            Self::CodeItalic => Font {
                style: Style::Italic,
                ..Font::MONOSPACE
            },
            Self::CodeBoldItalic => Font {
                weight: Weight::Bold,
                style: Style::Italic,
                ..Font::MONOSPACE
            },
            Self::LatinRegular => Font::with_name("Noto Sans"),
            Self::LatinBold => Font {
                weight: Weight::Bold,
                ..Font::with_name("Noto Sans")
            },
            Self::LatinItalic => Font {
                style: Style::Italic,
                ..Font::with_name("Noto Sans")
            },
            Self::Arabic => Font::with_name("Noto Sans Arabic"),
            Self::Hebrew => Font::with_name("Noto Sans Hebrew"),
            Self::Japanese => Font {
                family: Family::Name("Noto Sans JP"),
                // Iced maps Normal to cosmic-text weight 400. This requests
                // that variable-font instance but does not expose axis control.
                weight: Weight::Normal,
                ..Font::DEFAULT
            },
            Self::SystemRegular => Font::DEFAULT,
            Self::SystemBold => Font {
                weight: Weight::Bold,
                ..Font::DEFAULT
            },
            Self::SystemItalic => Font {
                style: Style::Italic,
                ..Font::DEFAULT
            },
            Self::SystemBoldItalic => Font {
                weight: Weight::Bold,
                style: Style::Italic,
                ..Font::DEFAULT
            },
            Self::EditorialRegular => Font::with_name("Literata"),
            Self::EditorialMedium => Font {
                weight: Weight::Medium,
                ..Font::with_name("Literata")
            },
            Self::EditorialBold => Font {
                weight: Weight::Bold,
                ..Font::with_name("Literata")
            },
            Self::EditorialItalic => Font {
                style: Style::Italic,
                ..Font::with_name("Literata")
            },
            Self::EditorialBoldItalic => Font {
                weight: Weight::Bold,
                style: Style::Italic,
                ..Font::with_name("Literata")
            },
        }
    }
}

/// Font decision for a byte-aligned range in mapped logical text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MappedRun {
    /// UTF-8 range in [`MappedParagraph::text`].
    pub bytes: std::ops::Range<usize>,
    /// Controlled fixture font requested for this run.
    pub role: FontRole,
}

/// Text and per-run native font requests for one workload paragraph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MappedParagraph {
    /// Logical Unicode text, with a leading RLM for explicit RTL intent only.
    pub text: String,
    /// Character-aligned style/script runs covering the text.
    pub runs: Vec<MappedRun>,
}

/// Maps logical text and fixture style ranges to Iced rich-text font runs.
///
/// Iced 0.14's public text API has no paragraph base-direction field. For RTL
/// workload items this prepends an RLM as a first-strong hint; logical text is
/// not reversed, and the actual native visual result remains an experiment.
pub fn map_paragraph(
    text: &str,
    base_direction: BaseDirection,
    style_runs: &[StyleRun],
) -> Result<MappedParagraph, String> {
    map_paragraph_with_rlm(
        text,
        base_direction,
        style_runs,
        base_direction == BaseDirection::Rtl,
    )
}

/// Maps product document text to Literata while allowing native script fallback.
pub fn map_document_paragraph(
    text: &str,
    base_direction: BaseDirection,
    style_runs: &[StyleRun],
) -> Result<MappedParagraph, String> {
    map_paragraph_with_fonts(
        text,
        base_direction,
        style_runs,
        base_direction == BaseDirection::Rtl,
        true,
    )
}

/// Maps logical text with an explicit leading-RLM choice (diagnostic conditions).
pub fn map_paragraph_with_rlm(
    text: &str,
    base_direction: BaseDirection,
    style_runs: &[StyleRun],
    leading_rlm: bool,
) -> Result<MappedParagraph, String> {
    map_paragraph_with_fonts(text, base_direction, style_runs, leading_rlm, false)
}

fn map_paragraph_with_fonts(
    text: &str,
    base_direction: BaseDirection,
    style_runs: &[StyleRun],
    leading_rlm: bool,
    document_fonts: bool,
) -> Result<MappedParagraph, String> {
    let mut previous_end = 0;
    for style_run in style_runs {
        if style_run.start_byte < previous_end
            || style_run.start_byte > style_run.end_byte
            || style_run.end_byte > text.len()
            || !text.is_char_boundary(style_run.start_byte)
            || !text.is_char_boundary(style_run.end_byte)
        {
            return Err(format!(
                "invalid UTF-8 style range {}..{} for {} bytes",
                style_run.start_byte,
                style_run.end_byte,
                text.len()
            ));
        }
        previous_end = style_run.end_byte;
    }

    let add_rlm = leading_rlm && base_direction == BaseDirection::Rtl;
    let mut mapped_text =
        String::with_capacity(text.len() + usize::from(add_rlm) * '\u{200f}'.len_utf8());
    if add_rlm {
        mapped_text.push('\u{200f}');
    }

    let mut runs: Vec<MappedRun> = Vec::new();
    for (source_start, character) in text.char_indices() {
        let source_end = source_start + character.len_utf8();
        let style = style_runs
            .iter()
            .find(|run| run.start_byte <= source_start && source_end <= run.end_byte)
            .map(|run| run.style);
        let role = if document_fonts {
            document_font_role(character, style)
        } else {
            font_role(character, style)
        };
        let mapped_start = mapped_text.len();
        mapped_text.push(character);
        let mapped_end = mapped_text.len();

        if let Some(last) = runs.last_mut()
            && last.role == role
            && last.bytes.end == mapped_start
        {
            last.bytes.end = mapped_end;
        } else {
            runs.push(MappedRun {
                bytes: mapped_start..mapped_end,
                role,
            });
        }
    }

    if add_rlm {
        if let Some(first) = runs.first_mut() {
            first.bytes.start = 0;
        } else {
            runs.push(MappedRun {
                bytes: 0..mapped_text.len(),
                role: if document_fonts {
                    FontRole::EditorialRegular
                } else {
                    FontRole::LatinRegular
                },
            });
        }
    }

    Ok(MappedParagraph {
        text: mapped_text,
        runs,
    })
}

fn font_role(character: char, style: Option<InlineStyle>) -> FontRole {
    if is_arabic(character) {
        return FontRole::Arabic;
    }
    if is_hebrew(character) {
        return FontRole::Hebrew;
    }
    if is_japanese_or_cjk(character) {
        return FontRole::Japanese;
    }
    match style {
        Some(InlineStyle::Bold | InlineStyle::BoldItalic) => FontRole::LatinBold,
        Some(InlineStyle::Italic) => FontRole::LatinItalic,
        None => FontRole::LatinRegular,
    }
}

fn document_font_role(character: char, style: Option<InlineStyle>) -> FontRole {
    // These scripts have no glyphs in Literata; keep platform script selection
    // instead of assigning a Latin font to an entire mixed run.
    if is_arabic(character) || is_hebrew(character) || is_japanese_or_cjk(character) {
        return system_font_role(style);
    }
    match style {
        Some(InlineStyle::Bold) => FontRole::EditorialBold,
        Some(InlineStyle::Italic) => FontRole::EditorialItalic,
        Some(InlineStyle::BoldItalic) => FontRole::EditorialBoldItalic,
        None => FontRole::EditorialRegular,
    }
}

fn system_font_role(style: Option<InlineStyle>) -> FontRole {
    match style {
        Some(InlineStyle::Bold) => FontRole::SystemBold,
        Some(InlineStyle::Italic) => FontRole::SystemItalic,
        Some(InlineStyle::BoldItalic) => FontRole::SystemBoldItalic,
        None => FontRole::SystemRegular,
    }
}

fn is_arabic(character: char) -> bool {
    matches!(
        character as u32,
        0x0600..=0x06ff
            | 0x0750..=0x077f
            | 0x0870..=0x089f
            | 0x08a0..=0x08ff
            | 0xfb50..=0xfdff
            | 0xfe70..=0xfeff
    )
}

fn is_hebrew(character: char) -> bool {
    matches!(character as u32, 0x0590..=0x05ff | 0xfb1d..=0xfb4f)
}

fn is_japanese_or_cjk(character: char) -> bool {
    matches!(
        character as u32,
        0x3000..=0x303f
            | 0x3040..=0x30ff
            | 0x31f0..=0x31ff
            | 0x3400..=0x4dbf
            | 0x4e00..=0x9fff
            | 0xff00..=0xffef
    )
}
