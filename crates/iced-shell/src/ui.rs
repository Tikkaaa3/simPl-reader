//! Shared chrome styles and keyboard-focus visibility for the normal reader.

use std::{cell::RefCell, sync::LazyLock};

use iced::advanced::widget::{Id, Operation, operation};
use iced::widget::{button, container, scrollable, text_input};
use iced::{Border, Color, Font, Rectangle, Theme, Vector};

#[derive(Clone, Copy)]
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
    pub button_bg: Color,
    pub button_text: Color,
    pub button_hover: Color,
    pub control_border: Color,
}

const DARK: Palette = Palette {
    background: Color::from_rgb8(0x0c, 0x0c, 0x0c),
    surface: Color::from_rgb8(0x10, 0x10, 0x10),
    raised: Color::from_rgb8(0x16, 0x16, 0x16),
    lowest: Color::from_rgb8(0x0c, 0x0c, 0x0c),
    border: Color::from_rgb8(0x27, 0x27, 0x27),
    text: Color::from_rgb8(0xed, 0xed, 0xed),
    secondary: Color::from_rgb8(0xb5, 0xb5, 0xb5),
    muted: Color::from_rgb8(0x85, 0x85, 0x85),
    accent: Color::from_rgb8(0xed, 0xed, 0xed),
    danger: Color::from_rgb8(0xff, 0xb4, 0xab),
    button_bg: Color::from_rgb8(0xe5, 0xe5, 0xe5),
    button_text: Color::from_rgb8(0x17, 0x17, 0x17),
    button_hover: Color::from_rgb8(0xff, 0xff, 0xff),
    control_border: Color::from_rgb8(0x2a, 0x2a, 0x2a),
};

const LIGHT: Palette = Palette {
    background: Color::from_rgb8(0xff, 0xff, 0xff),
    surface: Color::from_rgb8(0xf6, 0xf6, 0xf6),
    raised: Color::from_rgb8(0xff, 0xff, 0xff),
    lowest: Color::from_rgb8(0xff, 0xff, 0xff),
    border: Color::from_rgb8(0xe5, 0xe5, 0xe5),
    text: Color::from_rgb8(0x10, 0x10, 0x10),
    secondary: Color::from_rgb8(0x52, 0x52, 0x52),
    muted: Color::from_rgb8(0x73, 0x73, 0x73),
    accent: Color::from_rgb8(0x10, 0x10, 0x10),
    danger: Color::from_rgb8(0xa3, 0x2b, 0x2b),
    button_bg: Color::from_rgb8(0x17, 0x17, 0x17),
    button_text: Color::from_rgb8(0xf5, 0xf5, 0xf5),
    button_hover: Color::from_rgb8(0x00, 0x00, 0x00),
    control_border: Color::from_rgb8(0xe2, 0xe2, 0xe2),
};
pub fn palette(theme: &Theme) -> Palette {
    if theme.extended_palette().is_dark {
        DARK
    } else {
        LIGHT
    }
}

pub fn primary_text(theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(palette(theme).text),
    }
}

pub fn secondary_text(theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(palette(theme).secondary),
    }
}

pub fn muted_text(theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(palette(theme).muted),
    }
}

pub fn accent_text(theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(palette(theme).accent),
    }
}

pub fn danger_text(theme: &Theme) -> iced::widget::text::Style {
    iced::widget::text::Style {
        color: Some(palette(theme).danger),
    }
}

pub const SANS: Font = Font::with_name("Geist");
pub const MEDIUM: Font = Font::with_name("simPl UI 560");
pub const SEMIBOLD: Font = MEDIUM;
pub const SERIF: Font = Font::with_name("Literata");
pub const ICONS: Font = Font::with_name("Material Symbols Outlined");

/// The text faces retain upstream glyph coverage; native fallback handles other scripts.
/// Embedded bytes keep the portable executable independent of installed fonts.
pub fn font_data() -> [&'static [u8]; 8] {
    [
        include_bytes!("../../../assets/fonts/Geist-Variable-Latin.ttf"),
        include_bytes!("../../../assets/fonts/Geist-UI-560.ttf"),
        include_bytes!("../../../assets/fonts/Literata-Regular.ttf"),
        include_bytes!("../../../assets/fonts/Literata-Medium.ttf"),
        include_bytes!("../../../assets/fonts/Literata-Bold.ttf"),
        include_bytes!("../../../assets/fonts/Literata-Italic.ttf"),
        include_bytes!("../../../assets/fonts/Literata-BoldItalic.ttf"),
        include_bytes!("../../../assets/fonts/MaterialSymbolsOutlined-Subset.ttf"),
    ]
}

