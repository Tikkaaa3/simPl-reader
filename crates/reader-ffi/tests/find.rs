use reader_ffi::*;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn wait(status: impl Fn() -> LayoutStatus) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while status() == LayoutStatus::Running {
        assert!(
            Instant::now() < deadline,
            "Search/layout must complete or cancel"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn book(path: String) -> Arc<OpenBook> {
    let task = open_book(path).unwrap();
    wait(|| task.status());
    task.result().unwrap().unwrap()
}
fn results(task: Arc<FindTask>) -> SearchResults {
    wait(|| task.status());
    task.result().unwrap().unwrap()
}

#[test]
fn search_locates_unicode_and_long_paragraphs_across_canonical_cuts_and_bounds_work() {
    let root = std::env::temp_dir().join(format!("simpl-p5-find-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    initialize(
        root.join("data").to_string_lossy().into(),
        root.join("cache").to_string_lossy().into(),
        "en".into(),
    )
    .unwrap();
    let path = root.join("find.html");
    std::fs::write(&path, format!("<meta charset='utf-8'><title>Search journey</title><p>Café İ e\u{301} 🛶</p><p>{}Late beacon</p><p>{}</p>", "A quiet passage beside the harbour. ".repeat(500), "needle ".repeat(1100))).unwrap();
    let opened = book(path.to_string_lossy().into());
    for (query, expected) in [("CAFÉ", "Café"), ("i", "İ"), ("e", "e\u{301}")] {
        let found = results(opened.clone().find(query.into()).unwrap());
        assert!(
            found
                .hits
                .iter()
                .any(|hit| opened.selection_text(hit.reflow.clone().unwrap()).unwrap() == expected)
        );
    }
    let found = results(opened.clone().find("late beacon".into()).unwrap());
    assert_eq!(found.hits.len(), 1);
    assert!(
        found.hits[0].page > 1,
        "A late match in a single paragraph crosses canonical cuts"
    );
    assert_eq!(
        opened
            .selection_text(found.hits[0].reflow.clone().unwrap())
            .unwrap(),
        "Late beacon"
    );
    let capped = results(opened.clone().find("needle".into()).unwrap());
    assert_eq!(capped.hits.len(), 1000);
    assert!(capped.limited);
    assert!(
        capped
            .hits
            .windows(2)
            .all(|pair| pair[0].page <= pair[1].page)
    );
    assert!(
        results(opened.clone().find(" \n ".into()).unwrap())
            .hits
            .is_empty()
    );
    assert!(opened.clone().find("x".repeat(257)).is_err());
    let cancelled = opened.find("quiet".into()).unwrap();
    cancelled.cancel();
    wait(|| cancelled.status());
    assert_eq!(cancelled.status(), LayoutStatus::Cancelled);
    assert!(cancelled.result().unwrap().is_none());

    let fixtures = reader_layout::golden::write_all(&root.join("fixtures")).unwrap();
    let epub = book(
        fixtures
            .iter()
            .find(|f| f.name == "chapters-epub")
            .unwrap()
            .path
            .to_string_lossy()
            .into(),
    );
    let found = results(epub.clone().find("tide".into()).unwrap());
    assert_eq!(epub.adjacent_chapter(0, 1).unwrap().section, 1);
    assert_eq!(epub.adjacent_chapter(1, 1).unwrap().section, 2);
    assert_eq!(epub.adjacent_chapter(0, -1).unwrap().section, 0);
    assert!(
        found
            .hits
            .iter()
            .any(|hit| hit.reflow.as_ref().unwrap().from.section == 2)
    );
    for hit in found.hits {
        assert_eq!(
            epub.selection_text(hit.reflow.unwrap())
                .unwrap()
                .to_lowercase(),
            "tide"
        );
    }

    let pdf_path = fixtures
        .iter()
        .find(|f| f.name == "tides-pdf")
        .unwrap()
        .path
        .to_string_lossy()
        .into();
    let pdf = open_pdf_document(pdf_path).unwrap();
    let found = results(pdf.clone().find("tide".into()).unwrap());
    assert!(!found.hits.is_empty());
    for hit in found.hits {
        assert_eq!(
            pdf.selection_text(hit.pdf.unwrap())
                .unwrap()
                .trim()
                .to_lowercase(),
            "tide"
        );
    }
    let restricted = root.join("restricted.pdf");
    std::fs::write(&restricted, include_bytes!("fixtures/copy-restricted.pdf")).unwrap();
    assert!(
        open_pdf_document(restricted.to_string_lossy().into())
            .unwrap()
            .find("keeper".into())
            .is_err()
    );
}
