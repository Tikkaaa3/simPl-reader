//! Opt-in visual QA using the real widget tree and tiny-skia renderer, no OS input.
use super::*;
use iced::advanced::{Layout, Renderer as _, layout, renderer, widget::Tree};
use iced::{Point, Rectangle};

#[derive(Default)]
struct PaperViewport {
    bounds: Rectangle,
    content: Rectangle,
    offset: iced::Vector,
}
impl iced::advanced::widget::Operation for PaperViewport {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation)) {
        operate(self);
    }
    fn scrollable(
        &mut self,
        id: Option<&iced::advanced::widget::Id>,
        bounds: Rectangle,
        content: Rectangle,
        offset: iced::Vector,
        _: &mut dyn iced::advanced::widget::operation::scrollable::Scrollable,
    ) {
        if id == Some(&scroll_id()) {
            self.bounds = bounds;
            self.content = content;
            self.offset = offset;
        }
    }
}

fn paper_viewport(reader: &Reader) -> PaperViewport {
    let renderer = iced::Renderer::new(ui::SANS, iced::Pixels(13.0));
    let mut element = view(reader);
    let mut tree = Tree::new(&element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(reader.window_size, reader.window_size),
    );
    let mut probe = PaperViewport::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut probe);
    probe
}

#[derive(Default)]
struct FocusedBounds(Option<Rectangle>);
impl iced::advanced::widget::Operation for FocusedBounds {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation)) {
        visit(self);
    }
    fn container(&mut self, id: Option<&iced::advanced::widget::Id>, bounds: Rectangle) {
        if id == Some(&iced::advanced::widget::Id::new(ui::FOCUSED_CONTROL)) {
            self.0 = Some(bounds);
        }
    }
}

fn focused_bounds(reader: &Reader) -> Option<Rectangle> {
    let renderer = iced::Renderer::new(ui::SANS, iced::Pixels(13.0));
    let mut element = view(reader);
    let mut tree = Tree::new(&element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(reader.window_size, reader.window_size),
    );
    let mut find = FocusedBounds::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut find);
    find.0
}

#[test]
#[ignore = "Single-paper native QA: SIMPL_PAGE_QA_BOOKS JSON list, OUTPUT, isolated STORE"]
fn single_page_reading_and_toolbar() {
    for bytes in ui::font_data() {
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Borrowed(bytes));
    }
    let output = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    let paths: Vec<PathBuf> =
        serde_json::from_str(&std::env::var("SIMPL_PAGE_QA_BOOKS").unwrap()).unwrap();
    for (case, path) in paths.iter().enumerate() {
        let book = if path.extension().is_some_and(|ext| ext == "epub") {
            let document = Arc::new(reader_document::epub::open(path).unwrap());
            Arc::new(load_epub_chapter(document, 0, None).unwrap())
        } else {
            Arc::new(display_book(
                reader_document::load_html(path).unwrap(),
                None,
                None,
            ))
        };
        let mut reader = Reader {
            book: Some(book),
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 0,
            fraction: 0.0,
        });
        settle_pagination(&mut reader);
        assert!(reader.error.is_none(), "{:?}", reader.error);
        let atlas = reader.atlas.clone().unwrap();
        for requested in [3.min(atlas.total), 4.min(atlas.total)] {
            let (section, page) = atlas.target(&requested.to_string()).unwrap();
            if let Some(epub) = reader.book.as_ref().unwrap().epub.as_ref() {
                reader.book = Some(Arc::new(
                    load_epub_chapter(epub.document.clone(), section, None).unwrap(),
                ));
                let _ = reader.rebuild_geometry(Anchor {
                    row: 0,
                    fraction: 0.0,
                });
            }
            let _ = reader.go_to_local_page(page);
            let selected = reader.active_page().unwrap().number;
            for zoom in [0.4, 1.0, 1.8] {
                let _ = update_inner(&mut reader, Message::Zoom(zoom));
                let _ = reader.refine_geometry();
                let before = paper_viewport(&reader);
                let expected = reader.active_page().unwrap().height * zoom;
                assert!(
                    (before.content.height - expected).abs() < 1.0,
                    "{} page {requested}: scroll contains extra paper: {} vs {expected}",
                    path.display(),
                    before.content.height
                );
                reader.bar = reading_chrome::Bar::hidden();
                let after = paper_viewport(&reader);
                assert_eq!(
                    after.bounds, before.bounds,
                    "hiding the floating bar never moves the page"
                );
                assert_eq!(
                    after.bounds.y, 0.0,
                    "the reading area starts at the top, under the bar"
                );
                assert_eq!(reader.active_page().unwrap().number, selected);
                let _ = reader.jump(f32::MAX);
                assert_eq!(
                    reader.active_page().unwrap().number,
                    selected,
                    "scroll must not turn pages"
                );
                let _ = reader.jump(0.0);
                assert_eq!(reader.active_page().unwrap().number, selected);
                reader.bar = Default::default();
            }
            let _ = update_inner(&mut reader, Message::Zoom(0.6));
            render(
                &mut reader,
                &output.join(format!("single-{case}-{requested}-toolbar.png")),
            );
            reader.bar = reading_chrome::Bar::hidden();
            render(
                &mut reader,
                &output.join(format!("single-{case}-{requested}-focus.png")),
            );
            reader.bar = Default::default();
            let old_generation = reader.generation;
            let old_top = reader.active_page().unwrap().top;
            if page + 1 < reader.pages().len() {
                let _ = reader.adjacent_book_page(true);
                assert_eq!(
                    reader.active_page().unwrap().number,
                    reader.pages()[page + 1].number
                );
                let position = reader.offset;
                let _ = update_inner(
                    &mut reader,
                    Message::Scroll {
                        generation: old_generation,
                        page_top: old_top,
                        offset: 9999.0,
                        viewport: 700.0,
                    },
                );
                assert_eq!(
                    reader.offset, position,
                    "old viewport events must not move the new page"
                );
                let _ = reader.adjacent_book_page(false);
                assert_eq!(reader.active_page().unwrap().number, selected);
            }
            assert_eq!(reader.page_total(), atlas.total);
        }
        println!(
            "Single-page/toolbar/zoom/navigation passed: {} ({} pages)",
            path.display(),
            atlas.total
        );
    }
}

#[test]
#[ignore = "Publisher label/theme QA: SIMPL_PREVIEW_PRIDE, OUTPUT, isolated STORE"]
fn publisher_labels_and_brand_themes() {
    for bytes in ui::font_data() {
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Borrowed(bytes));
    }
    let output = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    let epub = Arc::new(
        reader_document::epub::open(&PathBuf::from(
            std::env::var_os("SIMPL_PREVIEW_PRIDE").unwrap(),
        ))
        .unwrap(),
    );
    assert!(epub.page_list.iter().any(|p| p.label == "{45}"));
    let source = epub.page_list.iter().find(|p| p.label == "{45}").unwrap();
    let book = Arc::new(load_epub_chapter(epub.clone(), source.chapter, None).unwrap());
    let mut reader = Reader {
        book: Some(book),
        ..Reader::default()
    };
    let _ = reader.rebuild_geometry(Anchor {
        row: 0,
        fraction: 0.0,
    });
    settle_pagination(&mut reader);
    assert!(reader.error.is_none(), "{:?}", reader.error);
    assert_eq!(reader.page_total(), epub.page_list.len());
    let atlas = reader.atlas.as_ref().unwrap();
    let target = atlas.target("45").unwrap();
    assert_eq!(target.0, source.chapter);
    assert_eq!(atlas.sections[target.0].pages[target.1].label, "45");
    let frozen = serde_json::to_vec(&**atlas).unwrap();
    let _ = reader.go_to_local_page(target.1);
    for (appearance, name) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
        reader.appearance = appearance;
        render(
            &mut reader,
            &output.join(format!("pride-page-45-{name}.png")),
        );
        assert_eq!(
            serde_json::to_vec(&**reader.atlas.as_ref().unwrap()).unwrap(),
            frozen
        );
        let mut library = Reader {
            appearance,
            ..Reader::default()
        };
        render(
            &mut library,
            &output.join(format!("brand-library-{name}.png")),
        );
        if let Some(path) = std::env::var_os("SIMPL_PREVIEW_LIBRARY") {
            let catalog: serde_json::Value =
                serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            let entries: Vec<library::Entry> =
                serde_json::from_value(catalog["entries"].clone()).unwrap();
            let _ = library.shelf.update(shelf::Message::Loaded(Ok(entries)));
            let _ = update_inner(&mut library, Message::Chrome(chrome::Action::Search));
            render(
                &mut library,
                &output.join(format!("quick-switcher-{name}.png")),
            );
        }
    }
    println!(
        "Publisher NCX label {{45}} displays as 45; all {} source pages retained",
        reader.page_total()
    );
}

#[test]
#[ignore = "Global-page native QA: EPUB, HTML, PDF, OUTPUT and isolated LOCALAPPDATA"]
fn global_pages_zoom_and_cross_chapter_jump() {
    for bytes in ui::font_data() {
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Borrowed(bytes));
    }
    let output = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    let epub = Arc::new(
        reader_document::epub::open(&PathBuf::from(
            std::env::var_os("SIMPL_PREVIEW_EPUB").unwrap(),
        ))
        .unwrap(),
    );
    let book = Arc::new(load_epub_chapter(epub.clone(), 6, None).unwrap());
    let mut reader = Reader {
        book: Some(book.clone()),
        ..Reader::default()
    };
    let _ = reader.rebuild_geometry(Anchor {
        row: 0,
        fraction: 0.0,
    });
    settle_pagination(&mut reader);
    assert!(reader.error.is_none(), "{:?}", reader.error);
    let atlas = reader.atlas.clone().unwrap();
    assert!(atlas.total >= 400);
    let frozen = serde_json::to_vec(&*atlas).unwrap();
    let target = atlas.target("400").unwrap();
    assert_ne!(target.0, 6);
    let _ = update_inner(&mut reader, Message::PageInput("400".into()));
    let _ = update_inner(&mut reader, Message::PageSubmit);
    assert!(reader.opening.is_some());
    let request = reader.request;
    let next = Arc::new(load_epub_chapter(epub.clone(), target.0, None).unwrap());
    let _ = update_inner(
        &mut reader,
        Message::Loaded {
            request,
            result: Ok(LoadReply {
                document: LoadedDocument::Reflow(next),
                catalog: None,
            }),
        },
    );
    assert_eq!(reader.pages()[target.1].number, 399);
    assert_eq!(
        book_pages::visible(reader.pages(), reader.offset, reader.viewport)
            .unwrap()
            .label,
        "400"
    );
    assert!(Arc::ptr_eq(&atlas, reader.atlas.as_ref().unwrap()));
    for (zoom, width) in [(1.0, 1280.0), (1.5, 1280.0), (0.6, 540.0)] {
        let _ = update_inner(&mut reader, Message::Zoom(zoom));
        reader.window_size.width = width;
        render(
            &mut reader,
            &output.join(format!("global-epub-400-{zoom}.png")),
        );
        assert_eq!(
            serde_json::to_vec(&**reader.atlas.as_ref().unwrap()).unwrap(),
            frozen
        );
        assert_eq!(reader.page_total(), atlas.total);
    }
    // Reopening reads the persisted map with identical page boundaries.
    assert!(
        book_map::cached(&book, epub.chapters.len()).is_some(),
        "fixed map must exist on disk"
    );
    let reopened = book_map::build(book, &AtomicBool::new(false))
        .unwrap()
        .unwrap();
    assert_eq!(serde_json::to_vec(&reopened).unwrap(), frozen);
    println!(
        "EPUB: {} global pages; page 400 opens spine section {}",
        atlas.total, target.0
    );
    let old = reader.offset;
    let _ = update_inner(&mut reader, Message::PageInput("9999999".into()));
    let _ = update_inner(&mut reader, Message::PageSubmit);
    assert_eq!(reader.offset, old);

    let html = Arc::new(display_book(
        reader_document::load_html(&PathBuf::from(
            std::env::var_os("SIMPL_PREVIEW_HTML").unwrap(),
        ))
        .unwrap(),
        None,
        None,
    ));
    let mut reader = Reader {
        book: Some(html),
        ..Reader::default()
    };
    let _ = reader.rebuild_geometry(Anchor {
        row: 0,
        fraction: 0.0,
    });
    settle_pagination(&mut reader);
    assert!(reader.error.is_none(), "{:?}", reader.error);
    assert!(reader.page_total() >= 400);
    let total = reader.page_total();
    let _ = update_inner(&mut reader, Message::PageInput("400".into()));
    let _ = update_inner(&mut reader, Message::PageSubmit);
    render(&mut reader, &output.join("global-html-400.png"));
    assert_eq!(
        book_pages::visible(reader.pages(), reader.offset, reader.viewport)
            .unwrap()
            .number,
        399
    );
    let _ = update_inner(&mut reader, Message::Zoom(1.5));
    render(&mut reader, &output.join("global-html-400-zoom.png"));
    assert_eq!(reader.page_total(), total);
    println!("HTML: {total} fixed global pages");
}

pub(super) fn render(reader: &mut Reader, output: &Path) {
    render_at_scale(reader, output, 1.0);
}

pub(super) fn render_at_scale(reader: &mut Reader, output: &Path, scale: f32) {
    render_frame(reader, output, scale, mouse::Cursor::Unavailable);
}

/// Renders hover as the runtime does, applying `mouse_area` enter messages
/// before drawing and settling the page strips' fade.
pub(super) fn render_with_cursor(reader: &mut Reader, output: &Path, point: Point) {
    render_frame(reader, output, 1.0, mouse::Cursor::Available(point));
}

fn render_frame(reader: &mut Reader, output: &Path, scale: f32, cursor: mouse::Cursor) {
    assert!(scale.is_finite() && (1.0..=3.0).contains(&scale));
    settle_pagination(reader);
    let _ = reader.request_pdf_book_raster();
    if let Some((document, page)) = reader.pdf_book_pending {
        let source = &reader
            .book
            .as_ref()
            .unwrap()
            .pdf_source
            .as_ref()
            .unwrap()
            .document;
        let result = complete(source.session.render(page, 1600)).map(Arc::new);
        let _ = update_inner(
            reader,
            Message::PdfBookRaster {
                document,
                page,
                result,
            },
        );
    }
    let size = reader.window_size;
    let bounds = Rectangle::with_size(size);
    let theme = ui::theme(reader.appearance, reader.theme);
    let mut renderer = iced::Renderer::new(ui::SANS, iced::Pixels(13.0));
    // Settle native paragraph heights before drawing the final tree.
    for _ in 0..3 {
        let mut element = view(reader);
        let mut tree = Tree::new(&element);
        let _ =
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        drop(element);
        if reader.measured_layout.is_some()
            && !reader
                .book
                .as_ref()
                .is_some_and(|book| book.pdf_source.is_some())
        {
            assert!(
                reader
                    .measurements
                    .lock()
                    .iter()
                    .all(|(_, _, width, generation)| *generation != reader.generation
                        || *width != reader.width),
                "completed pagination must agree with visible native layout"
            );
        }
        let _ = reader.refine_geometry();
    }
    if let Some(page) = reader.active_page() {
        let geometry = paper_viewport(reader);
        assert!(
            (geometry.content.height - page.height * reader.zoom).abs() < 1.0,
            "Book scroll extent must contain exactly one paper"
        );
    }
    let mut tree = Tree::empty();
    let mut hover_rounds = 0;
    let (mut element, node) = loop {
        let offset = reader.local_offset() * reader.zoom;
        let mut element = view(reader);
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
                    y: Some(offset),
                },
            ),
        );
        if let Some(pdf) = &reader.pdf {
            use iced_futures::futures::{StreamExt, executor::block_on};
            use iced_runtime::{Action as RuntimeAction, task::into_stream};
            block_on(async {
                if let Some(mut stream) = into_stream(pdf.restore_scroll()) {
                    while let Some(action) = stream.next().await {
                        if let RuntimeAction::Widget(mut operation) = action {
                            loop {
                                element.as_widget_mut().operate(
                                    &mut tree,
                                    Layout::new(&node),
                                    &renderer,
                                    operation.as_mut(),
                                );
                                match operation.finish() {
                                    iced::advanced::widget::operation::Outcome::Chain(next) => {
                                        operation = next
                                    }
                                    _ => break,
                                }
                            }
                        }
                    }
                }
            });
        }
        if reader.show_settings && reader.focused.is_some() {
            use iced::advanced::widget::{Operation, operation::Outcome};
            let mut operation: Box<dyn Operation<()>> = Box::new(ui::reveal_focus_operation());
            loop {
                element.as_widget_mut().operate(
                    &mut tree,
                    Layout::new(&node),
                    &renderer,
                    operation.as_mut(),
                );
                match operation.finish() {
                    Outcome::Chain(next) => operation = next,
                    _ => break,
                }
            }
        }
        let Some(position) = cursor.position() else {
            break (element, node);
        };
        let mut messages = Vec::new();
        element.as_widget_mut().update(
            &mut tree,
            &iced::Event::Mouse(mouse::Event::CursorMoved { position }),
            Layout::new(&node),
            cursor,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut messages),
            &bounds,
        );
        if messages.is_empty() && !reader.strips.needs_frames() {
            break (element, node);
        }
        drop(element);
        // Hover fades in over a few frames; draw where it settles.
        reader.strips.settle();
        for message in messages {
            let _ = update_inner(reader, message);
        }
        hover_rounds += 1;
        assert!(hover_rounds < 50, "hover messages must settle");
    };
    // A redraw event resolves enabled/hover/focus styles just as the runtime does.
    let mut messages = Vec::new();
    element.as_widget_mut().update(
        &mut tree,
        &iced::Event::Window(window::Event::RedrawRequested(std::time::Instant::now())),
        Layout::new(&node),
        cursor,
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut iced::advanced::Shell::new(&mut messages),
        &bounds,
    );
    renderer.reset(bounds);
    let style = renderer::Style {
        text_color: theme.palette().text,
    };
    element.as_widget().draw(
        &tree,
        &mut renderer,
        &theme,
        &style,
        Layout::new(&node),
        cursor,
        &bounds,
    );
    // Tooltips and menus are overlays, drawn above the tree as the runtime does.
    if let Some(mut overlay) = element.as_widget_mut().overlay(
        &mut tree,
        Layout::new(&node),
        &renderer,
        &bounds,
        iced::Vector::ZERO,
    ) {
        let layout = overlay.as_overlay_mut().layout(&renderer, size);
        renderer.with_layer(bounds, |renderer| {
            overlay
                .as_overlay()
                .draw(renderer, &theme, &style, Layout::new(&layout), cursor);
        });
    }
    let physical = Size::new(
        (size.width * scale).round() as u32,
        (size.height * scale).round() as u32,
    );
    let mut pixels = tiny_skia::Pixmap::new(physical.width, physical.height).unwrap();
    let mut mask = tiny_skia::Mask::new(physical.width, physical.height).unwrap();
    renderer.draw(
        &mut pixels.as_mut(),
        &mut mask,
        &iced::advanced::graphics::Viewport::with_physical_size(physical, scale),
        &[Rectangle::new(Point::ORIGIN, size)],
        theme.palette().background,
    );
    // The native CPU compositor produces BGRA pixels for its window surface.
    for pixel in pixels.data_mut().chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    ::image::save_buffer(
        output,
        pixels.data(),
        physical.width,
        physical.height,
        ::image::ColorType::Rgba8,
    )
    .unwrap();
}

