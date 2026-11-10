use iced::advanced::{
    Layout, Renderer as _, Widget, image::Renderer as _, layout, mouse, renderer, widget::Tree,
};
use iced::{Element, Length, Point, Rectangle, Size};
use reader_pdf::{Selection, TextLayer, TextPoint};
use std::sync::Arc;

use super::{Clicked, Mark, Message};

const HIGHLIGHT: iced::Color = iced::Color::from_rgba8(0x38, 0x80, 0xe0, 0.48);
const RIBBON: iced::Color = iced::Color::from_rgb(0.78, 0.20, 0.17);

/// Joins glyph boxes that sit on one line, so a highlight is a few quads per line.
fn merge_lines(boxes: Vec<Rectangle>) -> Vec<Rectangle> {
    let mut lines: Vec<Rectangle> = Vec::new();
    for glyph in boxes {
        if let Some(last) = lines.last_mut()
            && (last.y - glyph.y).abs() < 1.5
            && (last.height - glyph.height).abs() < 3.0
            && glyph.x >= last.x - 0.5
            && glyph.x <= last.x + last.width + 4.0
        {
            let right = (glyph.x + glyph.width).max(last.x + last.width);
            let bottom = (glyph.y + glyph.height).max(last.y + last.height);
            last.y = last.y.min(glyph.y);
            last.width = right - last.x;
            last.height = bottom - last.y;
        } else {
            lines.push(glyph);
        }
    }
    lines
}

pub(super) struct Page<'a> {
    pub number: u32,
    pub width: f32,
    pub height: f32,
    pub image: Option<&'a iced::widget::image::Handle>,
    pub text: Option<&'a Arc<TextLayer>>,
    pub selection: Option<Selection>,
    pub dragging: bool,
    /// Saved highlights, oldest first.
    pub marks: &'a [Mark],
    pub spoken: Option<std::ops::Range<usize>>,
    pub bookmarked: bool,
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
    let (index, dx, dy) = text.closest(x, y, bounds.width, bounds.height)?;
    // A press just outside letter ink (including a source-space glyph between
    // words) starts selection, but blank page margins do not grab distant text.
    if exact && (dx > 12.0 || dy > 8.0) {
        None
    } else {
        Some(TextPoint { page, index })
    }
}

