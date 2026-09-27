//! Scale the completed paper, including pointer coordinates, without reflowing text.
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, renderer, widget};
use iced::{Element, Event, Length, Rectangle, Size, Transformation};

pub fn wrap<'a, M: 'a>(content: Element<'a, M>, width: f32, scale: f32) -> Element<'a, M> {
    Element::new(Zoom {
        content,
        width,
        scale,
    })
}
struct Zoom<'a, M> {
    content: Element<'a, M>,
    width: f32,
    scale: f32,
}
impl<M> Zoom<'_, M> {
    fn transform(&self, layout: Layout<'_>) -> Transformation {
        let p = layout.position();
        Transformation::translate(p.x, p.y)
            * Transformation::scale(self.scale)
            * Transformation::translate(-p.x, -p.y)
    }
}
impl<M> Widget<M, iced::Theme, iced::Renderer> for Zoom<'_, M> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.width * self.scale), Length::Shrink)
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
        _: &layout::Limits,
    ) -> layout::Node {
        let child = self.content.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &layout::Limits::new(
                Size::new(self.width, 0.0),
                Size::new(self.width, f32::INFINITY),
            ),
        );
        layout::Node::with_children(
            Size::new(self.width * self.scale, child.size().height * self.scale),
            vec![child],
        )
    }
    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            layout.children().next().unwrap(),
            renderer,
            operation,
        );
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
        let inverse = self.transform(layout).inverse();
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout.children().next().unwrap(),
            cursor * inverse,
            renderer,
            clipboard,
            shell,
            &(*viewport * inverse),
        );
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
        use iced::advanced::Renderer;
        let transform = self.transform(layout);
        let inverse = transform.inverse();
        renderer.with_layer(*viewport, |renderer| {
            renderer.with_transformation(transform, |renderer| {
                self.content.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    layout.children().next().unwrap(),
                    cursor * inverse,
                    &(*viewport * inverse),
                );
            })
        });
    }
    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let inverse = self.transform(layout).inverse();
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout.children().next().unwrap(),
            cursor * inverse,
            &(*viewport * inverse),
            renderer,
        )
    }
}
