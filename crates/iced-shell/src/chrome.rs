//! Borderless native window chrome. The host owns the native window tasks.

use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer, widget};
use iced::widget::{button, container, row, space, text, tooltip};
use iced::{
    Background, Border, Color, Element, Event, Length, Rectangle, Size, Theme, Vector, window,
};

use crate::ui;
use reader_document::preferences::WindowControls;

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
    ToggleToolbar,
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

fn blend(theme: &Theme, foreground: Color, opacity: f32) -> Color {
    Color::from_rgb(
        foreground.r * opacity + ui::palette(theme).lowest.r * (1.0 - opacity),
        foreground.g * opacity + ui::palette(theme).lowest.g * (1.0 - opacity),
        foreground.b * opacity + ui::palette(theme).lowest.b * (1.0 - opacity),
    )
}

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

fn hint(
    content: impl Into<Element<'static, Action>>,
    caption: &'static str,
) -> Element<'static, Action> {
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

fn dot(
    action: Action,
    color: impl Fn(&Theme) -> Color + 'static,
    focused: Option<Action>,
    caption: &'static str,
) -> Element<'static, Action> {
    let selected = focused == Some(action);
    let dot = container(space().width(12).height(12)).style(move |theme| container::Style {
        background: Some(color(theme).into()),
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
        .style(move |theme, status| button_style(theme, status, selected));
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
            .font(
                if matches!(action, Action::ToggleAppearance | Action::ToggleToolbar) {
                    iced::Font::with_name("Segoe UI Symbol")
                } else {
                    ui::ICONS
                },
            )
            .size(19)
            .line_height(1.0)
            .shaping(text::Shaping::Advanced),
    )
    .on_press(action)
    .width(32)
    .height(32)
    .padding(6)
    .style(move |theme, status| button_style(theme, status, selected));
    hint(focus_marker(control, selected), caption)
}

/// A Windows caption button drawn with the system's Segoe MDL2 Assets glyphs.
fn caption(
    action: Action,
    glyph: &'static str,
    focused: Option<Action>,
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
    hint(focus_marker(control, selected), tip)
}

/// Native window button style and state shown in the header.
#[derive(Clone, Copy, Debug)]
pub struct Controls {
    pub style: WindowControls,
    pub maximized: bool,
}