#[test]
fn render_with_cursor_shows_card_hover() {
    ui::load_test_fonts();
    let mut reader = Reader {
        window_size: Size::new(1280.0, 800.0),
        ..Reader::default()
    };
    let entries =
        shelf::sample_entries(&[("Frankenstein", reader_document::recent::DocumentKind::Epub)]);
    let _ = reader
        .shelf
        .update(shelf::Message::ShelvesLoaded(Ok(Default::default())));
    let _ = reader.shelf.update(shelf::Message::Loaded(Ok(entries)));
    let _ = reader.shelf.resize(reader.window_size);
    reader.focused = Some(Control::Shelf(shelf::Control::Favourite(0, false)));
    let favourite = focused_bounds(&reader).expect("a focused card shows its favourite button");
    reader.focused = None;
    let output = std::env::temp_dir().join("simpl-render-with-cursor");
    std::fs::create_dir_all(&output).unwrap();
    render(&mut reader, &output.join("idle.png"));
    render_with_cursor(&mut reader, &output.join("hover.png"), favourite.center());
    // The app-state hover persists without a cursor; only the widget under the cursor changes.
    render(&mut reader, &output.join("cursorless.png"));
    let patch = |name: &str, area: Rectangle| {
        let image = ::image::open(output.join(name)).unwrap().to_rgba8();
        let (x, y) = (area.x as u32, area.y as u32);
        ::image::imageops::crop_imm(&image, x, y, area.width as u32, area.height as u32)
            .to_image()
            .into_raw()
    };
    assert_ne!(
        patch("idle.png", favourite),
        patch("hover.png", favourite),
        "the hovered card must show its favourite button"
    );
    assert_ne!(
        patch("hover.png", favourite),
        patch("cursorless.png", favourite),
        "the favourite button under the cursor must show its hover style"
    );
    let corner = Rectangle::new(Point::ORIGIN, Size::new(40.0, 40.0));
    assert_eq!(
        patch("idle.png", corner),
        patch("hover.png", corner),
        "hover must not repaint the window corner"
    );
}

#[test]
#[ignore = "Annotation visual QA: set SIMPL_PREVIEW_OUTPUT"]
fn render_annotation_views() {
    for bytes in ui::font_data() {
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Borrowed(bytes));
    }
    let output = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/book-structure/structured.html");
    let book = Arc::new(display_book(
        reader_document::load_html(&path).unwrap(),
        None,
        None,
    ));
    let fingerprint = book.fingerprint.clone();
    let mut reader = Reader {
        book: Some(book.clone()),
        ..Reader::default()
    };
    let _ = reader.rebuild_geometry(Anchor {
        row: 0,
        fraction: 0.0,
    });
    settle_pagination(&mut reader);
    let (row, source) = book
        .items
        .iter()
        .enumerate()
        .find_map(|(row, item)| {
            item.text()
                .filter(|text| text.len() > 12)
                .map(|text| (row, text))
        })
        .unwrap();
    let quote = source.get(..12).unwrap().to_owned();
    let mut data = reader_document::annotations::Annotations::new(&fingerprint).unwrap();
    let id = data
        .add_highlight(
            reader_document::annotations::Place::Reflow {
                chapter: None,
                from: reader_document::annotations::ReflowPoint {
                    item_id: book.items[row].id().to_owned(),
                    byte: 0,
                },
                to: reader_document::annotations::ReflowPoint {
                    item_id: book.items[row].id().to_owned(),
                    byte: quote.len(),
                },
            },
            reader_document::annotations::HighlightColor::Yellow,
            "1".into(),
            quote,
        )
        .unwrap();
    data.set_note(id, "A short note for this passage.").unwrap();
    let _ = data
        .add_bookmark(
            reader_document::annotations::BookmarkPlace::Reflow {
                chapter: None,
                item_id: book.items[row].id().to_owned(),
                within: 0.0,
                page_number: reader.active_page().unwrap().number,
            },
            "1".into(),
            "Opening passage".into(),
        )
        .unwrap();
    reader.notes.book = Some(fingerprint);
    reader.notes.data = Some(data);
    reader.refresh_marks();
    reader.context_at(Endpoint {
        item_id: book.items[row].id().to_owned(),
        byte_offset: 4,
    });
    assert_eq!(
        reader.notes.popup.as_ref().map(|popup| popup.target),
        Some(notes::Target::Highlight(id))
    );
    assert_eq!(
        reader.notes.popup.as_ref().unwrap().entries().last(),
        Some(&notes::PopupItem::RemoveHighlight)
    );
    reader.notes.popup = None;
    render(&mut reader, &output.join("annotations-closed.png"));
    reader.notes.list = Some(notes::ListTab::Highlights);
    render(&mut reader, &output.join("annotations-list.png"));
    reader.notes.popup = Some(notes::Popup {
        at: Point::new(500.0, 250.0),
        target: notes::Target::Highlight(id),
        menu: true,
    });
    render(&mut reader, &output.join("annotations-menu.png"));
    reader.notes.popup = None;
    let _ = reader.notes_action(notes::Action::EditNote(id));
    render(&mut reader, &output.join("annotations-editor.png"));
    reader.notes.editor = None;
    let long_note = format!("A long note remains readable.\n{}", "a".repeat(500));
    reader
        .notes
        .data
        .as_mut()
        .unwrap()
        .set_note(id, &long_note)
        .unwrap();
    let _ = reader.notes_action(notes::Action::ToggleNote(id));
    assert_eq!(reader.notes.expanded_note, Some(id));
    render(&mut reader, &output.join("annotations-expanded-note.png"));
    let _ = reader.notes_action(notes::Action::EditNote(id));
    render(
        &mut reader,
        &output.join("annotations-editor-long-note.png"),
    );
    reader.notes.editor = None;
    assert!(reader.page_is_bookmarked());
    let _ = reader.notes_action(notes::Action::ToggleBookmark);
    assert!(!reader.page_is_bookmarked());
}

#[test]
fn wheel_over_the_bar_scrolls_the_page_under_it() {
    ui::load_test_fonts();
    let mut reader = Reader {
        book: Some(super::tests::book("wheel")),
        window_size: Size::new(1280.0, 500.0),
        ..Reader::default()
    };
    let _ = reader.rebuild_geometry(Anchor {
        row: 0,
        fraction: 0.0,
    });
    settle_pagination(&mut reader);
    let top = Point::new(reader.window_size.width / 2.0, 20.0);
    let wheel = |reader: &Reader| wheel_at(reader, top);

    reader.bar = reading_chrome::Bar::hidden();
    let (direct, _, _) = wheel(&reader);
    assert_eq!(
        direct, 60.0,
        "the hidden bar's reveal band lets the wheel reach the page"
    );

    reader.bar = Default::default();
    let (direct, forwarded, messages) = wheel(&reader);
    assert_eq!(direct, 0.0, "the shown bar covers the page");
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::ChromeWheel(_)))
    );
    assert_eq!(
        forwarded, 60.0,
        "the bar forwards one wheel line to the page"
    );
}

/// Drives the real widget tree with synthetic input, keeping widget state
/// between events and applying every published message through `update`.
struct Harness {
    tree: Tree,
    renderer: iced::Renderer,
}

impl Harness {
    fn new() -> Self {
        ui::load_test_fonts();
        Self {
            tree: Tree::empty(),
            renderer: iced::Renderer::new(ui::SANS, iced::Pixels(13.0)),
        }
    }

    fn send(&mut self, reader: &mut Reader, event: iced::Event, at: Point) -> Vec<Message> {
        self.send_all(reader, &[event], at)
    }

    /// Delivers `events` to one widget tree back to back, as input arrives
    /// between two frames, then applies what they published.
    fn send_all(&mut self, reader: &mut Reader, events: &[iced::Event], at: Point) -> Vec<Message> {
        let size = reader.window_size;
        let mut element = view(reader);
        self.tree.diff(element.as_widget());
        let node = element.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(size, size),
        );
        let mut messages = Vec::new();
        for event in events {
            element.as_widget_mut().update(
                &mut self.tree,
                event,
                Layout::new(&node),
                mouse::Cursor::Available(at),
                &self.renderer,
                &mut iced::advanced::clipboard::Null,
                &mut iced::advanced::Shell::new(&mut messages),
                &Rectangle::with_size(size),
            );
        }
        drop(element);
        for message in messages.clone() {
            let _ = update(reader, message);
        }
        messages
    }

    fn press(&mut self, reader: &mut Reader, at: Point) -> Vec<Message> {
        self.send(
            reader,
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            at,
        )
    }

    fn release(&mut self, reader: &mut Reader, at: Point) -> Vec<Message> {
        self.send(
            reader,
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            at,
        )
    }

    /// Two quick presses on the same point.
    fn double_press(&mut self, reader: &mut Reader, at: Point) -> Vec<Message> {
        let press = iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let release = iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        self.send_all(
            reader,
            &[press.clone(), release.clone(), press, release],
            at,
        )
    }

    /// A click as the runtime delivers it: the press reaches the widgets, then
    /// the app's own press handling runs before the release.
    fn click(&mut self, reader: &mut Reader, at: Point) -> Vec<Message> {
        let mut messages = self.press(reader, at);
        let _ = update(
            reader,
            Message::Event(
                iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                window::Id::unique(),
            ),
        );
        messages.extend(self.release(reader, at));
        messages
    }

    fn type_text(&mut self, reader: &mut Reader, value: &str, at: Point) -> Vec<Message> {
        let mut messages = Vec::new();
        for character in value.chars() {
            let key = Key::Character(character.to_string().into());
            messages.extend(self.send(
                reader,
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key: key.clone(),
                    modified_key: key,
                    physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
                    location: keyboard::Location::Standard,
                    modifiers: keyboard::Modifiers::empty(),
                    text: Some(character.to_string().into()),
                    repeat: false,
                }),
                at,
            ));
        }
        messages
    }
}

/// The bounds of the text input with `id`.
fn input_bounds(reader: &Reader, id: iced::advanced::widget::Id) -> Rectangle {
    text_input(reader, id).0
}

/// The text the page field shows.
fn page_field(reader: &Reader) -> String {
    let id = if reader.pdf.is_some() {
        pdf_reader::page_input_id()
    } else {
        iced::advanced::widget::Id::new("book-page")
    };
    text_input(reader, id).1
}

/// The bounds and the visible text of the text input with `id`.
fn text_input(reader: &Reader, id: iced::advanced::widget::Id) -> (Rectangle, String) {
    struct Find(iced::advanced::widget::Id, Option<(Rectangle, String)>);
    impl iced::advanced::widget::Operation for Find {
        fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation)) {
            visit(self);
        }
        fn text_input(
            &mut self,
            id: Option<&iced::advanced::widget::Id>,
            bounds: Rectangle,
            input: &mut dyn iced::advanced::widget::operation::TextInput,
        ) {
            if id == Some(&self.0) {
                self.1 = Some((bounds, input.text().to_owned()));
            }
        }
    }
    let renderer = iced::Renderer::new(ui::SANS, iced::Pixels(13.0));
    let mut element = view(reader);
    let mut tree = Tree::new(&element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(reader.window_size, reader.window_size),
    );
    let mut find = Find(id, None);
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut find);
    find.1.expect("the text input is on screen")
}

fn bounds_of(reader: &mut Reader, control: Control) -> Rectangle {
    let previous = reader.focused.replace(control);
    let bounds = focused_bounds(reader).expect("the control is on screen");
    reader.focused = previous;
    bounds
}

fn reading_bar_book(width: f32) -> Reader {
    let mut reader = Reader {
        book: Some(super::tests::book("bar-input")),
        window_size: Size::new(width, 700.0),
        ..Reader::default()
    };
    let _ = reader.rebuild_geometry(Anchor {
        row: 0,
        fraction: 0.0,
    });
    settle_pagination(&mut reader);
    reader
}

fn bar_test_pdf(width: f32) -> Option<Reader> {
    test_pdf(
        crate::pdf_reader::tests::tall_pdf(),
        Size::new(width, 700.0),
    )
}

/// A PDF reader in a window of `size`, or `None` when PDFium is missing.
fn test_pdf(bytes: Vec<u8>, size: Size) -> Option<Reader> {
    use crate::pdf_reader::tests::complete;
    let dll = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("pdfium.dll");
    if !dll.exists() {
        assert!(
            std::env::var_os("SIMPL_REQUIRE_PDFIUM").is_none(),
            "SIMPL_REQUIRE_PDFIUM is set but pdfium.dll is not beside the test executable"
        );
        println!("SKIPPED the PDF half: pdfium.dll is not beside the test executable");
        return None;
    }
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "simpl-test-pdf-{}-{}.pdf",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, bytes).unwrap();
    let document = complete(reader_pdf::open(path.clone())).unwrap();
    let _ = std::fs::remove_file(path);
    let (pdf, _) = pdf_reader::Reader::new(document, None, size, 1.0);
    Some(Reader {
        pdf: Some(pdf),
        window_size: size,
        ..Reader::default()
    })
}

/// Clicks into a page field, types, waits past the leave delay with the
/// pointer below the bar, then clicks the page and checks the bar hides.
fn assert_typing_holds_the_bar(
    reader: &mut Reader,
    field: iced::advanced::widget::Id,
    focused: fn(&Reader) -> bool,
) {
    use reading_chrome::BarState;
    let mut ui = Harness::new();
    let field = input_bounds(reader, field).center();
    let page = Point::new(reader.window_size.width / 2.0, 400.0);
    ui.click(reader, field);
    assert!(focused(reader), "a click focuses the page field");
    assert!(
        reader.focused.is_none(),
        "the click is not a keyboard focus"
    );
    ui.type_text(reader, "2", field);
    let _ = update(
        reader,
        Message::Event(
            iced::Event::Mouse(mouse::Event::CursorMoved { position: page }),
            window::Id::unique(),
        ),
    );
    reader
        .bar
        .tick(std::time::Instant::now() + std::time::Duration::from_secs(5));
    assert_eq!(
        reader.bar.state(),
        BarState::Shown { hide_at: None },
        "typing in the page field holds the bar"
    );
    ui.click(reader, page);
    assert!(!focused(reader), "a click on the page leaves the field");
    let BarState::Shown { hide_at: Some(at) } = reader.bar.state() else {
        panic!("leaving the field starts the leave delay");
    };
    reader.bar.tick(at);
    assert_eq!(reader.bar.state(), BarState::Hidden);
}

#[test]
fn clicking_into_the_book_page_field_holds_the_bar_while_typing() {
    let mut reader = reading_bar_book(1280.0);
    assert_typing_holds_the_bar(
        &mut reader,
        iced::advanced::widget::Id::new("book-page"),
        |reader| reader.bar_held(),
    );
    assert!(
        reader.page_input.is_none(),
        "leaving the field without Enter drops the draft"
    );
}

