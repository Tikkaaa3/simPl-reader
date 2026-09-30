//! Opt-in, isolated production application workflows; no personal books or OS input.
use super::*;
use iced_futures::futures::{StreamExt, executor::block_on};
use iced_runtime::{Action as RuntimeAction, task::into_stream};

pub(super) fn pump(reader: &mut Reader, task: Task<Message>) {
    block_on(async {
        let mut queue = std::collections::VecDeque::new();
        queue.extend(into_stream(task));
        let mut count = 0;
        while let Some(mut stream) = queue.pop_front() {
            while let Some(action) = stream.next().await {
                count += 1;
                assert!(count < 2000, "application tasks must settle without a loop");
                if let RuntimeAction::Output(message) = action {
                    queue.extend(into_stream(update(reader, message)));
                }
                // Native window/clipboard/widget operations are outside this
                // state/storage probe; production widget layout is rendered below.
            }
        }
    });
}
pub(super) fn send(reader: &mut Reader, message: Message) {
    let task = update(reader, message);
    pump(reader, task);
}
pub(super) fn boot() -> Reader {
    let mut reader = Reader::default();
    pump(
        &mut reader,
        Task::batch([
            Task::perform(async { preferences::load() }, Message::PreferencesLoaded),
            Task::perform(async { recent::load() }, Message::RecentLoaded),
            shelf::Shelf::load().map(Message::Shelf),
        ]),
    );
    reader
}
pub(super) fn opened(reader: &mut Reader, source: PathBuf) {
    let started = std::time::Instant::now();
    let task = reader.open_with_import(source.clone(), None, true);
    pump(reader, task);
    settle_pagination(reader);
    assert!(reader.opening.is_none() && reader.pagination.is_none());
    assert!(
        reader.error.is_none(),
        "{}: {:?}",
        source.display(),
        reader.error
    );
    assert!(reader.book.is_some() || reader.pdf.is_some());
    println!(
        "Opened {} in {:?} (state/storage, excludes display)",
        source.display(),
        started.elapsed()
    );
}

