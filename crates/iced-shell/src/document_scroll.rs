//! Keep Iced's scrolling/virtualization and middle-button autoscroll, but provide
//! a usable vertical thumb even when the document contains thousands of pages.
//! Ctrl+wheel (also how Windows delivers touchpad pinch) zooms instead of scrolling.
//! Dragging a selection towards or past the top or bottom edge scrolls the document.
use iced::advanced::widget::operation::scrollable::{AbsoluteOffset, Scrollable as ScrollState};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer, widget};
use iced::{Border, Element, Event, Length, Point, Rectangle, Size, Vector};

use crate::ui;

const RAIL_WIDTH: f32 = 14.0;
const THUMB_WIDTH: f32 = 8.0;
const MIN_THUMB: f32 = 40.0;
/// Pixel scroll deltas (precision touchpads) per wheel notch.
const PIXELS_PER_NOTCH: f32 = 50.0;
/// Distance from the top or bottom edge where a selection drag starts scrolling.
const EDGE: f32 = 32.0;
/// Scroll speed, in DIP per second, for each DIP the pointer is past `EDGE`.
const EDGE_ACCELERATION: f32 = 18.0;
const MAX_EDGE_SPEED: f32 = 3000.0;

/// `zoom` receives Ctrl+wheel movement in notches; positive values zoom in.
pub fn wrap<'a, Message: 'a>(
    zoom: impl Fn(f32) -> Message + 'a,
    scrollable: iced::widget::Scrollable<'a, Message>,
) -> Element<'a, Message> {
    Element::new(DocumentScroll {
        zoom: Box::new(zoom),
        content: scrollable
            .auto_scroll(true)
            .style(|theme, status| {
                let mut style = ui::scroll_style(theme, status);
                // This wrapper owns the vertical rail. Iced draws its rails in
                // a separate renderer layer, so painting over one is not enough.
                // Keep native horizontal scrolling.
                style.vertical_rail.background = None;
                style.vertical_rail.border = Border::default();
                style.vertical_rail.scroller.background = iced::Color::TRANSPARENT.into();
                style.vertical_rail.scroller.border = Border::default();
                // Iced's fixed 40px overlay is replaced by our small origin marker.
                style.auto_scroll.background = iced::Color::TRANSPARENT.into();
                style.auto_scroll.border = Border::default();
                style.auto_scroll.shadow = iced::Shadow::default();
                style.auto_scroll.icon = iced::Color::TRANSPARENT;
                style
            })
            .into(),
    })
}

struct DocumentScroll<'a, Message> {
    zoom: Box<dyn Fn(f32) -> Message + 'a>,
    content: Element<'a, Message>,
}

/// Whether Ctrl is held. Precision touchpads report pinch as wheel input with a
/// synthetic Ctrl that is not always delivered as a keyboard event.
fn control_pressed(modifiers: iced::keyboard::Modifiers) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL};
    // SAFETY: GetKeyState reads this thread's keyboard state; it has no preconditions.
    modifiers.control() || unsafe { GetKeyState(i32::from(VK_CONTROL)) } < 0
}

#[derive(Clone, Copy, Default)]
struct Geometry {
    viewport: Rectangle,
    content: Rectangle,
    offset: Vector,
}

impl Geometry {
    fn rail(self) -> Option<Rectangle> {
        if self.content.height <= self.viewport.height {
            return None;
        }
        let bottom = if self.content.width > self.viewport.width {
            RAIL_WIDTH
        } else {
            0.0
        };
        Some(Rectangle {
            x: self.viewport.x + self.viewport.width - RAIL_WIDTH,
            y: self.viewport.y,
            width: RAIL_WIDTH,
            height: (self.viewport.height - bottom).max(0.0),
        })
    }

    fn thumb(self) -> Option<Rectangle> {
        let rail = self.rail()?;
        let height = (rail.height * self.viewport.height / self.content.height)
            .max(MIN_THUMB)
            .min(rail.height);
        let fraction =
            (self.offset.y / (self.content.height - self.viewport.height)).clamp(0.0, 1.0);
        Some(Rectangle {
            x: rail.x + (RAIL_WIDTH - THUMB_WIDTH) / 2.0,
            y: rail.y + fraction * (rail.height - height),
            width: THUMB_WIDTH,
            height,
        })
    }