#[test]
fn clicking_into_the_pdf_page_field_holds_the_bar_while_typing() {
    let Some(mut reader) = bar_test_pdf(1280.0) else {
        return;
    };
    assert_typing_holds_the_bar(&mut reader, pdf_reader::page_input_id(), |reader| {
        reader
            .pdf
            .as_ref()
            .is_some_and(pdf_reader::Reader::page_focused)
    });
}

fn chrome_actions(messages: &[Message]) -> Vec<chrome::Action> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::Chrome(action) => Some(*action),
            _ => None,
        })
        .collect()
}

#[test]
fn only_the_bar_background_drags_the_window() {
    let mut reader = reading_bar_book(1280.0);
    let mut ui = Harness::new();
    // While a save runs the document's controls are disabled.
    reader.saving = true;
    let disabled = bounds_of(&mut reader, Control::Find).center();
    let field = input_bounds(&reader, iced::advanced::widget::Id::new("book-page"));
    let total = Point::new(field.x + field.width + 10.0, field.center_y());
    for at in [disabled, total] {
        let messages = ui.double_press(&mut reader, at);
        assert!(
            chrome_actions(&messages).is_empty(),
            "a press at {at:?} on a control or label never drags or maximizes"
        );
    }
    reader.saving = false;
    let library = bounds_of(&mut reader, Control::Close);
    let background = Point::new(library.x + library.width + 40.0, library.center_y());
    assert_eq!(
        chrome_actions(&ui.double_press(&mut reader, background)),
        [chrome::Action::Drag, chrome::Action::Maximize],
        "a press on the background drags, and a double click maximizes"
    );

    reader.fullscreen = true;
    let mut messages = ui.double_press(&mut reader, background);
    messages.extend(ui.send(
        &mut reader,
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        background,
    ));
    assert!(
        chrome_actions(&messages).is_empty(),
        "fullscreen offers no drag, maximize or system menu"
    );
}

#[test]
fn a_click_on_a_focused_bar_button_still_activates_it() {
    let mut reader = reading_bar_book(1280.0);
    let mut ui = Harness::new();
    reader.focused = Some(Control::Notes(crate::notes::Focus::ToggleList));
    let notes = focused_bounds(&reader).unwrap().center();
    ui.click(&mut reader, notes);
    assert_eq!(reader.focused, None, "the press takes keyboard focus away");
    assert_eq!(
        reader.notes.list,
        Some(crate::notes::ListTab::default()),
        "the focused Notes button opens the drawer"
    );

    let Some(mut reader) = bar_test_pdf(1280.0) else {
        return;
    };
    let mut ui = Harness::new();
    reader.focused = Some(Control::Menu(reading_chrome::Menu::Aa));
    let aa = focused_bounds(&reader).unwrap().center();
    ui.click(&mut reader, aa);
    assert_eq!(
        reader.menu,
        Some(reading_chrome::Menu::Aa),
        "the focused Aa button opens its panel"
    );
}

#[test]
fn notes_follows_listen_in_the_bar_and_focusing_it_reveals_the_bar() {
    use crate::notes::{Focus, ListTab};
    use reading_chrome::BarState;
    let notes = Control::Notes(Focus::ToggleList);
    let mut reader = reading_bar_book(1000.0);
    let bar: Vec<Control> = reader
        .controls()
        .filter(|control| reading_chrome::is_bar_control(*control))
        .collect();
    assert_eq!(
        bar,
        [
            Control::Close,
            Control::BookPageInput,
            Control::Find,
            Control::ReadAloud,
            notes,
            Control::Menu(reading_chrome::Menu::Aa),
            Control::Chrome(chrome::Action::ToggleAppearance),
            Control::Chrome(chrome::Action::Search),
            Control::Chrome(chrome::Action::Settings),
            Control::Chrome(chrome::Action::Minimize),
            Control::Chrome(chrome::Action::Maximize),
            Control::Chrome(chrome::Action::Close),
        ]
    );
    assert_eq!(
        reader.document_view_size().width,
        1000.0,
        "a closed drawer takes no width"
    );
    let _ = update(
        &mut reader,
        Message::Notes(crate::notes::Action::ToggleList),
    );
    assert_eq!(
        reader.document_view_size().width,
        1000.0,
        "the page does not fit beside the drawer, which lays over it"
    );
    assert_eq!(
        reader
            .controls()
            .filter(|control| *control == notes)
            .count(),
        1,
        "Notes is in the focus order once, in the bar"
    );
    assert!(!reading_chrome::is_bar_control(Control::Notes(Focus::Tab(
        ListTab::Bookmarks
    ))));

    reader.bar.step_by(std::time::Duration::from_secs(3));
    assert_eq!(reader.bar.state(), BarState::Hidden);
    let now = std::time::Instant::now();
    reader.focused = Some(Control::Notes(Focus::Tab(ListTab::Bookmarks)));
    reader.sync_bar(now, reading_chrome::Input::default());
    assert_eq!(
        reader.bar.state(),
        BarState::Hidden,
        "drawer rows do not hold the bar"
    );
    reader.focused = Some(notes);
    reader.sync_bar(now, reading_chrome::Input::default());
    assert_eq!(reader.bar.state(), BarState::Shown { hide_at: None });

    let Some(reader) = bar_test_pdf(1000.0) else {
        return;
    };
    let bar: Vec<Control> = reader
        .controls()
        .filter(|control| reading_chrome::is_bar_control(*control))
        .collect();
    let listen = bar
        .iter()
        .position(|control| *control == Control::ReadAloud);
    assert_eq!(bar.get(listen.unwrap() + 1), Some(&notes));
    assert_eq!(
        bar[..bar.len() - 3],
        [
            Control::Close,
            Control::BookMode,
            Control::PdfPage,
            Control::Find,
            Control::ReadAloud,
            notes,
            Control::Menu(reading_chrome::Menu::Aa),
            Control::Chrome(chrome::Action::ToggleAppearance),
            Control::Chrome(chrome::Action::Search),
            Control::Chrome(chrome::Action::Settings),
        ]
    );
}

#[test]
fn ctrl_wheel_over_the_bar_zooms_and_shift_wheel_scrolls_sideways() {
    let mut reader = reading_bar_book(1280.0);
    let before = reader.zoom;
    let _ = update(
        &mut reader,
        Message::Event(
            iced::Event::Keyboard(keyboard::Event::ModifiersChanged(keyboard::Modifiers::CTRL)),
            window::Id::unique(),
        ),
    );
    let _ = update(
        &mut reader,
        Message::ChromeWheel(mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }),
    );
    assert!(reader.zoom > before, "Ctrl+wheel up over the bar zooms in");
    let zoomed = reader.zoom;
    let _ = update(
        &mut reader,
        Message::ChromeWheel(mouse::ScrollDelta::Pixels { x: 0.0, y: -50.0 }),
    );
    assert!(reader.zoom < zoomed, "a pinch out over the bar zooms out");

    let line = mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 };
    let down = reading_chrome::wheel_offset(line, false);
    assert_eq!((down.x, down.y), (0.0, 60.0));
    let sideways = reading_chrome::wheel_offset(line, true);
    assert_eq!((sideways.x, sideways.y), (60.0, 0.0));
    let pixels = reading_chrome::wheel_offset(mouse::ScrollDelta::Pixels { x: 3.0, y: -7.0 }, true);
    assert_eq!((pixels.x, pixels.y), (-3.0, 7.0));
}

#[test]
fn escape_hides_the_idle_bar_and_still_clears_the_selection() {
    use reading_chrome::BarState;
    let mut reader = reading_bar_book(1280.0);
    // The book opens: the bar shows with its deadline pending.
    reader.sync_bar(std::time::Instant::now(), reading_chrome::Input::default());
    assert!(matches!(
        reader.bar.state(),
        BarState::Shown { hide_at: Some(_) }
    ));
    let book = reader.book.clone().unwrap();
    let first = &book.items[0];
    reader.selection.begin(Endpoint {
        item_id: first.id().to_owned(),
        byte_offset: 0,
    });
    reader.selection.extend(Endpoint {
        item_id: first.id().to_owned(),
        byte_offset: 10,
    });
    reader.selection.end_drag();
    assert!(reader.selection.bounds(&book.items).is_some());
    let _ = update(
        &mut reader,
        Message::Event(
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(key::Named::Escape),
                modified_key: Key::Named(key::Named::Escape),
                physical_key: key::Physical::Code(key::Code::Escape),
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::empty(),
                text: None,
                repeat: false,
            }),
            window::Id::unique(),
        ),
    );
    assert_eq!(reader.bar.state(), BarState::Hidden);
    assert!(
        reader.selection.bounds(&book.items).is_none(),
        "the same Escape clears the selection"
    );
}

#[test]
fn the_notes_sidebar_starts_below_the_bar() {
    for width in [1280.0, 600.0] {
        let mut reader = reading_bar_book(width);
        reader.notes.list = Some(crate::notes::ListTab::Bookmarks);
        let tab = Control::Notes(crate::notes::Focus::Tab(crate::notes::ListTab::Bookmarks));
        let shown = bounds_of(&mut reader, tab);
        assert!(
            shown.y >= reading_chrome::HEIGHT,
            "the bar ({}px at {width}) covers the sidebar tabs at y={}",
            reading_chrome::HEIGHT,
            shown.y
        );
        reader.bar = reading_chrome::Bar::hidden();
        assert_eq!(
            bounds_of(&mut reader, tab).y,
            shown.y,
            "the sidebar never moves"
        );
    }
}

#[test]
fn a_press_on_the_page_closes_the_menu_and_the_wheel_still_scrolls_it() {
    let mut reader = strip_book(1280.0, 700.0);
    let _ = update(&mut reader, Message::ToggleMenu(reading_chrome::Menu::Aa));
    assert!(reader.bar.visible(), "opening the panel shows the bar");
    let page = Point::new(400.0, 400.0);
    let (direct, _, _) = wheel_at(&reader, page);
    assert_eq!(
        direct, 60.0,
        "the wheel beside the open panel scrolls the page"
    );

    let mut ui = Harness::new();
    let card_corner = Point::new(
        1280.0 - 8.0 - reading_panel::WIDTH + 16.0,
        reading_chrome::HEIGHT + 8.0 + 16.0,
    );
    let inside = ui.click(&mut reader, card_corner);
    assert!(
        !inside
            .iter()
            .any(|message| matches!(message, Message::CloseMenu)),
        "a press inside the card keeps it open"
    );
    assert_eq!(reader.menu, Some(reading_chrome::Menu::Aa));
    let outside = ui.click(&mut reader, page);
    assert!(
        outside
            .iter()
            .any(|message| matches!(message, Message::CloseMenu))
    );
    assert_eq!(reader.menu, None, "a press on the page closes the panel");
}

/// A book of long paragraphs over many pages, open on its first page in a
/// `width` x `height` window, with the bar faded out as while reading.
fn strip_book(width: f32, height: f32) -> Reader {
    let mut source = Arc::try_unwrap(super::tests::book("strips")).unwrap();
    let sentence = "A readable paragraph with enough words to wrap over several lines. ";
    source.items = (0..40)
        .map(|index| Item::Paragraph {
            id: format!("paragraph-{index}"),
            text: sentence.repeat(8),
            base_direction: BaseDirection::Ltr,
            style_runs: Vec::new(),
        })
        .collect();
    let mut reader = Reader {
        book: Some(Arc::new(source)),
        window_size: Size::new(width, height),
        ..Reader::default()
    };
    let _ = reader.rebuild_geometry(Anchor {
        row: 0,
        fraction: 0.0,
    });
    settle_pagination(&mut reader);
    hide_bar(&mut reader);
    reader
}

fn hide_bar(reader: &mut Reader) {
    reader.sync_bar(std::time::Instant::now(), reading_chrome::Input::default());
    reader.bar.step_by(std::time::Duration::from_secs(3));
    reader.bar.step_by(std::time::Duration::from_secs(1));
    assert!(!reader.bar.visible(), "the bar has faded out");
}

/// Where the right strip ends: inside the scrollbar rail beside a docked notes
/// drawer, or at an overlaid one's edge.
fn right_strip_end(reader: &Reader) -> f32 {
    let drawers = reader.drawers();
    reader.window_size.width - (drawers.docked().right + 14.0).max(drawers.overlaid().right)
}

/// A point at height `y` in the middle of the right strip.
fn right_strip(reader: &Reader, y: f32) -> Point {
    let right = right_strip_end(reader);
    Point::new(
        right - page_strips::width(reader.window_size.width) / 2.0,
        y,
    )
}

fn left_strip(reader: &Reader, y: f32) -> Point {
    Point::new(page_strips::width(reader.window_size.width) / 2.0, y)
}

fn page_turns(messages: &[Message]) -> Vec<bool> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::BookPage(next) => Some(*next),
            Message::Pdf {
                message: pdf_reader::Message::Next,
                ..
            } => Some(true),
            Message::Pdf {
                message: pdf_reader::Message::Previous,
                ..
            } => Some(false),
            _ => None,
        })
        .collect()
}

/// The cursor the window shows at `at`.
pub(super) fn interaction_at(reader: &Reader, at: Point) -> mouse::Interaction {
    let renderer = iced::Renderer::new(ui::SANS, iced::Pixels(13.0));
    let size = reader.window_size;
    let mut element = view(reader);
    let mut tree = Tree::new(&element);
    let node =
        element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
    element.as_widget().mouse_interaction(
        &tree,
        Layout::new(&node),
        mouse::Cursor::Available(at),
        &Rectangle::with_size(size),
        &renderer,
    )
}

fn app_event(reader: &mut Reader, event: mouse::Event) {
    let _ = update(
        reader,
        Message::Event(iced::Event::Mouse(event), window::Id::unique()),
    );
}

#[test]
fn strip_press_turns_the_page() {
    let mut reader = strip_book(1280.0, 700.0);
    reader.bar.toggle_pin(std::time::Instant::now());
    assert!(reader.page_total() >= 4, "three turns need four pages");
    assert_eq!(page_field(&reader), "1");
    let mut ui = Harness::new();
    for (turn, fraction) in [0.1, 0.5, 0.9].into_iter().enumerate() {
        let at = right_strip(&reader, 700.0 * fraction);
        assert_eq!(interaction_at(&reader, at), mouse::Interaction::Pointer);
        let pressed = ui.press(&mut reader, at);
        assert_eq!(page_turns(&pressed), [true], "one turn per press at {at:?}");
        let page = (turn + 2).to_string();
        assert_eq!(page_field(&reader), page);
        assert!(ui.release(&mut reader, at).is_empty(), "nothing on release");
        app_event(
            &mut reader,
            mouse::Event::ButtonReleased(mouse::Button::Left),
        );
        assert_eq!(page_field(&reader), page);
    }
    let at = left_strip(&reader, 350.0);
    let pressed = ui.press(&mut reader, at);
    assert_eq!(page_turns(&pressed), [false]);
    assert_eq!(page_field(&reader), "3");
}

#[test]
fn strip_press_on_the_first_or_last_page_does_nothing() {
    let mut reader = strip_book(1280.0, 700.0);
    let mut ui = Harness::new();
    let at = left_strip(&reader, 350.0);
    assert_ne!(interaction_at(&reader, at), mouse::Interaction::Pointer);
    let mut messages = ui.send(
        &mut reader,
        iced::Event::Mouse(mouse::Event::CursorMoved { position: at }),
        at,
    );
    messages.extend(ui.click(&mut reader, at));
    assert!(page_turns(&messages).is_empty());
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, Message::StripHover(Some(_)))),
        "no hover on a strip that can't turn"
    );
    reader.bar.toggle_pin(std::time::Instant::now());
    assert_eq!(page_field(&reader), "1");

    let last = reader.pages().len() - 1;
    let _ = reader.go_to_local_page(last);
    assert!(!reader.can_turn(true));
    let label = page_field(&reader);
    let at = right_strip(&reader, 350.0);
    assert_ne!(interaction_at(&reader, at), mouse::Interaction::Pointer);
    assert!(page_turns(&ui.click(&mut reader, at)).is_empty());
    assert_eq!(page_field(&reader), label);
}

#[test]
fn a_resting_pointer_sees_the_strip_again_once_it_can_turn() {
    let mut reader = strip_book(1280.0, 700.0);
    let last = reader.pages().len() - 1;
    let _ = reader.go_to_local_page(last - 1);
    let at = right_strip(&reader, 350.0);
    let mut ui = Harness::new();
    ui.send(
        &mut reader,
        iced::Event::Mouse(mouse::Event::CursorMoved { position: at }),
        at,
    );
    reader.strips.settle();
    assert_eq!(reader.strips.alpha(page_strips::Side::Next), 1.0);
    assert_eq!(page_turns(&ui.click(&mut reader, at)), [true]);
    assert!(!reader.can_turn(true));
    assert_eq!(
        reader.strips.alpha(page_strips::Side::Next),
        0.0,
        "the last page blanks the strip under the pointer"
    );
    let _ = update(&mut reader, Message::BookPage(false));
    assert!(reader.strips.needs_frames(), "it fades back in");
    reader.strips.settle();
    assert_eq!(reader.strips.alpha(page_strips::Side::Next), 1.0);
}

