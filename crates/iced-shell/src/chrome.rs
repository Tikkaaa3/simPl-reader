//! Borderless native window chrome. The host owns the native window tasks.

use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer, widget};
use iced::widget::{button, container, row, space, text, tooltip};
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
const EDGE: f32 = 4.0;
const CORNER: f32 = 8.0;
const HEADER: Color = Color::from_rgb8(0x0b, 0x0f, 0x15);

fn blend(foreground: Color, opacity: f32) -> Color {
    Color::from_rgb(
        foreground.r * opacity + HEADER.r * (1.0 - opacity),
        foreground.g * opacity + HEADER.g * (1.0 - opacity),
        foreground.b * opacity + HEADER.b * (1.0 - opacity),
    )
}

fn button_style(status: button::Status, focused: bool) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: hovered.then_some(ui::RAISED.into()),
        text_color: if hovered { ui::TEXT } else { ui::SECONDARY },
        border: Border {
            color: if focused {
                ui::ACCENT
            } else {
                Color::TRANSPARENT
            },
            width: if focused { 2.0 } else { 0.0 },
            radius: 2.0.into(),
        },
        ..button::Style::default()
    }
}

fn hint(
    content: impl Into<Element<'static, Action>>,
    caption: &'static str,
) -> Element<'static, Action> {
    tooltip(
        content,
        text(caption).font(ui::SANS).size(12).color(ui::TEXT),
        tooltip::Position::Bottom,
    )
    .gap(5)
    .padding(7)
    .style(|_| container::Style {
        background: Some(ui::RAISED.into()),
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

fn dot(
    action: Action,
    color: Color,
    focused: Option<Action>,
    caption: &'static str,
) -> Element<'static, Action> {
    let selected = focused == Some(action);
    let dot = container(space().width(12).height(12)).style(move |_| container::Style {
        background: Some(color.into()),
        border: Border {
            radius: 6.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    });
    let control = button(dot)
        .on_press(action)
        .width(20)
        .height(30)
        .padding([9, 4])
        .style(move |_, status| button_style(status, selected));
    hint(focus_marker(control, selected), caption)
}

fn icon(
    action: Action,
    name: &'static str,
    focused: Option<Action>,
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
    .style(move |_, status| button_style(status, selected));
    hint(focus_marker(control, selected), caption)
}

/// The 48-DIP draggable header. Controls receive events before the drag surface.
/// `active` changes visual emphasis, never whether the native controls work.
pub fn view(focused: Option<Action>, active: bool) -> Element<'static, Action> {
    let left = row![
        dot(
            Action::Close,
            blend(ui::DANGER, 0.8),
            focused,
            "Close window"
        ),
        dot(
            Action::Minimize,
            blend(ui::MUTED, 0.6),
            focused,
            "Minimize window"
        ),
        dot(
            Action::Maximize,
            blend(ui::BORDER, 0.8),
            focused,
            "Maximize or restore window"
        ),
    ]
    .align_y(iced::Alignment::Center);
    let right = row![
        icon(
            Action::Search,
            "\u{e8b6}",
            focused,
            "Search or switch (Ctrl+K)"
        ),
        icon(Action::Settings, "\u{e8b8}", focused, "Settings"),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);
    // Iced Text has no letter-spacing property; separate glyphs preserve the
    // reference's 1.2-DIP tracking without inserting visible word spaces.
    let title_color = if active { ui::SECONDARY } else { ui::MUTED };
    let title = row![
        text("S").font(ui::MEDIUM).size(12).color(title_color),
        text("I").font(ui::MEDIUM).size(12).color(title_color),
        text("M").font(ui::MEDIUM).size(12).color(title_color),
        text("P").font(ui::MEDIUM).size(12).color(title_color),
        text("L").font(ui::MEDIUM).size(12).color(title_color),
    ]
    .spacing(1.2);
    let content = row![
        container(left)
            .width(112)
            .height(HEIGHT)
            .padding([0, 20])
            .align_y(iced::alignment::Vertical::Center),
        container(title)
            .width(Length::Fill)
            .height(HEIGHT)
            .center_x(Length::Fill)
            .center_y(HEIGHT),
        container(right)
            .width(112)
            .height(HEIGHT)
            .padding([0, 24])
            .align_x(iced::alignment::Horizontal::Right)
            .align_y(iced::alignment::Vertical::Center),
    ]
    .width(Length::Fill)
    .height(HEIGHT)
    .align_y(iced::Alignment::Center);
    Element::new(DragHeader {
        content: container(content)
            .width(Length::Fill)
            .height(HEIGHT)
            .style(|_| container::Style {
                background: Some(Background::Color(HEADER)),
                ..container::Style::default()
            })
            .into(),
    })
}

struct DragHeader<'a> {
    content: Element<'a, Action>,
}

#[derive(Default)]
struct HeaderState {
    previous_click: Option<mouse::Click>,
}

impl Widget<Action, Theme, iced::Renderer> for DragHeader<'_> {
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
        shell: &mut Shell<'_, Action>,
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
                    shell.publish(if click.kind() == mouse::click::Kind::Double {
                        Action::Maximize
                    } else {
                        Action::Drag
                    });
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                shell.publish(Action::Menu);
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
    ) -> Option<overlay::Element<'b, Action, Theme, iced::Renderer>> {
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
