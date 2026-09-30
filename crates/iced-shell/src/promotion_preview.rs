//! Reproducible full-window media from production widgets and isolated demo data.
use super::*;
use release_audit::{boot, opened, pump, send};

#[derive(serde::Deserialize)]
struct DemoBook {
    source: String,
    cover: String,
    title: String,
    author: String,
}

fn capture(reader: &mut Reader, output: &Path, name: &str) {
    book_preview::render_at_scale(reader, &output.join(format!("{name}.png")), 2.0);
    println!("Captured {name}: 2560 × 1600, production UI at 2×");
}

#[test]
#[ignore = "Promotion media: isolated STORE=LOCALAPPDATA, FIXTURES and OUTPUT required"]
fn render_promotion_gallery() {
    let store = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_STORE").expect("isolated store"));
    assert_eq!(
        std::env::var_os("LOCALAPPDATA"),
        Some(store.clone().into_os_string())
    );
    assert!(
        !store.join("simPl/library.json").exists(),
        "use a fresh demo profile"
    );
    let fixtures = PathBuf::from(std::env::var_os("SIMPL_PROMOTION_FIXTURES").unwrap());
    let output = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    for bytes in ui::font_data() {
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Borrowed(bytes));
    }
    let books: Vec<DemoBook> =
        serde_json::from_slice(&std::fs::read(fixtures.join("books.json")).unwrap()).unwrap();
    let mut reader = boot();
    reader.window_size = Size::new(1280.0, 800.0);
    let task = reader.shelf.resize(reader.window_size).map(Message::Shelf);
    pump(&mut reader, task);
    send(
        &mut reader,
        Message::SetWindowControls(WindowControls::Windows),
    );
    for (index, demo) in books.iter().enumerate().rev() {
        opened(&mut reader, fixtures.join(&demo.source));
        if index < 3 {
            send(
                &mut reader,
                Message::PageInput(["4", "8", "2"][index].into()),
            );
            send(&mut reader, Message::PageSubmit);
        }
        let task = reader.close(CloseAction::Document);
        pump(&mut reader, task);
    }
    let mut entries = library::load().unwrap();
    for demo in &books {
        let entry = entries
            .iter_mut()
            .find(|entry| {
                entry.document.title == demo.title
                    || entry
                        .document
                        .path
                        .file_stem()
                        .is_some_and(|stem| stem == demo.title.as_str())
            })
            .unwrap();
        entry.document.title = demo.title.clone();
        entry.author = Some(demo.author.clone());
        let image = ::image::open(fixtures.join(&demo.cover))
            .unwrap()
            .into_rgba8();
        library::cache_cover(
            &entry.document.fingerprint,
            &reader_document::ImageAsset {
                width: image.width(),
                height: image.height(),
                rgba: image.into_raw(),
            },
        )
        .unwrap();
        entry.cover = true;
    }
    library::save(&entries).unwrap();
    reader = boot();
    reader.window_size = Size::new(1280.0, 800.0);
    let task = reader.shelf.resize(reader.window_size).map(Message::Shelf);
    pump(&mut reader, task);
    reader.focused = None;
    reader.appearance = Appearance::Light;
    capture(&mut reader, &output, "library-light");
    reader.appearance = Appearance::Dark;
    capture(&mut reader, &output, "library-dark");

    opened(&mut reader, fixtures.join(&books[0].source));
    send(&mut reader, Message::PageInput("1".into()));
    send(&mut reader, Message::PageSubmit);
    for (theme, name) in [(0, "default"), (1, "soft"), (2, "clear"), (3, "compact")] {
        send(&mut reader, Message::SetTheme(theme));
        settle_theme(&mut reader);
        for (appearance, tone) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
            reader.appearance = appearance;
            capture(&mut reader, &output, &format!("reading-{name}-{tone}"));
        }
    }
    send(&mut reader, Message::SetTheme(0));
    settle_theme(&mut reader);
    reader.appearance = Appearance::Dark;
    capture(&mut reader, &output, "reading-dark");
    reader.appearance = Appearance::Light;

    let book = reader.book.as_ref().unwrap().clone();
    let (row, source) = book
        .items
        .iter()
        .enumerate()
        .find_map(|(row, item)| {
            item.text()
                .filter(|s| s.starts_with("The book lay open"))
                .map(|s| (row, s.to_owned()))
        })
        .unwrap();
    let id = book.items[row].id().to_owned();
    let quote = "The book lay open on the kitchen table";
    let mark = reader
        .notes
        .data
        .as_mut()
        .unwrap()
        .add_highlight(
            reader_document::annotations::Place::Reflow {
                chapter: book
                    .epub
                    .as_ref()
                    .map(|e| e.document.chapters[e.index].href.clone()),
                from: reader_document::annotations::ReflowPoint {
                    item_id: id.clone(),
                    byte: 0,
                },
                to: reader_document::annotations::ReflowPoint {
                    item_id: id.clone(),
                    byte: quote.len(),
                },
            },
            reader_document::annotations::HighlightColor::Yellow,
            "1".into(),
            quote.into(),
        )
        .unwrap();
    reader
        .notes
        .data
        .as_mut()
        .unwrap()
        .set_note(
            mark,
            "A quiet opening. Notice how the garden appears one detail at a time.",
        )
        .unwrap();
    reader.refresh_marks();
    send(&mut reader, Message::Notes(notes::Action::ToggleBookmark));
    reader.notes.list = Some(notes::ListTab::Highlights);
    capture(&mut reader, &output, "notes-highlights");
    reader.notes.list = None;

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    reader
        .word_translation
        .store
        .import(
            &root.join("assets/dictionaries/packs/en-tr-2026-09-30.zip"),
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
    let begin = source.find("book").unwrap();
    reader.selection.begin(Endpoint {
        item_id: id.clone(),
        byte_offset: begin,
    });
    reader.selection.extend(Endpoint {
        item_id: id.clone(),
        byte_offset: begin + 4,
    });
    reader.selection.end_drag();
    reader.pointer = iced::Point::new(690.0, 230.0);
    let task = reader.translate_selection();
    pump(&mut reader, task);
    assert!(reader.word_translation.popup.is_some());
    capture(&mut reader, &output, "word-translation");
    reader.word_translation.dismiss();
    reader.selection.clear();

    reader.mute_preview_speech();
    send(&mut reader, Message::ReadAloud(read_aloud::Action::Toggle));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
    while std::time::Instant::now() < deadline {
        send(&mut reader, Message::ReadAloudTick);
        if reader.spoken_word(&id).is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        reader.spoken_word(&id).is_some(),
        "real SAPI must report a spoken word"
    );
    send(&mut reader, Message::ReadAloud(read_aloud::Action::Pause));
    capture(&mut reader, &output, "listen-word-highlight");
    send(&mut reader, Message::ReadAloud(read_aloud::Action::Toggle));

    send(&mut reader, Message::Chrome(chrome::Action::Settings));
    reader.focused = None;
    capture(&mut reader, &output, "theme-settings");
    reader.focused = Some(Control::Reading(reading_ui::Focus::Reset));
    capture(&mut reader, &output, "reading-settings");
    send(
        &mut reader,
        Message::Dictionary(word_translation::Action::ManagePackages),
    );
    reader.focused = Some(Control::Dictionary(word_translation::Focus::Download(
        reader_document::dictionary::PackageId(5),
    )));
    capture(&mut reader, &output, "dictionary-downloads");
    reader.focused = Some(Control::Dictionary(word_translation::Focus::Download(
        reader_document::dictionary::PackageId(12),
    )));
    capture(&mut reader, &output, "dictionary-downloads-asian");
    send(
        &mut reader,
        Message::Dictionary(word_translation::Action::ManagePackages),
    );
    reader.focused = Some(Control::Profile(profile_ui::Focus::Export));
    capture(&mut reader, &output, "backup-export");
    send(&mut reader, Message::Chrome(chrome::Action::Settings));
    reader.focused = None;
    reader.window = Some(window::Id::unique());
    send(&mut reader, Message::ToggleFullscreen);
    assert!(reader.fullscreen);
    capture(&mut reader, &output, "reading-fullscreen");
    send(&mut reader, Message::ToggleFullscreen);
    let task = reader.close(CloseAction::Document);
    pump(&mut reader, task);

    send(&mut reader, Message::Chrome(chrome::Action::Settings));
    reader.focused = Some(Control::Profile(profile_ui::Focus::Restore));
    capture(&mut reader, &output, "library-data");
    send(&mut reader, Message::Chrome(chrome::Action::Settings));

    opened(&mut reader, fixtures.join(&books[5].source));
    let document = reader.pdf.as_ref().unwrap().document().id;
    send(
        &mut reader,
        Message::Pdf {
            document,
            message: pdf_reader::Message::ActualSize,
        },
    );
    send(
        &mut reader,
        Message::Pdf {
            document,
            message: pdf_reader::Message::ZoomBy(-5.0),
        },
    );
    reader.focused = None;
    capture(&mut reader, &output, "pdf-document");
    send(&mut reader, Message::BookMode);
    capture(&mut reader, &output, "pdf-book");
}
