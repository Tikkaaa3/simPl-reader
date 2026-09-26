//! Shared chrome styles and keyboard-focus visibility for the normal reader.

use std::{cell::RefCell, sync::LazyLock};

use iced::advanced::widget::{Id, Operation, operation};
use iced::widget::{button, container, scrollable, text_input};
use iced::{Border, Color, Font, Rectangle, Theme, Vector, font};

pub const BACKGROUND: Color = Color::from_rgb8(0x15, 0x17, 0x19);
pub const SURFACE: Color = Color::from_rgb8(0x1d, 0x21, 0x23);
pub const RAISED: Color = Color::from_rgb8(0x26, 0x2c, 0x2e);
pub const BORDER: Color = Color::from_rgb8(0x34, 0x3c, 0x3d);
pub const TEXT: Color = Color::from_rgb8(0xe7, 0xeb, 0xe7);
pub const MUTED: Color = Color::from_rgb8(0xa0, 0xab, 0xaa);
pub const ACCENT: Color = Color::from_rgb8(0xb4, 0xce, 0xb8);
pub const DANGER: Color = Color::from_rgb8(0xe8, 0xa4, 0x9a);

pub const SEMIBOLD: Font = Font {
    family: font::Family::Name("Segoe UI"),
    weight: font::Weight::Semibold,
    ..Font::DEFAULT
};

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
        (None, Color::from_rgb8(0x65, 0x70, 0x70))
    } else {
        match tone {
            ButtonTone::Primary => (
                Some(if hovered {
                    Color::from_rgb8(0xca, 0xdf, 0xcd)
                } else {
                    ACCENT
                }),
                BACKGROUND,
            ),
            ButtonTone::Destructive if hovered => {
                (Some(Color::from_rgb8(0x3c, 0x2a, 0x29)), DANGER)
            }
            _ if selected => (Some(Color::from_rgb8(0x2b, 0x3a, 0x30)), ACCENT),
            _ if hovered => (Some(RAISED), TEXT),
            ButtonTone::Quiet => (Some(RAISED), TEXT),
            ButtonTone::Subtle | ButtonTone::Destructive => (None, MUTED),
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
            color: BORDER,
            width: 1.0,
            radius: 12.0.into(),
        },
        ..container::Style::default()
    }
}

pub fn header(_: &Theme) -> container::Style {
    container::Style {
        background: Some(SURFACE.into()),
        ..container::Style::default()
    }
}

pub fn inset(_: &Theme) -> container::Style {
    container::Style {
        background: Some(RAISED.into()),
        border: Border {
            radius: 8.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

pub fn input_style(_: &Theme, status: text_input::Status) -> text_input::Style {
    let focused = matches!(status, text_input::Status::Focused { .. });
    text_input::Style {
        background: BACKGROUND.into(),
        border: Border {
            color: if focused { ACCENT } else { BORDER },
            width: if focused { 2.0 } else { 1.0 },
            radius: 8.0.into(),
        },
        icon: MUTED,
        placeholder: MUTED,
        value: if matches!(status, text_input::Status::Disabled) {
            MUTED
        } else {
            TEXT
        },
        selection: Color::from_rgb8(0x45, 0x60, 0x4c),
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
            Color::from_rgb8(0x65, 0x70, 0x70)
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
