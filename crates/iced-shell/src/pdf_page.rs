use iced::advanced::{
    Layout, Renderer as _, Widget, image::Renderer as _, layout, mouse, renderer, widget::Tree,
};
use iced::{Element, Length, Point, Rectangle, Size};
use reader_pdf::{Selection, TextLayer, TextPoint};
use std::sync::Arc;

use super::Message;

const HIGHLIGHT: iced::Color = iced::Color::from_rgba8(0x38, 0x80, 0xe0, 0.48);

pub(super) struct Page<'a> {
    pub number: u32,
    pub width: f32,
    pub height: f32,
    pub image: Option<&'a iced::widget::image::Handle>,
    pub text: Option<&'a Arc<TextLayer>>,
    pub selection: Option<Selection>,
    pub dragging: bool,
}

pub(super) fn hit(
    text: &TextLayer,
    page: u32,
    bounds: Rectangle,
    position: Point,
    exact: bool,
) -> Option<TextPoint> {
    let x = ((position.x - bounds.x) / bounds.width).clamp(0.0, 1.0);
    let y = ((position.y - bounds.y) / bounds.height).clamp(0.0, 1.0);
    let mut nearest = None;
    let mut distance = f32::INFINITY;
    for (index, glyph) in text.glyphs.iter().enumerate() {
        let Some(rect) = glyph.bounds else { continue };
        let dx = (rect.left - x).max(x - rect.right).max(0.0);
        let dy = (rect.top - y).max(y - rect.bottom).max(0.0);
        // Vertical proximity takes priority when selecting across lines.
        let score = dy * dy * 4.0 + dx * dx;
        if score < distance {
            distance = score;
            nearest = Some(TextPoint { page, index });
        }
    }
    if exact && distance > 0.0 {
        None
    } else {
        nearest
    }
}

impl Widget<Message, iced::Theme, iced::Renderer> for Page<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.width), Length::Fixed(self.height))
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.resolve(
            Length::Fixed(self.width),
            Length::Fixed(self.height),
            Size::new(self.width, self.height),
        ))
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let Some(clip) = bounds.intersection(viewport) else {
            return;
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                ..Default::default()
            },
            iced::Background::Color(iced::Color::WHITE),
        );
        if let Some(image) = self.image {
            renderer.draw_image(
                iced::advanced::image::Image::new(image.clone()),
                bounds,
                clip,
            );
        }
        if let (Some(text), Some(selection)) = (self.text, self.selection) {
            let (start, end) = if selection.anchor <= selection.focus {
                (selection.anchor, selection.focus)
            } else {
                (selection.focus, selection.anchor)
            };
            if start.page <= self.number && self.number <= end.page {
                // tiny-skia batches images after quads within a layer. Put
                // selection ink in a later layer so the opaque page cannot hide it.
                renderer.with_layer(clip, |renderer| {
                    for (index, glyph) in text.glyphs.iter().enumerate() {
                        if (self.number == start.page && index < start.index)
                            || (self.number == end.page && index > end.index)
                        {
                            continue;
                        }
                        let Some(rect) = glyph.bounds else { continue };
                        let box_bounds = Rectangle::new(
                            Point::new(
                                bounds.x + rect.left * bounds.width,
                                bounds.y + rect.top * bounds.height,
                            ),
                            Size::new(
                                (rect.right - rect.left).max(0.0) * bounds.width,
                                (rect.bottom - rect.top).max(0.0) * bounds.height,
                            ),
                        );
                        if box_bounds.intersection(&clip).is_some() {
                            renderer.fill_quad(
                                renderer::Quad {
                                    bounds: box_bounds,
                                    ..Default::default()
                                },
                                iced::Background::Color(HIGHLIGHT),
                            );
                        }
                    }
                });
            }
        }
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut iced::advanced::Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if !layout.bounds().intersects(viewport) {
            return;
        }
        match event {
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)) => {
                if let Some(position) = cursor.position_over(layout.bounds()) {
                    let point = self
                        .text
                        .and_then(|text| hit(text, self.number, layout.bounds(), position, true));
                    shell.publish(point.map_or(Message::ClearSelection, Message::SelectStart));
                }
            }
            iced::Event::Mouse(iced::mouse::Event::CursorMoved { .. }) if self.dragging => {
                if let (Some(text), Some(position)) = (
                    self.text,
                    cursor.position().filter(|position| {
                        position.y >= layout.bounds().y
                            && position.y <= layout.bounds().y + layout.bounds().height
                    }),
                ) && let Some(point) = hit(text, self.number, layout.bounds(), position, false)
                {
                    shell.publish(Message::SelectMove(point));
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        if self.text.is_some_and(|text| !text.glyphs.is_empty()) && cursor.is_over(layout.bounds())
        {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }
}

impl<'a> From<Page<'a>> for Element<'a, Message> {
    fn from(page: Page<'a>) -> Self {
        Element::new(page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reader_pdf::{Glyph, Rect};

    #[test]
    fn blank_space_does_not_start_selection_but_dragging_finds_a_source_glyph() {
        let text = TextLayer {
            text: "é A".into(),
            glyphs: vec![
                Glyph {
                    start: 0,
                    end: 2,
                    bounds: Some(Rect {
                        left: 0.1,
                        top: 0.1,
                        right: 0.2,
                        bottom: 0.2,
                    }),
                },
                Glyph {
                    start: 2,
                    end: 3,
                    bounds: None,
                },
                Glyph {
                    start: 3,
                    end: 4,
                    bounds: Some(Rect {
                        left: 0.3,
                        top: 0.1,
                        right: 0.4,
                        bottom: 0.2,
                    }),
                },
            ],
        };
        let bounds = Rectangle::new(Point::new(20.0, 30.0), Size::new(100.0, 200.0));
        let gap = Point::new(49.0, 60.0);
        assert_eq!(hit(&text, 7, bounds, gap, true), None);
        assert_eq!(
            hit(&text, 7, bounds, gap, false),
            Some(TextPoint { page: 7, index: 2 })
        );
        assert_eq!(
            hit(&text, 7, bounds, Point::new(55.0, 60.0), true),
            Some(TextPoint { page: 7, index: 2 })
        );
    }
}