#[test]
fn a_selection_dragged_into_a_strip_keeps_its_page_and_text() {
    let mut reader = strip_book(1280.0, 700.0);
    let mut ui = Harness::new();
    let start = Point::new(560.0, 200.0);
    let further = Point::new(700.0, 260.0);
    let edge = right_strip(&reader, 300.0);
    let mut messages = ui.press(&mut reader, start);
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::SelectStart(_))),
        "the press lands on text"
    );
    app_event(
        &mut reader,
        mouse::Event::ButtonPressed(mouse::Button::Left),
    );
    for at in [further, edge] {
        messages.extend(ui.send(
            &mut reader,
            iced::Event::Mouse(mouse::Event::CursorMoved { position: at }),
            at,
        ));
        app_event(&mut reader, mouse::Event::CursorMoved { position: at });
    }
    messages.extend(ui.release(&mut reader, edge));
    app_event(
        &mut reader,
        mouse::Event::ButtonReleased(mouse::Button::Left),
    );
    assert!(page_turns(&messages).is_empty());
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, Message::StripHover(Some(_)))),
        "the strip stays blank during the drag"
    );
    let book = reader.book.clone().unwrap();
    let copied = reader.selection.copy_text(&book.items).unwrap_or_default();
    assert!(copied.len() > 10, "selected {copied:?}");
    reader.bar.toggle_pin(std::time::Instant::now());
    assert_eq!(page_field(&reader), "1");
}

/// Turns the wheel one line down at `at` and applies any forwarded scroll;
/// returns the page offset before forwarding, after it, and the messages.
fn wheel_at(reader: &Reader, at: Point) -> (f32, f32, Vec<Message>) {
    use iced_futures::futures::{StreamExt, executor::block_on};
    use iced_runtime::{Action as RuntimeAction, task::into_stream};
    let size = reader.window_size;
    let bounds = Rectangle::with_size(size);
    let renderer = iced::Renderer::new(ui::SANS, iced::Pixels(13.0));
    let mut element = view(reader);
    let mut tree = Tree::new(&element);
    let node =
        element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
    let mut messages = Vec::new();
    for event in [
        mouse::Event::CursorMoved { position: at },
        mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 },
        },
    ] {
        element.as_widget_mut().update(
            &mut tree,
            &iced::Event::Mouse(event),
            Layout::new(&node),
            mouse::Cursor::Available(at),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut iced::advanced::Shell::new(&mut messages),
            &bounds,
        );
    }
    let mut probe = PaperViewport::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut probe);
    let direct = probe.offset.y;
    for message in &messages {
        let Message::ChromeWheel(delta) = message else {
            continue;
        };
        block_on(async {
            let mut stream = into_stream(reading_chrome::forward_wheel(
                reader,
                *delta,
                keyboard::Modifiers::default(),
            ))
            .expect("a forwarded scroll");
            while let Some(action) = stream.next().await {
                if let RuntimeAction::Widget(mut operation) = action {
                    element.as_widget_mut().operate(
                        &mut tree,
                        Layout::new(&node),
                        &renderer,
                        operation.as_mut(),
                    );
                }
            }
        });
    }
    let mut probe = PaperViewport::default();
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut probe);
    (direct, probe.offset.y, messages)
}

#[test]
fn strip_wheel_scrolls_the_page() {
    let reader = strip_book(1280.0, 500.0);
    let (direct, forwarded, messages) = wheel_at(&reader, right_strip(&reader, 250.0));
    assert_eq!(direct, 0.0, "the strip takes the wheel from the page");
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::ChromeWheel(_)))
    );
    let line = reading_chrome::wheel_offset(mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 }, false);
    assert_eq!(forwarded, line.y, "the page moves as far as for the bar");
    assert_eq!(forwarded, 60.0);
    assert!(page_turns(&messages).is_empty());

    let (direct, _, _) = wheel_at(&reader, Point::new(640.0, 250.0));
    assert_eq!(direct, 60.0, "the page itself scrolls the same distance");
}

#[test]
fn ctrl_wheel_over_a_strip_zooms_like_over_the_bar() {
    let wheel = mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 };
    let zoom_at = |pinned: bool, at: fn(&Reader) -> Point| {
        let mut reader = strip_book(1280.0, 700.0);
        if pinned {
            reader.bar.toggle_pin(std::time::Instant::now());
        }
        let _ = update(
            &mut reader,
            Message::Event(
                iced::Event::Keyboard(keyboard::Event::ModifiersChanged(keyboard::Modifiers::CTRL)),
                window::Id::unique(),
            ),
        );
        let at = at(&reader);
        let mut ui = Harness::new();
        ui.send(
            &mut reader,
            iced::Event::Mouse(mouse::Event::WheelScrolled { delta: wheel }),
            at,
        );
        reader.zoom
    };
    let strip = zoom_at(false, |reader| right_strip(reader, 350.0));
    let bar = zoom_at(true, |reader| {
        Point::new(reader.window_size.width / 2.0, 20.0)
    });
    assert!(
        strip > 1.0,
        "Ctrl+wheel up over a strip zooms in to {strip}"
    );
    assert_eq!(strip, bar);
}

#[test]
fn a_strip_that_cant_turn_draws_nothing_on_hover() {
    let directory = std::env::temp_dir().join(format!("simpl-strips-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let pixels = |path: &Path| ::image::open(path).unwrap().to_rgba8().into_raw();
    let mut reader = strip_book(1000.0, 600.0);
    let idle = directory.join("idle.png");
    render(&mut reader, &idle);
    let blank = directory.join("previous.png");
    let at = left_strip(&reader, 300.0);
    render_with_cursor(&mut reader, &blank, at);
    assert!(
        pixels(&idle) == pixels(&blank),
        "the first page's left strip stays blank"
    );
    let hovered = directory.join("next.png");
    let at = right_strip(&reader, 300.0);
    render_with_cursor(&mut reader, &hovered, at);
    assert_eq!(reader.strips.alpha(page_strips::Side::Next), 1.0);
    let red = |path: &Path, x: f32| {
        ::image::open(path)
            .unwrap()
            .to_rgba8()
            .get_pixel(x as u32, 100)[0]
    };
    let darkened = |x| i32::from(red(&idle, x)) - i32::from(red(&hovered, x));
    let outer = 1000.0 - 14.0 - 2.0;
    let inner = outer - page_strips::width(1000.0) + 4.0;
    let (outer, inner) = (darkened(outer), darkened(inner));
    assert!(
        outer >= 20,
        "the shade is darkest at the window edge: {outer}"
    );
    assert!(
        (5..=outer - 8).contains(&inner),
        "and lighter but still visible at the strip's inner edge, so the band          reads from top to bottom: {inner} against {outer}"
    );
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn the_right_strip_ends_at_the_notes_drawer_inside_the_rail() {
    let mut reader = strip_book(1200.0, 700.0);
    let _ = update(
        &mut reader,
        Message::Notes(crate::notes::Action::ToggleList),
    );
    assert_eq!(reader.document_view_size().width, 860.0);
    let strip_right = 1200.0 - 340.0 - 14.0;
    let strip_left = strip_right - page_strips::width(1200.0);
    assert_eq!(strip_left, 762.0);
    let mut ui = Harness::new();
    for (x, turns) in [
        (strip_left - 1.0, false),
        (strip_left + 1.0, true),
        (strip_right - 1.0, true),
        (strip_right + 1.0, false),
        (1200.0 - 340.0 + 10.0, false),
    ] {
        let messages = ui.click(&mut reader, Point::new(x, 350.0));
        app_event(
            &mut reader,
            mouse::Event::ButtonReleased(mouse::Button::Left),
        );
        assert_eq!(
            page_turns(&messages).len(),
            usize::from(turns),
            "a click at x={x}"
        );
    }
}

#[test]
fn the_left_strip_turns_back_beside_the_contents_drawer() {
    for window in [1400.0, 860.0] {
        let mut reader = full_bar_book(window, 700.0);
        let _ = reader.go_to_local_page(2);
        let _ = update(&mut reader, Message::ToggleContents);
        assert!(reader.show_contents);
        let drawer = drawers::width(window);
        let mut ui = Harness::new();
        let inside = ui.click(&mut reader, Point::new(drawer - 10.0, 350.0));
        assert!(
            page_turns(&inside).is_empty(),
            "a click in the drawer at {window} stays there"
        );
        let beside = ui.click(&mut reader, Point::new(drawer + 10.0, 350.0));
        assert_eq!(
            page_turns(&beside),
            [false],
            "a click just right of the drawer at {window} turns back"
        );
        assert_eq!(page_field(&reader), "2");
    }
}

fn toggle_notes(reader: &mut Reader) {
    let _ = update(reader, Message::Notes(crate::notes::Action::ToggleList));
}

fn resize(reader: &mut Reader, width: f32) {
    let id = *reader.window.get_or_insert_with(window::Id::unique);
    let height = reader.window_size.height;
    let _ = update(
        reader,
        Message::Event(
            iced::Event::Window(window::Event::Resized(Size::new(width, height))),
            id,
        ),
    );
}

#[test]
fn a_window_without_room_for_both_drawers_keeps_one() {
    // Both drawers leave the page 766px from 1446px, what it needs at zoom 1.
    for (width, both) in [(1446.0, true), (1445.0, false), (860.0, false)] {
        let mut reader = full_bar_book(width, 600.0);
        let _ = update(&mut reader, Message::ToggleContents);
        toggle_notes(&mut reader);
        assert!(reader.notes.list.is_some(), "at {width} notes open");
        assert_eq!(
            reader.show_contents, both,
            "at {width} contents stays open beside notes"
        );
        if !both {
            let _ = update(&mut reader, Message::ToggleContents);
            assert!(reader.show_contents, "at {width} contents opens again");
            assert!(
                reader.notes.list.is_none(),
                "at {width} and contents closes notes"
            );
        }
    }
    let mut reader = full_bar_book(1600.0, 600.0);
    let _ = update(&mut reader, Message::ToggleContents);
    toggle_notes(&mut reader);
    assert!(reader.show_contents && reader.notes.list.is_some());
    reader.focused = Some(Control::ContentsEntry(0));
    resize(&mut reader, 1500.0);
    assert_eq!(
        reader.focused,
        Some(Control::ContentsEntry(0)),
        "a resize that keeps both drawers keeps focus in contents"
    );
    resize(&mut reader, 1400.0);
    assert_eq!(reader.focused, None, "closing contents drops focus from it");
    assert!(
        !reader.show_contents && reader.notes.list.is_some(),
        "shrinking to 1400 keeps the notes drawer"
    );
    assert_eq!(
        reader.drawers().docked().right,
        340.0,
        "and docks it beside the page"
    );
}

#[test]
fn a_drawer_docks_beside_a_page_fitted_to_its_width_and_overlays_a_wider_page() {
    let mut reader = full_bar_book(1000.0, 600.0);
    toggle_notes(&mut reader);
    assert_eq!(
        (
            reader.drawers().overlaid().right,
            reader.document_view_size().width
        ),
        (340.0, 1000.0),
        "the 766px page does not fit in the 660px beside the drawer"
    );
    let _ = update(&mut reader, Message::FitBookWidth);
    assert_eq!(
        (
            reader.drawers().docked().right,
            reader.document_view_size().width
        ),
        (340.0, 660.0),
        "fit width shrinks the page to the room beside the drawer"
    );
    assert!(!scrolls_sideways(&reader));
}

#[test]
fn zooming_in_lays_a_docked_drawer_over_the_page_and_closes_contents_beside_notes() {
    let mut reader = full_bar_book(1200.0, 600.0);
    toggle_notes(&mut reader);
    assert_eq!(reader.document_view_size().width, 860.0);
    let _ = update(&mut reader, Message::Zoom(1.2));
    assert_eq!(
        (
            reader.drawers().overlaid().right,
            reader.document_view_size().width
        ),
        (340.0, 1200.0),
        "the zoomed page no longer fits beside the drawer"
    );
    let _ = update(&mut reader, Message::Zoom(1.0));
    assert_eq!(
        reader.drawers().docked().right,
        340.0,
        "zooming out docks it again"
    );

    let mut reader = full_bar_book(1600.0, 600.0);
    let _ = update(&mut reader, Message::ToggleContents);
    toggle_notes(&mut reader);
    assert!(reader.show_contents && reader.notes.list.is_some());
    let _ = update(&mut reader, Message::Zoom(1.3));
    assert!(
        !reader.show_contents && reader.notes.list.is_some(),
        "zooming in past the room between both drawers keeps notes"
    );
}

#[test]
fn a_pdf_notes_drawer_docks_at_fit_width_and_overlays_a_page_zoomed_past_it() {
    let Some(mut reader) = bar_test_pdf(1000.0) else {
        return;
    };
    let document = reader.pdf.as_ref().unwrap().document().id;
    let pdf = |reader: &mut Reader, message: pdf_reader::Message| {
        let _ = update(reader, Message::Pdf { document, message });
    };
    toggle_notes(&mut reader);
    assert_eq!(
        (
            reader.drawers().docked().right,
            reader.document_view_size().width
        ),
        (340.0, 660.0),
        "fit width docks the drawer"
    );
    let fitted = reader.pdf.as_ref().unwrap().zoom_percent();
    assert!(
        (fitted - 144.0).abs() < 0.01,
        "the 300pt page fits the 628px beside the drawer at 144%, got {fitted}"
    );
    // A 300pt page at 177% is 748px with its margins: wider than the 628px
    // beside the drawer, narrower than the 968px of the whole window.
    pdf(&mut reader, pdf_reader::Message::ActualSize);
    pdf(&mut reader, pdf_reader::Message::ZoomBy(6.0));
    assert_eq!(
        (
            reader.drawers().overlaid().right,
            reader.document_view_size().width
        ),
        (340.0, 1000.0),
        "the zoomed page overlays the drawer"
    );
    assert!(
        !reader.pdf.as_ref().unwrap().scrolls_sideways(),
        "the PDF lays out again across the whole window"
    );
}

#[test]
fn floating_controls_stay_usable_beside_a_drawer() {
    // At 900 fit width docks a drawer beside a 576px page; 540 lays one over
    // the page.
    for (width, contents) in [(900.0, true), (900.0, false), (540.0, true), (540.0, false)] {
        let mut reader = full_bar_book(width, 600.0);
        let _ = update(&mut reader, Message::FitBookWidth);
        if contents {
            let _ = update(&mut reader, Message::ToggleContents);
        } else {
            let _ = update(
                &mut reader,
                Message::Notes(crate::notes::Action::ToggleList),
            );
        }
        let _ = update(&mut reader, Message::FindOpen);
        reader.begin_silent_listening();
        let drawer = drawers::width(width);
        let (left, right) = if contents {
            (drawer, width)
        } else {
            (0.0, width - drawer)
        };
        let input = bounds_of(&mut reader, Control::FindInput);
        assert!(
            input.width >= 100.0,
            "at {width} the find input keeps room to type, got {input:?}"
        );
        for control in [
            Control::FindPrevious,
            Control::FindNext,
            Control::FindClose,
            Control::ReadAloudSlower,
            Control::ReadAloudFaster,
            Control::ReadAloudPause,
        ] {
            let button = bounds_of(&mut reader, control);
            assert!(
                left <= button.x && button.x + button.width <= right && button.width >= 24.0,
                "at {width} {control:?} at {button:?} shows whole between the drawers {left} and {right}"
            );
        }
    }
}

/// A listening book whose bar has idled away, and the middle of its Pause
/// button while the pill showed.
fn listening_with_hidden_bar() -> (Reader, Point) {
    let mut reader = full_bar_book(1400.0, 600.0);
    reader.begin_silent_listening();
    let pause = bounds_of(&mut reader, Control::ReadAloudPause);
    hide_bar(&mut reader);
    (reader, pause.center())
}

fn paused(reader: &Reader) -> bool {
    reader.read_aloud.session.as_ref().unwrap().paused
}

fn move_pointer(reader: &mut Reader, to: Point) {
    app_event(reader, mouse::Event::CursorMoved { position: to });
}

#[test]
fn the_read_aloud_pill_hides_and_shows_with_the_bar() {
    let (mut reader, pause) = listening_with_hidden_bar();
    let mut ui = Harness::new();
    let messages = ui.click(&mut reader, pause);
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, Message::ReadAloud(_))),
        "a click where the hidden pill was reaches the page: {messages:?}"
    );
    assert!(!paused(&reader));

    let _ = update(&mut reader, Message::TogglePinnedBar);
    assert!(reader.bar.visible());
    ui.click(&mut reader, pause);
    assert!(paused(&reader), "the pill shows with the pinned bar");

    let _ = update(&mut reader, Message::TogglePinnedBar);
    hide_bar(&mut reader);
    let _ = update(
        &mut reader,
        Message::Event(
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Character("U".into()),
                modified_key: Key::Character("U".into()),
                physical_key: key::Physical::Code(key::Code::KeyU),
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::CTRL | keyboard::Modifiers::SHIFT,
                text: None,
                repeat: false,
            }),
            window::Id::unique(),
        ),
    );
    assert!(
        !reader.read_aloud.active(),
        "Ctrl+Shift+U still stops reading while the pill is hidden"
    );
}

