use reader_ffi::*;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn open(path: String) -> Arc<OpenBook> {
    let task = open_book(path).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while task.status() == LayoutStatus::Running {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    task.result().unwrap().unwrap()
}
fn range(section: u32, row: u32, start: u32, end: u32) -> ReflowSelection {
    ReflowSelection {
        from: SourcePoint {
            section,
            row,
            byte: start,
        },
        to: SourcePoint {
            section,
            row,
            byte: end,
        },
    }
}

#[test]
fn annotations_share_desktop_sources_merge_rules_and_atomic_storage() {
    let root = std::env::temp_dir().join(format!("simpl-m6-annotations-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    initialize(
        root.join("data").to_string_lossy().into(),
        root.join("cache").to_string_lossy().into(),
        "en".into(),
    )
    .unwrap();
    let fixtures = reader_layout::golden::write_all(&root.join("fixtures")).unwrap();
    for name in [
        "structured-html",
        "structured-epub",
        "chapters-epub",
        "paged-epub",
        "prose-html",
        "notes-txt",
        "guide-md",
    ] {
        let fixture = fixtures.iter().find(|f| f.name == name).unwrap();
        let entry = import_library_book(fixture.path.to_string_lossy().into()).unwrap();
        let book = open(entry.path.clone());
        let fragments = book.page(1).unwrap();
        let fragment = fragments
            .iter()
            .find(|p| {
                p.rows
                    .iter()
                    .any(|r| r.text.as_ref().is_some_and(|t| t.len() > 12))
            })
            .unwrap();
        let row = fragment
            .rows
            .iter()
            .find(|r| r.text.as_ref().is_some_and(|t| t.len() > 12))
            .unwrap();
        let selected = book
            .selection_word(SourcePoint {
                section: fragment.section,
                row: row.index,
                byte: 0,
            })
            .unwrap();
        let quote = book.selection_text(selected.clone()).unwrap();
        assert!(!quote.is_empty());
        let yellow = book
            .highlight_selection(
                selected.clone(),
                AnnotationColor::Yellow,
                Some("A retained note".into()),
            )
            .unwrap()[0];
        assert_eq!(
            book.highlight_selection(selected.clone(), AnnotationColor::Yellow, None)
                .unwrap(),
            vec![yellow]
        );
        for color in [
            AnnotationColor::Green,
            AnnotationColor::Blue,
            AnnotationColor::Pink,
        ] {
            book.highlight_selection(selected.clone(), color, None)
                .unwrap();
        }
        let data = load_annotations(entry.fingerprint.clone()).unwrap();
        assert_eq!(
            data.highlights.len(),
            4,
            "{name}: colors overlap independently"
        );
        assert_eq!(data.highlights[0].note.as_deref(), Some("A retained note"));
        assert_eq!(book.reader_marks(fragment.section).unwrap().len(), 4);
        let target = book.annotation_target(yellow, false).unwrap();
        assert_eq!(
            book.selection_text(target.selection.unwrap()).unwrap(),
            quote
        );
        assert!(target.location.page <= book.reader_info().unwrap().total);
        let original = reader_document::annotations::load(&entry.fingerprint).unwrap();
        assert!(matches!(
            original.highlights[0].place,
            reader_document::annotations::Place::Reflow { .. }
        ));
        assert!(
            edit_annotation(
                entry.fingerprint.clone(),
                yellow,
                AnnotationColor::Blue,
                "x".repeat(16 * 1024 + 1)
            )
            .is_err()
        );
        assert_eq!(
            load_annotations(entry.fingerprint.clone())
                .unwrap()
                .highlights[0]
                .color,
            Some(AnnotationColor::Yellow)
        );
        edit_annotation(
            entry.fingerprint.clone(),
            yellow,
            AnnotationColor::Pink,
            "Edited note".into(),
        )
        .unwrap();
        assert_eq!(
            open(entry.path.clone())
                .reader_marks(fragment.section)
                .unwrap()
                .len(),
            4
        );
        let bookmarks = book.toggle_bookmark(1).unwrap();
        assert_eq!(bookmarks.bookmarks.len(), 1);
        let target = book
            .annotation_target(bookmarks.bookmarks[0].id, true)
            .unwrap();
        assert_eq!(target.location.page, 1);
        assert!(book.toggle_bookmark(1).unwrap().bookmarks.is_empty());
        for h in load_annotations(entry.fingerprint.clone())
            .unwrap()
            .highlights
        {
            remove_annotation(entry.fingerprint.clone(), h.id, false).unwrap();
        }
        assert!(book.reader_marks(fragment.section).unwrap().is_empty());
        if name == "chapters-epub" {
            let all = (1..=book.reader_info().unwrap().total)
                .flat_map(|page| book.page(page).unwrap())
                .collect::<Vec<_>>();
            let sections = all
                .iter()
                .map(|p| p.section)
                .collect::<std::collections::BTreeSet<_>>();
            let first = all
                .iter()
                .find(|p| p.section == *sections.first().unwrap())
                .unwrap();
            let second = *sections.iter().nth(1).unwrap();
            let last = all.iter().find(|p| p.section == second).unwrap();
            let a = all
                .iter()
                .filter(|p| p.section == first.section)
                .flat_map(|p| &p.rows)
                .filter(|r| r.text.is_some())
                .max_by_key(|r| r.index)
                .unwrap();
            let b = last.rows.iter().find(|r| r.text.is_some()).unwrap();
            let selected = ReflowSelection {
                from: SourcePoint {
                    section: first.section,
                    row: a.index,
                    byte: a.text.as_ref().unwrap().len().saturating_sub(1) as u32,
                },
                to: SourcePoint {
                    section: last.section,
                    row: b.index,
                    byte: b.text.as_ref().unwrap().len() as u32,
                },
            };
            // Chapter parts remain separate in the unchanged desktop schema.
            let ids = book
                .highlight_selection(selected, AnnotationColor::Green, None)
                .unwrap();
            assert_eq!(ids.len(), 2);
            let stored = reader_document::annotations::load(&entry.fingerprint).unwrap();
            assert!(stored.highlights.iter().all(|h| matches!(
                h.place,
                reader_document::annotations::Place::Reflow {
                    chapter: Some(_),
                    ..
                }
            )));
        }
        drop(book);
    }

    let path = root.join("unicode.html");
    let text = "Café e\u{301} 👩‍👩‍👧‍👦 العربية עברית";
    let long = (0..400)
        .map(|n| format!("Passage{n} harbour lighthouse "))
        .collect::<String>();
    std::fs::write(
        &path,
        format!("<html><body><p>{text}</p><p>{long}</p></body></html>"),
    )
    .unwrap();
    let entry = import_library_book(path.to_string_lossy().into()).unwrap();
    let book = open(entry.path.clone());
    let rows = book.page(1).unwrap()[0].rows.clone();
    let row = rows
        .iter()
        .find(|r| r.text.as_deref() == Some(text))
        .unwrap()
        .index;
    let combining = text.find('e').unwrap() as u32;
    assert!(
        book.selection_text(range(0, row, combining, combining + 1))
            .is_err()
    );
    assert!(
        book.selection_text(range(0, row, combining, combining + 2))
            .is_err()
    );
    assert_eq!(
        book.selection_text(range(0, row, combining, combining + 3))
            .unwrap(),
        "e\u{301}"
    );
    let emoji = text.find('👩').unwrap() as u32;
    assert!(
        book.selection_text(range(0, row, emoji, emoji + 4))
            .is_err()
    );
    let long_row = rows
        .iter()
        .find(|r| r.text.as_deref() == Some(long.trim()))
        .unwrap()
        .index;
    let byte = long.find("Passage300").unwrap() as u32;
    let selected = book
        .selection_word(SourcePoint {
            section: 0,
            row: long_row,
            byte,
        })
        .unwrap();
    let id = book
        .highlight_selection(selected, AnnotationColor::Blue, None)
        .unwrap()[0];
    let target = book.annotation_target(id, false).unwrap();
    assert!(
        target.location.page > 1,
        "A highlight inside a cut paragraph must not go to its first page"
    );
    let fragment = book.page(target.location.page).unwrap();
    assert!(fragment[0].rows.iter().any(|r| r.index == long_row));
    // Store bytes and ids are source coordinates, never Compose UTF-16 or visual cuts.
    let stored = reader_document::annotations::load(&entry.fingerprint).unwrap();
    let reader_document::annotations::Place::Reflow { from, .. } = &stored.highlights[0].place
    else {
        panic!()
    };
    assert_eq!(from.byte, byte as usize);
    let annotation_path = root
        .join("data/simPl/annotations")
        .join(format!("{}.json", entry.fingerprint));
    let saved = std::fs::read(&annotation_path).unwrap();
    std::fs::write(&annotation_path, b"corrupt").unwrap();
    assert!(book.toggle_bookmark(1).is_err());
    assert_eq!(std::fs::read(&annotation_path).unwrap(), b"corrupt");
    std::fs::write(&annotation_path, saved).unwrap();

    let library = if cfg!(target_os = "android") {
        "libpdfium.so"
    } else {
        "pdfium.dll"
    };
    if std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join(library)
        .exists()
    {
        let fixture = fixtures.iter().find(|f| f.name == "tides-pdf").unwrap();
        let entry = import_library_book(fixture.path.to_string_lossy().into()).unwrap();
        let pdf = open_pdf_document(entry.path).unwrap();
        let last = pdf.text(2, 600).unwrap().glyphs.len() as u32 - 1;
        let selection = PdfSelection {
            from: PdfSelectionPoint { page: 1, index: 0 },
            to: PdfSelectionPoint {
                page: 2,
                index: last,
            },
        };
        let quote = pdf.selection_text(selection.clone()).unwrap();
        assert!(quote.contains('\n'));
        let id = pdf
            .highlight_selection(
                selection.clone(),
                AnnotationColor::Yellow,
                Some("Across two pages".into()),
            )
            .unwrap();
        assert_eq!(
            pdf.highlight_selection(selection, AnnotationColor::Yellow, None)
                .unwrap(),
            id
        );
        assert_eq!(pdf.pdf_marks().unwrap().len(), 1);
        let target = pdf.annotation_target(id, false).unwrap();
        assert_eq!(
            pdf.selection_text(target.selection.unwrap()).unwrap(),
            quote
        );
        let bookmark = pdf.toggle_bookmark(2).unwrap().bookmarks[0].id;
        assert_eq!(pdf.annotation_target(bookmark, true).unwrap().page, 2);
        assert!(pdf.toggle_bookmark(2).unwrap().bookmarks.is_empty());
        // A desktop/imported record with an unavailable endpoint cannot wrap
        // into an unrelated, valid glyph in the Android adapter.
        let mut stored = reader_document::annotations::load(&entry.fingerprint).unwrap();
        let original = stored.highlights[0].place.clone();
        stored.highlights[0].place = reader_document::annotations::Place::Pdf {
            from: reader_document::annotations::PdfPoint {
                page: 0,
                index: usize::MAX,
            },
            to: reader_document::annotations::PdfPoint {
                page: 1,
                index: usize::MAX,
            },
        };
        reader_document::annotations::save(&stored).unwrap();
        assert!(pdf.pdf_marks().unwrap().is_empty());
        assert!(pdf.annotation_target(id, false).is_err());
        stored.highlights[0].place = original;
        reader_document::annotations::save(&stored).unwrap();
        let restricted_path = root.join("restricted.pdf");
        std::fs::write(
            &restricted_path,
            include_bytes!("fixtures/copy-restricted.pdf"),
        )
        .unwrap();
        let restricted = open_pdf_document(restricted_path.to_string_lossy().into()).unwrap();
        let selection = PdfSelection {
            from: PdfSelectionPoint { page: 1, index: 0 },
            to: PdfSelectionPoint { page: 1, index: 1 },
        };
        assert!(
            restricted
                .highlight_selection(selection, AnnotationColor::Yellow, None)
                .is_err()
        );
        assert_eq!(restricted.toggle_bookmark(1).unwrap().bookmarks.len(), 1);
    }
    drop(book);
    std::fs::remove_dir_all(root).unwrap();
}
