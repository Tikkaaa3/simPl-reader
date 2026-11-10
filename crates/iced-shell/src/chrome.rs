//! Borderless native window chrome. The host owns the native window tasks.

use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer, widget};
use iced::widget::{button, container, row, text, tooltip};
use iced::{
    Background, Border, Color, Element, Event, Length, Rectangle, Size, Theme, Vector, window,
};

use crate::ui;

/// Requests for native window operations and the two header destinations.
#[derive(Clone, Copy, Debug)]
pub enum Action {
    Close,
    Minimize,
    Maximize,
    Drag,
    Menu,
    Search,
    Settings,
    ToggleAppearance,
    Resize(window::Direction),
}

// iced::window::Direction is a fieldless enum, but iced does not derive Eq on it.
impl PartialEq for Action {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Resize(a), Self::Resize(b)) => {
                std::mem::discriminant(a) == std::mem::discriminant(b)
            }
            (Self::Resize(_), _) | (_, Self::Resize(_)) => false,
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
}
impl Eq for Action {}

const HEIGHT: f32 = 48.0;
/// Size of a Windows caption button: compact and rounded like the other header tools.
const CAPTION: f32 = 36.0;
const CAPTION_HEIGHT: f32 = 32.0;
const CAPTION_GAP: f32 = 4.0;
/// Space between the close button and the window's right edge.
const CAPTION_EDGE: f32 = 8.0;
const EDGE: f32 = 4.0;
const CORNER: f32 = 8.0;

fn button_style(theme: &Theme, status: button::Status, focused: bool) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: hovered.then_some(ui::palette(theme).raised.into()),
        text_color: if hovered {
            ui::palette(theme).text
        } else {
            ui::palette(theme).secondary
        },
        border: Border {
            color: if focused {
                ui::palette(theme).accent
            } else {
                Color::TRANSPARENT
            },
            width: if focused { 2.0 } else { 0.0 },
            radius: 2.0.into(),
        },
        ..button::Style::default()
    }
}

/// A header control's tooltip, below it in the raised tone.
pub(crate) fn hint<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    caption: impl text::IntoFragment<'a>,
    enabled: bool,
) -> Element<'a, M> {
    if !enabled {
        return content.into();
    }
    tooltip(
        content,
        text(caption)
            .font(ui::SANS)
            .size(12)
            .style(ui::primary_text),
        tooltip::Position::Bottom,
    )
    .gap(5)
    .padding(7)
    .style(|theme| container::Style {
        background: Some(ui::palette(theme).raised.into()),
        border: Border {
            radius: 5.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

fn focus_marker(
    content: impl Into<Element<'static, Action>>,
    selected: bool,
) -> Element<'static, Action> {
    let control = container(content)
        .width(Length::Shrink)
        .height(Length::Shrink);
    if selected {
        control.id(ui::FOCUSED_CONTROL).into()
    } else {
        control.into()
    }
}

fn icon(
    action: Action,
    name: &'static str,
    focused: Option<Action>,
    hints: bool,
    caption: &'static str,
) -> Element<'static, Action> {
    let selected = focused == Some(action);
    let control = button(
        text(name)
            .font(ui::ICONS)
            .size(19)
            .line_height(1.0)
            .shaping(text::Shaping::Advanced),
    )
    .on_press(action)
    .width(32)
    .height(32)
    .padding(6)
    .style(move |theme, status| button_style(theme, status, selected));
    hint(focus_marker(control, selected), caption, hints)
}

/// A Windows caption button drawn with the system's Segoe MDL2 Assets glyphs.
fn caption(
    action: Action,
    glyph: &'static str,
    focused: Option<Action>,
    hints: bool,
    tip: &'static str,
) -> Element<'static, Action> {
    let selected = focused == Some(action);
    let close = action == Action::Close;
    let control = button(
        container(
            text(glyph)
                .font(iced::Font::with_name("Segoe MDL2 Assets"))
                .size(10)
                .line_height(1.0),
        )
        .center(Length::Fill),
    )
    .on_press(action)
    .width(CAPTION)
    .height(CAPTION_HEIGHT)
    .padding(0)
    .style(move |theme, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let mut style = button_style(theme, status, selected);
        style.border.radius = 6.0.into();
        if close && hovered {
            style.background = Some(Color::from_rgb8(0xC4, 0x2B, 0x1C).into());
            style.text_color = Color::WHITE;
        }
        style
    });
    hint(focus_marker(control, selected), tip, hints)
}