fn make_theme(colors: Palette, name: &str) -> Theme {
    Theme::custom(
        name.to_owned(),
        iced::theme::Palette {
            background: colors.background,
            text: colors.text,
            primary: colors.accent,
            success: colors.accent,
            warning: colors.accent,
            danger: colors.danger,
        },
    )
}
static LIGHT_THEME: LazyLock<Theme> = LazyLock::new(|| make_theme(LIGHT, "simPl Light"));
static DARK_THEME: LazyLock<Theme> = LazyLock::new(|| make_theme(DARK, "simPl Dark"));

pub fn theme(appearance: reader_document::preferences::Appearance) -> Theme {
    match appearance {
        reader_document::preferences::Appearance::Light => LIGHT_THEME.clone(),
        reader_document::preferences::Appearance::Dark => DARK_THEME.clone(),
    }
}

#[derive(Clone, Copy)]
pub enum ButtonTone {
    Quiet,
    Surface,
    Subtle,
    Destructive,
}

pub fn button_style(
    theme: &Theme,
    status: button::Status,
    tone: ButtonTone,
    focused: bool,
    selected: bool,
) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    let disabled = matches!(status, button::Status::Disabled);
    let (background, text_color) = if disabled {
        (None, palette(theme).muted)
    } else {
        match tone {
            ButtonTone::Destructive if hovered => (
                Some(palette(theme).danger.scale_alpha(0.12)),
                palette(theme).danger,
            ),
            ButtonTone::Quiet => (
                Some(if hovered {
                    palette(theme).button_hover
                } else {
                    palette(theme).button_bg
                }),
                palette(theme).button_text,
            ),
            ButtonTone::Surface => (
                Some(if hovered || selected {
                    palette(theme).raised
                } else {
                    palette(theme).surface
                }),
                palette(theme).text,
            ),
            _ if selected => (Some(palette(theme).raised), palette(theme).text),
            _ if hovered => (Some(palette(theme).surface), palette(theme).text),
            ButtonTone::Subtle | ButtonTone::Destructive => (None, palette(theme).secondary),
        }
    };
    button::Style {
        background: background.map(Into::into),
        text_color,
        border: Border {
            color: if focused && !disabled {
                palette(theme).accent
            } else if matches!(tone, ButtonTone::Surface) && !disabled {
                if selected {
                    palette(theme).secondary.scale_alpha(0.4)
                } else {
                    palette(theme)
                        .border
                        .scale_alpha(if hovered { 0.5 } else { 0.2 })
                }
            } else {
                Color::TRANSPARENT
            },
            width: if focused && !disabled {
                2.0
            } else if matches!(tone, ButtonTone::Surface) && !disabled {
                1.0
            } else {
                0.0
            },
            radius: 8.0.into(),
        },
        ..button::Style::default()
    }
}

pub fn panel(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(palette(theme).surface.into()),
        border: Border {
            color: palette(theme).border,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..container::Style::default()
    }
}

pub fn header(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(palette(theme).lowest.into()),
        ..container::Style::default()
    }
}

