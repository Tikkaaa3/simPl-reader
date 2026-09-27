//! UI-independent reflow model, local HTML/EPUB loading, and reading positions.

#![deny(unsafe_op_in_unsafe_fn)]

use std::{collections::HashMap, path::PathBuf};

pub mod epub;
mod html;
pub mod library;
pub mod position;
pub mod recent;
pub use html::load_html;

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
    pub images: HashMap<String, ImageAsset>,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct ImageAsset {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