#[test]
fn the_pointer_where_the_pill_sits_reveals_the_bar_and_holds_it() {
    let (mut reader, pause) = listening_with_hidden_bar();
    let _ = update(&mut reader, Message::ReadAloud(read_aloud::Action::Toggle));
    assert!(!reader.read_aloud.active());
    move_pointer(&mut reader, pause);
    assert!(
        !reader.bar.visible(),
        "without the pill the bottom of the page reveals nothing"
    );

    let (mut reader, pause) = listening_with_hidden_bar();
    move_pointer(&mut reader, pause);
    assert_eq!(
        reader.bar.state(),
        reading_chrome::BarState::Shown { hide_at: None }
    );
    reader.bar.step_by(std::time::Duration::from_secs(5));
    assert!(
        reader.bar.visible(),
        "the pointer on the pill holds the bar"
    );
    let mut ui = Harness::new();
    ui.click(&mut reader, pause);
    assert!(paused(&reader), "and Pause takes the click");

    move_pointer(&mut reader, Point::new(700.0, 300.0));
    reader.bar.step_by(std::time::Duration::from_secs(2));
    reader.bar.step_by(std::time::Duration::from_secs(1));
    assert!(
        !reader.bar.visible(),
        "leaving the pill lets the bar idle away"
    );
}

#[test]
fn focus_on_the_pill_reveals_the_bar() {
    let (mut reader, _) = listening_with_hidden_bar();
    reader.focused = Some(Control::ReadAloudPause);
    reader.sync_bar(std::time::Instant::now(), reading_chrome::Input::default());
    assert_eq!(
        reader.bar.state(),
        reading_chrome::BarState::Shown { hide_at: None }
    );
    reader.bar.step_by(std::time::Duration::from_secs(5));
    assert!(reader.bar.visible(), "focus on Pause holds the bar");
    assert!(
        focused_bounds(&reader).is_some(),
        "and the focused Pause shows"
    );
}

#[test]
fn stopping_reading_releases_the_bar_held_by_the_pill() {
    let (mut reader, pause) = listening_with_hidden_bar();
    reader.focused = Some(Control::ReadAloudPause);
    let _ = update(&mut reader, Message::ReadAloud(read_aloud::Action::Toggle));
    assert!(!reader.read_aloud.active());
    reader.bar.step_by(std::time::Duration::from_secs(3));
    reader.bar.step_by(std::time::Duration::from_secs(1));
    assert!(
        !reader.bar.visible(),
        "focus left on the gone Pause does not hold the bar"
    );

    let (mut reader, pause_point) = (listening_with_hidden_bar().0, pause);
    move_pointer(&mut reader, pause_point);
    assert!(
        reader.bar.visible(),
        "the pointer on the pill shows the bar"
    );
    let _ = update(&mut reader, Message::ReadAloud(read_aloud::Action::Toggle));
    reader.bar.step_by(std::time::Duration::from_secs(3));
    reader.bar.step_by(std::time::Duration::from_secs(1));
    assert!(
        !reader.bar.visible(),
        "a resting pointer where the pill was does not hold the bar"
    );
}

#[test]
fn starting_to_listen_shows_the_bar_and_the_pill_briefly() {
    let mut reader = full_bar_book(1400.0, 600.0);
    hide_bar(&mut reader);
    reader.begin_silent_listening();
    reader.sync_bar(std::time::Instant::now(), reading_chrome::Input::default());
    assert!(reader.bar.visible(), "listening shows the bar");
    assert!(
        bounds_of(&mut reader, Control::ReadAloudPause).width > 0.0,
        "and the pill with it"
    );
    reader.bar.step_by(std::time::Duration::from_secs(3));
    reader.bar.step_by(std::time::Duration::from_secs(1));
    assert!(!reader.bar.visible(), "then both idle away");
    reader.sync_bar(std::time::Instant::now(), reading_chrome::Input::default());
    assert!(
        !reader.bar.visible(),
        "while reading goes on, the bar stays hidden"
    );
}

#[test]
fn strips_reach_the_bottom_unless_the_page_scrolls_sideways() {
    let mut reader = strip_book(1000.0, 600.0);
    // Below the window's own resize border, which takes the last few pixels.
    let bottom = 600.0 - 10.0;
    assert!(!scrolls_sideways(&reader));
    assert_eq!(
        interaction_at(&reader, right_strip(&reader, bottom)),
        mouse::Interaction::Pointer,
        "the strip runs to the bottom edge"
    );
    reader.zoom = 3.0;
    assert!(scrolls_sideways(&reader));
    assert_ne!(
        interaction_at(&reader, right_strip(&reader, bottom)),
        mouse::Interaction::Pointer,
        "the horizontal scrollbar keeps its strip of the bottom"
    );
    assert_eq!(
        interaction_at(&reader, right_strip(&reader, 600.0 - 15.0)),
        mouse::Interaction::Pointer,
        "and the strip stops just above it"
    );
}

#[test]
fn a_pdf_strip_turns_the_pdf_page_above_the_status_line() {
    let Some(mut reader) = test_pdf(
        crate::pdf_reader::tests::pages_pdf(3),
        Size::new(1000.0, 700.0),
    ) else {
        return;
    };
    let mut ui = Harness::new();
    let page = |reader: &Reader| reader.pdf.as_ref().unwrap().page_index();
    let bottom = 700.0 - pdf_reader::STATUS_HEIGHT;
    let at = right_strip(&reader, bottom - 1.0);
    let messages = ui.click(&mut reader, at);
    assert_eq!(page_turns(&messages), [true]);
    assert_eq!(page(&reader), 1);
    assert_eq!(page_field(&reader), "2");
    let at = right_strip(&reader, bottom + 1.0);
    let messages = ui.click(&mut reader, at);
    assert!(
        page_turns(&messages).is_empty(),
        "the status line is not a strip"
    );
    let at = right_strip(&reader, 100.0);
    let messages = ui.click(&mut reader, at);
    assert_eq!(page_turns(&messages), [true]);
    assert_eq!(page(&reader), 2);
    let at = right_strip(&reader, 350.0);
    assert_ne!(
        interaction_at(&reader, at),
        mouse::Interaction::Pointer,
        "the last page has no next strip"
    );
    let at = left_strip(&reader, 350.0);
    let messages = ui.click(&mut reader, at);
    assert_eq!(page_turns(&messages), [false]);
    assert_eq!(page(&reader), 1);
}

/// A point `fraction` of the way along the progress line, in its hover band.
fn on_the_line(reader: &Reader, fraction: f32) -> Point {
    let bottom_inset = page_strips::bottom_inset(reader);
    let window = reader.window_size;
    let strip = page_strips::width(window.width);
    let right = right_strip_end(reader) - strip;
    Point::new(
        strip + (right - strip) * fraction,
        window.height - bottom_inset - 12.0,
    )
}

fn progress_jumps(messages: &[Message]) -> Vec<u32> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::ProgressJump(page) => Some(*page),
            _ => None,
        })
        .collect()
}

#[test]
fn progress_jump_lands_on_the_page() {
    let mut reader = strip_book(1280.0, 700.0);
    reader.bar.toggle_pin(std::time::Instant::now());
    assert_eq!(reader.page_total(), 14);
    let _ = update(&mut reader, Message::ProgressJump(7));
    assert_eq!(page_field(&reader), "7");
    assert_eq!(reader.active_page().unwrap().number + 1, 7);
    assert_eq!(reader.progress().unwrap().label(), "Page 7 of 14 · 43%");
}

#[test]
fn a_press_on_the_progress_line_jumps_to_its_page() {
    let mut reader = strip_book(1280.0, 700.0);
    reader.bar.toggle_pin(std::time::Instant::now());
    let mut ui = Harness::new();
    let middle = on_the_line(&reader, 0.5);
    assert_eq!(interaction_at(&reader, middle), mouse::Interaction::Pointer);
    let messages = ui.click(&mut reader, middle);
    assert_eq!(progress_jumps(&messages), [7]);
    assert_eq!(page_field(&reader), "7");
    let end = on_the_line(&reader, 1.0) - iced::Vector::new(1.0, 0.0);
    let _ = ui.click(&mut reader, end);
    assert_eq!(page_field(&reader), "14");
    let above = middle - iced::Vector::new(0.0, 24.0);
    assert!(progress_jumps(&ui.click(&mut reader, above)).is_empty());
    assert_eq!(page_field(&reader), "14", "the band is 24px tall");
}

#[test]
fn a_pdf_progress_line_jumps_above_the_status_line() {
    let Some(mut reader) = test_pdf(
        crate::pdf_reader::tests::pages_pdf(3),
        Size::new(1000.0, 700.0),
    ) else {
        return;
    };
    let mut ui = Harness::new();
    let page = |reader: &Reader| reader.pdf.as_ref().unwrap().page_index();
    let at = on_the_line(&reader, 0.9);
    let messages = ui.click(&mut reader, at);
    assert_eq!(progress_jumps(&messages), [3]);
    assert_eq!(page(&reader), 2);
    assert_eq!(page_field(&reader), "3");
    let status = Point::new(at.x, 700.0 - pdf_reader::STATUS_HEIGHT / 2.0);
    assert!(progress_jumps(&ui.click(&mut reader, status)).is_empty());
}

#[test]
#[ignore = "UX-3 review: writes a hovered strip in both themes to target/ui-ux-review"]
fn render_strip_look() {
    ui::load_test_fonts();
    let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-ux-review");
    std::fs::create_dir_all(&output).unwrap();
    for (appearance, name) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
        let mut reader = strip_book(1400.0, 900.0);
        reader.appearance = appearance;
        let at = right_strip(&reader, 450.0);
        render_with_cursor(
            &mut reader,
            &output.join(format!("UX-3-look-{name}.png")),
            at,
        );
        assert_eq!(reader.strips.alpha(page_strips::Side::Next), 1.0);
    }
}

#[test]
#[ignore = "UX-4 review: writes the progress line at rest and hovered to target/ui-ux-review"]
fn render_progress_line_look() {
    ui::load_test_fonts();
    let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-ux-review");
    std::fs::create_dir_all(&output).unwrap();
    for (appearance, name) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
        for (width, height) in [(1400.0, 900.0), (540.0, 360.0)] {
            let mut reader = strip_book(width, height);
            reader.appearance = appearance;
            let _ = update(&mut reader, Message::ProgressJump(4));
            let file = |state: &str| output.join(format!("UX-4-line-{name}-{width}-{state}.png"));
            render(&mut reader, &file("rest"));
            let at = on_the_line(&reader, 0.5);
            render_with_cursor(&mut reader, &file("hover"), at);
        }
    }
}

#[test]
fn the_library_banner_shows_while_reading_without_holding_the_bar() {
    let mut reader = reading_bar_book(1280.0);
    reader.library.notice = Some("Your library moved.".into());
    assert!(!reader.bar_held());
    let dismiss = bounds_of(&mut reader, Control::Library(library_ui::Focus::Dismiss));
    assert!(
        dismiss.y >= reading_chrome::HEIGHT,
        "the banner hangs below the bar, at y={}",
        dismiss.y
    );
    let mut ui = Harness::new();
    ui.click(&mut reader, dismiss.center());
    assert_eq!(reader.library.notice, None, "Dismiss clears the notice");

    // A library on a disconnected drive keeps its banner up indefinitely; the
    // bar still hides, and the banner stays usable without it.
    reader.library.location = Some(Err("The library drive is not connected.".into()));
    reader.library.notice = Some("Your library moved.".into());
    assert!(!reader.bar_held());
    hide_bar(&mut reader);
    let retry = bounds_of(
        &mut reader,
        Control::Library(library_ui::Focus::BannerRetry),
    );
    assert!(
        retry.y >= reading_chrome::REVEAL_ZONE,
        "Retry hangs below the reveal band, at y={}",
        retry.y
    );
    let dismiss = bounds_of(&mut reader, Control::Library(library_ui::Focus::Dismiss));
    ui.click(&mut reader, dismiss.center());
    assert_eq!(
        reader.library.notice, None,
        "Dismiss answers with the bar hidden"
    );
}

#[test]
fn a_tap_on_a_strip_turns_the_page() {
    let mut reader = strip_book(1280.0, 700.0);
    let mut ui = Harness::new();
    let at = right_strip(&reader, 350.0);
    let finger = iced::touch::Finger(0);
    let pressed = ui.send(
        &mut reader,
        iced::Event::Touch(iced::touch::Event::FingerPressed {
            id: finger,
            position: at,
        }),
        at,
    );
    assert_eq!(page_turns(&pressed), [true]);
    let lifted = ui.send(
        &mut reader,
        iced::Event::Touch(iced::touch::Event::FingerLifted {
            id: finger,
            position: at,
        }),
        at,
    );
    assert!(page_turns(&lifted).is_empty(), "nothing on lift");
    reader.bar.toggle_pin(std::time::Instant::now());
    assert_eq!(page_field(&reader), "2");
}

#[test]
fn a_right_click_on_a_strip_still_opens_the_page_menu() {
    let mut reader = strip_book(1280.0, 700.0);
    let at = right_strip(&reader, 350.0);
    assert_eq!(interaction_at(&reader, at), mouse::Interaction::Pointer);
    let mut ui = Harness::new();
    let messages = ui.send(
        &mut reader,
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        at,
    );
    assert!(
        messages
            .iter()
            .any(|message| matches!(message, Message::ContextBlank)),
        "the margin under the strip gets the right click: {messages:?}"
    );
    assert!(page_turns(&messages).is_empty());
    let released = ui.send(
        &mut reader,
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)),
        at,
    );
    assert!(page_turns(&released).is_empty());
    assert_eq!(
        page_turns(&ui.click(&mut reader, at)),
        [true],
        "a left click there still turns the page"
    );
}

#[test]
fn a_strip_that_moves_under_a_resting_pointer_drops_its_shade() {
    let mut reader = strip_book(1000.0, 700.0);
    let at = right_strip(&reader, 350.0);
    let mut ui = Harness::new();
    ui.send(
        &mut reader,
        iced::Event::Mouse(mouse::Event::CursorMoved { position: at }),
        at,
    );
    reader.strips.settle();
    assert_eq!(reader.strips.alpha(page_strips::Side::Next), 1.0);
    let _ = update(
        &mut reader,
        Message::Notes(crate::notes::Action::ToggleList),
    );
    assert!(at.x > 1000.0 - 340.0, "the pointer is over the drawer now");
    let redraw = ui.send(
        &mut reader,
        iced::Event::Window(window::Event::RedrawRequested(std::time::Instant::now())),
        at,
    );
    assert!(
        redraw
            .iter()
            .any(|message| matches!(message, Message::StripHover(None))),
        "the redraw notices the strip moved away: {redraw:?}"
    );
    reader.strips.settle();
    assert_eq!(reader.strips.alpha(page_strips::Side::Next), 0.0);
}

#[test]
#[ignore = "Reading bar QA: set SIMPL_PREVIEW_OUTPUT"]
fn render_reading_bar_previews() {
    ui::load_test_fonts();
    let output = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_OUTPUT").expect("output directory"));
    std::fs::create_dir_all(&output).unwrap();
    let mut library = Reader::default();
    render(&mut library, &output.join("bar-library.png"));
    for (appearance, name) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
        for width in [1280.0, 860.0] {
            let mut reader = Reader {
                book: Some(super::tests::book("reading-bar")),
                window_size: Size::new(width, 700.0),
                appearance,
                ..Reader::default()
            };
            reader.returns.push(ReturnLocation {
                chapter: None,
                item_id: "paragraph-2".into(),
                within: 0.0,
            });
            let _ = reader.rebuild_geometry(Anchor {
                row: 0,
                fraction: 0.0,
            });
            settle_pagination(&mut reader);
            render(
                &mut reader,
                &output.join(format!("bar-shown-{name}-{width}.png")),
            );
            let _ = update_inner(&mut reader, Message::FindOpen);
            render(
                &mut reader,
                &output.join(format!("bar-find-{name}-{width}.png")),
            );
            let _ = update_inner(&mut reader, Message::FindClose);
            reader.bar = reading_chrome::Bar::hidden();
            render(
                &mut reader,
                &output.join(format!("bar-hidden-{name}-{width}.png")),
            );
            reader.bar.hover(std::time::Instant::now(), true);
            reader.bar.step_by(std::time::Duration::from_millis(75));
            render(
                &mut reader,
                &output.join(format!("bar-fading-{name}-{width}.png")),
            );
            reader.bar = Default::default();
            reader.fullscreen = true;
            render(
                &mut reader,
                &output.join(format!("bar-fullscreen-{name}-{width}.png")),
            );
        }
    }
    use crate::pdf_reader::tests::{complete, tall_pdf};
    let path = std::env::temp_dir().join(format!("simpl-bar-preview-{}.pdf", std::process::id()));
    std::fs::write(&path, tall_pdf()).unwrap();
    let document = complete(reader_pdf::open(path.clone())).unwrap();
    let _ = std::fs::remove_file(path);
    for width in [1280.0, 860.0] {
        let (pdf, _) =
            pdf_reader::Reader::new(document.clone(), None, Size::new(width, 700.0), 1.0);
        let mut reader = Reader {
            pdf: Some(pdf),
            window_size: Size::new(width, 700.0),
            ..Reader::default()
        };
        render(&mut reader, &output.join(format!("bar-pdf-{width}.png")));
    }
}