/// The first character of the line beside `position` or, between lines, of
/// the next line down. After the page's last line, the position after its
/// last character.
fn below(text: &TextLayer, page: u32, bounds: Rectangle, position: Point) -> TextPoint {
    let y = (position.y - bounds.y) / bounds.height;
    let index = text
        .glyphs
        .iter()
        .position(|glyph| glyph.bounds.is_some_and(|rect| rect.bottom >= y))
        .unwrap_or(text.glyphs.len());
    TextPoint { page, index }
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
        theme: &iced::Theme,
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
        if self.bookmarked {
            renderer.with_layer(clip, |renderer| {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle::new(
                            Point::new(bounds.x + bounds.width - 44.0, bounds.y),
                            Size::new(20.0, 36.0),
                        ),
                        border: iced::Border {
                            radius: iced::border::Radius {
                                top_left: 0.0,
                                top_right: 0.0,
                                bottom_right: 6.0,
                                bottom_left: 6.0,
                            },
                            ..iced::Border::default()
                        },
                        ..Default::default()
                    },
                    iced::Background::Color(RIBBON),
                );
            });
        }
        if let Some(text) = self.text {
            let spans = crate::notes::coalesce_colored_ranges(
                self.marks
                    .iter()
                    .filter_map(|mark| {
                        crate::notes::pdf_range_on_page(
                            mark.from,
                            mark.to,
                            self.number,
                            text.glyphs.len(),
                        )
                        .map(|range| (range, mark.tint, mark.note))
                    })
                    .collect(),
            );
            for (range, tint, note) in spans {
                let boxes = text.glyphs[range]
                    .iter()
                    .filter_map(|glyph| glyph.bounds)
                    .map(|rect| {
                        Rectangle::new(
                            Point::new(
                                bounds.x + rect.left * bounds.width,
                                bounds.y + rect.top * bounds.height,
                            ),
                            Size::new(
                                (rect.right - rect.left).max(0.0) * bounds.width,
                                (rect.bottom - rect.top).max(0.0) * bounds.height,
                            ),
                        )
                    })
                    .collect();
                renderer.with_layer(clip, |renderer| {
                    for line in merge_lines(boxes) {
                        if line.intersection(&clip).is_none() {
                            continue;
                        }
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: line,
                                ..Default::default()
                            },
                            iced::Background::Color(tint),
                        );
                        if note {
                            renderer.fill_quad(
                                renderer::Quad {
                                    bounds: Rectangle {
                                        y: line.y + line.height - 2.0,
                                        height: 2.0,
                                        ..line
                                    },
                                    ..Default::default()
                                },
                                iced::Background::Color(iced::Color { a: 1.0, ..tint }),
                            );
                        }
                    }
                });
            }
        }
        if let (Some(text), Some(selection)) = (self.text, self.selection) {
            let (start, end) = if selection.anchor <= selection.focus {
                (selection.anchor, selection.focus)
            } else {
                (selection.focus, selection.anchor)
            };
            if start.page <= self.number && self.number <= end.page {
                let first = (if self.number == start.page {
                    start.index
                } else {
                    0
                })
                .min(text.glyphs.len());
                let last = (if self.number == end.page {
                    end.index.saturating_add(1)
                } else {
                    text.glyphs.len()
                })
                .min(text.glyphs.len());
                // tiny-skia batches images after quads within a layer. Put
                // selection ink in a later layer so the opaque page cannot hide it.
                renderer.with_layer(clip, |renderer| {
                    for glyph in &text.glyphs[first..last] {
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
        if let Some(text) = self.text
            && let Some(range) = &self.spoken
        {
            let boxes = text
                .glyphs
                .get(range.clone())
                .unwrap_or_default()
                .iter()
                .filter_map(|glyph| glyph.bounds)
                .map(|rect| {
                    Rectangle::new(
                        Point::new(
                            bounds.x + rect.left * bounds.width,
                            bounds.y + rect.top * bounds.height,
                        ),
                        Size::new(
                            (rect.right - rect.left).max(0.0) * bounds.width,
                            (rect.bottom - rect.top).max(0.0) * bounds.height,
                        ),
                    )
                })
                .collect();
            renderer.with_layer(clip, |renderer| {
                for line in merge_lines(boxes) {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: line,
                            border: iced::Border {
                                color: theme.palette().primary,
                                width: 1.5,
                                radius: 3.0.into(),
                            },
                            ..Default::default()
                        },
                        theme.palette().primary.scale_alpha(0.18),
                    );
                }
            });
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
                if let Some(position) = cursor
                    .position_over(layout.bounds())
                    .filter(|position| viewport.contains(*position))
                {
                    let point = self
                        .text
                        .and_then(|text| hit(text, self.number, layout.bounds(), position, true));
                    shell.publish(point.map_or(Message::ClearSelection, Message::SelectStart));
                }
            }
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Right)) => {
                if let Some(position) = cursor
                    .position_over(layout.bounds())
                    .filter(|position| viewport.contains(*position))
                {
                    let bounds = layout.bounds();
                    let clicked = self.text.map(|text| {
                        match hit(text, self.number, bounds, position, true) {
                            Some(point) => Clicked::Text(point),
                            None => Clicked::Blank {
                                below: below(text, self.number, bounds, position),
                            },
                        }
                    });
                    shell.publish(Message::Context(clicked));
                    shell.capture_event();
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
        viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        if self.text.is_some_and(|text| {
            cursor
                .position_over(layout.bounds())
                .filter(|position| viewport.contains(*position))
                .is_some_and(|position| {
                    hit(text, self.number, layout.bounds(), position, true).is_some()
                })
        }) {
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
    fn nearby_whitespace_starts_selection_but_blank_page_does_not() {
        let text = TextLayer::new(
            "é A".into(),
            vec![
                Glyph {
                    start: 0,
                    end: 2,
                    bounds: Some(Rect {
                        left: 0.1,
                        top: 0.1,
                        right: 0.2,
                        bottom: 0.13,
                    }),
                    style: None,
                },
                Glyph {
                    start: 2,
                    end: 3,
                    bounds: None,
                    style: None,
                },
                Glyph {
                    start: 3,
                    end: 4,
                    bounds: Some(Rect {
                        left: 0.3,
                        top: 0.1,
                        right: 0.4,
                        bottom: 0.13,
                    }),
                    style: None,
                },
            ],
        );
        let bounds = Rectangle::new(Point::new(20.0, 30.0), Size::new(100.0, 200.0));
        assert_eq!(
            hit(&text, 7, bounds, Point::new(21.0, 54.0), true),
            Some(TextPoint { page: 7, index: 0 })
        );
        assert_eq!(
            hit(&text, 7, bounds, Point::new(49.0, 54.0), true),
            Some(TextPoint { page: 7, index: 2 })
        );
        assert_eq!(hit(&text, 7, bounds, Point::new(23.0, 95.0), true), None);
        assert_eq!(
            hit(&text, 7, bounds, Point::new(23.0, 95.0), false),
            Some(TextPoint { page: 7, index: 0 })
        );
    }

    /// One letter on a line a tenth down the page and one on a line halfway
    /// down, with the space between them.
    fn two_lines() -> Arc<TextLayer> {
        let letter = |start, top| Glyph {
            start,
            end: start + 1,
            bounds: Some(Rect {
                left: 0.1,
                top,
                right: 0.2,
                bottom: top + 0.03,
            }),
            style: None,
        };
        let space = Glyph {
            start: 1,
            end: 2,
            bounds: None,
            style: None,
        };
        Arc::new(TextLayer::new(
            "A B".into(),
            vec![letter(0, 0.1), space, letter(2, 0.5)],
        ))
    }

    #[test]
    fn a_right_click_reports_the_character_under_it_or_the_first_one_below() {
        use iced::advanced::{Shell, widget::Tree};
        let text = two_lines();
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(100.0, 200.0));
        let right_click = |text: Option<&Arc<TextLayer>>, at: Point| {
            let mut page: Element<'_, Message> = Page {
                number: 7,
                width: bounds.width,
                height: bounds.height,
                image: None,
                text,
                selection: None,
                dragging: false,
                marks: &[],
                spoken: None,
                bookmarked: false,
            }
            .into();
            let renderer = iced::Renderer::new(iced::Font::DEFAULT, iced::Pixels(13.0));
            let mut tree = Tree::new(&page);
            let node = page.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, bounds.size()),
            );
            let mut messages = Vec::new();
            page.as_widget_mut().update(
                &mut tree,
                &iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
                Layout::new(&node),
                mouse::Cursor::Available(at),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &bounds,
            );
            match messages.as_slice() {
                [Message::Context(clicked)] => *clicked,
                other => panic!("one right-click message, got {other:?}"),
            }
        };
        let at = |index| TextPoint { page: 7, index };
        assert_eq!(
            right_click(Some(&text), Point::new(15.0, 23.0)),
            Some(Clicked::Text(at(0))),
            "on the first letter"
        );
        assert_eq!(
            right_click(Some(&text), Point::new(60.0, 60.0)),
            Some(Clicked::Blank { below: at(2) }),
            "between the lines, beside them"
        );
        assert_eq!(
            right_click(Some(&text), Point::new(80.0, 23.0)),
            Some(Clicked::Blank { below: at(0) }),
            "far beside the first line: that line"
        );
        assert_eq!(
            right_click(Some(&text), Point::new(15.0, 150.0)),
            Some(Clicked::Blank { below: at(3) }),
            "under the last line: the end of the page"
        );
        assert_eq!(
            right_click(None, Point::new(15.0, 23.0)),
            None,
            "the page's text is not loaded"
        );
    }
}
