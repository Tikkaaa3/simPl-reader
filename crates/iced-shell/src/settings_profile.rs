//! Settings damage regression and opt-in production CPU compositor timing.
use super::*;
use iced::advanced::{Layout, Renderer as _, layout, renderer, widget::Tree};
use iced::{Point, Rectangle};
use std::time::Instant;

fn scenario(with_book: bool, narrow: bool) -> Reader {
    let mut reader = Reader {
        show_settings: true,
        ..Default::default()
    };
    if narrow {
        reader.window_size = Size::new(640.0, 480.0);
        reader.appearance = reader_document::preferences::Appearance::Light;
        reader.word_translation.picker = Some(word_translation::Picker::Source);
        reader.word_translation.packages_expanded = true;
        reader.word_translation.inventory = vec![
            reader_document::dictionary::PackageState::Missing;
            reader_document::dictionary::packages().len()
        ];
    }
    if with_book {
        reader.book = Some(super::tests::book("settings-profile"));
        let _ = reader.rebuild_geometry(Anchor {
            row: 0,
            fraction: 0.0,
        });
        settle_pagination(&mut reader);
    }
    reader
}

#[test]
fn settings_scroll_damage_stays_contiguous() {
    ui::load_test_fonts();
    run_case("book", scenario(true, false), 1.0, false);
    run_case("shelf", scenario(false, false), 1.0, false);
    run_case("narrow-picker", scenario(true, true), 1.5, false);
    let mut dark = scenario(false, false);
    dark.appearance = reader_document::preferences::Appearance::Dark;
    run_case("dark-shelf", dark, 1.0, false);
}

#[test]
fn cached_control_glyph_respects_its_scroll_clip() {
    use iced::advanced::text::Renderer as _;
    ui::load_test_fonts();
    let bounds = Rectangle::new(Point::ORIGIN, Size::new(120.0, 100.0));
    let clip = Rectangle {
        height: 50.0,
        ..bounds
    };
    let mut renderer = iced::Renderer::new(ui::SANS, iced::Pixels(20.0));
    renderer.reset(bounds);
    renderer.with_layer(clip, |renderer| {
        renderer.fill_text(
            iced::advanced::Text {
                content: "V".into(),
                bounds: Size::new(20.0, 24.0),
                size: iced::Pixels(20.0),
                line_height: iced::advanced::text::LineHeight::default(),
                font: ui::SANS,
                align_x: iced::advanced::text::Alignment::Default,
                align_y: iced::alignment::Vertical::Top,
                shaping: iced::advanced::text::Shaping::Advanced,
                wrapping: iced::advanced::text::Wrapping::None,
            },
            Point::new(40.0, 44.0),
            iced::Color::BLACK,
            clip,
        );
    });
    let mut pixels = tiny_skia::Pixmap::new(120, 100).unwrap();
    let mut mask = tiny_skia::Mask::new(120, 100).unwrap();
    renderer.draw(
        &mut pixels.as_mut(),
        &mut mask,
        &iced::advanced::graphics::Viewport::with_physical_size(Size::new(120, 100), 1.0),
        &[bounds],
        iced::Color::WHITE,
    );
    assert!(
        pixels.data()[..120 * 50 * 4]
            .chunks_exact(4)
            .any(|pixel| pixel != [255; 4]),
        "control case must paint visible ink"
    );
    assert!(
        pixels.data()[120 * 50 * 4..]
            .chunks_exact(4)
            .all(|pixel| pixel == [255; 4]),
        "cached glyph leaked below its scroll clip"
    );
}

#[test]
#[ignore = "Settings scroll CPU profile: use --release --ignored --nocapture"]
fn profile_settings_scroll() {
    ui::load_test_fonts();
    for (name, with_book, scale, narrow) in [
        ("book-1x", true, 1.0, false),
        ("shelf-1x", false, 1.0, false),
        ("book-1.5x", true, 1.5, false),
        ("book-2x", true, 2.0, false),
        ("narrow-picker-1.5x", true, 1.5, true),
    ] {
        run_case(name, scenario(with_book, narrow), scale, true);
    }
    let mut dark = scenario(true, false);
    dark.appearance = reader_document::preferences::Appearance::Dark;
    run_case("dark-book-1x", dark, 1.0, true);
}