/// Native window button state shown in the header.
#[derive(Clone, Copy, Debug)]
pub struct Controls {
    pub maximized: bool,
    /// Suppress background hover overlays while a product modal is open.
    pub tooltips: bool,
}

/// Space between the window's left edge and the first header tool.
const TOOLS_EDGE: f32 = 24.0;
/// Width of the Settings, appearance and Search tools.
const TOOLS_WIDTH: f32 = 3.0 * 32.0 + 2.0 * 8.0;
/// Width of the three caption buttons and their right edge.
const CAPTIONS_WIDTH: f32 = 3.0 * CAPTION + 2.0 * CAPTION_GAP + CAPTION_EDGE;

/// Minimize, maximize or restore, and close, ending `CAPTION_EDGE` from the right.
pub fn captions(focused: Option<Action>, controls: Controls) -> Element<'static, Action> {
    container(
        row![
            caption(
                Action::Minimize,
                "\u{e921}",
                focused,
                controls.tooltips,
                "Minimize"
            ),
            if controls.maximized {
                caption(
                    Action::Maximize,
                    "\u{e923}",
                    focused,
                    controls.tooltips,
                    "Restore down",
                )
            } else {
                caption(
                    Action::Maximize,
                    "\u{e922}",
                    focused,
                    controls.tooltips,
                    "Maximize",
                )
            },
            caption(
                Action::Close,
                "\u{e8bb}",
                focused,
                controls.tooltips,
                "Close"
            ),
        ]
        .spacing(CAPTION_GAP)
        .align_y(iced::Alignment::Center),
    )
    .padding(iced::Padding {
        right: CAPTION_EDGE,
        ..iced::Padding::ZERO
    })
    .into()
}

/// Settings, the light and dark toggle, and Search, `TOOLS_EDGE` from the left
/// of the Library header.
fn tools(
    focused: Option<Action>,
    hints: bool,
    appearance: reader_document::preferences::Appearance,
) -> Element<'static, Action> {
    row![
        icon(Action::Settings, "\u{e8b8}", focused, hints, "Settings"),
        icon(
            Action::ToggleAppearance,
            // The same pair as the reading tools.
            if appearance == reader_document::preferences::Appearance::Dark {
                "\u{e518}"
            } else {
                "\u{e51c}"
            },
            focused,
            hints,
            appearance.toggle_label(),
        ),
        icon(
            Action::Search,
            "\u{e8b6}",
            focused,
            hints,
            "Search or switch (Ctrl+K)",
        ),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .into()
}

/// The 48-DIP draggable Library header. Controls receive events before the drag surface.
/// `active` changes visual emphasis, never whether the native controls work.
pub fn view<'a>(
    focused: Option<Action>,
    active: bool,
    controls: Controls,
    appearance: reader_document::preferences::Appearance,
) -> Element<'a, Action> {
    let title_style = move |theme: &Theme| text::Style {
        color: Some(if active {
            ui::palette(theme).secondary
        } else {
            ui::palette(theme).muted
        }),
    };
    let title = row![
        text("S").font(ui::MEDIUM).size(12).style(title_style),
        text("I").font(ui::MEDIUM).size(12).style(title_style),
        text("M").font(ui::MEDIUM).size(12).style(title_style),
        text("P").font(ui::MEDIUM).size(12).style(title_style),
        text("L").font(ui::MEDIUM).size(12).style(title_style),
    ]
    .spacing(1.2);
    // Both sides share one width so the wordmark stays centred in the window.
    let side_width = (TOOLS_WIDTH + 2.0 * TOOLS_EDGE).max(CAPTIONS_WIDTH);
    let content = row![
        container(tools(focused, controls.tooltips, appearance))
            .width(side_width)
            .height(HEIGHT)
            .padding([0.0, TOOLS_EDGE])
            .align_y(iced::alignment::Vertical::Center),
        container(title)
            .width(Length::Fill)
            .height(HEIGHT)
            .center_x(Length::Fill)
            .center_y(HEIGHT)
            .clip(true),
        container(captions(focused, controls))
            .width(side_width)
            .height(HEIGHT)
            .align_x(iced::alignment::Horizontal::Right)
            .align_y(iced::alignment::Vertical::Center),
    ]
    .width(Length::Fill)
    .height(HEIGHT)
    .align_y(iced::Alignment::Center);
    drag_region(
        container(content)
            .width(Length::Fill)
            .height(HEIGHT)
            .style(|theme| container::Style {
                background: Some(Background::Color(ui::palette(theme).lowest)),
                ..container::Style::default()
            })
            .into(),
        |action| action,
    )
}

