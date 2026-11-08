use super::release_audit::{boot, send};
use super::*;
use std::fs;
use std::process::Command;

const STORE: &str = "SIMPL_ISOLATED_STORE";
const PREFIX: &str = "simpl-isolated-";

fn run_isolated(name: &str) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "{PREFIX}{}-{}-{nanos}",
        name.replace("::", "-"),
        std::process::id()
    ));
    let profile = root.join("profile");
    fs::create_dir_all(&profile).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            name,
            "--test-threads=1",
            "--nocapture",
        ])
        .env("LOCALAPPDATA", &profile)
        .env(STORE, &root)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // `--exact` with a wrong name runs nothing and still exits successfully.
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "{name} failed; profile kept at {}\n{stdout}\n{}",
        root.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    fs::remove_dir_all(&root).unwrap();
}

fn store() -> (PathBuf, PathBuf) {
    let root = PathBuf::from(std::env::var_os(STORE).expect("run through run_isolated"));
    assert!(
        root.parent() == Some(std::env::temp_dir().as_path())
            && root
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(PREFIX)),
        "isolated store must be a fresh temp folder, not {}",
        root.display()
    );
    let profile = root.join("profile");
    assert_eq!(
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
        Some(profile.clone())
    );
    assert_eq!(
        reader_document::managed::root(),
        profile.join("simPl").join("documents")
    );
    let books = root.join("books");
    fs::create_dir_all(&books).unwrap();
    (profile, books)
}

macro_rules! isolated {
    ($name:ident) => {
        mod $name {
            use super::*;

            #[test]
            fn run() {
                let path = module_path!().split_once("::").unwrap().1;
                run_isolated(&format!("{path}::child"));
            }

            #[test]
            #[ignore = "runs in an isolated profile through run()"]
            fn child() {
                super::$name();
            }
        }
    };
}

fn item_texts(book: &Book) -> Vec<&str> {
    book.items.iter().filter_map(Item::text).collect()
}

fn dropped(path: &Path) -> Message {
    Message::Event(
        iced::Event::Window(window::Event::FileDropped(path.to_path_buf())),
        window::Id::unique(),
    )
}

