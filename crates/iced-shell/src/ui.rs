//! Shared chrome styles and keyboard-focus visibility for the normal reader.

use std::cell::RefCell;

use iced::advanced::widget::{Id, Operation, operation};
use iced::widget::{button, container, scrollable, text_input};
use iced::{Border, Color, Font, Rectangle, Theme, Vector};

pub use reader_layout::themes::Palette;
pub(crate) use reader_layout::themes::{DARK, LIGHT};

pub fn palette(theme: &Theme) -> Palette {
    crate::themes::palette_of(theme).unwrap_or(if theme.extended_palette().is_dark {
        DARK
    } else {
        LIGHT
    })
}

/// `amount` of `foreground` over an opaque `background`.
pub fn mix(background: Color, foreground: Color, amount: f32) -> Color {
    Color::from_rgb(
        background.r + (foreground.r - background.r) * amount,
        background.g + (foreground.g - background.g) * amount,
        background.b + (foreground.b - background.b) * amount,
    )
}

/// A 1px horizontal rule in the border tone.
pub fn rule(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(palette(theme).border.scale_alpha(0.6).into()),
        ..container::Style::default()
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
/// Editorial headings: the reading serif gives the chrome its literary voice.
pub const HEADING: Font = Font {
    weight: iced::font::Weight::Medium,
    ..SERIF
};
pub const SERIF_ITALIC: Font = Font {
    style: iced::font::Style::Italic,
    ..SERIF
};
pub const ICONS: Font = Font::with_name("Material Symbols Outlined");

/// The text faces retain upstream glyph coverage; native fallback handles other scripts.
/// Embedded bytes keep the portable executable independent of installed fonts. The
/// reading faces come from `reader-layout`, the one copy both apps measure with.
pub fn font_data() -> [&'static [u8]; 18] {
    use reader_layout::fonts;
    [
        fonts::GEIST,
        include_bytes!("../../../assets/fonts/Geist-UI-560.ttf"),
        fonts::LITERATA_REGULAR,
        fonts::LITERATA_MEDIUM,
        fonts::LITERATA_BOLD,
        fonts::LITERATA_ITALIC,
        fonts::LITERATA_BOLD_ITALIC,
        include_bytes!("../../../assets/fonts/MaterialSymbolsOutlined-Subset.ttf"),
        // Reading families offered by the bundled themes (see themes.rs).
        fonts::SPECTRAL_REGULAR,
        fonts::SPECTRAL_MEDIUM,
        fonts::SPECTRAL_BOLD,
        fonts::SPECTRAL_ITALIC,
        fonts::SPECTRAL_BOLD_ITALIC,
        fonts::FIRA_SANS_REGULAR,
        fonts::FIRA_SANS_MEDIUM,
        fonts::FIRA_SANS_BOLD,
        fonts::FIRA_SANS_ITALIC,
        fonts::FIRA_SANS_BOLD_ITALIC,
    ]
}

/// Initialize the shared renderer once before parallel widget tests. Reloading
/// fonts mid-frame invalidates text caches and changes otherwise stable damage.
#[cfg(test)]
pub(crate) fn load_test_fonts() {
    static LOADED: std::sync::Once = std::sync::Once::new();
    LOADED.call_once(|| {
        let mut system = iced::advanced::graphics::text::font_system()
            .write()
            .unwrap();
        for bytes in font_data() {
            system.load_font(std::borrow::Cow::Borrowed(bytes));
        }
    });
}

pub(crate) fn make_theme(colors: Palette, name: &str) -> Theme {
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
pub fn theme(
    appearance: reader_document::preferences::Appearance,
    reading: &crate::themes::ReadingTheme,
) -> Theme {
    crate::themes::iced_theme(reading, appearance)
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
            ButtonTone::Destructive => (
                hovered.then(|| palette(theme).danger.scale_alpha(0.12)),
                palette(theme).danger,
            ),
            // The primary action: a tinted accent, never a solid block of color.
            ButtonTone::Quiet => (
                Some(palette(theme).accent.scale_alpha(if hovered || selected {
                    0.24
                } else {
                    0.14
                })),
                palette(theme).accent,
            ),
            ButtonTone::Surface => (
                Some(if hovered || selected {
                    palette(theme).raised
                } else {
                    palette(theme).surface
                }),
                palette(theme).text,
            ),
            ButtonTone::Subtle if selected => (Some(palette(theme).raised), palette(theme).text),
            ButtonTone::Subtle if hovered => (Some(palette(theme).surface), palette(theme).text),
            ButtonTone::Subtle => (None, palette(theme).secondary),
        }
    };
    button::Style {
        background: background.map(Into::into),
        text_color,
        border: Border {
            color: if focused && !disabled {
                palette(theme).accent
            } else if matches!(tone, ButtonTone::Quiet) && !disabled {
                palette(theme).accent.scale_alpha(0.35)
            } else if matches!(tone, ButtonTone::Surface) && !disabled {
                if selected {
                    palette(theme).accent.scale_alpha(0.7)
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
            } else if matches!(tone, ButtonTone::Surface | ButtonTone::Quiet) && !disabled {
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
            palette(theme).muted.scale_alpha(0.5)
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
    iced::advanced::widget::operate(reveal_focus_operation())
}

/// The same operation can be driven by the renderer preview without a window.
pub fn reveal_focus_operation<T>() -> impl Operation<T> {
    RevealFocus::default()
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