/// A book with contents and a passage to go back to, so its bar shows every
/// button, in a `width` x `height` window.
pub(super) fn full_bar_book(width: f32, height: f32) -> Reader {
    let mut source = Arc::try_unwrap(super::tests::book("The Quiet Hours")).unwrap();
    source.items[0] = Item::Heading {
        id: "paragraph-0".into(),
        text: "Chapter One".into(),
        level: 1,
    };
    let mut reader = Reader {
        book: Some(Arc::new(source)),
        window_size: Size::new(width, height),
        ..Reader::default()
    };
    let _ = reader.rebuild_geometry(Anchor {
        row: 0,
        fraction: 0.0,
    });
    settle_pagination(&mut reader);
    reader.returns.push(ReturnLocation {
        chapter: None,
        item_id: "paragraph-2".into(),
        within: 0.0,
    });
    let _ = reader.sync_reading_settings();
    let fingerprint = reader.book.as_ref().unwrap().fingerprint.clone();
    let _ = reader.reading_loaded(fingerprint, Ok(None));
    reader
}

#[test]
#[ignore = "UX-5 review: writes the icon bar and the Aa panel to target/ui-ux-review"]
fn render_ux5_bar_look() {
    ui::load_test_fonts();
    let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-ux-review");
    std::fs::create_dir_all(&output).unwrap();
    for (appearance, name) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
        for (width, height) in [(1400.0, 900.0), (540.0, 360.0)] {
            let mut reader = full_bar_book(width, height);
            reader.appearance = appearance;
            render(
                &mut reader,
                &output.join(format!("UX-5-bar-{name}-{width}.png")),
            );
        }
    }
    let mut reader = full_bar_book(1400.0, 900.0);
    let find = bounds_of(&mut reader, Control::Find).center();
    render_with_cursor(&mut reader, &output.join("UX-5-tooltip.png"), find);
    let _ = update(&mut reader, Message::ToggleMenu(reading_chrome::Menu::Aa));
    render(&mut reader, &output.join("UX-5-aa-book.png"));
    let mut reader = full_bar_book(540.0, 600.0);
    let _ = update(&mut reader, Message::ToggleMenu(reading_chrome::Menu::More));
    render(&mut reader, &output.join("UX-5-more-540.png"));
    let Some(mut reader) = test_pdf(
        crate::pdf_reader::tests::tall_pdf(),
        Size::new(1400.0, 900.0),
    ) else {
        return;
    };
    render(&mut reader, &output.join("UX-5-bar-pdf-1400.png"));
    let _ = update(&mut reader, Message::ToggleMenu(reading_chrome::Menu::Aa));
    render(&mut reader, &output.join("UX-5-aa-pdf.png"));
}

#[test]
#[ignore = "Per-book theme review: writes the Aa panel and Settings to target/ui-ux-review"]
fn render_book_theme_look() {
    ui::load_test_fonts();
    let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-ux-review");
    std::fs::create_dir_all(&output).unwrap();
    for (appearance, name) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
        let mut reader = full_bar_book(1400.0, 900.0);
        reader.appearance = appearance;
        let _ = update(&mut reader, Message::ToggleMenu(reading_chrome::Menu::Aa));
        let clear = Control::Theme(reading_ui::Target::Book, 2);
        let _ = reader.activate(clear);
        settle_theme(&mut reader);
        reader.focused = None;
        render(&mut reader, &output.join(format!("theme-aa-{name}.png")));
        let _ = update(&mut reader, Message::ToggleMenu(reading_chrome::Menu::Aa));
        reader.show_settings = true;
        render(
            &mut reader,
            &output.join(format!("theme-settings-{name}.png")),
        );
    }
    let mut reader = full_bar_book(1400.0, 900.0);
    let _ = update(&mut reader, Message::SetTheme(3));
    settle_theme(&mut reader);
    let _ = update(&mut reader, Message::ToggleMenu(reading_chrome::Menu::Aa));
    render(&mut reader, &output.join("theme-aa-following-compact.png"));
}

#[test]
#[ignore = "Visual QA: set SIMPL_PREVIEW_HTML, SIMPL_PREVIEW_EPUB and SIMPL_PREVIEW_OUTPUT"]
fn render_book_previews() {
    for bytes in ui::font_data() {
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Borrowed(bytes));
    }
    let output = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_OUTPUT").expect("output directory"));
    std::fs::create_dir_all(&output).unwrap();
    let html = reader_document::load_html(&PathBuf::from(
        std::env::var_os("SIMPL_PREVIEW_HTML").expect("HTML path"),
    ))
    .unwrap();
    let epub = Arc::new(
        reader_document::epub::open(&PathBuf::from(
            std::env::var_os("SIMPL_PREVIEW_EPUB").expect("EPUB path"),
        ))
        .unwrap(),
    );
    let index = std::env::var("SIMPL_PREVIEW_CHAPTER")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let chapter = epub.load_chapter(index).unwrap();
    let books = [
        ("html", Arc::new(display_book(html, None, None))),
        (
            "epub",
            Arc::new(display_book(
                chapter.document,
                None,
                Some(EpubChapter {
                    document: epub,
                    index,
                }),
            )),
        ),
    ];
    println!(
        "HTML parser retained {} headings; EPUB has {} spine chapters and {} contents entries",
        books[0]
            .1
            .items
            .iter()
            .filter(|i| matches!(i, Item::Heading { .. }))
            .count(),
        books[1].1.epub.as_ref().unwrap().document.chapters.len(),
        books[1].1.epub.as_ref().unwrap().document.contents.len()
    );
    for (appearance, label) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
        for width in [540.0, 1280.0] {
            let mut reader = Reader {
                appearance,
                window_size: Size::new(width, 800.0),
                focused: Some(Control::Chrome(chrome::Action::ToggleAppearance)),
                ..Reader::default()
            };
            render(
                &mut reader,
                &output.join(format!("home-{label}-{width}.png")),
            );
        }
    }
    for (kind, book) in books {
        for (appearance, label) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
            for (width, font_size, size_label) in [
                (1280.0, 20.0, "wide"),
                (540.0, 20.0, "narrow"),
                (540.0, 36.0, "large"),
            ] {
                let mut reader = Reader {
                    book: Some(book.clone()),
                    appearance,
                    font_size,
                    window_size: Size::new(width, 800.0),
                    focused: Some(Control::Find),
                    ..Reader::default()
                };
                let _ = reader.rebuild_geometry(Anchor {
                    row: std::env::var("SIMPL_PREVIEW_ROW")
                        .ok()
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(0)
                        .min(book.items.len() - 1),
                    fraction: 0.0,
                });
                settle_pagination(&mut reader);
                if let Some(item) = book
                    .items
                    .iter()
                    .find(|item| matches!(item, Item::Paragraph { .. }))
                {
                    let text = item.text().unwrap();
                    let end = text
                        .char_indices()
                        .nth(24)
                        .map_or(text.len(), |(index, _)| index);
                    reader.selection.begin(Endpoint {
                        item_id: item.id().into(),
                        byte_offset: 0,
                    });
                    reader.selection.extend(Endpoint {
                        item_id: item.id().into(),
                        byte_offset: end,
                    });
                    reader.selection.end_drag();
                    assert_eq!(
                        reader.selection.copy_text(&book.items).unwrap(),
                        text[..end]
                    );
                }
                let link_control = reader
                    .controls()
                    .find(|c| matches!(c, Control::BookLink(_, _)));
                if let Some(control) = link_control {
                    reader.focused = Some(control);
                }
                render(
                    &mut reader,
                    &output.join(format!("{kind}-{label}-{size_label}.png")),
                );
                assert!(!reader.pages().is_empty());
                let pages = reader.pages();
                if book_pages::visible(pages, reader.offset, reader.viewport)
                    .is_some_and(|p| p.number < pages.last().unwrap().number)
                {
                    let before = reader.offset;
                    let _ = update_inner(&mut reader, Message::BookPage(true));
                    assert!(reader.offset > before);
                }
                if size_label == "wide" {
                    render(
                        &mut reader,
                        &output.join(format!("{kind}-{label}-next-page.png")),
                    );
                    if kind == "html" {
                        let chapter = reader.contents().iter().find(|c| c.label == "CHAPTER I");
                        if let Some(chapter) = chapter {
                            let _ = reader.open_chapter(chapter.chapter, None);
                            assert!(reader.reading_title().unwrap().ends_with("~ CHAPTER I"));
                            render(
                                &mut reader,
                                &output.join(format!("html-{label}-chapter.png")),
                            );
                        }
                    }
                }
            }
        }
    }
}

fn complete<T>(future: impl std::future::Future<Output = T>) -> T {
    struct Wake(std::thread::Thread);
    impl std::task::Wake for Wake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = std::task::Waker::from(Arc::new(Wake(std::thread::current())));
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => return value,
            std::task::Poll::Pending => std::thread::park(),
        }
    }
}

#[test]
#[ignore = "Isolated managed-library QA: SIMPL_PREVIEW_STORE must match LOCALAPPDATA; HTML/EPUB/OUTPUT required"]
fn render_library_workflow() {
    let store = std::env::var_os("SIMPL_PREVIEW_STORE").expect("isolated store");
    assert_eq!(std::env::var_os("LOCALAPPDATA"), Some(store));
    let output = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    for bytes in ui::font_data() {
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Borrowed(bytes));
    }
    let mut entries = Vec::new();
    let mut sources = Vec::new();
    for variable in ["SIMPL_PREVIEW_HTML", "SIMPL_PREVIEW_EPUB"] {
        let source = PathBuf::from(std::env::var_os(variable).unwrap());
        let original = std::fs::read(&source).unwrap();
        let path = reader_document::managed::import(&source).unwrap();
        assert_ne!(path, source);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let loaded = complete(load_document(path.clone(), None, None)).unwrap();
        entries.push(loaded.catalog.unwrap());
        sources.push((source, original));
    }
    let mut reader = Reader {
        window_size: Size::new(1280.0, 1600.0),
        ..Reader::default()
    };
    let _ = reader.shelf.update(shelf::Message::Loaded(Ok(entries)));
    let _ = reader.shelf.resize(reader.window_size);
    let _ = reader
        .shelf
        .update(shelf::Message::Activate(shelf::Control::Favourite(
            0, false,
        )));
    assert!(reader.shelf.entries[0].favourite);
    // Exercise native hit testing: the overlay actions must not also open the card.
    for control in [
        shelf::Control::Favourite(0, false),
        shelf::Control::Remove(0, false),
    ] {
        reader.focused = Some(Control::Shelf(control));
        let renderer = iced::Renderer::new(ui::SANS, iced::Pixels(13.0));
        let mut element = view(&reader);
        let mut tree = Tree::new(&element);
        let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(reader.window_size, reader.window_size),
        );
        let mut find = FocusedBounds::default();
        element
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut find);
        let point = find.0.expect("focused card action").center();
        let mut messages = Vec::new();
        for event in [
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            element.as_widget_mut().update(
                &mut tree,
                &iced::Event::Mouse(event),
                Layout::new(&node),
                mouse::Cursor::Available(point),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut iced::advanced::Shell::new(&mut messages),
                &Rectangle::with_size(reader.window_size),
            );
        }
        let actions = messages
            .iter()
            .filter_map(|m| match m {
                Message::Shelf(shelf::Message::Activate(c)) => Some(*c),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(actions, [control]);
    }
    reader.focused = None;
    library::save(&reader.shelf.entries).unwrap();
    assert!(library::load().unwrap()[0].favourite);
    assert!(
        reader
            .shelf
            .controls()
            .any(|c| c == shelf::Control::FavouriteDocument(0))
    );
    for (appearance, name) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
        reader.appearance = appearance;
        let _ = reader
            .shelf
            .update(shelf::Message::Hover(Some(shelf::Control::Document(0))));
        render(&mut reader, &output.join(format!("library-{name}.png")));
    }
    let path = reader.shelf.entries[0].document.path.clone();
    let _ = update_inner(&mut reader, Message::RemoveLibrary(0));
    assert!(reader.confirm_remove.is_some());
    assert!(reader.removing.is_none());
    assert!(path.exists());
    render(&mut reader, &output.join("remove-confirmation.png"));
    let _ = update_inner(&mut reader, Message::DismissOverlay);
    assert!(reader.confirm_remove.is_none());
    assert!(path.exists());
    assert_eq!(reader.shelf.entries.len(), 2);
    let _ = update_inner(&mut reader, Message::RemoveLibrary(0));
    let _ = update_inner(&mut reader, Message::ConfirmRemove);
    assert!(reader.confirm_remove.is_none());
    assert_eq!(reader.removing.as_ref(), Some(&path));
    // Execute the worker synchronously in this isolated native test.
    reader_document::managed::remove(&path).unwrap();
    let _ = update_inner(
        &mut reader,
        Message::RemovedLibrary {
            path: path.clone(),
            result: Ok(()),
        },
    );
    assert!(!path.exists());
    assert_eq!(reader.shelf.entries.len(), 1);
    library::save(&reader.shelf.entries).unwrap();
    assert_eq!(library::load().unwrap().len(), 1);
    for (source, original) in sources {
        assert_eq!(std::fs::read(source).unwrap(), original);
    }
    println!(
        "Managed import, persistent favourites, physical copy removal and original preservation passed"
    );
}

#[test]
#[ignore = "Shelves visual QA: set SIMPL_PREVIEW_OUTPUT"]
fn render_shelves() {
    use reader_document::recent::DocumentKind;
    let output = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    for bytes in ui::font_data() {
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Borrowed(bytes));
    }
    let mut reader = Reader {
        window_size: Size::new(1280.0, 1300.0),
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
    for name in ["Okunacaklar", "Bitenler", "Ders"] {
        let _ = reader
            .shelf
            .update(shelf::Message::Activate(shelf::Control::NewShelf));
        let _ = reader.shelf.update(shelf::Message::NameInput(name.into()));
        let _ = reader.shelf.update(shelf::Message::NameSubmit);
    }
    for (book, shelf_id) in [(0, 2), (1, 1), (2, 3), (4, 1), (4, 3)] {
        let _ = reader
            .shelf
            .update(shelf::Message::Activate(shelf::Control::ToggleShelf(
                book, false, shelf_id,
            )));
    }
    for (appearance, name) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
        reader.appearance = appearance;
        let _ = reader
            .shelf
            .update(shelf::Message::Activate(shelf::Control::Filter(None)));
        let _ = reader
            .shelf
            .update(shelf::Message::Hover(Some(shelf::Control::Document(1))));
        let _ = reader
            .shelf
            .update(shelf::Message::Activate(shelf::Control::ShelfMenu(
                1, false,
            )));
        render(
            &mut reader,
            &output.join(format!("shelves-menu-{name}.png")),
        );
        let _ = reader
            .shelf
            .update(shelf::Message::Activate(shelf::Control::Filter(Some(1))));
        render(
            &mut reader,
            &output.join(format!("shelves-filtered-{name}.png")),
        );
    }
    let _ = reader
        .shelf
        .update(shelf::Message::Activate(shelf::Control::NewShelf));
    let _ = reader
        .shelf
        .update(shelf::Message::NameInput("Okunacaklar".into()));
    let _ = reader.shelf.update(shelf::Message::NameSubmit);
    render(&mut reader, &output.join("shelves-name-error.png"));
}

