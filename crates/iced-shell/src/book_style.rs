//! simPl Minimal: presentation only, independent of document extraction and chrome.
//! The typography metrics live in `reader-layout`, shared with the Android app.
use iced::{Theme, widget::container};
use reader_document::BlockKind;
pub use reader_layout::style::{BookStyle, MINIMAL};

pub fn surface(theme: &Theme) -> container::Style {
    let colors = crate::ui::palette(theme);
    container::Style {
        background: Some(colors.surface.into()),
        text_color: Some(colors.text),
        ..Default::default()
    }
}

/// A sheet lifted off the desk by tone and a faint rim rather than a hard outline.
pub fn paper(theme: &Theme) -> container::Style {
    let mut style = surface(theme);
    let dark = theme.extended_palette().is_dark;
    style.border = iced::Border {
        color: crate::ui::palette(theme)
            .border
            .scale_alpha(if dark { 0.45 } else { 0.7 }),
        width: 1.0,
        radius: 4.0.into(),
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