pub fn inset(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(palette(theme).raised.into()),
        border: Border {
            radius: 4.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

pub fn input_style(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let focused = matches!(status, text_input::Status::Focused { .. });
    text_input::Style {
        background: palette(theme).raised.into(),
        border: Border {
            color: if focused {
                palette(theme).accent
            } else {
                palette(theme).control_border
            },
            width: if focused { 2.0 } else { 1.0 },
            radius: 4.0.into(),
        },
        icon: palette(theme).muted,
        placeholder: palette(theme).muted,
        value: if matches!(status, text_input::Status::Disabled) {
            palette(theme).muted
        } else {
            palette(theme).text
        },
        selection: palette(theme).accent.scale_alpha(0.25),
    }
}

pub fn scrollbar() -> scrollable::Scrollbar {
    scrollable::Scrollbar::new()
        .width(4)
        .scroller_width(4)
        .margin(2)
}

pub fn vertical_scrollbar() -> scrollable::Direction {
    scrollable::Direction::Vertical(scrollbar())
}

pub fn scroll_style(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let mut style = scrollable::default(theme, status);
    let (vertical_disabled, horizontal_disabled, vertical_active, horizontal_active) = match status
    {
        scrollable::Status::Active {
            is_vertical_scrollbar_disabled,
            is_horizontal_scrollbar_disabled,
        } => (
            is_vertical_scrollbar_disabled,
            is_horizontal_scrollbar_disabled,
            false,
            false,
        ),
        scrollable::Status::Hovered {
            is_vertical_scrollbar_disabled,
            is_horizontal_scrollbar_disabled,
            is_vertical_scrollbar_hovered,
            is_horizontal_scrollbar_hovered,
        } => (
            is_vertical_scrollbar_disabled,
            is_horizontal_scrollbar_disabled,
            is_vertical_scrollbar_hovered,
            is_horizontal_scrollbar_hovered,
        ),
        scrollable::Status::Dragged {
            is_vertical_scrollbar_disabled,
            is_horizontal_scrollbar_disabled,
            is_vertical_scrollbar_dragged,
            is_horizontal_scrollbar_dragged,
        } => (
            is_vertical_scrollbar_disabled,
            is_horizontal_scrollbar_disabled,
            is_vertical_scrollbar_dragged,
            is_horizontal_scrollbar_dragged,
        ),
    };
    for (rail, disabled, active) in [
        (&mut style.vertical_rail, vertical_disabled, vertical_active),
        (
            &mut style.horizontal_rail,
            horizontal_disabled,
            horizontal_active,
        ),
    ] {
        rail.background = None;
        rail.scroller.background = if disabled {
            Color::TRANSPARENT
        } else if active {
            palette(theme).accent
        } else {
            palette(theme).muted
        }
        .into();
        rail.scroller.border.radius = 3.0.into();
    }
    style
}

pub const FOCUSED_CONTROL: &str = "focused-reader-control";

/// Reveal the focused chrome control through every enclosing vertical panel.
/// Runs on focus/panel changes and window resizing, never on idle frames.
pub fn reveal_focus<Message: Send + 'static>() -> iced::Task<Message> {
    iced::advanced::widget::operate(RevealFocus::default())
}

struct FocusViewport {
    id: Option<Id>,
    bounds: Rectangle,
    content_height: f32,
    offset: f32,
}

#[derive(Default)]
struct RevealFocus {
    target: Option<Rectangle>,
    pending: Option<FocusViewport>,
    // Empty/visible focus allocates no adjustment storage. Only ancestors that
    // actually need scrolling are retained for the second widget-tree pass.
    changes: RefCell<Vec<(Id, f32)>>,
}

impl<T> Operation<T> for RevealFocus {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation<T>)) {
        if self.target.is_some() {
            return;
        }
        let viewport = self.pending.take();
        visit(self);
        if let (Some(viewport), Some(target)) = (viewport, self.target.as_mut()) {
            target.y -= viewport.offset;
            let delta = if target.height > viewport.bounds.height || target.y < viewport.bounds.y {
                target.y - viewport.bounds.y
            } else {
                (target.y + target.height - viewport.bounds.y - viewport.bounds.height).max(0.0)
            };
            let offset = (viewport.offset + delta).clamp(
                0.0,
                (viewport.content_height - viewport.bounds.height).max(0.0),
            );
            if let Some(id) = viewport.id
                && (offset - viewport.offset).abs() > 0.5
            {
                self.changes.get_mut().push((id, offset));
                target.y -= offset - viewport.offset;
            }
        }
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&Id::new(FOCUSED_CONTROL)) {
            self.target = Some(bounds);
        }
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        translation: Vector,
        _: &mut dyn operation::Scrollable,
    ) {
        self.pending = Some(FocusViewport {
            id: id.cloned(),
            bounds,
            content_height: content_bounds.height,
            offset: translation.y,
        });
    }

    fn finish(&self) -> operation::Outcome<T> {
        let changes = self.changes.take();
        if changes.is_empty() {
            operation::Outcome::None
        } else {
            operation::Outcome::Chain(Box::new(ApplyFocusScroll(changes)))
        }
    }
}

struct ApplyFocusScroll(Vec<(Id, f32)>);

