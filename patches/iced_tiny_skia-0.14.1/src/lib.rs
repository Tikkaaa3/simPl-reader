#![allow(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]
pub mod window;

mod engine;
mod layer;
mod primitive;
mod settings;
mod text;

#[cfg(feature = "image")]
mod raster;

#[cfg(feature = "svg")]
mod vector;

#[cfg(feature = "geometry")]
pub mod geometry;

use iced_debug as debug;
pub use iced_graphics as graphics;
pub use iced_graphics::core;

pub use layer::Layer;
pub use primitive::Primitive;
pub use settings::Settings;

#[cfg(feature = "geometry")]
pub use geometry::Geometry;

use crate::core::renderer;
use crate::core::{
    Background, Color, Font, Pixels, Point, Rectangle, Size, Transformation,
};
use crate::engine::Engine;
use crate::graphics::Viewport;
use crate::graphics::compositor;
use crate::graphics::text::{Editor, Paragraph};

/// A [`tiny-skia`] graphics renderer for [`iced`].
///
/// [`tiny-skia`]: https://github.com/RazrFalcon/tiny-skia
/// [`iced`]: https://github.com/iced-rs/iced
#[derive(Debug)]
pub struct Renderer {
    default_font: Font,
    default_text_size: Pixels,
    layers: layer::Stack,
    engine: Engine, // TODO: Shared engine
}

#[cfg(test)]
mod occlusion_tests {
    use super::*;
    use crate::core::Renderer as _;

    #[test]
    fn opaque_layer_skip_matches_all_layers_including_corners_and_clips() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for alpha in [0.5, 1.0] {
                for radius in [0.0, 12.0] {
                    let size = Size::new(200.0, 140.0);
                    let viewport = Viewport::with_physical_size(
                        Size::new(
                            (size.width * scale) as u32,
                            (size.height * scale) as u32,
                        ),
                        scale,
                    );
                    let mut renderer =
                        Renderer::new(Font::DEFAULT, Pixels(13.0));
                    renderer.reset(Rectangle::new(Point::ORIGIN, size));
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle::new(Point::ORIGIN, size),
                            ..Default::default()
                        },
                        Color::from_rgb(0.7, 0.1, 0.2),
                    );
                    renderer.start_layer(Rectangle::new(
                        Point::new(10.5, 10.25),
                        Size::new(170.0, 110.0),
                    ));
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle::new(
                                Point::new(8.25, 8.5),
                                Size::new(180.0, 120.0),
                            ),
                            border: core::Border {
                                radius: radius.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        Color::from_rgba(0.1, 0.6, 0.8, alpha),
                    );
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle::new(
                                Point::new(40.0, 35.0),
                                Size::new(60.0, 15.0),
                            ),
                            ..Default::default()
                        },
                        Color::BLACK,
                    );
                    renderer.end_layer();
                    for damage in [
                        Rectangle::new(
                            Point::new(30.0, 30.0),
                            Size::new(100.0, 65.0),
                        ),
                        Rectangle::new(
                            Point::new(7.0, 7.0),
                            Size::new(22.0, 22.0),
                        ),
                        Rectangle::new(Point::ORIGIN, size),
                    ] {
                        let physical = viewport.physical_size();
                        let mut actual = tiny_skia::Pixmap::new(
                            physical.width,
                            physical.height,
                        )
                        .unwrap();
                        actual.fill(tiny_skia::Color::from_rgba8(
                            40, 50, 60, 255,
                        ));
                        let mut expected = actual.clone();
                        let mut mask = tiny_skia::Mask::new(
                            physical.width,
                            physical.height,
                        )
                        .unwrap();
                        renderer.draw_inner(
                            &mut actual.as_mut(),
                            &mut mask,
                            &viewport,
                            &[damage],
                            Color::WHITE,
                            true,
                        );
                        renderer.draw_inner(
                            &mut expected.as_mut(),
                            &mut mask,
                            &viewport,
                            &[damage],
                            Color::WHITE,
                            false,
                        );
                        assert!(
                            actual.data() == expected.data(),
                            "scale={scale}, alpha={alpha}, radius={radius}, damage={damage:?}"
                        );
                    }
                }
            }
        }
    }
}

