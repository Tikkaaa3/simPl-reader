//! Shared chrome styles and keyboard-focus visibility for the normal reader.

use std::{cell::RefCell, sync::LazyLock};

use iced::advanced::widget::{Id, Operation, operation};
use iced::widget::{button, container, scrollable, text_input};
use iced::{Border, Color, Font, Rectangle, Theme, Vector, font};

pub const BACKGROUND: Color = Color::from_rgb8(0x10, 0x14, 0x1a);
pub const SURFACE: Color = Color::from_rgb8(0x1c, 0x20, 0x26);
pub const RAISED: Color = Color::from_rgb8(0x26, 0x2a, 0x31);
pub const LOWEST: Color = Color::from_rgb8(0x0a, 0x0e, 0x14);
pub const BORDER: Color = Color::from_rgb8(0x41, 0x47, 0x52);
pub const TEXT: Color = Color::from_rgb8(0xdf, 0xe2, 0xeb);
pub const SECONDARY: Color = Color::from_rgb8(0xc0, 0xc7, 0xd4);
pub const MUTED: Color = Color::from_rgb8(0x8b, 0x91, 0x9d);
pub const ACCENT: Color = Color::from_rgb8(0x58, 0xa6, 0xff);
pub const DANGER: Color = Color::from_rgb8(0xff, 0xb4, 0xab);

pub const SANS: Font = Font::with_name("Inter");
pub const MEDIUM: Font = Font {
    weight: font::Weight::Medium,
    ..SANS
};
pub const SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..SANS
};
pub const SERIF: Font = Font::with_name("Literata");
pub const SERIF_MEDIUM: Font = Font {
    weight: font::Weight::Medium,
    ..SERIF
};
pub const SERIF_ITALIC: Font = Font {
    style: font::Style::Italic,
    ..SERIF
};
pub const ICONS: Font = Font::with_name("Material Symbols Outlined");

/// The text faces retain upstream glyph coverage; native fallback handles other scripts.
/// Embedded bytes keep the portable executable independent of installed fonts.
pub fn font_data() -> [&'static [u8]; 9] {
    [
        include_bytes!("../../../assets/fonts/Inter-Regular.ttf"),
        include_bytes!("../../../assets/fonts/Inter-Medium.ttf"),
        include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf"),
        include_bytes!("../../../assets/fonts/Literata-Regular.ttf"),
        include_bytes!("../../../assets/fonts/Literata-Medium.ttf"),
        include_bytes!("../../../assets/fonts/Literata-Bold.ttf"),
        include_bytes!("../../../assets/fonts/Literata-Italic.ttf"),
        include_bytes!("../../../assets/fonts/Literata-BoldItalic.ttf"),
        include_bytes!("../../../assets/fonts/MaterialSymbolsOutlined-Subset.ttf"),
    ]
}

static THEME: LazyLock<Theme> = LazyLock::new(|| {
    Theme::custom(
        "simPl",
        iced::theme::Palette {
            background: BACKGROUND,
            text: TEXT,
            primary: ACCENT,
            success: ACCENT,
            warning: Color::from_rgb8(0xdf, 0xc2, 0x8f),
            danger: DANGER,
        },
    )
});

pub fn theme() -> Theme {
    THEME.clone()
}

#[derive(Clone, Copy)]
pub enum ButtonTone {
    Primary,
    Quiet,
    Subtle,
    Destructive,
}

pub fn button_style(
    status: button::Status,
    tone: ButtonTone,
    focused: bool,
    selected: bool,
) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    let disabled = matches!(status, button::Status::Disabled);
    let (background, text_color) = if disabled {
        (None, MUTED)
    } else {
        match tone {
            ButtonTone::Primary => (
                Some(ACCENT.scale_alpha(if hovered { 0.2 } else { 0.12 })),
                Color::from_rgb8(0xa2, 0xc9, 0xff),
            ),
            ButtonTone::Destructive if hovered => {
                (Some(Color::from_rgb8(0x47, 0x29, 0x2b)), DANGER)
            }
            _ if selected => (Some(RAISED), TEXT),
            _ if hovered => (Some(RAISED), TEXT),
            ButtonTone::Quiet => (Some(SURFACE), TEXT),
            ButtonTone::Subtle | ButtonTone::Destructive => (None, SECONDARY),
        }
    };
    button::Style {
        background: background.map(Into::into),
        text_color,
        border: Border {
            color: if focused && !disabled {
                ACCENT
            } else {
                Color::TRANSPARENT
            },
            width: if focused && !disabled { 2.0 } else { 0.0 },
            radius: 8.0.into(),
        },
        ..button::Style::default()
    }
}

pub fn panel(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        border: Border {
            color: Color::from_rgb8(0x24, 0x28, 0x2e),
            width: 1.0,
            radius: 8.0.into(),
        },
        ..container::Style::default()
    }
}

pub fn header(_: &Theme) -> container::Style {
    container::Style {
        background: Some(LOWEST.into()),
        ..container::Style::default()
    }
}

pub fn inset(_: &Theme) -> container::Style {
    container::Style {
        background: Some(RAISED.into()),
        border: Border {
            radius: 4.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

pub fn input_style(_: &Theme, status: text_input::Status) -> text_input::Style {
    let focused = matches!(status, text_input::Status::Focused { .. });
    text_input::Style {
        background: LOWEST.into(),
        border: Border {
            color: if focused { ACCENT } else { BORDER },
            width: if focused { 2.0 } else { 1.0 },
            radius: 4.0.into(),
        },
        icon: MUTED,
        placeholder: MUTED,
        value: if matches!(status, text_input::Status::Disabled) {
            MUTED
        } else {
            TEXT
        },
        selection: Color::from_rgb8(0x00, 0x3a, 0x6b),
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
            ACCENT
        } else {
            MUTED
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
