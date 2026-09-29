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
                let _ = update_inner(&mut reader, Message::ToggleToolbar);
                let after = paper_viewport(&reader);
                assert!(
                    after.bounds.height >= before.bounds.height + 30.0,
                    "toolbar must release its whole height"
                );
                assert_eq!(
                    after.bounds.y, 48.0,
                    "only native title bar remains above reading area"
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
                let _ = update_inner(&mut reader, Message::ToggleToolbar);
            }
            let _ = update_inner(&mut reader, Message::Zoom(0.6));
            render(
                &mut reader,
                &output.join(format!("single-{case}-{requested}-toolbar.png")),
            );
            let _ = update_inner(&mut reader, Message::ToggleToolbar);
            render(
                &mut reader,
                &output.join(format!("single-{case}-{requested}-focus.png")),
            );
            let _ = update_inner(&mut reader, Message::ToggleToolbar);
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
    let offset = reader.local_offset() * reader.zoom;
    let mut element = view(reader);
    let mut tree = Tree::new(&element);
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
    // A redraw event resolves enabled/hover/focus styles just as the runtime does.
    let mut messages = Vec::new();
    element.as_widget_mut().update(
        &mut tree,
        &iced::Event::Window(window::Event::RedrawRequested(std::time::Instant::now())),
        Layout::new(&node),
        mouse::Cursor::Unavailable,
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
        mouse::Cursor::Unavailable,
        &bounds,
    );
    let mut pixels = tiny_skia::Pixmap::new(size.width as u32, size.height as u32).unwrap();
    let mut mask = tiny_skia::Mask::new(size.width as u32, size.height as u32).unwrap();
    renderer.draw(
        &mut pixels.as_mut(),
        &mut mask,
        &iced::advanced::graphics::Viewport::with_physical_size(
            Size::new(size.width as u32, size.height as u32),
            1.0,
        ),
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
        size.width as u32,
        size.height as u32,
        ::image::ColorType::Rgba8,
    )
    .unwrap();
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
                    focused: Some(Control::FontUp),
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
        #[derive(Default)]
        struct Find(Option<Rectangle>);
        impl iced::advanced::widget::Operation for Find {
            fn traverse(
                &mut self,
                visit: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation),
            ) {
                visit(self);
            }
            fn container(&mut self, id: Option<&iced::advanced::widget::Id>, bounds: Rectangle) {
                if id == Some(&iced::advanced::widget::Id::new(ui::FOCUSED_CONTROL)) {
                    self.0 = Some(bounds);
                }
            }
        }
        let mut find = Find::default();
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
    reader.window_controls = WindowControls::Windows;
    for (appearance, name) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
        reader.appearance = appearance;
        render(
            &mut reader,
            &output.join(format!("windows-controls-{name}.png")),
        );
    }
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
                window_controls: if std::env::var_os("SIMPL_PREVIEW_WINDOWS").is_some() {
                    WindowControls::Windows
                } else {
                    WindowControls::Mac
                },
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
            assert!(!reader.controls().any(|c| c == Control::PreviousBookPage));
            let last = reader.pages().last().unwrap().rows.start;
            let _ = reader.rebuild_geometry(Anchor {
                row: last,
                fraction: 0.0,
            });
            settle_pagination(&mut reader);
            let _ = reader.adjacent_book_page(true);
            assert_eq!(reader.anchor().row, last);
            assert!(!reader.controls().any(|c| c == Control::NextBookPage));
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
    assert_eq!(entry.format(), DocumentKind::Html);
    entry.source_kind = None;
    library::save(&[entry]).unwrap();
    std::fs::remove_file(managed.parent().unwrap().join(".simpl-source-format")).unwrap();
    let entries = library::load().unwrap();
    assert_eq!(entries[0].format(), DocumentKind::Html);
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