impl Renderer {
    pub fn new(default_font: Font, default_text_size: Pixels) -> Self {
        Self {
            default_font,
            default_text_size,
            layers: layer::Stack::new(),
            engine: Engine::new(),
        }
    }

    pub fn layers(&mut self) -> &[Layer] {
        self.layers.flush();
        self.layers.as_slice()
    }

    pub fn draw(
        &mut self,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        clip_mask: &mut tiny_skia::Mask,
        viewport: &Viewport,
        damage: &[Rectangle],
        background_color: Color,
    ) {
        self.draw_inner(
            pixels,
            clip_mask,
            viewport,
            damage,
            background_color,
            true,
        );
    }

    fn draw_inner(
        &mut self,
        pixels: &mut tiny_skia::PixmapMut<'_>,
        clip_mask: &mut tiny_skia::Mask,
        viewport: &Viewport,
        damage: &[Rectangle],
        background_color: Color,
        occlude: bool,
    ) {
        let scale_factor = viewport.scale_factor();
        self.engine.reset_clip_mask();

        self.layers.flush();

        for &damage_bounds in damage {
            let damage_bounds = damage_bounds * scale_factor;
            // An opaque later surface completely covering this damage makes
            // every earlier layer invisible. The inset excludes rounded edges,
            // strokes and clip-mask edge coverage. In particular, scrolling a
            // modal must not repaint the book and dimming underneath its paper.
            let first = if occlude {
                self.layers
                        .as_slice()
                        .iter()
                        .enumerate()
                        .rev()
                        .find_map(|(index, layer)| {
                            if !damage_bounds
                                .expand(1.0)
                                .is_within(&(layer.bounds * scale_factor))
                            {
                                return None;
                            }
                            layer.quads.iter().any(|(quad, background)| {
                        if !matches!(background, Background::Color(color) if color.a == 1.0) {
                            return false;
                        }
                        let radius = <[f32; 4]>::from(quad.border.radius).into_iter()
                            .fold(quad.border.width, f32::max) * scale_factor + 1.0;
                        let inner = (quad.bounds * scale_factor).shrink(radius);
                        inner.width > 0.0 && inner.height > 0.0 && damage_bounds.is_within(&inner)
                    }).then_some(index)
                        })
                        .unwrap_or(0)
            } else {
                0
            };

            let path = tiny_skia::PathBuilder::from_rect(
                tiny_skia::Rect::from_xywh(
                    damage_bounds.x,
                    damage_bounds.y,
                    damage_bounds.width,
                    damage_bounds.height,
                )
                .expect("Create damage rectangle"),
            );

            pixels.fill_path(
                &path,
                &tiny_skia::Paint {
                    shader: tiny_skia::Shader::SolidColor(engine::into_color(
                        background_color,
                    )),
                    anti_alias: false,
                    blend_mode: tiny_skia::BlendMode::Source,
                    ..Default::default()
                },
                tiny_skia::FillRule::default(),
                tiny_skia::Transform::identity(),
                None,
            );

            for layer in self.layers.iter().skip(first) {
                let Some(layer_bounds) =
                    damage_bounds.intersection(&(layer.bounds * scale_factor))
                else {
                    continue;
                };

                self.engine.adjust_clip_mask(clip_mask, layer_bounds);

                if !layer.quads.is_empty() {
                    let render_span = debug::render(debug::Primitive::Quad);
                    for (quad, background) in &layer.quads {
                        self.engine.draw_quad(
                            quad,
                            background,
                            Transformation::scale(scale_factor),
                            pixels,
                            clip_mask,
                            layer_bounds,
                        );
                    }
                    render_span.finish();
                }

                if !layer.primitives.is_empty() {
                    let render_span = debug::render(debug::Primitive::Triangle);

                    for group in &layer.primitives {
                        let Some(group_bounds) = (group.clip_bounds()
                            * scale_factor)
                            .intersection(&layer_bounds)
                        else {
                            continue;
                        };

                        self.engine.adjust_clip_mask(clip_mask, group_bounds);

                        for primitive in group.as_slice() {
                            self.engine.draw_primitive(
                                primitive,
                                Transformation::scale(scale_factor)
                                    * group.transformation(),
                                pixels,
                                clip_mask,
                                group_bounds,
                            );
                        }

                        self.engine.adjust_clip_mask(clip_mask, layer_bounds);
                    }

                    render_span.finish();
                }

                if !layer.images.is_empty() {
                    let render_span = debug::render(debug::Primitive::Image);

                    for image in &layer.images {
                        self.engine.draw_image(
                            image,
                            Transformation::scale(scale_factor),
                            pixels,
                            clip_mask,
                            layer_bounds,
                        );
                    }

                    render_span.finish();
                }

                if !layer.text.is_empty() {
                    let render_span = debug::render(debug::Primitive::Image);

                    for group in &layer.text {
                        for text in group.as_slice() {
                            self.engine.draw_text(
                                text,
                                Transformation::scale(scale_factor)
                                    * group.transformation(),
                                pixels,
                                clip_mask,
                                layer_bounds,
                            );
                        }
                    }

                    render_span.finish();
                }
            }
        }

        self.engine.trim();
    }
}

