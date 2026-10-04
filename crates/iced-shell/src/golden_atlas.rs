//! The desktop's canonical page atlases and theme layouts for the golden books of
//! `reader_layout::golden` must equal the recorded `reader-layout/tests/golden`
//! files. `SIMPL_RECORD_GOLDEN=1` rewrites them instead (only when the layout
//! is meant to change; the files keep page numbers stable across platforms).
use super::*;
use reader_layout::golden::{self, Kind};

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reader-layout/tests/golden")
}

fn options(font: &str, size: u16, margin: u16, spacing: u16) -> reader_document::reading::Options {
    use reader_document::reading::Font;
    reader_document::reading::Options {
        font: match font {
            "literata" => Font::Literata,
            "spectral" => Font::Spectral,
            "fira-sans" => Font::FiraSans,
            _ => Font::Theme,
        },
        size,
        spacing,
        margin,
    }
}

/// The book a section is read from, as the desktop opens it.
fn section_book(first: &Arc<Book>, section: usize) -> Arc<Book> {
    match &first.epub {
        Some(epub) if section != epub.index => {
            Arc::new(load_epub_chapter(epub.document.clone(), section, None).unwrap())
        }
        _ => first.clone(),
    }
}

fn open(fixture: &golden::Fixture) -> Option<Arc<Book>> {
    Some(Arc::new(match fixture.kind {
        Kind::Html => display_book(
            reader_document::load_html(&fixture.path).unwrap(),
            None,
            None,
        ),
        Kind::Epub => {
            let document = Arc::new(reader_document::epub::open(&fixture.path).unwrap());
            load_epub_chapter(document, 0, None).unwrap()
        }
        Kind::Pdf => {
            use iced_futures::futures::executor::block_on;
            let document = match block_on(reader_pdf::open(fixture.path.clone())) {
                Ok(document) => document,
                // Like the PDFium text test: Windows test runs have no bundled DLL.
                Err(error) if error.contains("PDFium") => return None,
                Err(error) => panic!("{}: {error}", fixture.name),
            };
            let position = PdfReadingPosition {
                fingerprint: document.fingerprint.clone(),
                page: 0,
                within: 0.0,
                horizontal: 0.0,
                zoom: position::PdfZoom::FitWidth,
            };
            block_on(load_pdf_book(document, position, DEFAULT_FONT_SIZE, false)).unwrap()
        }
    }))
}

fn record(fixture: &golden::Fixture) -> Option<serde_json::Value> {
    let book = open(fixture)?;
    let cancel = AtomicBool::new(false);
    let atlas = book_map::build(book.clone(), &cancel).unwrap().unwrap();
    let mut adapted = Vec::new();
    // PDF Book always keeps the default typography; only reflowable books adapt.
    if book.pdf_source.is_none() {
        for (theme, font, size, margin, spacing) in golden::ADAPTATIONS {
            let sections = (0..atlas.sections.len())
                .map(|section| {
                    book_map::adapt_section_with(
                        section_book(&book, section),
                        &atlas.sections[section],
                        themes::find(theme),
                        options(font, size, margin, spacing),
                        &cancel,
                    )
                    .unwrap()
                    .unwrap()
                })
                .collect::<Vec<_>>();
            adapted.push(serde_json::json!({
                "layout": format!("{theme}/{font}/{size}/{margin}/{spacing}"),
                "sections": sections,
            }));
        }
    }
    Some(serde_json::json!({ "atlas": atlas, "adapted": adapted }))
}

#[test]
fn desktop_page_atlases_match_the_golden_books() {
    ui::load_test_fonts();
    let scratch = std::env::temp_dir().join(format!("simpl-golden-shell-{}", std::process::id()));
    let fixtures = golden::write_all(&scratch).unwrap();
    let recording = std::env::var_os("SIMPL_RECORD_GOLDEN").is_some_and(|v| v == "1");
    let mut differences = Vec::new();
    for fixture in &fixtures {
        let Some(value) = record(fixture) else {
            eprintln!(
                "skipped {}: PDFium is not beside the test executable",
                fixture.name
            );
            continue;
        };
        let text = serde_json::to_string_pretty(&value).unwrap() + "\n";
        let path = golden_dir().join(format!("{}.json", fixture.name));
        if recording {
            std::fs::create_dir_all(golden_dir()).unwrap();
            std::fs::write(&path, &text).unwrap();
        } else if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
            let actual = path.with_extension("actual.json");
            std::fs::write(&actual, &text).unwrap();
            differences.push(format!("{} (see {})", fixture.name, actual.display()));
        }
    }
    let _ = std::fs::remove_dir_all(scratch);
    assert!(
        differences.is_empty(),
        "page atlases differ from the golden books: {differences:?}"
    );
}

/// Keep the selectable desktop widgets and the window-free path in agreement,
/// including system-font blocks and every retained theme/option combination.
#[test]
fn shared_measurement_matches_the_desktop_widgets() {
    use iced::advanced::{layout, widget::Tree};
    ui::load_test_fonts();
    let scratch = std::env::temp_dir().join(format!("simpl-golden-widgets-{}", std::process::id()));
    for fixture in golden::write_all(&scratch).unwrap() {
        let Some(book) = open(&fixture) else {
            continue;
        };
        let count = book.epub.as_ref().map_or(1, |e| e.document.chapters.len());
        for section in 0..count {
            let current = section_book(&book, section);
            for (theme, font, size, margin, spacing) in golden::ADAPTATIONS {
                let theme = themes::find(theme);
                let options = options(font, size, margin, spacing);
                let width = book_map::PAPER - margin as f32 * 2.0;
                let body = size as f32;
                let shared = reader_layout::measure::measure_book_with(
                    current.clone(),
                    width,
                    body,
                    &AtomicBool::new(false),
                    theme,
                    options,
                )
                .unwrap();
                let mut reader = Reader {
                    width,
                    font_size: body,
                    theme,
                    ..Reader::default()
                };
                reader.reading.defaults = options;
                let style = reading_ui::effective_style(theme, &current, options);
                let renderer = iced::Renderer::new(ui::SANS, iced::Pixels(13.0));
                let limits = layout::Limits::new(Size::ZERO, Size::new(width, f32::INFINITY));
                let desktop = current
                    .items
                    .iter()
                    .enumerate()
                    .map(|(row, item)| {
                        let mut element = render_item(&reader, &current, row, item, None);
                        let mut tree = Tree::new(&element);
                        element
                            .as_widget_mut()
                            .layout(&mut tree, &renderer, &limits)
                            .size()
                            .height
                            + if row + 1 == current.items.len() {
                                0.0
                            } else {
                                style.gap(body)
                            }
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    shared, desktop,
                    "{} section {section}, {}",
                    fixture.name, theme.id
                );
            }
        }
    }
    std::fs::remove_dir_all(scratch).unwrap();
}