fn run_case(name: &str, reader: Reader, scale: f32, profile: bool) {
    let size = reader.window_size;
    let bounds = Rectangle::with_size(size);
    let theme = ui::theme(reader.appearance, reader.theme);
    let viewport = iced::advanced::graphics::Viewport::with_physical_size(
        Size::new((size.width * scale) as u32, (size.height * scale) as u32),
        scale,
    );
    let mut pixels =
        tiny_skia::Pixmap::new((size.width * scale) as u32, (size.height * scale) as u32).unwrap();
    let mut mask = tiny_skia::Mask::new(pixels.width(), pixels.height()).unwrap();
    let mut renderer = iced::Renderer::new(ui::SANS, iced::Pixels(13.0));
    let mut tree = Tree::new(view(&reader));
    let mut times = Vec::new();
    let mut damage_counts = Vec::new();
    let mut previous_layers = None::<Vec<iced_tiny_skia::Layer>>;
    for frame in 0..if profile { 70 } else { 6 } {
        let started = Instant::now();
        let mut element = view(&reader);
        tree.diff(element.as_widget());
        let node =
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        let built = started.elapsed();
        let step = if size.width < 800.0 { 32.0 } else { 12.0 };
        let offset = if frame < 35 {
            frame as f32 * step
        } else {
            (70 - frame) as f32 * step
        };
        element.as_widget_mut().operate(
            &mut tree,
            Layout::new(&node),
            &renderer,
            &mut iced::advanced::widget::operation::scrollable::scroll_to::<()>(
                iced::advanced::widget::Id::new("workspace-overlay"),
                iced::advanced::widget::operation::scrollable::AbsoluteOffset {
                    x: None,
                    y: Some(offset),
                },
            ),
        );
        let mut messages = Vec::new();
        element.as_widget_mut().update(
            &mut tree,
            &iced::Event::Window(window::Event::RedrawRequested(Instant::now())),
            Layout::new(&node),
            mouse::Cursor::Available(Point::new(800.0, 400.0)),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut messages),
            &bounds,
        );
        renderer.reset(bounds);
        let draw_started = Instant::now();
        element.as_widget().draw(
            &tree,
            &mut renderer,
            &theme,
            &renderer::Style {
                text_color: theme.palette().text,
            },
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &bounds,
        );
        let commands = draw_started.elapsed();
        let raster_started = Instant::now();
        let damage = previous_layers.as_ref().map_or_else(
            || vec![bounds],
            |previous| {
                iced::advanced::graphics::damage::diff(
                    previous,
                    renderer.layers(),
                    |layer| vec![layer.bounds],
                    iced_tiny_skia::Layer::damage,
                )
            },
        );
        previous_layers = Some(renderer.layers().to_vec());
        let damage = iced::advanced::graphics::damage::group(damage, bounds);
        assert!(
            damage.len() <= 1,
            "{name} frame {frame}: fragmented {damage:?}"
        );
        if frame >= 10 {
            damage_counts.push(damage.len());
        }
        if profile {
            renderer.draw(
                &mut pixels.as_mut(),
                &mut mask,
                &viewport,
                &damage,
                theme.palette().background,
            );
        }
        let raster = raster_started.elapsed();
        if frame >= 10 {
            times.push([
                built.as_secs_f64() * 1000.0,
                commands.as_secs_f64() * 1000.0,
                raster.as_secs_f64() * 1000.0,
                started.elapsed().as_secs_f64() * 1000.0,
            ]);
        }
        if profile && matches!(frame, 0 | 20 | 50) {
            let mut reference = tiny_skia::Pixmap::new(pixels.width(), pixels.height()).unwrap();
            renderer.draw(
                &mut reference.as_mut(),
                &mut mask,
                &viewport,
                &[bounds],
                theme.palette().background,
            );
            let differing = pixels
                .data()
                .iter()
                .zip(reference.data())
                .filter(|(a, b)| a != b)
                .count();
            assert_eq!(
                differing, 0,
                "{name} frame {frame}: incremental paint differs from full redraw"
            );
            if frame == 20
                && let Some(output) = std::env::var_os("SIMPL_PREVIEW_OUTPUT")
            {
                let output = PathBuf::from(output);
                std::fs::create_dir_all(&output).unwrap();
                for pixel in reference.data_mut().chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
                ::image::save_buffer(
                    output.join(format!("{name}.png")),
                    reference.data(),
                    reference.width(),
                    reference.height(),
                    ::image::ColorType::Rgba8,
                )
                .unwrap();
            }
        }
    }
    if !profile {
        return;
    }
    damage_counts.sort_unstable();
    println!(
        "{name} damage regions: median={} max={}",
        damage_counts[damage_counts.len() / 2],
        damage_counts.last().unwrap()
    );
    for (column, stage) in ["build/layout", "draw commands", "raster", "total"]
        .iter()
        .enumerate()
    {
        let mut samples: Vec<_> = times.iter().map(|time| time[column]).collect();
        samples.sort_by(f64::total_cmp);
        println!(
            "{name} {stage}: median={:.2} ms p95={:.2} ms",
            samples[samples.len() / 2],
            samples[samples.len() * 95 / 100]
        );
    }
}