#[test]
#[ignore = "Native PDF QA: SIMPL_PREVIEW_PDF + SIMPL_PREVIEW_OUTPUT; PDFium beside the test executable"]
fn render_pdf_book_previews() {
    for bytes in ui::font_data() {
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Borrowed(bytes));
    }
    let output = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_OUTPUT").expect("output directory"));
    std::fs::create_dir_all(&output).unwrap();
    let path = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_PDF").expect("PDF path"));
    let document = complete(reader_pdf::open(path)).unwrap();
    let page = std::env::var("SIMPL_PREVIEW_PAGE")
        .ok()
        .and_then(|p| p.parse::<u32>().ok())
        .unwrap_or(0);
    let raster = complete(document.session.render(page, 900)).unwrap();
    ::image::save_buffer(
        output.join("original.png"),
        &raster.rgba,
        raster.width,
        raster.height,
        ::image::ColorType::Rgba8,
    )
    .unwrap();
    // This separate extraction is the reference for every recorded source range.
    let conversion = complete(document.session.book());
    if let Ok(expected) = std::env::var("SIMPL_EXPECT_BOOK_ERROR") {
        let error = conversion.unwrap_err();
        assert!(error.contains(&expected), "{error}");
        std::fs::write(output.join("conversion-error.txt"), &error).unwrap();
        assert!(
            !complete(document.session.render(0, 400))
                .unwrap()
                .rgba
                .is_empty()
        );
        println!("Expected conversion rejection: {error}; original still renders");
        return;
    }
    let conversion = conversion.unwrap();
    let mut dump = String::new();
    let mut checked_page = u32::MAX;
    let mut layer = None;
    for block in &conversion.blocks {
        assert!(
            block
                .sources
                .iter()
                .all(|r| r.page == block.sources[0].page)
        );
        dump.push_str(&format!(
            "{} | page {}\n{}\n\n",
            block.id,
            block.sources[0].page + 1,
            block.text
        ));
        let mut source_text = String::new();
        for range in &block.sources {
            if checked_page != range.page {
                checked_page = range.page;
                layer = Some(complete(document.session.text(range.page, 1600)).unwrap());
            }
            let text = &layer.as_ref().unwrap().text;
            assert!(range.start <= range.end && range.end <= text.len());
            assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
            source_text.push_str(&text[range.start..range.end]);
            assert!((0.0..=1.0).contains(&range.top) && range.top <= range.bottom);
        }
        if !block.id.ends_with("-empty") {
            let content = |s: &str| {
                s.chars()
                    .filter(|c| !c.is_whitespace() && *c != '-' && *c != '\u{ad}')
                    .collect::<String>()
            };
            assert_eq!(
                content(&source_text),
                content(&block.text),
                "Source text changed outside permitted whitespace/hyphen cleanup: {}",
                block.id
            );
        }
    }
    std::fs::write(output.join("book-text.txt"), dump).unwrap();
    std::fs::write(
        output.join("conversion-notices.txt"),
        conversion.warnings.join("\n"),
    )
    .unwrap();
    let mut origin = default_pdf_position(&document);
    origin.page = page;
    let book =
        Arc::new(complete(load_pdf_book(document.clone(), origin.clone(), 20.0, true)).unwrap());
    if document
        .path
        .file_name()
        .is_some_and(|name| name == "layout.pdf")
    {
        let entry = book
            .items
            .iter()
            .position(|item| {
                book.structure.get(item.id()).is_some_and(|semantics| {
                    semantics.links.iter().any(|link| link.href == "pdf-page:2")
                })
            })
            .expect("Contents link reached Book semantics");
        let mut reading = Reader {
            book: Some(book.clone()),
            window_size: Size::new(1280.0, 800.0),
            ..Reader::default()
        };
        let _ = reading.rebuild_geometry(Anchor {
            row: entry,
            fraction: 0.0,
        });
        settle_pagination(&mut reading);
        let _ = reading.follow_link("pdf-page:2".into());
        settle_pagination(&mut reading);
        assert_eq!(reading.pdf_return_position().unwrap().page, 2);
        let _ = reading.go_back();
        settle_pagination(&mut reading);
        assert_eq!(reading.pdf_return_position().unwrap().page, 1);
    }
    if let Some(store) = std::env::var_os("SIMPL_PREVIEW_STORE") {
        assert_eq!(
            std::env::var_os("LOCALAPPDATA"),
            Some(store),
            "Use an isolated preference directory for persistence QA"
        );
        let chosen = book
            .items
            .iter()
            .find(|item| item.text().is_some())
            .unwrap();
        let saved = ReadingPosition {
            fingerprint: document.fingerprint.clone(),
            item_id: chosen.id().into(),
            within: 0.4,
            font_size: 30.0,
        };
        let mut legacy = saved.clone();
        legacy.item_id = legacy.item_id.replacen(
            &format!("pdf-b{}-", reader_pdf::book::VERSION),
            "pdf-b1-",
            1,
        );
        position::save_pdf_book(&document.path, &legacy).unwrap();
        let migrated =
            complete(load_pdf_book(document.clone(), origin.clone(), 20.0, true)).unwrap();
        assert_eq!(migrated.restored.as_ref().unwrap().item_id, saved.item_id);
        assert_eq!(
            migrated.restored.as_ref().unwrap().font_size,
            saved.font_size
        );
        // A toolbar switch must ignore an older independently saved Book location.
        let mut latest = origin.clone();
        latest.page = 37.min(document.pages.len() as u32 - 1);
        latest.within = 0.9;
        let switched =
            complete(load_pdf_book(document.clone(), latest.clone(), 20.0, false)).unwrap();
        let location = switched.restored.clone().unwrap();
        let row = switched
            .items
            .iter()
            .position(|i| i.id() == location.item_id)
            .unwrap();
        assert_eq!(
            switched.pdf_source.as_ref().unwrap().conversion.blocks[row].sources[0].page,
            latest.page
        );
        let mut reading = Reader {
            book: Some(Arc::new(switched)),
            ..Reader::default()
        };
        let _ = reading.rebuild_geometry(Anchor {
            row,
            fraction: location.within,
        });
        settle_pagination(&mut reading);
        let _ = reading.go_to_local_page(44.min(document.pages.len() - 1));
        let returned = reading.pdf_return_position().unwrap();
        assert_eq!(returned.page, 44.min(document.pages.len() as u32 - 1));
        let (original_reader, _) = pdf_reader::Reader::new(
            document.clone(),
            Some(returned),
            Size::new(1280.0, 800.0),
            1.0,
        );
        assert_eq!(
            original_reader.position().page,
            44.min(document.pages.len() as u32 - 1)
        );
        println!(
            "Document page 38 overrides old Book resume; Book page 45 returns to Document page 45"
        );
        let mut original_saved = origin.clone();
        original_saved.zoom = position::PdfZoom::Scale(1.25);
        position::save_pdf(&document.path, &original_saved).unwrap();
        SavedPosition::PdfBook(document.path.clone(), saved.clone())
            .save()
            .unwrap();
        let reopened = complete(load_document(document.path.clone(), None, None)).unwrap();
        let LoadedDocument::Reflow(reopened) = reopened.document else {
            panic!("Book preference was not restored");
        };
        let restored = reopened.restored.as_ref().unwrap();
        assert_eq!(restored.item_id, saved.item_id);
        assert_eq!(restored.within, 0.4);
        assert_eq!(restored.font_size, 30.0);
        assert_eq!(
            position::load_pdf(&document.path).unwrap().unwrap().zoom,
            position::PdfZoom::Scale(1.25)
        );
        SavedPosition::Pdf(document.path.clone(), original_saved.clone())
            .save()
            .unwrap();
        let reopened = complete(load_document(document.path.clone(), None, None)).unwrap();
        let LoadedDocument::Pdf { restored, .. } = reopened.document else {
            panic!("Document preference was not restored");
        };
        assert_eq!(restored.unwrap().page, original_saved.page);
        assert_eq!(
            position::load_pdf_book(&document.path)
                .unwrap()
                .unwrap()
                .item_id,
            saved.item_id
        );
        println!("Independent Book/Document positions and chosen mode survived reopening");
    }
    let row = book
        .pdf_source
        .as_ref()
        .unwrap()
        .conversion
        .blocks
        .iter()
        .position(|b| b.sources[0].page >= page)
        .unwrap_or(0);
    if std::env::var_os("SIMPL_SCROLL_PROBE").is_some() {
        let mut reading = Reader {
            book: Some(book.clone()),
            window_size: Size::new(1280.0, 800.0),
            ..Reader::default()
        };
        let _ = reading.rebuild_geometry(Anchor { row, fraction: 0.0 });
        settle_pagination(&mut reading);
        let sheet = reading.active_page().unwrap();
        let size = reading.window_size;
        let renderer = iced::Renderer::new(ui::SANS, iced::Pixels(13.0));
        let mut element = view(&reading);
        let mut tree = Tree::new(&element);
        let _ =
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        drop(element);
        let mut reused = std::time::Duration::ZERO;
        let mut fresh = std::time::Duration::ZERO;
        for step in 1..=12 {
            reading.offset = sheet.top + (step as f32 * 20.0).min((sheet.height - 800.0).max(0.0));
            let mut element = view(&reading);
            let start = std::time::Instant::now();
            tree.diff(&element);
            let _ = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            reused += start.elapsed();
            let start = std::time::Instant::now();
            let mut new_tree = Tree::new(&element);
            let _ = element.as_widget_mut().layout(
                &mut new_tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            fresh += start.elapsed();
            drop(element);
            let _ = reading.refine_geometry();
        }
        println!("PDF Book 12 scroll layouts: retained state {reused:?}, fresh state {fresh:?}");
    }
    for (appearance, label) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
        for (width, font, name) in [
            (1280.0, 20.0, "wide"),
            (540.0, 20.0, "narrow"),
            (540.0, 36.0, "large"),
        ] {
            let mut reader = Reader {
                book: Some(book.clone()),
                appearance,
                zoom: font / DEFAULT_FONT_SIZE,
                window_size: Size::new(width, 800.0),
                focused: Some(Control::DocumentMode),
                ..Reader::default()
            };
            let _ = reader.rebuild_geometry(Anchor { row, fraction: 0.0 });
            settle_pagination(&mut reader);
            assert!(reader.controls().any(|c| c == Control::DocumentMode));
            assert!(matches!(
                reader.document_position(),
                Some(SavedPosition::PdfBook(_, _))
            ));
            render(
                &mut reader,
                &output.join(format!("pdf-book-{label}-{name}.png")),
            );
            let source_page = reader
                .pages()
                .iter()
                .find(|p| p.rows.contains(&row))
                .unwrap()
                .number;
            if source_page + 1 < document.pages.len() as u32 {
                let _ = reader.adjacent_book_page(true);
                assert_eq!(
                    pdf_source_range(&conversion, reader.anchor()).unwrap().page,
                    source_page + 1
                );
                let _ = reader.adjacent_book_page(false);
                assert_eq!(
                    pdf_source_range(&conversion, reader.anchor()).unwrap().page,
                    source_page
                );
            }
            let anchor = reader.anchor();
            let _ = update_inner(&mut reader, Message::Zoom(1.2));
            let _ = reader.rebuild_geometry(anchor);
            settle_pagination(&mut reader);
            assert_eq!(
                pdf_source_range(&conversion, reader.anchor()).unwrap().page,
                source_page
            );
            let first = reader.pages()[0].rows.start;
            let _ = reader.rebuild_geometry(Anchor {
                row: first,
                fraction: 0.0,
            });
            settle_pagination(&mut reader);
            let _ = reader.adjacent_book_page(false);
            assert_eq!(reader.anchor().row, first);
            assert!(!reader.can_turn(false));
            let last = reader.pages().last().unwrap().rows.start;
            let _ = reader.rebuild_geometry(Anchor {
                row: last,
                fraction: 0.0,
            });
            settle_pagination(&mut reader);
            let _ = reader.adjacent_book_page(true);
            assert_eq!(reader.anchor().row, last);
            assert!(!reader.can_turn(true));
            let _ = reader.rebuild_geometry(anchor);
            settle_pagination(&mut reader);
            if name == "wide" {
                let pages = reader.pages();
                let sheet = pages.iter().find(|p| p.number == source_page).unwrap();
                let _ = reader.jump(sheet.top + sheet.height - reader.viewport * 0.65);
                render(
                    &mut reader,
                    &output.join(format!("pdf-book-{label}-boundary.png")),
                );
            }
        }
    }
    let (pdf, _) =
        pdf_reader::Reader::new(document.clone(), Some(origin), Size::new(540.0, 800.0), 1.0);
    let mut reader = Reader {
        pdf: Some(pdf),
        window_size: Size::new(540.0, 800.0),
        ..Reader::default()
    };
    let _ = update_inner(&mut reader, Message::BookMode);
    assert!(
        reader.opening.is_some(),
        "Book click must start conversion immediately"
    );
    let _ = update_inner(&mut reader, Message::CancelConversion);
    assert!(reader.opening.is_none());
    let before = reader.pdf.as_ref().unwrap().position();
    reader.request = 22;
    let _ = update_inner(
        &mut reader,
        Message::Loaded {
            request: 22,
            result: Err("conversion rejected".into()),
        },
    );
    assert_eq!(reader.pdf.as_ref().unwrap().position().page, before.page);
    render(&mut reader, &output.join("pdf-document-toolbar.png"));
    println!(
        "Converted {} pages into {} blocks; source ranges verified",
        document.pages.len(),
        conversion.blocks.len()
    );
}

#[test]
#[ignore = "Native long-paragraph QA: SIMPL_PREVIEW_EPUB and SIMPL_PREVIEW_OUTPUT"]
fn render_long_paragraph_pages() {
    for bytes in ui::font_data() {
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Borrowed(bytes));
    }
    let output = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    let epub = Arc::new(
        reader_document::epub::open(&PathBuf::from(
            std::env::var_os("SIMPL_PREVIEW_EPUB").unwrap(),
        ))
        .unwrap(),
    );
    let index = epub
        .chapters
        .iter()
        .position(|c| c.href.ends_with("2554-h-5.htm.xhtml"))
        .expect("saved CHAPTER III");
    let chapter = epub.load_chapter(index).unwrap();
    let book = Arc::new(display_book(
        chapter.document,
        None,
        Some(EpubChapter {
            document: epub,
            index,
        }),
    ));
    let long_row = book
        .items
        .iter()
        .enumerate()
        .max_by_key(|(_, i)| i.text().map_or(0, str::len))
        .unwrap()
        .0;
    for (width, font_size) in [(1280.0, 12.0), (1280.0, 20.0), (540.0, 36.0)] {
        let mut reader = Reader {
            book: Some(book.clone()),
            zoom: font_size / DEFAULT_FONT_SIZE,
            window_size: Size::new(width, 1000.0),
            viewport: 900.0,
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 0,
            fraction: 0.0,
        });
        settle_pagination(&mut reader);
        let renderer = iced::Renderer::new(ui::SANS, iced::Pixels(13.0));
        // Independently audit every production measurement against the native row widget.
        for (row, item) in book.items.iter().enumerate() {
            let mut element = render_item(&reader, &book, row, item, None);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::new(reader.width, f32::INFINITY)),
            );
            let gap = if row + 1 == book.items.len() {
                0.0
            } else {
                MINIMAL.gap(DEFAULT_FONT_SIZE)
            };
            assert!((reader.heights.height(row) - node.size().height - gap).abs() < 0.01);
        }
        reader.pending_anchor = None;
        let fixed_pages = reader
            .pages()
            .iter()
            .map(|p| (p.content.clone(), p.top, p.height))
            .collect::<Vec<_>>();
        for index in [
            0,
            fixed_pages.len() / 4,
            fixed_pages.len() / 2,
            fixed_pages.len() - 1,
            0,
        ] {
            reader.offset = fixed_pages[index].1;
            render(
                &mut reader,
                &output.join(format!("stable-{width}-{font_size}.png")),
            );
            assert_eq!(
                fixed_pages,
                reader
                    .pages()
                    .iter()
                    .map(|p| (p.content.clone(), p.top, p.height))
                    .collect::<Vec<_>>(),
                "scrolling through the chapter must preserve every page boundary and total"
            );
        }
        let pages = reader.pages().to_vec();
        let paper = (reader.width + book_map::MARGIN * 2.0) * 1.414;
        assert!(pages.iter().all(|p| p.height <= paper + 0.1));
        for pair in pages.windows(2) {
            assert_eq!(pair[0].content.end, pair[1].content.start);
        }
        assert_eq!(pages.last().unwrap().content.end, reader.heights.total());
        for page in &pages {
            reader.pending_anchor = None;
            reader.offset = page.top;
            let expected = page.top.min(
                (book_pages::scroll_total(&pages, reader.viewport) - reader.viewport).max(0.0),
            );
            assert!((reader.offset_for(reader.anchor()) - expected).abs() < 0.1);
        }
        let fragments = pages
            .iter()
            .filter(|p| p.rows.contains(&long_row))
            .collect::<Vec<_>>();
        assert!(fragments.len() > 2);
        let fragment = fragments[1];
        reader.offset = fragment.top;
        let saved = reader.anchor();
        assert_eq!(saved.row, long_row);
        assert!(saved.fraction > 0.0);
        assert!((reader.offset_for(saved) - fragment.top).abs() < 0.1);
        let before = reader.offset;
        let _ = reader.adjacent_book_page(true);
        assert!(reader.offset > before);
        let _ = reader.adjacent_book_page(false);
        assert!((reader.offset - before).abs() < 0.1);
        // Hit-test the continuation using the original source ID and byte offsets.
        let mut element: Element<'_, Message> = virtual_reader::VisibleRows::new(
            long_row..long_row + 1,
            &reader.heights,
            reader.width,
            MINIMAL.gap(DEFAULT_FONT_SIZE),
            vec![render_item(
                &reader,
                &book,
                long_row,
                &book.items[long_row],
                None,
            )],
            virtual_reader::LayoutReports {
                measurements: Default::default(),
                generation: reader.generation,
                counters: None,
            },
        )
        .slice(fragment.content.clone())
        .into();
        element = book_zoom::wrap(element, reader.width, reader.zoom);
        let mut tree = Tree::new(&element);
        let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(reader.width, f32::INFINITY)),
        );
        let viewport = Rectangle::with_size(Size::new(reader.width, node.size().height + 100.0));
        let mut hits = Vec::new();
        for y in [
            DEFAULT_FONT_SIZE * 0.8 * reader.zoom,
            DEFAULT_FONT_SIZE * 2.4 * reader.zoom,
            node.size().height + 10.0,
        ] {
            let mut messages = Vec::new();
            element.as_widget_mut().update(
                &mut tree,
                &iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Layout::new(&node),
                mouse::Cursor::Available(Point::new(10.0 * reader.zoom, y)),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut iced::advanced::Shell::new(&mut messages),
                &viewport,
            );
            if y > node.size().height {
                assert!(messages.is_empty(), "page margin must not hit hidden text");
            }
            for message in messages {
                if let Message::SelectStart(endpoint) = message {
                    hits.push(endpoint);
                }
            }
        }
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].item_id, book.items[long_row].id());
        assert!(hits[0].byte_offset > 0);
        assert!(hits[1].byte_offset > hits[0].byte_offset);
        reader.selection.begin(hits[0].clone());
        reader.selection.extend(hits[1].clone());
        reader.selection.end_drag();
        assert_eq!(
            reader.selection.copy_text(&book.items).unwrap(),
            &book.items[long_row].text().unwrap()[hits[0].byte_offset..hits[1].byte_offset]
        );
        drop(element);
        // Show bottom margin and next sheet, then the continued text from its top.
        reader.pending_anchor = None;
        reader.offset = (fragment.top - 220.0).max(0.0);
        render(
            &mut reader,
            &output.join(format!("long-{width}-{font_size}-boundary.png")),
        );
        reader.offset = fragment.top;
        render(
            &mut reader,
            &output.join(format!("long-{width}-{font_size}-continued.png")),
        );
        // Reproduce the screenshot: final sheet fully visible below the previous sheet's footer.
        reader.pending_anchor = None;
        reader.viewport = 900.0;
        reader.offset = (book_pages::total(&pages) - reader.viewport).max(0.0);
        assert_eq!(
            book_pages::visible(&pages, reader.offset, reader.viewport)
                .unwrap()
                .number,
            pages.last().unwrap().number
        );
        assert!(reader.can_turn(true));
        reader.appearance = Appearance::Dark;
        render(
            &mut reader,
            &output.join(format!("last-{width}-{font_size}.png")),
        );
        let _ = update_inner(&mut reader, Message::BookPage(true));
        assert!(
            reader.opening.is_some(),
            "last-page Next must start the next chapter load"
        );
        assert!(matches!(
            reader.pending_navigation,
            Some(Navigation::Preserve)
        ));
        let chapter = book.epub.as_ref().unwrap();
        let next = load_epub_chapter(chapter.document.clone(), chapter.index + 1, None).unwrap();
        let request = reader.request;
        let _ = update_inner(
            &mut reader,
            Message::Loaded {
                request,
                result: Ok(LoadReply {
                    document: LoadedDocument::Reflow(Arc::new(next)),
                    catalog: None,
                }),
            },
        );
        assert_eq!(
            reader.book.as_ref().unwrap().epub.as_ref().unwrap().index,
            chapter.index + 1
        );
        assert!(reader.reading_title().unwrap().ends_with("~ CHAPTER IV"));
        render(
            &mut reader,
            &output.join(format!("next-chapter-{width}-{font_size}.png")),
        );
        println!(
            "CHAPTER III at zoom {font_size}/20 and width {width}: {} bounded sheets, longest paragraph spans {} sheets",
            pages.len(),
            fragments.len()
        );
    }
}

