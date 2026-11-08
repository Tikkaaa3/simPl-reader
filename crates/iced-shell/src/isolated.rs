//! Real opens, imports and saves, each run in a child test process whose
//! LOCALAPPDATA is a fresh temp folder, so the user's profile is never touched.
use super::release_audit::{boot, send};
use super::*;
use std::fs;
use std::process::Command;

const STORE: &str = "SIMPL_ISOLATED_STORE";
const PREFIX: &str = "simpl-isolated-";

/// Runs ignored test `name` (e.g. "app::isolated::x::child") in a child process
/// with its own profile, and fails with the child's output when it fails.
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

/// In the child: the `(profile, books)` folders. Panics before any storage
/// access unless `run_isolated` started this process with a temp profile.
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

/// `isolated!(name)` adds the tests `name::run`, which runs `name()` in a child
/// process through `run_isolated`, and the ignored `name::child` it starts.
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