/// The 48-DIP draggable header. Controls receive events before the drag surface.
/// `active` changes visual emphasis, never whether the native controls work.
pub fn view<'a>(
    focused: Option<Action>,
    active: bool,
    document_title: Option<String>,
    controls: Controls,
    appearance: Option<reader_document::preferences::Appearance>,
    toolbar: Option<bool>,
) -> Element<'a, Action> {
    let dots = row![
        dot(
            Action::Close,
            |theme| blend(theme, ui::palette(theme).danger, 0.8),
            focused,
            "Close window"
        ),
        dot(
            Action::Minimize,
            |theme| blend(theme, ui::palette(theme).muted, 0.6),
            focused,
            "Minimize window"
        ),
        dot(
            Action::Maximize,
            |theme| blend(theme, ui::palette(theme).border, 0.8),
            focused,
            "Maximize or restore window"
        ),
    ]
    .align_y(iced::Alignment::Center);
    let captions = row![
        caption(Action::Minimize, "\u{e921}", focused, "Minimize"),
        if controls.maximized {
            caption(Action::Maximize, "\u{e923}", focused, "Restore down")
        } else {
            caption(Action::Maximize, "\u{e922}", focused, "Maximize")
        },
        caption(Action::Close, "\u{e8bb}", focused, "Close"),
    ]
    .spacing(CAPTION_GAP)
    .align_y(iced::Alignment::Center);
    // Mirror the complete tool order when window controls move to the right:
    // Settings becomes the outside button on the Windows left edge.
    let mut tools = row![];
    if controls.style == WindowControls::Windows {
        tools = tools.push(icon(Action::Settings, "\u{e8b8}", focused, "Settings"));
    } else {
        tools = tools.push(icon(
            Action::Search,
            "\u{e8b6}",
            focused,
            "Search or switch (Ctrl+K)",
        ));
    }
    let appearance_icon = appearance.map(|appearance| {
        icon(
            Action::ToggleAppearance,
            if appearance == reader_document::preferences::Appearance::Dark {
                "☼"
            } else {
                "☾"
            },
            focused,
            appearance.toggle_label(),
        )
    });
    let toolbar_icon = toolbar.map(|expanded| {
        icon(
            Action::ToggleToolbar,
            "▤",
            focused,
            if expanded {
                "Hide reading toolbar (F8)"
            } else {
                "Show reading toolbar (F8)"
            },
        )
    });
    if controls.style == WindowControls::Windows {
        if let Some(icon) = toolbar_icon {
            tools = tools.push(icon);
        }
        if let Some(icon) = appearance_icon {
            tools = tools.push(icon);
        }
        tools = tools.push(icon(
            Action::Search,
            "\u{e8b6}",
            focused,
            "Search or switch (Ctrl+K)",
        ));
    } else {
        if let Some(icon) = appearance_icon {
            tools = tools.push(icon);
        }
        if let Some(icon) = toolbar_icon {
            tools = tools.push(icon);
        }
        tools = tools.push(icon(Action::Settings, "\u{e8b8}", focused, "Settings"));
    }
    let tools = tools.spacing(8).align_y(iced::Alignment::Center);
    let tool_count: f32 = 2.0 + f32::from(appearance.is_some()) + f32::from(toolbar.is_some());
    let tools_width = tool_count * 32.0 + (tool_count - 1.0) * 8.0;
    // Library keeps the tracked wordmark; a document uses its own title.
    let title_style = move |theme: &Theme| text::Style {
        color: Some(if active {
            ui::palette(theme).secondary
        } else {
            ui::palette(theme).muted
        }),
    };
    let title: Element<'a, Action> = if let Some(document_title) = document_title {
        text(document_title)
            .font(ui::MEDIUM)
            .size(12)
            .style(title_style)
            .wrapping(text::Wrapping::None)
            .into()
    } else {
        row![
            text("S").font(ui::MEDIUM).size(12).style(title_style),
            text("I").font(ui::MEDIUM).size(12).style(title_style),
            text("M").font(ui::MEDIUM).size(12).style(title_style),
            text("P").font(ui::MEDIUM).size(12).style(title_style),
            text("L").font(ui::MEDIUM).size(12).style(title_style),
        ]
        .spacing(1.2)
        .into()
    };
    // Both sides share one width so the title stays centred in the window.
    let (left, right, side_width, right_padding): (Element<'a, Action>, Element<'a, Action>, _, _) =
        match controls.style {
            WindowControls::Mac => (dots.into(), tools.into(), tools_width + 40.0, 24.0),
            // Caption buttons own the right edge, so the tools move to the left.
            WindowControls::Windows => (
                tools.into(),
                captions.into(),
                (tools_width + 48.0).max(3.0 * CAPTION + 2.0 * CAPTION_GAP + CAPTION_EDGE),
                CAPTION_EDGE,
            ),
        };
    let content = row![
        container(left)
            .width(side_width)
            .height(HEIGHT)
            .padding([
                0,
                if controls.style == WindowControls::Windows {
                    24
                } else {
                    20
                }
            ])
            .align_y(iced::alignment::Vertical::Center),
        container(title)
            .width(Length::Fill)
            .height(HEIGHT)
            .center_x(Length::Fill)
            .center_y(HEIGHT)
            .clip(true),
        container(right)
            .width(side_width)
            .height(HEIGHT)
            .padding(iced::Padding {
                right: right_padding,
                ..iced::Padding::ZERO
            })
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
            .style(|theme| container::Style {
                background: Some(Background::Color(ui::palette(theme).lowest)),
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