impl core::Renderer for Renderer {
    fn start_layer(&mut self, bounds: Rectangle) {
        self.layers.push_clip(bounds);
    }

    fn end_layer(&mut self) {
        self.layers.pop_clip();
    }

    fn start_transformation(&mut self, transformation: Transformation) {
        self.layers.push_transformation(transformation);
    }

    fn end_transformation(&mut self) {
        self.layers.pop_transformation();
    }

    fn fill_quad(
        &mut self,
        quad: renderer::Quad,
        background: impl Into<Background>,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_quad(quad, background.into(), transformation);
    }

    fn reset(&mut self, new_bounds: Rectangle) {
        self.layers.reset(new_bounds);
    }

    fn allocate_image(
        &mut self,
        _handle: &core::image::Handle,
        callback: impl FnOnce(Result<core::image::Allocation, core::image::Error>)
        + Send
        + 'static,
    ) {
        #[cfg(feature = "image")]
        #[allow(unsafe_code)]
        // TODO: Concurrency
        callback(self.engine.raster_pipeline.load(_handle));

        #[cfg(not(feature = "image"))]
        callback(Err(core::image::Error::Unsupported));
    }
}

impl core::text::Renderer for Renderer {
    type Font = Font;
    type Paragraph = Paragraph;
    type Editor = Editor;

    const ICON_FONT: Font = Font::with_name("Iced-Icons");
    const CHECKMARK_ICON: char = '\u{f00c}';
    const ARROW_DOWN_ICON: char = '\u{e800}';
    const ICED_LOGO: char = '\u{e801}';
    const SCROLL_UP_ICON: char = '\u{e802}';
    const SCROLL_DOWN_ICON: char = '\u{e803}';
    const SCROLL_LEFT_ICON: char = '\u{e804}';
    const SCROLL_RIGHT_ICON: char = '\u{e805}';

    fn default_font(&self) -> Self::Font {
        self.default_font
    }

    fn default_size(&self) -> Pixels {
        self.default_text_size
    }

    fn fill_paragraph(
        &mut self,
        text: &Self::Paragraph,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();

        layer.draw_paragraph(
            text,
            position,
            color,
            clip_bounds,
            transformation,
        );
    }

    fn fill_editor(
        &mut self,
        editor: &Self::Editor,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_editor(editor, position, color, clip_bounds, transformation);
    }