#[test]
#[ignore = "PDF cache and blank-page QA; requires PDF_PATHS JSON, OUTPUT and isolated STORE"]
fn pdf_book_cache_and_blank_pages() {
    let store = std::env::var_os("SIMPL_PREVIEW_STORE").expect("isolated store");
    assert_eq!(std::env::var_os("LOCALAPPDATA"), Some(store));
    for bytes in ui::font_data() {
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Borrowed(bytes));
    }
    let paths: Vec<PathBuf> = serde_json::from_slice(
        &std::fs::read(std::env::var_os("SIMPL_PDF_PATHS").unwrap()).unwrap(),
    )
    .unwrap();
    let output = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    for (index, path) in paths.into_iter().enumerate() {
        let document = complete(reader_pdf::open(path.clone())).unwrap();
        let start = std::time::Instant::now();
        let book = Arc::new(
            complete(load_pdf_book(
                document.clone(),
                default_pdf_position(&document),
                20.0,
                false,
            ))
            .unwrap(),
        );
        println!(
            "{} first load_pdf_book: {:?}",
            document.title,
            start.elapsed()
        );
        let original = serde_json::to_value(&book.pdf_source.as_ref().unwrap().conversion).unwrap();
        let mut reader = Reader {
            book: Some(book),
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 0,
            fraction: 0.0,
        });
        settle_pagination(&mut reader);
        assert_eq!(reader.page_total(), document.pages.len());
        let _ = reader.go_to_local_page(if index == 0 { 49 } else { 20 });
        render(&mut reader, &output.join(format!("{index}-prose.png")));
        assert!(reader.pdf_book_raster.is_none());
        let _ = reader.go_to_local_page(2);
        let range = reader.active_page().unwrap().rows.clone();
        assert!(
            reader
                .book
                .as_ref()
                .unwrap()
                .pdf_source
                .as_ref()
                .unwrap()
                .conversion
                .blocks[range]
                .iter()
                .all(|block| block.text.is_empty())
        );
        render(&mut reader, &output.join(format!("{index}-blank.png")));
        if index == 1 {
            let _ = reader.go_to_local_page(21);
            render(&mut reader, &output.join("sherlock-illustration.png"));
            assert!(
                reader
                    .pdf_book_raster
                    .as_ref()
                    .is_some_and(|(_, page, result)| *page == 21
                        && result.as_ref().is_ok_and(|images| !images.is_empty()))
            );
        }
        let position = reader.pdf_return_position().unwrap();
        let start = std::time::Instant::now();
        let warm = complete(load_pdf_book(
            document.clone(),
            position.clone(),
            20.0,
            false,
        ))
        .unwrap();
        assert_eq!(
            original,
            serde_json::to_value(&warm.pdf_source.as_ref().unwrap().conversion).unwrap()
        );
        let target = pdf_book_anchor(&warm.pdf_source.as_ref().unwrap().conversion, &position);
        reader.book = Some(Arc::new(warm));
        let _ = reader.rebuild_geometry(target);
        settle_pagination(&mut reader);
        assert_eq!(reader.active_page().unwrap().number, position.page);
        println!(
            "{} warm load + layout: {:?}",
            document.title,
            start.elapsed()
        );
        drop(reader);
        drop(document);
        let document = complete(reader_pdf::open(path)).unwrap();
        let start = std::time::Instant::now();
        let reopened = complete(load_pdf_book(document.clone(), position, 20.0, false)).unwrap();
        assert_eq!(
            original,
            serde_json::to_value(&reopened.pdf_source.as_ref().unwrap().conversion).unwrap()
        );
        assert!(book_map::cached(&reopened, 1).is_some());
        println!(
            "{} reopened load_pdf_book: {:?}",
            document.title,
            start.elapsed()
        );
    }
}

#[test]
#[ignore = "Real PDF illustrations and HTML folder QA; requires isolated STORE and QA_DESKTOP"]
fn hybrid_pdf_and_html_folder() {
    let store = std::env::var_os("SIMPL_PREVIEW_STORE").expect("isolated store");
    assert_eq!(std::env::var_os("LOCALAPPDATA"), Some(store));
    for bytes in ui::font_data() {
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Borrowed(bytes));
    }
    let desktop = PathBuf::from(std::env::var_os("SIMPL_QA_DESKTOP").unwrap());
    let output = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    for (filename, prose_page, expected, short) in [
        (
            "Pride_and_prejudice_(IA_prideprejudice00aust_5).pdf",
            49,
            440,
            "pride",
        ),
        ("The_Adventures_of_Sherlock_Holmes.pdf", 20, 359, "sherlock"),
    ] {
        let document = complete(reader_pdf::open(desktop.join(filename))).unwrap();
        let book = Arc::new(
            complete(load_pdf_book(
                document.clone(),
                default_pdf_position(&document),
                20.0,
                false,
            ))
            .unwrap(),
        );
        let conversion = &book.pdf_source.as_ref().unwrap().conversion;
        let prose: std::collections::HashSet<_> = conversion
            .blocks
            .iter()
            .filter(|b| b.text.len() > 80)
            .map(|b| b.sources[0].page)
            .collect();
        let empty: Vec<_> = conversion
            .blocks
            .iter()
            .filter(|b| b.id.ends_with("-empty"))
            .map(|b| b.sources[0].page + 1)
            .collect();
        println!(
            "{short}: {} prose pages / {expected}; {} illustration regions; empty pages {empty:?}",
            prose.len(),
            conversion.illustrations.len()
        );
        std::fs::write(
            output.join(format!("{short}-diagnostics.txt")),
            format!("{conversion:#?}"),
        )
        .unwrap();
        assert!(
            prose.len() * 100 / expected > 80,
            "Too many prose pages failed reconstruction"
        );
        assert!(prose.contains(&prose_page));
        assert!(
            !conversion
                .blocks
                .iter()
                .any(|b| b.sources[0].page == prose_page
                    && conversion.illustrations.contains_key(&b.id)),
            "Text page became an illustration"
        );
        let mut reader = Reader {
            book: Some(book),
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 0,
            fraction: 0.0,
        });
        settle_pagination(&mut reader);
        assert_eq!(reader.page_total(), expected);
        let _ = reader.go_to_local_page(prose_page as usize);
        render(&mut reader, &output.join(format!("{short}-prose.png")));
        assert!(reader.pdf_book_raster.is_none());
        if short == "sherlock" {
            let _ = reader.go_to_local_page(21);
            render(&mut reader, &output.join("sherlock-illustration.png"));
            assert!(
                reader
                    .pdf_book_raster
                    .as_ref()
                    .is_some_and(|(_, page, result)| *page == 21
                        && result.as_ref().is_ok_and(|images| !images.is_empty()))
            );
            assert_eq!(reader.pdf_return_position().unwrap().page, 21);
        }
    }
    let source = desktop.join("lets-go.html");
    let managed = reader_document::managed::import(&source).unwrap();
    let epub = Arc::new(reader_document::epub::open(&managed).unwrap());
    assert_eq!(epub.chapters.len(), 88);
    let mut entry = catalog_entry(
        Entry {
            path: managed.clone(),
            title: epub.title.clone(),
            fingerprint: epub.fingerprint.clone(),
            kind: DocumentKind::Epub,
        },
        epub.author.clone(),
        false,
    );
    assert_eq!(entry.format(), library::SourceFormat::Html);
    entry.source_kind = None;
    library::save(&[entry]).unwrap();
    std::fs::remove_file(managed.parent().unwrap().join(".simpl-source-format")).unwrap();
    let entries = library::load().unwrap();
    assert_eq!(entries[0].format(), library::SourceFormat::Html);
    let mut shelf = Reader::default();
    let _ = shelf.shelf.update(shelf::Message::Loaded(Ok(entries)));
    render(&mut shelf, &output.join("html-library-format.png"));

    assert_eq!(reader_document::managed::import(&source).unwrap(), managed);
    let last = epub.chapters.len() - 1;
    let mut images = 0;
    for index in 0..epub.chapters.len() {
        let chapter = epub.load_chapter(index).unwrap();
        assert!(!chapter.document.items.is_empty());
        images += chapter.document.images.len();
    }
    assert!(images > 0);
    let book = Arc::new(load_epub_chapter(epub.clone(), 2, None).unwrap());
    let mut reader = Reader {
        book: Some(book),
        ..Reader::default()
    };
    let _ = reader.rebuild_geometry(Anchor {
        row: 0,
        fraction: 0.0,
    });
    settle_pagination(&mut reader);
    assert!(reader.page_total() > 100);
    assert!(reader.atlas.as_ref().unwrap().section(last).is_some());
    render(&mut reader, &output.join("html-folder.png"));
    println!(
        "HTML folder: {} chapters, {images} images, {} global pages",
        epub.chapters.len(),
        reader.page_total()
    );
    drop(reader);
    drop(epub);
    reader_document::managed::remove(&managed).unwrap();
    assert!(source.join("index.html").is_file());
    assert!(!managed.exists());
}

#[test]
#[ignore = "Reading-theme QA: set SIMPL_PREVIEW_HTML and SIMPL_PREVIEW_OUTPUT"]
fn render_reading_themes() {
    for bytes in ui::font_data() {
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Borrowed(bytes));
    }
    let output = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_OUTPUT").expect("output directory"));
    std::fs::create_dir_all(&output).unwrap();
    let html = reader_document::load_html(&PathBuf::from(
        std::env::var_os("SIMPL_PREVIEW_HTML").expect("HTML path"),
    ))
    .unwrap();
    let book = Arc::new(display_book(html, None, None));
    let row = std::env::var("SIMPL_PREVIEW_ROW")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
        .min(book.items.len() - 1);
    let mut totals = Vec::new();
    for (index, theme) in themes::THEMES.iter().enumerate() {
        for (appearance, label) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
            let mut reader = Reader {
                book: Some(book.clone()),
                appearance,
                window_size: Size::new(1280.0, 800.0),
                ..Reader::default()
            };
            let _ = reader.rebuild_geometry(Anchor { row, fraction: 0.0 });
            settle_pagination(&mut reader);
            let _ = update_inner(&mut reader, Message::SetTheme(index));
            settle_theme(&mut reader);
            totals.push((theme.id, reader.page_total(), reader.pages().len()));
            render(
                &mut reader,
                &output.join(format!("theme-{}-{label}.png", theme.id)),
            );
        }
    }
    // Every theme, light and dark, must report the same fixed page count.
    assert!(
        totals
            .windows(2)
            .all(|pair| pair[0].1 == pair[1].1 && pair[0].2 == pair[1].2),
        "{totals:?}"
    );
    println!("page counts per theme: {totals:?}");
    for (appearance, label) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
        let mut reader = Reader {
            book: Some(book.clone()),
            appearance,
            show_settings: true,
            window_size: Size::new(1280.0, 800.0),
            ..Reader::default()
        };
        let _ = reader.rebuild_geometry(Anchor { row, fraction: 0.0 });
        settle_pagination(&mut reader);
        render(&mut reader, &output.join(format!("settings-{label}.png")));
    }
}

#[test]
#[ignore = "Release audit visuals: production empty/populated shelves at responsive breakpoints"]
fn render_release_audit_shelves() {
    use reader_document::{library::SourceFormat, recent::DocumentKind};
    let output = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_OUTPUT").expect("output directory"));
    std::fs::create_dir_all(&output).unwrap();
    for bytes in ui::font_data() {
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Borrowed(bytes));
    }
    for populated in [false, true] {
        for width in [540.0, 640.0, 767.0, 768.0, 900.0, 1024.0, 1280.0] {
            for (appearance, name) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
                let mut reader = Reader {
                    window_size: Size::new(width, 800.0),
                    appearance,
                    ..Default::default()
                };
                let mut entries = if populated {
                    shelf::sample_entries(&[
                        ("Notes for a quiet afternoon", DocumentKind::Html),
                        ("Markdown — Café & 日本語", DocumentKind::Html),
                        ("Pride and Prejudice", DocumentKind::Epub),
                        (
                            "A long document title that should fit every supported shelf card without covering the controls",
                            DocumentKind::Pdf,
                        ),
                    ])
                } else {
                    Vec::new()
                };
                if populated {
                    entries[0].source_kind = Some(SourceFormat::Text);
                    entries[1].source_kind = Some(SourceFormat::Markdown);
                }
                let _ = reader.shelf.update(shelf::Message::Loaded(Ok(entries)));
                let _ = reader
                    .shelf
                    .update(shelf::Message::ShelvesLoaded(Ok(Default::default())));
                let _ = reader.shelf.resize(reader.window_size);
                reader.focused = Some(Control::Shelf(shelf::Control::Add));
                render(
                    &mut reader,
                    &output.join(format!(
                        "shelf-{}-{width}-{name}.png",
                        if populated { "populated" } else { "empty" }
                    )),
                );
            }
        }
    }
}

#[test]
fn aa_zoom_buttons_zoom_the_book() {
    let mut reader = full_bar_book(1400.0, 900.0);
    let _ = update(&mut reader, Message::ToggleMenu(reading_chrome::Menu::Aa));
    let mut ui = Harness::new();
    let plus = bounds_of(&mut reader, Control::Zoom(reading_panel::Zoom::In)).center();
    let messages = ui.click(&mut reader, plus);
    assert!(
        messages.iter().any(|m| matches!(m, Message::Zoom(_))),
        "the press sends a zoom: {messages:?}"
    );
    assert_eq!((reader.zoom * 100.0).round(), 110.0);
}