    /// The document area, excluding the vertical rail and horizontal scrollbar.
    fn content_area(self) -> Rectangle {
        let bottom = if self.content.width > self.viewport.width {
            RAIL_WIDTH
        } else {
            0.0
        };
        Rectangle {
            width: (self.viewport.width - RAIL_WIDTH).max(0.0),
            height: (self.viewport.height - bottom).max(0.0),
            ..self.viewport
        }
    }

    /// Vertical scroll speed for a selection drag with the pointer at `y`.
    fn edge_speed(self, y: f32) -> f32 {
        if self.content.height <= self.viewport.height {
            return 0.0;
        }
        let area = self.content_area();
        let top = area.y + EDGE;
        let bottom = area.y + area.height - EDGE;
        let past = if y < top {
            y - top
        } else if y > bottom {
            y - bottom
        } else {
            return 0.0;
        };
        (past * EDGE_ACCELERATION).clamp(-MAX_EDGE_SPEED, MAX_EDGE_SPEED)
    }

    fn drag_offset(self, pointer_y: f32, grabbed_at: f32) -> f32 {
        let (Some(rail), Some(thumb)) = (self.rail(), self.thumb()) else {
            return 0.0;
        };
        let travel = rail.height - thumb.height;
        if travel <= 0.0 {
            return 0.0;
        }
        ((pointer_y - rail.y - grabbed_at) / travel).clamp(0.0, 1.0)
            * (self.content.height - self.viewport.height)
    }
}

#[derive(Default)]
struct State {
    geometry: Geometry,
    grabbed_at: Option<f32>,
    hovered: bool,
    auto_scroll_origin: Option<Point>,
    modifiers: iced::keyboard::Modifiers,
    /// A left-button drag that started in the document (a text selection).
    selecting: bool,
    pointer: Option<Point>,
    last_tick: Option<std::time::Instant>,
}

// The wrapped widget is the document scrollable itself. Do not traverse into
// content: a paragraph or future embedded control must not become the target.
#[derive(Default)]
struct Probe {
    geometry: Geometry,
    target_y: Option<f32>,
}

impl widget::Operation for Probe {
    fn traverse(&mut self, _operate: &mut dyn FnMut(&mut dyn widget::Operation)) {}