    fn fill_text(
        &mut self,
        text: core::Text,
        position: Point,
        color: Color,
        clip_bounds: Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_text(text, position, color, clip_bounds, transformation);
    }
}

impl graphics::text::Renderer for Renderer {
    fn fill_raw(&mut self, raw: graphics::text::Raw) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_text_raw(raw, transformation);
    }
}

#[cfg(feature = "geometry")]
impl graphics::geometry::Renderer for Renderer {
    type Geometry = Geometry;
    type Frame = geometry::Frame;

    fn new_frame(&self, bounds: Rectangle) -> Self::Frame {
        geometry::Frame::new(bounds)
    }

    fn draw_geometry(&mut self, geometry: Self::Geometry) {
        let (layer, transformation) = self.layers.current_mut();

        match geometry {
            Geometry::Live {
                primitives,
                images,
                text,
                clip_bounds,
            } => {
                layer.draw_primitive_group(
                    primitives,
                    clip_bounds,
                    transformation,
                );

                for image in images {
                    layer.draw_image(image, transformation);
                }

                layer.draw_text_group(text, clip_bounds, transformation);
            }
            Geometry::Cache(cache) => {
                layer.draw_primitive_cache(
                    cache.primitives,
                    cache.clip_bounds,
                    transformation,
                );

                for image in cache.images.iter() {
                    layer.draw_image(image.clone(), transformation);
                }

                layer.draw_text_cache(
                    cache.text,
                    cache.clip_bounds,
                    transformation,
                );
            }
        }
    }
}

impl graphics::mesh::Renderer for Renderer {
    fn draw_mesh(&mut self, _mesh: graphics::Mesh) {
        log::warn!("iced_tiny_skia does not support drawing meshes");
    }

    fn draw_mesh_cache(&mut self, _cache: iced_graphics::mesh::Cache) {
        log::warn!("iced_tiny_skia does not support drawing meshes");
    }
}

#[cfg(feature = "image")]
impl core::image::Renderer for Renderer {
    type Handle = core::image::Handle;

    fn load_image(
        &self,
        handle: &Self::Handle,
    ) -> Result<core::image::Allocation, core::image::Error> {
        self.engine.raster_pipeline.load(handle)
    }

    fn measure_image(
        &self,
        handle: &Self::Handle,
    ) -> Option<crate::core::Size<u32>> {
        self.engine.raster_pipeline.dimensions(handle)
    }

    fn draw_image(
        &mut self,
        image: core::Image,
        bounds: Rectangle,
        clip_bounds: Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_raster(image, bounds, clip_bounds, transformation);
    }
}

#[cfg(feature = "svg")]
impl core::svg::Renderer for Renderer {
    fn measure_svg(
        &self,
        handle: &core::svg::Handle,
    ) -> crate::core::Size<u32> {
        self.engine.vector_pipeline.viewport_dimensions(handle)
    }

    fn draw_svg(
        &mut self,
        svg: core::Svg,
        bounds: Rectangle,
        clip_bounds: Rectangle,
    ) {
        let (layer, transformation) = self.layers.current_mut();
        layer.draw_svg(svg, bounds, clip_bounds, transformation);
    }
}

impl compositor::Default for Renderer {
    type Compositor = window::Compositor;
}

impl renderer::Headless for Renderer {
    async fn new(
        default_font: Font,
        default_text_size: Pixels,
        backend: Option<&str>,
    ) -> Option<Self> {
        if backend.is_some_and(|backend| {
            !["tiny-skia", "tiny_skia"].contains(&backend)
        }) {
            return None;
        }

        Some(Self::new(default_font, default_text_size))
    }

    fn name(&self) -> String {
        "tiny-skia".to_owned()
    }

    fn screenshot(
        &mut self,
        size: Size<u32>,
        scale_factor: f32,
        background_color: Color,
    ) -> Vec<u8> {
        let viewport = Viewport::with_physical_size(size, scale_factor);

        window::compositor::screenshot(self, &viewport, background_color)
    }
}
