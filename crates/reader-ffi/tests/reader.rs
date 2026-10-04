use reader_ffi::*;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn wait(status: impl Fn() -> LayoutStatus) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while status() == LayoutStatus::Running {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(status(), LayoutStatus::Complete);
}
fn open(path: String) -> Arc<OpenBook> {
    let task = open_book(path).unwrap();
    wait(|| task.status());
    task.result().unwrap().unwrap()
}

#[test]
fn reader_navigation_semantics_assets_options_and_positions_roundtrip() {
    let root = std::env::temp_dir().join(format!("simpl-ffi-reader-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    initialize(
        root.join("data").to_string_lossy().into_owned(),
        root.join("cache").to_string_lossy().into_owned(),
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
        let entry = import_library_book(fixture.path.to_string_lossy().into_owned()).unwrap();
        let book = open(entry.path.clone());
        let info = book.reader_info().unwrap();
        assert!(info.total > 0);
        let all = (1..=info.total)
            .flat_map(|n| book.page(n).unwrap())
            .collect::<Vec<_>>();
        let contents = book.contents().unwrap();
        assert!(
            contents
                .iter()
                .all(|entry| entry.location.page <= info.total)
        );
        let options = LayoutOptions {
            font: ReadingFont::Spectral,
            size: 28,
            spacing: 180,
            margin: 64,
        };
        book.save_options(options).unwrap();
        assert_eq!(book.reader_info().unwrap().options.size, 28);
        let before = book.atlas_json().unwrap();
        let task = book.clone().adapt("soft".into(), options).unwrap();
        wait(|| task.status());
        let adapted = task.result().unwrap().unwrap();
        assert_eq!(adapted.atlas().unwrap().total, info.total);
        assert_eq!(book.atlas_json().unwrap(), before);
        assert!(book.jump("missing label".into()).is_err());
        for number in 1..=info.total {
            let content = book.page(number).unwrap();
            let fragment = content.iter().find(|p| !p.rows.is_empty()).unwrap();
            let row = &fragment.rows[0];
            let cut = &fragment.layout.start_cut;
            let within = cut
                .as_ref()
                .filter(|cut| cut.row == row.index)
                .map_or(0.0, |cut| cut.line as f32 / cut.lines as f32);
            book.save_location(
                ReaderLocation {
                    page: number,
                    section: fragment.section,
                    row: row.index,
                    within,
                },
                options.size,
            )
            .unwrap();
            let restored = book.reader_info().unwrap().restored.unwrap();
            assert_eq!(restored.page, number, "{name}: split paragraph restore");
            assert_eq!(restored.row, row.index);
            assert_eq!(book.jump(fragment.layout.label.clone()).unwrap(), number);
        }
        let reopened = open(entry.path.clone());
        assert_eq!(
            reopened.reader_info().unwrap().restored.unwrap().page,
            info.total
        );
        assert_eq!(reopened.reader_info().unwrap().options.size, 28);
        let catalog = load_library().unwrap();
        let saved = catalog
            .books
            .iter()
            .find(|b| b.fingerprint == entry.fingerprint)
            .unwrap();
        assert_eq!(
            (saved.current, saved.total, saved.progress),
            (info.total, info.total, 1.0)
        );
        assert!(
            book.save_location(
                ReaderLocation {
                    page: 0,
                    section: 0,
                    row: 0,
                    within: f32::NAN
                },
                20
            )
            .is_err()
        );
        if name.starts_with("structured") {
            let rows = all.iter().flat_map(|p| &p.rows).collect::<Vec<_>>();
            for kind in [
                SemanticKind::ListItem,
                SemanticKind::Code,
                SemanticKind::TableRow,
                SemanticKind::Footnote,
            ] {
                assert!(
                    rows.iter().any(|r| r.semantics.kind == kind),
                    "{name} {kind:?}"
                );
            }
            assert!(rows.iter().any(|r| r.right_to_left));
            let image = all
                .iter()
                .find_map(|p| {
                    p.rows
                        .iter()
                        .find_map(|r| r.image_asset.as_ref().map(|asset| (p.section, asset)))
                })
                .unwrap();
            let image = book.image(image.0, image.1.clone()).unwrap().unwrap();
            assert_eq!(image.rgba.len(), (image.width * image.height * 4) as usize);
            let (section, link) = all
                .iter()
                .find_map(|p| {
                    p.rows.iter().find_map(|r| {
                        r.semantics
                            .links
                            .iter()
                            .find(|l| l.kind == BookLinkKind::Note)
                            .map(|l| (p.section, l))
                    })
                })
                .unwrap();
            let note = book
                .follow_link(section, link.href.clone(), "default".into(), options)
                .unwrap();
            if name.ends_with("epub") {
                assert!(note.location.is_none());
                assert!(
                    note.note_rows.len() >= 2,
                    "Auxiliary heading and note body must both be available"
                );
                let back = note
                    .note_rows
                    .iter()
                    .flat_map(|r| &r.semantics.links)
                    .find(|l| l.kind == BookLinkKind::Backlink)
                    .unwrap();
                assert!(
                    book.follow_link(note.section, back.href.clone(), "default".into(), options)
                        .unwrap()
                        .location
                        .is_some()
                );
            } else {
                assert!(note.location.is_some());
            }
            assert!(
                book.follow_link(section, "#missing".into(), "default".into(), options)
                    .is_err()
            );
            assert!(
                book.follow_link(
                    section,
                    "https://example.invalid".into(),
                    "default".into(),
                    options
                )
                .is_err()
            );
        }
        remove_library_book(entry.fingerprint).unwrap();
    }
    assert_eq!(reader_themes().len(), 4);
    std::fs::remove_dir_all(root).unwrap();
}