#[test]
#[ignore = "Isolated release QA: OUTPUT and STORE=LOCALAPPDATA; authored EPUB/PDF fixtures required"]
fn production_format_workflows() {
    use reader_document::library::SourceFormat;
    let store = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_STORE").expect("isolated store"));
    assert_eq!(
        std::env::var_os("LOCALAPPDATA"),
        Some(store.clone().into_os_string())
    );
    assert!(
        !store.join("simPl/library.json").exists(),
        "use a fresh profile"
    );
    let output = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    for bytes in ui::font_data() {
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Borrowed(bytes));
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sources = output.join("sources");
    std::fs::create_dir_all(&sources).unwrap();
    let txt = sources.join("Café reading notes.txt");
    let md = sources.join("Reading checklist.md");
    std::fs::write(
        &txt,
        "A book is a place to think. Café, İstanbul, العربية and 日本語.\n\n".repeat(120),
    )
    .unwrap();
    std::fs::write(&md, "# Reading checklist\n\nA **book** with _emphasis_, a [local link](#second) and text.\n\n## Second\n\n- One idea\n- Another idea\n\n> A calm quotation.\n").unwrap();
    let mut reader = boot();
    for (name, source, format, query) in [
        ("txt", txt.clone(), SourceFormat::Text, "book"),
        ("markdown", md, SourceFormat::Markdown, "book"),
        (
            "html",
            root.join("fixtures/book-structure/structured.html"),
            SourceFormat::Html,
            "page",
        ),
        (
            "epub",
            root.join("target/book-milestone2/structured.epub"),
            SourceFormat::Epub,
            "page",
        ),
        (
            "pdf",
            root.join("target/book-milestone3/fixtures/prose.pdf"),
            SourceFormat::Pdf,
            "book",
        ),
    ] {
        let original = std::fs::read(&source).unwrap();
        opened(&mut reader, source.clone());
        let path = reader
            .book
            .as_ref()
            .map(|book| book.path.clone())
            .unwrap_or_else(|| reader.pdf.as_ref().unwrap().document().path.clone());
        let index = reader
            .shelf
            .entries
            .iter()
            .position(|entry| entry.document.path == path)
            .unwrap();
        assert_eq!(reader.shelf.entries[index].format(), format);
        send(
            &mut reader,
            Message::Shelf(shelf::Message::Activate(shelf::Control::Favourite(
                index, false,
            ))),
        );
        assert!(
            library::load()
                .unwrap()
                .iter()
                .any(|entry| entry.document.path == path && entry.favourite)
        );
        send(&mut reader, Message::FindOpen);
        send(&mut reader, Message::FindChanged(query.into()));
        let found = reader.find.as_ref().unwrap();
        assert!(
            !found.matches.is_empty() || !found.pdf_matches.is_empty(),
            "{name}: search should find {query}"
        );
        for width in [540.0, 900.0, 1280.0] {
            send(
                &mut reader,
                Message::Event(
                    iced::Event::Window(window::Event::Resized(Size::new(width, 800.0))),
                    window::Id::unique(),
                ),
            );
            for (appearance, theme) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
                reader.appearance = appearance;
                book_preview::render(
                    &mut reader,
                    &output.join(format!("{name}-{width}-{theme}.png")),
                );
            }
        }
        send(&mut reader, Message::FindClose);
        assert!(reader.find.is_none());
        send(&mut reader, Message::Notes(notes::Action::ToggleBookmark));
        assert!(!reader.notes.data.as_ref().unwrap().bookmarks.is_empty());
        send(&mut reader, Message::Notes(notes::Action::ToggleList));
        book_preview::render(&mut reader, &output.join(format!("{name}-bookmarks.png")));
        send(&mut reader, Message::Notes(notes::Action::ToggleList));
        if name == "pdf" {
            send(&mut reader, Message::BookMode);
            settle_pagination(&mut reader);
            assert!(
                reader
                    .book
                    .as_ref()
                    .is_some_and(|book| book.pdf_source.is_some())
            );
            assert_eq!(reader.page_total(), 5);
            book_preview::render(&mut reader, &output.join("pdf-book.png"));
            send(&mut reader, Message::DocumentMode { source_page: true });
            assert!(reader.pdf.is_some());
        } else {
            let total = reader.page_total();
            for theme in 0..themes::THEMES.len() {
                send(&mut reader, Message::SetTheme(theme));
                settle_theme(&mut reader);
                assert_eq!(
                    reader.page_total(),
                    total,
                    "theme must preserve global page numbering"
                );
            }
            if name == "txt" {
                assert!(total > 1);
                send(&mut reader, Message::BookPage(true));
                assert_eq!(reader.active_page().unwrap().number, 1);
            }
        }
        let task = reader.close(CloseAction::Document);
        pump(&mut reader, task);
        assert!(reader.book.is_none() && reader.pdf.is_none());
        assert_eq!(std::fs::read(&source).unwrap(), original);
    }
    drop(reader);
    let mut reopened = boot();
    assert_eq!(reopened.shelf.entries.len(), 5);
    assert!(reopened.shelf.entries.iter().all(|entry| entry.favourite));
    opened(&mut reopened, txt);
    assert_eq!(reopened.active_page().unwrap().number, 1);
    assert!(!reopened.notes.data.as_ref().unwrap().bookmarks.is_empty());
    let title = reopened.book.as_ref().unwrap().title.clone();
    let broken = sources.join("broken.epub");
    std::fs::write(&broken, b"not a zip").unwrap();
    let task = reopened.open_with_import(broken, None, true);
    pump(&mut reopened, task);
    assert!(reopened.error.is_some());
    assert_eq!(reopened.book.as_ref().unwrap().title, title);
    book_preview::render(&mut reopened, &output.join("bad-open-keeps-book.png"));
    opened(
        &mut reopened,
        root.join("target/book-milestone3/fixtures/restricted.pdf"),
    );
    send(&mut reopened, Message::BookMode);
    assert!(reopened.pdf.is_some());
    assert!(
        reopened
            .error
            .as_ref()
            .is_some_and(|error| error.contains("permissions"))
    );
    book_preview::render(
        &mut reopened,
        &output.join("restricted-pdf-keeps-original.png"),
    );
    println!(
        "All five formats: imports, retained labels, find, favourites, bookmarks, themes, reopen/resume, errors and PDF modes passed"
    );
}

