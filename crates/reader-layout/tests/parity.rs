//! Recorded before the desktop move. Device runs embed every expected byte.
use reader_document::reading::{Font, Options};
use reader_layout::{atlas, book, golden, themes};
use std::sync::{Arc, atomic::AtomicBool};

fn complete<T>(future: impl std::future::Future<Output = T>) -> T {
    struct Unpark(std::thread::Thread);
    impl std::task::Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = std::task::Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => return value,
            std::task::Poll::Pending => std::thread::park(),
        }
    }
}

const EXPECTED: [(&str, &str); 8] = [
    (
        "structured-html",
        include_str!("golden/structured-html.json"),
    ),
    (
        "structured-epub",
        include_str!("golden/structured-epub.json"),
    ),
    ("prose-html", include_str!("golden/prose-html.json")),
    ("chapters-epub", include_str!("golden/chapters-epub.json")),
    ("paged-epub", include_str!("golden/paged-epub.json")),
    ("notes-txt", include_str!("golden/notes-txt.json")),
    ("guide-md", include_str!("golden/guide-md.json")),
    ("tides-pdf", include_str!("golden/tides-pdf.json")),
];

#[test]
fn shared_atlases_match_the_pre_move_desktop_bytes() {
    let scratch = std::env::temp_dir().join(format!("simpl-layout-parity-{}", std::process::id()));
    let fixtures = golden::write_all(&scratch).unwrap();
    let cancel = AtomicBool::new(false);
    let mut compared = 0;
    let mut failures = Vec::new();
    for fixture in fixtures {
        let opened = if fixture.kind == golden::Kind::Pdf {
            match complete(book::open_pdf(fixture.path.clone())) {
                Err(error) if !cfg!(target_os = "android") && error.contains("PDFium") => {
                    eprintln!(
                        "{} skipped: stage PDFium beside the test executable",
                        fixture.name
                    );
                    continue;
                }
                value => value,
            }
        } else {
            book::open(&fixture.path)
        };
        let first = Arc::new(opened.unwrap());
        let atlas = atlas::build_with_cache(first.clone(), &cancel, false)
            .unwrap()
            .unwrap();
        let mut adapted = Vec::new();
        if first.pdf_source.is_none() {
            for (theme, font, size, margin, spacing) in golden::ADAPTATIONS {
                let options = Options {
                    font: match font {
                        "literata" => Font::Literata,
                        "spectral" => Font::Spectral,
                        "fira-sans" => Font::FiraSans,
                        _ => Font::Theme,
                    },
                    size,
                    margin,
                    spacing,
                };
                let sections = (0..atlas.sections.len())
                    .map(|section| {
                        let current = match &first.epub {
                            Some(epub) if section != epub.index => Arc::new(
                                book::load_epub_chapter(epub.document.clone(), section, None)
                                    .unwrap(),
                            ),
                            _ => first.clone(),
                        };
                        atlas::adapt_section_with(
                            current,
                            &atlas.sections[section],
                            themes::find(theme),
                            options,
                            &cancel,
                        )
                        .unwrap()
                        .unwrap()
                    })
                    .collect::<Vec<_>>();
                adapted.push(serde_json::json!({
                    "layout": format!("{theme}/{font}/{size}/{margin}/{spacing}"), "sections": sections,
                }));
            }
        }
        let actual = serde_json::to_string_pretty(
            &serde_json::json!({ "atlas": atlas, "adapted": adapted }),
        )
        .unwrap()
            + "\n";
        let expected = EXPECTED
            .iter()
            .find(|(name, _)| *name == fixture.name)
            .unwrap()
            .1;
        if cfg!(target_os = "android") && !fixture.portable {
            eprintln!(
                "{}: system-font fixture, desktop parity={}",
                fixture.name,
                actual == expected
            );
        } else {
            if actual != expected {
                std::fs::write(
                    scratch.join(format!("{}.actual.json", fixture.name)),
                    &actual,
                )
                .unwrap();
                let a: serde_json::Value = serde_json::from_str(&actual).unwrap();
                let b: serde_json::Value = serde_json::from_str(expected).unwrap();
                let mut differences = Vec::new();
                diff(&a, &b, "", &mut differences);
                failures.push(format!("{}: {}", fixture.name, differences.join("; ")));
            }
            compared += 1;
        }
    }
    assert!(
        failures.is_empty(),
        "atlas parity failed (actual files in {}): {}",
        scratch.display(),
        failures.join("\n")
    );
    assert!(compared >= if cfg!(target_os = "android") { 6 } else { 7 });
    std::fs::remove_dir_all(scratch).unwrap();
}

fn diff(
    actual: &serde_json::Value,
    expected: &serde_json::Value,
    path: &str,
    out: &mut Vec<String>,
) {
    if actual == expected || out.len() >= 8 {
        return;
    }
    match (actual, expected) {
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            for (key, value) in a {
                diff(value, &b[key], &format!("{path}/{key}"), out);
            }
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) if a.len() == b.len() => {
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                diff(a, b, &format!("{path}/{i}"), out);
            }
        }
        _ => out.push(format!("{path}: actual={actual}, expected={expected}")),
    }
}

#[test]
fn cancelled_atlas_and_adaptation_return_without_results() {
    let scratch = std::env::temp_dir().join(format!("simpl-layout-cancel-{}", std::process::id()));
    let fixtures = golden::write_all(&scratch).unwrap();
    let fixture = fixtures.iter().find(|f| f.name == "prose-html").unwrap();
    let book = Arc::new(book::open(&fixture.path).unwrap());
    let atlas = atlas::build_with_cache(book.clone(), &AtomicBool::new(false), false)
        .unwrap()
        .unwrap();
    let cancel = AtomicBool::new(true);
    assert!(
        atlas::build_with_cache(book.clone(), &cancel, false)
            .unwrap()
            .is_none()
    );
    assert!(
        atlas::adapt_section_with(
            book,
            &atlas.sections[0],
            themes::find("soft"),
            Options::default(),
            &cancel
        )
        .unwrap()
        .is_none()
    );
    std::fs::remove_dir_all(scratch).unwrap();
}