impl<T> Operation<T> for ApplyFocusScroll {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation<T>)) {
        visit(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        _: Rectangle,
        _: Rectangle,
        _: Vector,
        state: &mut dyn operation::Scrollable,
    ) {
        if let Some((_, offset)) = self.0.iter().find(|(target, _)| Some(target) == id) {
            state.scroll_to(operation::scrollable::AbsoluteOffset {
                x: None,
                y: Some(*offset),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use operation::scrollable::{AbsoluteOffset, RelativeOffset};

    struct ReadOnlyScroll;

    impl operation::Scrollable for ReadOnlyScroll {
        fn snap_to(&mut self, _: RelativeOffset<Option<f32>>) {
            panic!("discovery must not mutate scroll state");
        }

        fn scroll_to(&mut self, _: AbsoluteOffset<Option<f32>>) {
            panic!("discovery must not mutate scroll state");
        }

        fn scroll_by(&mut self, _: AbsoluteOffset, _: Rectangle, _: Rectangle) {
            panic!("discovery must not mutate scroll state");
        }
    }

    fn nested_focus(outer_offset: f32, inner_offset: f32, target_y: f32) -> RevealFocus {
        let outer = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 100.0,
        };
        let inner = Rectangle {
            y: 120.0,
            height: 60.0,
            ..outer
        };
        let mut reveal = RevealFocus {
            pending: Some(FocusViewport {
                id: Some(Id::new("outer")),
                bounds: outer,
                content_height: 500.0,
                offset: outer_offset,
            }),
            ..RevealFocus::default()
        };
        Operation::<()>::traverse(&mut reveal, &mut |operation| {
            operation.scrollable(
                Some(&Id::new("inner")),
                inner,
                Rectangle {
                    height: 500.0,
                    ..inner
                },
                Vector::new(0.0, inner_offset),
                &mut ReadOnlyScroll,
            );
            operation.traverse(&mut |operation| {
                operation.container(
                    Some(&Id::new(FOCUSED_CONTROL)),
                    Rectangle {
                        x: 10.0,
                        y: target_y,
                        width: 50.0,
                        height: 30.0,
                    },
                );
            });
            // A later reading viewport can geometrically overlap the raw target
            // bounds. It is not an ancestor and must never be scrolled.
            operation.scrollable(
                Some(&Id::new("unrelated-reading")),
                outer,
                Rectangle {
                    height: 2_000.0,
                    ..outer
                },
                Vector::ZERO,
                &mut ReadOnlyScroll,
            );
            operation.traverse(&mut |_| {});
        });
        reveal
    }

    #[test]
    fn focus_below_nested_viewports_reveals_the_whole_control() {
        let reveal = nested_focus(0.0, 200.0, 500.0);
        assert_eq!(reveal.target.unwrap().y, 70.0);
        assert_eq!(
            reveal.changes.into_inner(),
            [(Id::new("inner"), 350.0), (Id::new("outer"), 80.0),]
        );
    }

    #[test]
    fn reverse_focus_scrolls_both_ancestors_back_into_view() {
        let reveal = nested_focus(170.0, 200.0, 150.0);
        assert_eq!(reveal.target.unwrap().y, 0.0);
        assert_eq!(
            reveal.changes.into_inner(),
            [(Id::new("inner"), 30.0), (Id::new("outer"), 120.0),]
        );
    }

    #[test]
    fn already_visible_focus_does_not_move_any_viewport() {
        let reveal = nested_focus(90.0, 30.0, 175.0);
        assert_eq!(reveal.target.unwrap().y, 55.0);
        assert!(reveal.changes.into_inner().is_empty());
    }

    #[test]
    fn oversized_help_context_keeps_its_heading_and_close_control_visible() {
        let bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 300.0,
            height: 60.0,
        };
        let mut reveal = RevealFocus {
            pending: Some(FocusViewport {
                id: Some(Id::new("panel")),
                bounds,
                content_height: 500.0,
                offset: 0.0,
            }),
            ..RevealFocus::default()
        };
        Operation::<()>::traverse(&mut reveal, &mut |operation| {
            operation.container(
                Some(&Id::new(FOCUSED_CONTROL)),
                Rectangle {
                    y: 120.0,
                    height: 160.0,
                    ..bounds
                },
            );
        });
        assert_eq!(reveal.target.unwrap().y, 0.0);
        assert_eq!(reveal.changes.into_inner(), [(Id::new("panel"), 120.0)]);
    }
}