fn names(folder: &Path) -> Vec<String> {
    let mut names = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn recent_paths(reader: &Reader) -> Vec<PathBuf> {
    reader
        .recent
        .iter()
        .map(|entry| entry.path.clone())
        .collect()
}

isolated!(library_card_outside_the_library_opens_in_place_without_a_copy);
fn library_card_outside_the_library_opens_in_place_without_a_copy() {
    let (profile, books) = store();
    let source = books.join("legacy.html");
    fs::write(&source, "<title>Legacy</title><h1>Legacy</h1>").unwrap();
    let seeded = reader_document::load_html(&source).unwrap();
    library::save(&[library::Entry {
        document: Entry {
            path: seeded.path.clone(),
            title: "Legacy".into(),
            fingerprint: seeded.fingerprint.clone(),
            kind: DocumentKind::Html,
        },
        author: None,
        byte_len: 36,
        opened_at: 0,
        progress: 0.0,
        current: 0,
        total: 0,
        cover: false,
        favourite: true,
        source_kind: None,
    }])
    .unwrap();
    fs::write(&source, "<title>Legacy</title><h1>Legacy</h1><p>edited</p>").unwrap();

    let mut reader = boot();
    send(&mut reader, Message::OpenLibrary(0));

    assert_eq!(reader.error, None);
    let canonical = fs::canonicalize(&source).unwrap();
    let book = reader.book.as_ref().unwrap();
    assert_eq!(book.title, "Legacy");
    assert_eq!(item_texts(book), ["Legacy", "edited"]);
    assert_eq!(book.path, canonical);
    assert_ne!(book.fingerprint, seeded.fingerprint);
    assert!(!profile.join("simPl").join("documents").exists());
    assert_eq!(reader.shelf.entries.len(), 1);
    assert_eq!(reader.shelf.entries[0].document.path, canonical);
    assert_eq!(
        reader.shelf.entries[0].document.fingerprint,
        book.fingerprint
    );
    assert!(reader.shelf.entries[0].favourite);
    assert_eq!(reader.recent.len(), 1);
    assert_eq!(reader.recent[0].path, canonical);
}

isolated!(dropped_markdown_opens_in_place_without_a_card_or_files);
fn dropped_markdown_opens_in_place_without_a_card_or_files() {
    let (profile, books) = store();
    let source = books.join("Notes.md");
    fs::write(&source, "# Field notes\n\nText.\n").unwrap();

    let mut reader = boot();
    send(&mut reader, dropped(&source));

    assert_eq!(reader.error, None);
    assert_eq!(reader.book.as_ref().unwrap().title, "Field notes");
    let canonical = fs::canonicalize(&source).unwrap();
    assert_eq!(reader.shelf.entries.len(), 0);
    assert_eq!(recent_paths(&reader), [canonical]);
    assert_eq!(reader.recent[0].kind, DocumentKind::Html);
    assert!(!profile.join("simPl").join("documents").exists());
    assert_eq!(names(&books), ["Notes.md"]);
    drop(reader);
    let reopened = boot();
    assert_eq!(reopened.shelf.entries.len(), 0);
    assert_eq!(reopened.recent.len(), 1);
}

isolated!(dropped_html_folder_is_imported_with_a_card);
fn dropped_html_folder_is_imported_with_a_card() {
    let (profile, books) = store();
    let bundle = books.join("html-book");
    fs::create_dir(&bundle).unwrap();
    let first = "<title>My Book</title><h1>First</h1><a href='02.html#end'>Next</a>";
    fs::write(bundle.join("index.html"), first).unwrap();
    fs::write(bundle.join("01.html"), first).unwrap();
    fs::write(bundle.join("02.html"), "<h1 id='end'>Last chapter</h1>").unwrap();

    let mut reader = boot();
    send(&mut reader, dropped(&bundle));

    assert_eq!(reader.error, None);
    assert_eq!(reader.shelf.entries.len(), 1);
    let path = reader.shelf.entries[0].document.path.clone();
    let documents = fs::canonicalize(&profile)
        .unwrap()
        .join("simPl")
        .join("documents");
    assert!(path.starts_with(&documents), "{}", path.display());
    assert_eq!(path.file_name().unwrap(), "book.epub");
    assert_eq!(recent_paths(&reader), [path]);
    assert_eq!(names(&bundle), ["01.html", "02.html", "index.html"]);
}

isolated!(library_locate_imports_and_recent_locate_does_not);
fn library_locate_imports_and_recent_locate_does_not() {
    let (profile, books) = store();
    let documents = profile.join("simPl").join("documents");
    let moved = books.join("moved");
    fs::create_dir(&moved).unwrap();
    let chosen = |path: &Path| Message::DialogChosen(Ok(Some(path.to_path_buf())));
    let a = books.join("a.html");
    fs::write(&a, "<title>A</title><p>Alpha</p>").unwrap();
    let mut reader = boot();
    let _ = update_inner(&mut reader, Message::ChooseFile(Storage::Import));
    send(&mut reader, chosen(&a));
    let first_copy = reader.shelf.entries[0].document.path.clone();
    assert!(first_copy.starts_with(fs::canonicalize(&documents).unwrap()));
    fs::remove_file(&first_copy).unwrap();
    fs::copy(&a, moved.join("a.html")).unwrap();

    let _ = update_inner(&mut reader, Message::LocateLibrary(0));
    send(&mut reader, chosen(&moved.join("a.html")));

    assert_eq!(reader.error, None);
    assert_eq!(reader.shelf.entries.len(), 1);
    let copy = reader.shelf.entries[0].document.path.clone();
    assert!(copy.starts_with(fs::canonicalize(&documents).unwrap()));
    assert_ne!(copy, first_copy);
    assert_eq!(recent_paths(&reader), std::slice::from_ref(&copy));

    let b = books.join("b.html");
    fs::write(&b, "<title>B</title><p>Beta</p>").unwrap();
    send(&mut reader, dropped(&b));
    fs::rename(&b, moved.join("b.html")).unwrap();
    let _ = update_inner(&mut reader, Message::LocateRecent(0));
    send(&mut reader, chosen(&moved.join("b.html")));

    assert_eq!(reader.error, None);
    assert_eq!(
        recent_paths(&reader),
        [
            fs::canonicalize(moved.join("b.html")).unwrap(),
            copy.clone()
        ]
    );
    assert_eq!(reader.shelf.entries.len(), 1);
    assert_eq!(reader.shelf.entries[0].document.path, copy);
    assert_eq!(names(&documents).len(), 2);
}

isolated!(library_locate_of_another_format_leaves_no_copy);
fn library_locate_of_another_format_leaves_no_copy() {
    let (profile, books) = store();
    let documents = profile.join("simPl").join("documents");
    let chosen = |path: &Path| Message::DialogChosen(Ok(Some(path.to_path_buf())));
    let a = books.join("a.html");
    fs::write(&a, "<title>A</title><p>Alpha</p>").unwrap();
    let wrong = books.join("a.pdf");
    fs::write(&wrong, b"%PDF-1.4").unwrap();
    let mut reader = boot();
    let _ = update_inner(&mut reader, Message::ChooseFile(Storage::Import));
    send(&mut reader, chosen(&a));

    let _ = update_inner(&mut reader, Message::LocateLibrary(0));
    send(&mut reader, chosen(&wrong));

    let error = reader.error.clone().unwrap();
    assert!(
        error.contains("Selected file is not the same format as"),
        "{error}"
    );
    assert_eq!(names(&documents).len(), 1);
    assert_eq!(reader.shelf.entries.len(), 1);
}

fn add_to_library_keeps_the_position(close: bool) {
    let (profile, books) = store();
    let source = books.join("story.txt");
    let story = (0..40)
        .map(|n| format!("Paragraph {n} of a story that is long enough to wrap.\n\n"))
        .collect::<String>();
    fs::write(&source, story).unwrap();
    let mut reader = boot();
    send(&mut reader, dropped(&source));
    let _ = reader.rebuild_geometry(Anchor {
        row: 20,
        fraction: 0.25,
    });
    settle_pagination(&mut reader);
    let id = reader.book.as_ref().unwrap().items[20].id().to_owned();
    if close {
        let task = reader.close(CloseAction::Document);
        super::release_audit::pump(&mut reader, task);
        assert!(reader.book.is_none());
    }

    send(&mut reader, Message::AddRecent(0));

    assert_eq!(reader.error, None);
    let book = reader.book.as_ref().unwrap();
    let documents = fs::canonicalize(&profile)
        .unwrap()
        .join("simPl")
        .join("documents");
    assert!(book.path.starts_with(&documents), "{}", book.path.display());
    assert_eq!(book.path.file_name().unwrap(), "story.html");
    assert_eq!(position::load(&book.path).unwrap().unwrap().item_id, id);
    assert_eq!(book.restored.as_ref().unwrap().item_id, id);
    assert_eq!(recent_paths(&reader), std::slice::from_ref(&book.path));
    assert_eq!(reader.shelf.entries.len(), 1);
    assert_eq!(reader.shelf.entries[0].document.path, book.path);
    assert_eq!(names(&books), ["story.txt"]);
}

isolated!(add_to_library_keeps_the_reading_position);
fn add_to_library_keeps_the_reading_position() {
    add_to_library_keeps_the_position(true);
}

isolated!(add_to_library_while_reading_carries_the_live_position);
fn add_to_library_while_reading_carries_the_live_position() {
    add_to_library_keeps_the_position(false);
}
