use reader_ffi::*;

#[test]
fn catalog_import_repair_shelves_and_removal_preserve_sources_and_state() {
    let root = std::env::temp_dir().join(format!("simpl-ffi-library-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let data = root.join("data");
    let cache = root.join("cache");
    initialize(
        data.to_string_lossy().into_owned(),
        cache.to_string_lossy().into_owned(),
        "en".into(),
    )
    .unwrap();
    let source = root.join("Notes.txt");
    std::fs::write(&source, "A catalog test.\n\nA second paragraph.").unwrap();
    let book = import_library_book(source.to_string_lossy().into_owned()).unwrap();
    assert_eq!(book.format, DocumentFormat::Text);
    assert_eq!(book.opened_at, 0);
    assert!(std::path::Path::new(&book.path).starts_with(&data));
    let shelf = create_library_shelf("  To   read ".into()).unwrap();
    assert!(create_library_shelf("TO READ".into()).is_err());
    set_library_favourite(book.fingerprint.clone(), true).unwrap();
    assert!(toggle_library_shelf(shelf, book.fingerprint.clone()).unwrap());
    let opened = open_library_book(book.fingerprint.clone()).unwrap();
    assert!(opened.opened_at > 0);
    // Different staging paths must not duplicate TXT imports.
    let other = root.join("other");
    std::fs::create_dir_all(&other).unwrap();
    let copy = other.join("Notes.txt");
    std::fs::copy(&source, &copy).unwrap();
    let duplicate = import_library_book(copy.to_string_lossy().into_owned()).unwrap();
    assert_eq!(duplicate.path, book.path);
    assert!(duplicate.favourite);
    assert_eq!(duplicate.opened_at, opened.opened_at);
    assert_eq!(load_library().unwrap().books.len(), 1);
    // Repair a missing private copy while retaining its fingerprint and shelves.
    std::fs::remove_file(&book.path).unwrap();
    let repaired = import_library_book(copy.to_string_lossy().into_owned()).unwrap();
    assert!(std::path::Path::new(&repaired.path).is_file());
    assert!(repaired.favourite);
    assert_eq!(repaired.opened_at, opened.opened_at);
    assert_eq!(load_library().unwrap().books.len(), 1);
    let corrupt = root.join("broken.epub");
    std::fs::write(&corrupt, "not a zip archive").unwrap();
    assert!(import_library_book(corrupt.to_string_lossy().into_owned()).is_err());
    assert_eq!(load_library().unwrap().books.len(), 1);
    // A history failure after publication must never delete the repaired copy.
    std::fs::remove_file(&repaired.path).unwrap();
    let profile = data.join("simPl");
    let catalog = profile.join("library.json");
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&catalog).unwrap()).unwrap();
    record["entries"][0]["document"]["path"] = root
        .join("missing.html")
        .to_string_lossy()
        .into_owned()
        .into();
    std::fs::write(&catalog, serde_json::to_vec(&record).unwrap()).unwrap();
    let history = profile.join("recent.json");
    let retained = profile.join("recent-before.json");
    std::fs::rename(&history, &retained).unwrap();
    std::fs::create_dir(&history).unwrap();
    let replacement_dir = root.join("located");
    std::fs::create_dir(&replacement_dir).unwrap();
    let replacement = replacement_dir.join("Notes.txt");
    std::fs::copy(&source, &replacement).unwrap();
    let failure = locate_library_book(
        replacement.to_string_lossy().into_owned(),
        book.fingerprint.clone(),
    )
    .unwrap_err();
    assert!(failure.to_string().contains("recent"), "{failure}");
    let published = load_library().unwrap().books.remove(0);
    assert!(!published.missing);
    assert!(std::path::Path::new(&published.path).is_file());
    std::fs::remove_dir(&history).unwrap();
    std::fs::rename(&retained, &history).unwrap();
    rename_library_shelf(shelf, "Finished".into()).unwrap();
    assert_eq!(load_library().unwrap().shelves[0].name, "Finished");
    assert!(toggle_library_shelf(shelf, "a".repeat(64)).is_err());
    remove_library_book(book.fingerprint).unwrap();
    let empty = load_library().unwrap();
    assert!(empty.books.is_empty());
    assert!(empty.shelves[0].books.is_empty());
    assert!(!std::path::Path::new(&repaired.path).exists());
    assert!(!std::path::Path::new(&published.path).exists());
    assert!(source.is_file());
    assert!(copy.is_file());
    delete_library_shelf(shelf).unwrap();
    assert!(load_library().unwrap().shelves.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}
