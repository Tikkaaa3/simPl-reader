//! Opt-in reader frame timing per screen state with the production CPU compositor.
use super::*;
use iced::Rectangle;
use iced::advanced::{Layout, Renderer as _, layout, renderer, widget::Tree};
use std::time::Instant;

const WARMUP_FRAMES: usize = 10;
const MEASURED_FRAMES: usize = 120;

/// A named screen state: how to build it, what each frame does before it
/// draws, and where the pointer is.
type State = (
    &'static str,
    fn() -> Reader,
    fn(&mut Reader, usize),
    fn(&Reader) -> mouse::Cursor,
);

const STATES: &[State] = &[
    ("library", library, still, away),
    ("reading-idle", reading, still, away),
    ("reading-page-turn", reading, turn_page, away),
    ("reading-bar-hidden", reading_bar_hidden, still, away),
    ("reading-bar-reveal", reading_bar_hidden, reveal_bar, away),
    (
        "reading-strip-hover",
        reading_strip_hover,
        still,
        over_right_strip,
    ),
    (
        "reading-progress-hover",
        reading_bar_hidden,
        still,
        over_progress_line,
    ),
    ("settings", settings, still, away),
];

fn library() -> Reader {
    use reader_document::recent::DocumentKind;
    let mut reader = Reader {
        window_size: Size::new(1280.0, 800.0),
        ..Reader::default()
    };
    let entries = shelf::sample_entries(&[
        ("Pride and Prejudice", DocumentKind::Epub),
        ("Frankenstein", DocumentKind::Epub),
        ("Dream Analysis", DocumentKind::Pdf),
        ("Let's Go", DocumentKind::Html),
        ("The Picture of Dorian Gray", DocumentKind::Epub),
    ]);
    let _ = reader
        .shelf
        .update(shelf::Message::ShelvesLoaded(Ok(Default::default())));
    let _ = reader.shelf.update(shelf::Message::Loaded(Ok(entries)));
    let _ = reader.shelf.resize(reader.window_size);
    reader
}

fn reading() -> Reader {
    let mut reader = Reader {
        window_size: Size::new(1280.0, 800.0),
        book: Some(super::tests::book("reader-profile")),
        ..Reader::default()
    };
    let _ = reader.rebuild_geometry(Anchor {
        row: 0,
        fraction: 0.0,
    });
    settle_pagination(&mut reader);
    assert!(reader.page_total() >= 2, "page turns need two pages");
    reader
}

fn reading_bar_hidden() -> Reader {
    Reader {
        bar: reading_chrome::Bar::hidden(),
        ..reading()
    }
}

/// The pointer rests on the right strip and its fade has finished.
fn reading_strip_hover() -> Reader {
    let mut reader = reading_bar_hidden();
    reader
        .strips
        .hover(Some(page_strips::Side::Next), Instant::now());
    reader.strips.settle();
    assert_eq!(reader.strips.alpha(page_strips::Side::Next), 1.0);
    reader
}

fn away(_: &Reader) -> mouse::Cursor {
    mouse::Cursor::Unavailable
}

fn over_right_strip(reader: &Reader) -> mouse::Cursor {
    let size = reader.window_size;
    let width = page_strips::width(size.width);
    mouse::Cursor::Available(iced::Point::new(
        size.width - 14.0 - width / 2.0,
        size.height / 2.0,
    ))
}

/// The pointer rests in the progress line's band, under the page centre.
fn over_progress_line(reader: &Reader) -> mouse::Cursor {
    let size = reader.window_size;
    mouse::Cursor::Available(iced::Point::new(size.width / 2.0, size.height - 12.0))
}

fn settings() -> Reader {
    Reader {
        show_settings: true,
        ..reading()
    }
}

fn still(_: &mut Reader, _: usize) {}

/// Every frame draws the next step of the bar's fade-in, restarting every ten frames.
fn reveal_bar(reader: &mut Reader, frame: usize) {
    if frame.is_multiple_of(10) {
        reader.bar = reading_chrome::Bar::hidden();
        reader.bar.hover(Instant::now(), true);
    } else {
        reader.bar.step_by(std::time::Duration::from_millis(15));
    }
    assert!(reader.bar.visible());
}

fn turn_page(reader: &mut Reader, frame: usize) {
    let before = reader.offset;
    let _ = update_inner(reader, Message::BookPage(frame.is_multiple_of(2)));
    assert_ne!(reader.offset, before, "every frame turns a page");
}

#[test]
#[ignore = "Reader frame CPU profile: use --release --ignored --nocapture"]
fn profile_reader_frames() {
    ui::load_test_fonts();
    for &(name, setup, step, cursor) in STATES {
        let (median, p95) = frame_times(setup(), step, cursor);
        println!("reader_frame_ms state={name} median={median:.2} p95={p95:.2}");
    }
}

fn frame_times(
    mut reader: Reader,
    step: fn(&mut Reader, usize),
    cursor: fn(&Reader) -> mouse::Cursor,
) -> (f64, f64) {
    let cursor = cursor(&reader);
    let size = reader.window_size;
    let bounds = Rectangle::with_size(size);
    let theme = ui::theme(reader.appearance, reader.theme);
    let physical = Size::new(size.width as u32, size.height as u32);
    let viewport = iced::advanced::graphics::Viewport::with_physical_size(physical, 1.0);
    let mut pixels = tiny_skia::Pixmap::new(physical.width, physical.height).unwrap();
    let mut mask = tiny_skia::Mask::new(physical.width, physical.height).unwrap();
    let mut renderer = iced::Renderer::new(ui::SANS, iced::Pixels(13.0));
    let mut tree = Tree::empty();
    let mut totals = Vec::with_capacity(MEASURED_FRAMES);
    for frame in 0..WARMUP_FRAMES + MEASURED_FRAMES {
        let started = Instant::now();
        step(&mut reader, frame);
        let mut element = view(&reader);
        tree.diff(element.as_widget());
        let node =
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        use iced::advanced::widget::operation::scrollable::{AbsoluteOffset, scroll_to};
        element.as_widget_mut().operate(
            &mut tree,
            Layout::new(&node),
            &renderer,
            &mut scroll_to::<()>(
                scroll_id(),
                AbsoluteOffset {
                    x: None,
                    y: Some(reader.local_offset() * reader.zoom),
                },
            ),
        );
        let mut messages = Vec::new();
        element.as_widget_mut().update(
            &mut tree,
            &iced::Event::Window(window::Event::RedrawRequested(Instant::now())),
            Layout::new(&node),
            cursor,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut messages),
            &bounds,
        );
        renderer.reset(bounds);
        element.as_widget().draw(
            &tree,
            &mut renderer,
            &theme,
            &renderer::Style {
                text_color: theme.palette().text,
            },
            Layout::new(&node),
            cursor,
            &bounds,
        );
        // A still screen has no damage, and the runtime never redraws one, so
        // every frame repaints the whole window to measure the screen itself.
        renderer.draw(
            &mut pixels.as_mut(),
            &mut mask,
            &viewport,
            &[bounds],
            theme.palette().background,
        );
        if frame >= WARMUP_FRAMES {
            totals.push(started.elapsed().as_secs_f64() * 1000.0);
        }
    }
    totals.sort_by(f64::total_cmp);
    (totals[totals.len() / 2], totals[totals.len() * 95 / 100])
}
