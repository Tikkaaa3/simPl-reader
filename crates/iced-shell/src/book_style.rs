//! simPl Minimal: presentation only, independent of document extraction and chrome.
use iced::{Theme, widget::container};
use reader_document::{BlockKind, BlockSemantics, Item};

pub const MINIMAL: BookStyle = BookStyle {
    default_size: 20.0,
    line_height: 1.6,
    column_em: 36.0,
    paragraph_gap_em: 0.75,
    heading_scales: [1.6, 1.3, 1.1],
};

pub struct BookStyle {
    pub default_size: f32,
    pub line_height: f32,
    pub column_em: f32,
    pub paragraph_gap_em: f32,
    pub heading_scales: [f32; 3],
}

impl BookStyle {
    pub fn side_padding(&self, window_width: f32) -> f32 {
        if window_width < 780.0 { 16.0 } else { 32.0 }
    }

    pub fn width(&self, window_width: f32, size: f32) -> f32 {
        (window_width - self.side_padding(window_width) * 2.0 - 14.0)
            .max(1.0)
            .min(self.column_em * size)
    }

    pub fn text_size(&self, item: &Item, body: f32) -> f32 {
        match item {
            Item::Heading { level, .. } => {
                body * self.heading_scales[level.saturating_sub(1).min(2) as usize]
            }
            _ => body,
        }
    }

    pub fn heading_space(&self, item: &Item, body: f32) -> f32 {
        if matches!(item, Item::Heading { .. }) {
            body
        } else {
            0.0
        }
    }

    pub fn block_size(&self, item: &Item, semantics: Option<&BlockSemantics>, body: f32) -> f32 {
        let scale = match semantics.map(|s| s.kind) {
            Some(BlockKind::Caption) => 0.85,
            Some(BlockKind::Footnote) => 0.9,
            Some(BlockKind::Preformatted | BlockKind::Formula | BlockKind::TableRow) => 0.85,
            _ => 1.0,
        };
        self.text_size(item, body) * scale
    }
    pub fn block_padding(
        &self,
        item: &Item,
        semantics: Option<&BlockSemantics>,
        body: f32,
        width: f32,
    ) -> iced::Padding {
        let mut padding = iced::Padding {
            top: self.heading_space(item, body),
            ..Default::default()
        };
        if let Some(s) = semantics {
            let depth = s.quote_depth.saturating_add(s.list_depth.saturating_sub(1));
            padding.left = (f32::from(depth) * body).min(width * 0.2);
            if s.quote_depth > 0 {
                padding.right = body.min(width * 0.1);
            }
            if matches!(
                s.kind,
                BlockKind::Preformatted | BlockKind::Formula | BlockKind::TableRow
            ) {
                padding.top += body * 0.4;
                padding.bottom = body * 0.4;
                padding.left += body * 0.5;
                padding.right += body * 0.5;
            }
        }
        padding
    }
    pub fn gap(&self, body: f32) -> f32 {
        body * self.paragraph_gap_em
    }
}

pub fn surface(theme: &Theme) -> container::Style {
    let colors = crate::ui::palette(theme);
    container::Style {
        background: Some(colors.surface.into()),
        text_color: Some(colors.text),
        ..Default::default()
    }
}

pub fn paper(theme: &Theme) -> container::Style {
    let mut style = surface(theme);
    style.border = iced::Border {
        color: crate::ui::palette(theme).border,
        width: 1.0,
        radius: 2.0.into(),
    };
    style
}

pub fn desk(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(crate::ui::palette(theme).background.into()),
        ..Default::default()
    }
}

pub fn block_surface(theme: &Theme, kind: BlockKind, quote: bool) -> container::Style {
    let mut style = container::Style::default();
    let text = surface(theme).text_color.expect("book text palette");
    if matches!(
        kind,
        BlockKind::Preformatted | BlockKind::Formula | BlockKind::TableRow
    ) {
        style.background = Some(text.scale_alpha(0.04).into());
    }
    if quote || matches!(kind, BlockKind::Caption | BlockKind::SceneBreak) {
        style.text_color = Some(crate::ui::palette(theme).secondary);
    }
    style
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn column_scales_with_font_and_fits_narrow_windows() {
        assert_eq!(MINIMAL.width(1280.0, 20.0), 720.0);
        assert_eq!(MINIMAL.width(1280.0, 24.0), 864.0);
        assert_eq!(MINIMAL.width(540.0, 20.0), 494.0);
        assert!(MINIMAL.width(540.0, 36.0) < 540.0 - 32.0);
    }
}