/// Makes `content` a window title bar: a press on its background drags the
/// window, a double click maximizes it, and a right click opens the system menu.
pub fn drag_region<'a, M: 'a>(
    content: Element<'a, M>,
    on_action: fn(Action) -> M,
) -> Element<'a, M> {
    Element::new(DragHeader { content, on_action })
}

struct DragHeader<'a, M> {
    content: Element<'a, M>,
    on_action: fn(Action) -> M,
}

#[derive(Default)]
struct HeaderState {
    previous_click: Option<mouse::Click>,
}

impl<M> Widget<M, Theme, iced::Renderer> for DragHeader<'_, M> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<HeaderState>()
    }
    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(HeaderState::default())
    }
    fn children(&self) -> Vec<widget::Tree> {
        vec![widget::Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut widget::Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        if shell.is_event_captured() || !cursor.is_over(layout.bounds()) {
            return;
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position() {
                    let state = tree.state.downcast_mut::<HeaderState>();
                    let click =
                        mouse::Click::new(position, mouse::Button::Left, state.previous_click);
                    state.previous_click = Some(click);
                    shell.publish((self.on_action)(
                        if click.kind() == mouse::click::Kind::Double {
                            Action::Maximize
                        } else {
                            Action::Drag
                        },
                    ));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                shell.publish((self.on_action)(Action::Menu));
                shell.capture_event();
            }
            _ => {}
        }
    }
    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let child = self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        );
        if child == mouse::Interaction::None && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Grab
        } else {
            child
        }
    }
    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut widget::Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

/// Transparent native resize affordances. Retains the child's exact layout and
/// forwards its overlays, events and widget operations everywhere except edges.
pub fn frame<'a, M: Clone + 'a>(
    content: Element<'a, M>,
    on_resize: fn(window::Direction) -> M,
) -> Element<'a, M> {
    Element::new(ResizeFrame { content, on_resize })
}

struct ResizeFrame<'a, M> {
    content: Element<'a, M>,
    on_resize: fn(window::Direction) -> M,
}

fn resize_direction(bounds: Rectangle, position: iced::Point) -> Option<window::Direction> {
    if !bounds.contains(position) {
        return None;
    }
    let x = position.x - bounds.x;
    let y = position.y - bounds.y;
    let west = x < CORNER;
    let east = x >= bounds.width - CORNER;
    let north = y < CORNER;
    let south = y >= bounds.height - CORNER;
    match (west, east, north, south) {
        (true, _, true, _) => Some(window::Direction::NorthWest),
        (_, true, true, _) => Some(window::Direction::NorthEast),
        (true, _, _, true) => Some(window::Direction::SouthWest),
        (_, true, _, true) => Some(window::Direction::SouthEast),
        _ if y < EDGE => Some(window::Direction::North),
        _ if y >= bounds.height - EDGE => Some(window::Direction::South),
        _ if x < EDGE => Some(window::Direction::West),
        _ if x >= bounds.width - EDGE => Some(window::Direction::East),
        _ => None,
    }
}

fn resizing_cursor(direction: window::Direction) -> mouse::Interaction {
    match direction {
        window::Direction::North | window::Direction::South => {
            mouse::Interaction::ResizingVertically
        }
        window::Direction::East | window::Direction::West => {
            mouse::Interaction::ResizingHorizontally
        }
        window::Direction::NorthWest | window::Direction::SouthEast => {
            mouse::Interaction::ResizingDiagonallyDown
        }
        window::Direction::NorthEast | window::Direction::SouthWest => {
            mouse::Interaction::ResizingDiagonallyUp
        }
    }
}

impl<M: Clone> Widget<M, Theme, iced::Renderer> for ResizeFrame<'_, M> {
    fn children(&self) -> Vec<widget::Tree> {
        vec![widget::Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut widget::Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event
            && let Some(direction) = cursor
                .position()
                .and_then(|p| resize_direction(layout.bounds(), p))
        {
            shell.publish((self.on_resize)(direction));
            shell.capture_event();
            return;
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }
    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        if let Some(direction) = cursor
            .position()
            .and_then(|p| resize_direction(layout.bounds(), p))
        {
            resizing_cursor(direction)
        } else {
            self.content.as_widget().mouse_interaction(
                &tree.children[0],
                layout,
                cursor,
                viewport,
                renderer,
            )
        }
    }
    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut widget::Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}