    fn scrollable(
        &mut self,
        _id: Option<&widget::Id>,
        viewport: Rectangle,
        content: Rectangle,
        offset: Vector,
        state: &mut dyn ScrollState,
    ) {
        self.geometry = Geometry {
            viewport,
            content,
            offset,
        };
        if let Some(y) = self.target_y {
            let y = y.clamp(0.0, (content.height - viewport.height).max(0.0));
            state.scroll_to(AbsoluteOffset {
                x: None,
                y: Some(y),
            });
            self.geometry.offset.y = y;
        }
    }
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for DocumentScroll<'_, Message> {
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }
    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }
    fn children(&self) -> Vec<widget::Tree> {
        vec![widget::Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut widget::Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
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
        let mut probe = Probe::default();
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, &mut probe);
        tree.state.downcast_mut::<State>().geometry = probe.geometry;
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let mut probe = Probe::default();
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, &mut probe);
        let state = tree.state.downcast_mut::<State>();
        state.geometry = probe.geometry;
        let rail = state.geometry.rail();
        let over_rail = rail.is_some_and(|rail| cursor.is_over(rail));
        if state.hovered != over_rail {
            state.hovered = over_rail;
            shell.request_redraw();
        }
        let was_auto_scrolling = state.auto_scroll_origin.is_some();
        if let Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.modifiers = *modifiers;
        }
        if let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event
            && cursor.is_over(layout.bounds())
            && control_pressed(state.modifiers)
        {
            let notches = match delta {
                mouse::ScrollDelta::Lines { y, .. } => *y,
                mouse::ScrollDelta::Pixels { y, .. } => *y / PIXELS_PER_NOTCH,
            };
            if notches != 0.0 {
                shell.publish((self.zoom)(notches));
            }
            shell.capture_event();
            return;
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                state.pointer = cursor.position();
                state.selecting = !over_rail
                    && state
                        .pointer
                        .is_some_and(|p| state.geometry.content_area().contains(p));
                state.last_tick = None;
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Window(iced::window::Event::Unfocused) => state.selecting = false,
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.selecting => {
                state.pointer = Some(*position);
                if state.geometry.edge_speed(position.y) != 0.0 {
                    shell.request_redraw();
                }
            }
            _ => {}
        }
        if let Event::Window(iced::window::Event::RedrawRequested(now)) = event
            && state.selecting
            && let Some(pointer) = state.pointer
        {
            let speed = state.geometry.edge_speed(pointer.y);
            let elapsed = state.last_tick.map_or(1.0 / 60.0, |last| {
                now.duration_since(last).as_secs_f32().min(0.05)
            });
            state.last_tick = (speed != 0.0).then_some(*now);
            let current = state.geometry.offset.y;
            let target = (current + speed * elapsed).clamp(
                0.0,
                (state.geometry.content.height - state.geometry.viewport.height).max(0.0),
            );
            if (target - current).abs() >= 0.5 {
                let mut probe = Probe {
                    target_y: Some(target),
                    ..Probe::default()
                };
                self.content.as_widget_mut().operate(
                    &mut tree.children[0],
                    layout,
                    renderer,
                    &mut probe,
                );
                state.geometry = probe.geometry;
                // The pointer may be outside the document or still; extend the
                // selection to the text now at the nearest visible edge.
                let area = state.geometry.content_area();
                let point = Point::new(
                    pointer
                        .x
                        .clamp(area.x + 1.0, (area.x + area.width - 1.0).max(area.x + 1.0)),
                    pointer
                        .y
                        .clamp(area.y + 1.0, (area.y + area.height - 1.0).max(area.y + 1.0)),
                );
                self.content.as_widget_mut().update(
                    &mut tree.children[0],
                    &Event::Mouse(mouse::Event::CursorMoved { position: point }),
                    layout,
                    mouse::Cursor::Available(point),
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
            }
            if speed != 0.0 {
                // A redraw requested while handling a redraw can be dropped on
                // Windows; a timed request wakes the event loop reliably.
                shell.request_redraw_at(iced::window::RedrawRequest::At(
                    *now + std::time::Duration::from_millis(16),
                ));
            }
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle))
                if !was_auto_scrolling && cursor.is_over(layout.bounds()) =>
            {
                state.auto_scroll_origin = cursor.position();
            }
            Event::Mouse(
                mouse::Event::ButtonPressed(_)
                | mouse::Event::ButtonReleased(mouse::Button::Middle | mouse::Button::Left)
                | mouse::Event::WheelScrolled { .. },
            )
            | Event::Keyboard(_)
            | Event::Touch(_)
            | Event::Window(iced::window::Event::Unfocused) => {
                state.auto_scroll_origin = None;
            }
            _ => {}
        }
        let unfocused = matches!(event, Event::Window(iced::window::Event::Unfocused));
        if unfocused {
            state.grabbed_at = None;
        }
        if unfocused
            || (was_auto_scrolling
                && matches!(
                    event,
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle))
                ))
        {
            // Iced's autoscroll toggles by default and ignores middle release.
            // End its interaction locally on release (even outside the viewport)
            // or focus loss, using the same cancellation as a native left release.
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
            shell.invalidate_layout();
            shell.request_redraw();
        }
        let target = match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if over_rail && !was_auto_scrolling =>
            {
                let point = cursor.position().expect("cursor is over rail");
                let thumb = state.geometry.thumb().expect("scrolling rail has a thumb");
                let grabbed = if point.y >= thumb.y && point.y <= thumb.y + thumb.height {
                    point.y - thumb.y
                } else {
                    thumb.height / 2.0
                };
                state.grabbed_at = Some(grabbed);
                Some(state.geometry.drag_offset(point.y, grabbed))
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.grabbed_at.is_some() => {
                Some(
                    state
                        .geometry
                        .drag_offset(position.y, state.grabbed_at.unwrap()),
                )
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.grabbed_at.take().is_some() =>
            {
                shell.capture_event();
                shell.request_redraw();
                return;
            }
            _ => None,
        };
        if let Some(y) = target {
            let mut probe = Probe {
                target_y: Some(y),
                ..Probe::default()
            };
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout,
                renderer,
                &mut probe,
            );
            state.geometry = probe.geometry;
            // The next redraw lets Iced publish its normal viewport notification;
            // virtualization, saved positions and page labels use one scroll path.
            shell.capture_event();
            shell.request_redraw();
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
        let mut probe = Probe::default();
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, &mut probe);
        state.geometry = probe.geometry;
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        let state = tree.state.downcast_ref::<State>();
        if let (Some(rail), Some(thumb)) = (state.geometry.rail(), state.geometry.thumb())
            && let Some(clip) = rail.intersection(viewport)
        {
            // Keep the rail above page rasters, including horizontally zoomed PDFs.
            renderer.with_layer(clip, |renderer| {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: rail,
                        ..renderer::Quad::default()
                    },
                    ui::palette(theme).background,
                );
                // At rest the thumb is a slim, faint line; it widens under the pointer.
                // The hit area stays the full rail either way.
                let engaged = state.grabbed_at.is_some() || state.hovered;
                let drawn = if engaged {
                    thumb
                } else {
                    Rectangle {
                        x: thumb.x + thumb.width / 2.0 - 2.0,
                        width: 4.0,
                        ..thumb
                    }
                };
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: drawn,
                        border: Border::default().rounded(4),
                        ..renderer::Quad::default()
                    },
                    if state.grabbed_at.is_some() {
                        ui::palette(theme).accent
                    } else if state.hovered {
                        ui::palette(theme).secondary
                    } else {
                        ui::palette(theme).muted.scale_alpha(0.45)
                    },
                );
            });
        }
        if let Some(origin) = state.auto_scroll_origin {
            let marker = Rectangle::new(origin - Vector::new(7.0, 7.0), Size::new(14.0, 14.0));
            if let Some(clip) = marker.intersection(viewport) {
                renderer.with_layer(clip, |renderer| {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: marker,
                            border: Border {
                                color: ui::palette(theme).secondary,
                                width: 1.0,
                                ..Border::default().rounded(7)
                            },
                            ..renderer::Quad::default()
                        },
                        ui::palette(theme).background,
                    );
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle::new(
                                origin - Vector::new(1.0, 1.0),
                                Size::new(2.0, 2.0),
                            ),
                            border: Border::default().rounded(1),
                            ..renderer::Quad::default()
                        },
                        ui::palette(theme).secondary,
                    );
                });
            }
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
        let state = tree.state.downcast_ref::<State>();
        if state.grabbed_at.is_some() {
            mouse::Interaction::Grabbing
        } else if state
            .geometry
            .rail()
            .is_some_and(|rail| cursor.is_over(rail))
        {
            mouse::Interaction::Pointer
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

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut widget::Tree,
        layout: Layout<'a>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn long_document(offset: f32) -> Geometry {
        Geometry {
            viewport: Rectangle {
                x: 20.0,
                y: 80.0,
                width: 800.0,
                height: 700.0,
            },
            content: Rectangle {
                width: 800.0,
                height: 120_000.0,
                ..Rectangle::default()
            },
            offset: Vector::new(0.0, offset),
        }
    }

    #[test]
    fn long_document_thumb_is_grabbable_and_reaches_both_ends() {
        let mut geometry = long_document(0.0);
        let rail = geometry.rail().unwrap();
        let thumb = geometry.thumb().unwrap();
        assert_eq!(thumb.height, MIN_THUMB);
        assert_eq!(thumb.y, rail.y);
        let max = geometry.content.height - geometry.viewport.height;
        geometry.offset.y = max;
        let bottom = geometry.thumb().unwrap();
        assert_eq!(bottom.y + bottom.height, rail.y + rail.height);
        assert_eq!(
            geometry.drag_offset(rail.y + rail.height - MIN_THUMB / 2.0, MIN_THUMB / 2.0),
            max
        );
        assert_eq!(geometry.drag_offset(rail.y - 100.0, MIN_THUMB / 2.0), 0.0);
    }

    #[test]
    fn dragging_preserves_the_grab_point_and_is_monotonic() {
        let geometry = long_document(45_000.0);
        let thumb = geometry.thumb().unwrap();
        let offset = geometry.drag_offset(thumb.y + 9.0, 9.0);
        assert!((offset - geometry.offset.y).abs() < 0.01);
        assert!(geometry.drag_offset(thumb.y + 30.0, 9.0) > offset);
    }

    #[test]
    fn short_viewports_bound_thumb_and_nonoverflow_has_no_rail() {
        let mut geometry = long_document(0.0);
        geometry.viewport.height = 24.0;
        assert_eq!(geometry.thumb().unwrap().height, 24.0);
        assert_eq!(geometry.drag_offset(400.0, 5.0), 0.0);
        geometry.content.height = 20.0;
        assert!(geometry.rail().is_none());
    }
}
