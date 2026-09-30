//! UI-independent reflow model, local HTML/EPUB loading, and reading positions.

#![deny(unsafe_op_in_unsafe_fn)]

use std::{collections::HashMap, path::PathBuf};

pub mod annotations;
pub mod backup;
pub mod dictionary;
pub mod epub;
mod html;
pub mod library;
pub mod managed;
pub mod position;
pub mod preferences;
pub mod reading;
pub mod recent;
pub mod shelves;
mod text;
pub use html::load_html;

/// Gutenberg's NCX labels sometimes wrap printed page numbers in braces.
/// Remove only that numeric decoration, never arbitrary source text.
pub fn display_page_label(label: &str) -> String {
    let label = label.trim();
    if let Some(inner) = label.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        let inner = inner.trim();
        if !inner.is_empty()
            && (inner.bytes().all(|b| b.is_ascii_digit())
                || inner.bytes().all(|b| b"ivxlcdmIVXLCDM".contains(&b)))
        {
            return inner.to_owned();
        }
    }
    label.to_owned()
}

#[cfg(test)]
mod page_label_tests {
    #[test]
    fn normalizes_printed_numbers_without_changing_other_labels() {
        for (input, expected) in [
            ("{45}", "45"),
            ("{vii}", "vii"),
            ("{Appendix}", "{Appendix}"),
            ("{}", "{}"),
            ("45", "45"),
        ] {
            assert_eq!(super::display_page_label(input), expected);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BaseDirection {
    Ltr,
    Rtl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InlineStyle {
    Bold,
    Italic,
    BoldItalic,
}

/// UTF-8 byte offsets into the paragraph's logical text, end exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StyleRun {
    pub start_byte: usize,
    pub end_byte: usize,
    pub style: InlineStyle,
}

/// Structural meaning, independent of the reader's fonts and colors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlockKind {
    #[default]
    Paragraph,
    ListItem,
    Caption,
    Preformatted,
    SceneBreak,
    TableRow,
    Formula,
    Footnote,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LinkKind {
    #[default]
    Reference,
    Note,
    Backlink,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    /// UTF-8 byte range in Item::text(), excluding collapsed surrounding space.
    pub start_byte: usize,
    pub end_byte: usize,
    pub href: String,
    pub kind: LinkKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockSemantics {
    pub kind: BlockKind,
    pub quote_depth: u16,
    pub list_depth: u16,
    pub figure: Option<usize>,
    /// Deterministic source element index within the unchanged HTML/chapter.
    pub source_node: usize,
    pub links: Vec<Link>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Heading {
        id: String,
        text: String,
        level: u8,
    },
    Paragraph {
        id: String,
        text: String,
        base_direction: BaseDirection,
        style_runs: Vec<StyleRun>,
    },
    /// Document-relative, stable local asset key.
    Image {
        id: String,
        asset_path: String,
    },
}

impl Item {
    pub fn id(&self) -> &str {
        match self {
            Self::Heading { id, .. } | Self::Paragraph { id, .. } | Self::Image { id, .. } => id,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Heading { text, .. } | Self::Paragraph { text, .. } => Some(text),
            Self::Image { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub item_id: String,
    pub byte_offset: usize,
}

#[derive(Debug)]
pub struct Document {
    pub path: PathBuf,
    pub title: String,
    pub author: Option<String>,
    pub fingerprint: String,
    pub items: Vec<Item>,
    pub structure: HashMap<String, BlockSemantics>,
    /// Source fragments and legacy item aliases resolve to actual readable items.
    pub anchors: HashMap<String, String>,
    /// Publisher page labels and fragment targets, in source order.
    pub page_breaks: Vec<(String, String)>,
    pub images: HashMap<String, ImageAsset>,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct ImageAsset {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Standalone HTML navigation is restricted to the currently opened file.
/// External URLs and other local documents remain readable link text.
pub fn resolve_html_link(path: &std::path::Path, href: &str) -> Result<Option<String>, String> {
    if href.len() > 4096 {
        return Err("Link target is too long".into());
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Document filename is not UTF-8")?;
    let (target, fragment) = epub::resolve_uri(name, href)?;
    if target != name {
        return Err("This link points outside the current document".into());
    }
    Ok(fragment)
}
