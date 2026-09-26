//! The single shared layout recipe for the two framework PoCs.
//!
//! These are PoC test inputs agreed for the comparison campaign, not
//! product UI defaults. Adapters must render with these exact inputs;
//! identical glyph rasterization and exact wrap offsets across engines
//! are deliberately *not* prescribed.
//!
//! This module is the single place the shared inputs are documented;
//! `fixtures/reader-workload/README.md` points here instead of repeating
//! the values.

/// Shared layout/font inputs, in logical pixels (DIP: device-independent
/// pixels; 1 DIP = 1 logical pixel at 1x scale).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutRecipe {
    /// Unit documentation string.
    pub units: &'static str,
    /// Content widths to exercise: wide and narrow.
    pub content_widths_dip: [u32; 2],
    /// Viewport height used in the comparison scenarios.
    pub viewport_height_dip: u32,
    /// Body font size.
    pub body_font_size_dip: f32,
    /// Body line height.
    pub body_line_height_dip: f32,
    /// Heading font size.
    pub heading_font_size_dip: f32,
    /// Heading line height.
    pub heading_line_height_dip: f32,
    /// Gap between consecutive block items.
    pub paragraph_gap_dip: f32,
    /// Image display size.
    pub image_display_size_dip: (u32, u32),
}

/// The one shared layout recipe.
pub const LAYOUT_RECIPE: LayoutRecipe = LayoutRecipe {
    units: "DIP (device-independent pixels; logical pixels)",
    content_widths_dip: [800, 480],
    viewport_height_dip: 600,
    body_font_size_dip: 18.0,
    body_line_height_dip: 27.0,
    heading_font_size_dip: 28.0,
    heading_line_height_dip: 38.0,
    paragraph_gap_dip: 12.0,
    image_display_size_dip: (240, 160),
};