#[test]
#[ignore = "Isolated portability QA: fresh STORE=LOCALAPPDATA and OUTPUT outside profile"]
fn production_backup_restore_and_export() {
    use reader_document::annotations::{HighlightColor, Place, ReflowPoint};
    use reader_document::backup::{self, ExportFormat};

    let store = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_STORE").expect("isolated store"));
    assert_eq!(
        std::env::var_os("LOCALAPPDATA"),
        Some(store.clone().into_os_string())
    );
    assert!(
        !store.join("simPl/library.json").exists(),
        "use a fresh profile"
    );
    let output = PathBuf::from(std::env::var_os("SIMPL_PREVIEW_OUTPUT").expect("output"));
    std::fs::create_dir_all(&output).unwrap();
    ui::load_test_fonts();
    let source = output.join("İstanbul reading.md");
    let original = "A book is a place to think. Café, İstanbul, 日本語 and 한국어.\n\n".repeat(120);
    std::fs::write(&source, &original).unwrap();
    let mut reader = boot();
    send(&mut reader, Message::Reading(reading_ui::Action::Size(22)));
    opened(&mut reader, source.clone());
    send(&mut reader, Message::Reading(reading_ui::Action::Size(26)));
    settle_theme(&mut reader);
    let book = reader.book.as_ref().unwrap();
    let fingerprint = book.fingerprint.clone();
    let managed = book.path.clone();
    let title = book.title.clone();
    let item = book
        .items
        .iter()
        .find(|item| item.text().is_some())
        .unwrap();
    let quote = item.text().unwrap()[..6].to_string();
    let place = Place::Reflow {
        chapter: None,
        from: ReflowPoint {
            item_id: item.id().into(),
            byte: 0,
        },
        to: ReflowPoint {
            item_id: item.id().into(),
            byte: 6,
        },
    };
    send(&mut reader, Message::Notes(notes::Action::ToggleBookmark));
    let annotations = reader.notes.data.as_mut().unwrap();
    let id = annotations
        .add_highlight(place, HighlightColor::Yellow, "1".into(), quote)
        .unwrap();
    annotations
        .set_note(id, "Kendi notum — français 😀")
        .unwrap();
    let expected = annotations.clone();
    reader.notes.changed();
    let task = reader.persist_notes();
    pump(&mut reader, task);
    let index = reader
        .shelf
        .entries
        .iter()
        .position(|entry| entry.document.path == managed)
        .unwrap();
    for action in [
        shelf::Message::Activate(shelf::Control::Favourite(index, false)),
        shelf::Message::Activate(shelf::Control::MenuNewShelf(index, false)),
        shelf::Message::NameInput("Okunacak — 日本語".into()),
        shelf::Message::NameSubmit,
    ] {
        send(&mut reader, Message::Shelf(action));
    }
    assert!(
        reader_document::shelves::load().unwrap().shelves[0]
            .books
            .contains(&fingerprint)
    );
    assert!(reader.page_total() > 1);
    send(&mut reader, Message::BookPage(true));
    assert_eq!(reader.active_page().unwrap().number, 1);
    for (extension, format) in [
        ("md", ExportFormat::Markdown),
        ("txt", ExportFormat::Text),
        ("json", ExportFormat::Json),
    ] {
        let path = output.join(format!("notes.{extension}"));
        backup::export_annotations(&path, &title, &expected, format).unwrap();
        assert!(
            std::fs::read_to_string(path)
                .unwrap()
                .contains("Kendi notum")
        );
    }
    let task = reader.close(CloseAction::Document);
    pump(&mut reader, task);
    assert!(reader.profile_ready());
    let archive = output.join("profile.zip");
    backup::create(
        &archive,
        backup::Options {
            documents: true,
            dictionaries: false,
        },
    )
    .unwrap();
    // Change the real profile, then exercise the same reviewed restore task and
    // application reload used by the Settings confirmation button.
    send(&mut reader, Message::Reading(reading_ui::Action::Size(30)));
    let other = output.join("later.txt");
    std::fs::write(&other, "A later book outside the saved snapshot.").unwrap();
    opened(&mut reader, other);
    let task = reader.close(CloseAction::Document);
    pump(&mut reader, task);
    assert_eq!(reader.shelf.entries.len(), 2);
    send(
        &mut reader,
        Message::ProfileReply(Ok(profile_ui::Reply::Inspected(
            archive.clone(),
            backup::inspect(&archive).unwrap(),
        ))),
    );
    assert!(reader.profile_ready());
    send(&mut reader, Message::Profile(profile_ui::Action::Confirm));
    assert!(!reader.profile.busy && reader.profile.pending.is_none());
    assert_eq!(reader.shelf.entries.len(), 1);
    assert!(reader.shelf.entries[0].favourite);
    assert_eq!(reader.reading.defaults.size, 22);
    assert!(
        reader
            .profile
            .notice
            .as_ref()
            .unwrap()
            .contains("Previous profile kept")
    );
    assert!(std::fs::read_dir(&store).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".simPl-before-restore-")
    }));
    opened(&mut reader, managed);
    assert_eq!(reader.reading_options().size, 26);
    assert_eq!(reader.active_page().unwrap().number, 1);
    assert_eq!(reader.notes.data.as_ref().unwrap(), &expected);
    assert!(
        reader_document::shelves::load().unwrap().shelves[0]
            .books
            .contains(&fingerprint)
    );
    assert_eq!(std::fs::read_to_string(source).unwrap(), original);
    let task = reader.close(CloseAction::Document);
    pump(&mut reader, task);
    assert!(reader.profile_ready());
    println!(
        "Real import/bookmark/save/export/backup/reviewed restore/reload/reopen preserved reading data and left original source untouched"
    );
}
