//! Golden page-atlas inputs: small project-authored books written from code, so
//! the same bytes exist on the desktop and inside an Android test run (which has
//! no repository checkout). The recorded atlases live in `tests/golden/`.
//!
//! `portable` books use only text the bundled fonts cover (no code blocks, no
//! Arabic/Hebrew/CJK), so their atlas must be byte-for-byte identical on every
//! platform. The others depend on system fallback fonts; they guard the desktop
//! before/after a change but are only reported, not required to match, elsewhere.

use std::io::Write;
use std::path::{Path, PathBuf};

/// The document format a fixture opens as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Html,
    Epub,
    /// PDF Book (needs the bundled PDFium).
    Pdf,
}

#[derive(Clone, Debug)]
pub struct Fixture {
    pub name: &'static str,
    pub kind: Kind,
    pub portable: bool,
    /// The file to open (TXT/Markdown are already the generated HTML page).
    pub path: PathBuf,
}

/// Reading layouts recorded for every section besides the canonical atlas:
/// (theme id, font choice, font size, margin, line spacing in percent; 0 = theme
/// spacing). Font choices: `theme`, `literata`, `spectral`, `fira-sans`.
pub const ADAPTATIONS: [(&str, &str, u16, u16, u16); 6] = [
    ("default", "theme", 20, 48, 0),
    ("soft", "theme", 20, 48, 0),
    ("clear", "theme", 20, 48, 0),
    ("compact", "theme", 20, 48, 0),
    ("default", "theme", 26, 64, 180),
    ("compact", "fira-sans", 16, 24, 150),
];

/// Write every fixture below `dir` (created if needed) and describe them.
pub fn write_all(dir: &Path) -> Result<Vec<Fixture>, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let write = |name: &str, bytes: &[u8]| -> Result<PathBuf, String> {
        let path = dir.join(name);
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    };
    let illustration = include_bytes!("../../../fixtures/book-structure/illustration.png");
    let structured = include_str!("../../../fixtures/book-structure/structured.html");
    write("illustration.png", illustration)?;

    let mut fixtures = vec![
        Fixture {
            name: "structured-html",
            kind: Kind::Html,
            portable: false,
            path: write("structured.html", structured.as_bytes())?,
        },
        Fixture {
            name: "structured-epub",
            kind: Kind::Epub,
            portable: false,
            path: write(
                "structured.epub",
                &structured_epub(structured, illustration)?,
            )?,
        },
        Fixture {
            name: "prose-html",
            kind: Kind::Html,
            portable: true,
            path: write("prose.html", prose_html().as_bytes())?,
        },
        Fixture {
            name: "chapters-epub",
            kind: Kind::Epub,
            portable: true,
            path: write("chapters.epub", &chapters_epub(false)?)?,
        },
        Fixture {
            name: "paged-epub",
            kind: Kind::Epub,
            portable: true,
            path: write("paged.epub", &chapters_epub(true)?)?,
        },
    ];
    for (name, file, text) in [
        ("notes-txt", "notes.txt", notes_text()),
        ("guide-md", "guide.md", guide_markdown()),
    ] {
        let source = write(file, text.as_bytes())?;
        let page = reader_document::text_page(&source, text.as_bytes())?;
        let stem = file.split('.').next().unwrap_or(file);
        fixtures.push(Fixture {
            name,
            kind: Kind::Html,
            portable: true,
            path: write(
                &format!("{stem}-{}.html", &file[stem.len() + 1..]),
                page.as_bytes(),
            )?,
        });
    }
    fixtures.push(Fixture {
        name: "tides-pdf",
        kind: Kind::Pdf,
        portable: true,
        path: write("tides.pdf", &tides_pdf())?,
    });
    Ok(fixtures)
}

const WORDS: [&str; 48] = [
    "harbour",
    "lamp",
    "quiet",
    "evening",
    "boats",
    "tide",
    "window",
    "letter",
    "morning",
    "river",
    "stone",
    "garden",
    "bridge",
    "walked",
    "slowly",
    "across",
    "the",
    "a",
    "of",
    "and",
    "with",
    "under",
    "before",
    "after",
    "light",
    "shadow",
    "station",
    "train",
    "kept",
    "every",
    "small",
    "house",
    "road",
    "north",
    "wind",
    "rain",
    "season",
    "music",
    "street",
    "old",
    "remembered",
    "carefully",
    "between",
    "afternoon",
    "library",
    "island",
    "map",
    "café",
];

/// Deterministic prose: `count` sentences from a fixed generator state.
fn sentences(seed: u32, count: usize) -> String {
    let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    let mut next = |bound: usize| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state as usize % bound
    };
    let mut text = String::new();
    for sentence in 0..count {
        if sentence > 0 {
            text.push(' ');
        }
        let length = 6 + next(14);
        for word in 0..length {
            let mut token = WORDS[next(WORDS.len())].to_owned();
            if word == 0 {
                token[..1].make_ascii_uppercase();
            } else {
                text.push(' ');
            }
            text.push_str(&token);
            if word + 1 < length && next(9) == 0 {
                text.push(',');
            }
        }
        text.push(['.', '.', '.', '?', '!'][next(5)]);
    }
    text
}

fn prose_html() -> String {
    let mut body = String::from("<h1>Harbour Lights</h1>");
    for (section, title) in ["The Quay", "Low Water", "Night Train"].iter().enumerate() {
        body.push_str(&format!("<h2>{title}</h2>"));
        for paragraph in 0..7 {
            let seed = (section * 10 + paragraph) as u32;
            if paragraph == 2 {
                body.push_str(&format!(
                    "<span epub:type=\"pagebreak\" id=\"page-{section}\" title=\"{}\"></span>",
                    section + 1
                ));
            }
            match paragraph {
                3 => body.push_str(&format!(
                    "<blockquote><p>{}</p></blockquote>",
                    sentences(seed, 3)
                )),
                4 => body.push_str(&format!(
                    "<ul><li>{}</li><li>{}<ul><li>{}</li></ul></li></ul>",
                    sentences(seed, 1),
                    sentences(seed + 1, 1),
                    sentences(seed + 2, 1)
                )),
                // One paragraph longer than a sheet, split on its line grid.
                5 if section == 1 => body.push_str(&format!("<p>{}</p>", sentences(seed, 60))),
                _ => body.push_str(&format!(
                    "<p>{} <em>{}</em> <strong>{}</strong></p>",
                    sentences(seed, 4),
                    sentences(seed + 50, 1),
                    sentences(seed + 51, 1)
                )),
            }
        }
    }
    format!(
        "<!DOCTYPE html><html xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><meta charset=\"utf-8\"><title>Harbour Lights</title></head><body>{body}</body></html>"
    )
}

fn notes_text() -> String {
    let mut text = String::new();
    for paragraph in 0..14 {
        // Hard-wrapped lines are joined back into paragraphs on import.
        let prose = sentences(100 + paragraph, if paragraph == 6 { 45 } else { 5 });
        let mut column = 0;
        for word in prose.split(' ') {
            if column > 0 && column + word.len() > 72 {
                text.push('\n');
                column = 0;
            } else if column > 0 {
                text.push(' ');
                column += 1;
            }
            text.push_str(word);
            column += word.len();
        }
        text.push_str("\n\n");
    }
    text
}

fn guide_markdown() -> String {
    let mut text = String::from("# A Guide to the Coast\n\n");
    for section in 0..4u32 {
        text.push_str(&format!("## Part {}\n\n", section + 1));
        text.push_str(&format!(
            "{} *{}* **{}**\n\n",
            sentences(200 + section, 4),
            sentences(210 + section, 1),
            sentences(220 + section, 1)
        ));
        text.push_str(&format!(
            "- {}\n- {}\n  - {}\n\n",
            sentences(230 + section, 1),
            sentences(240 + section, 2),
            sentences(250 + section, 1)
        ));
        text.push_str(&format!("> {}\n\n", sentences(260 + section, 3)));
        text.push_str(&format!(
            "### Notes\n\n{}\n\n",
            sentences(270 + section, if section == 2 { 50 } else { 6 })
        ));
    }
    text
}

/// A stored (uncompressed) ZIP writer is enough for these small packages.
fn epub(files: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored = zip::write::SimpleFileOptions::default()
        // Preserve the pre-move Windows fixture bytes on every test target.
        // Otherwise ZIP's platform default changes the source fingerprint.
        .system(zip::System::Dos)
        .compression_method(zip::CompressionMethod::Stored)
        .last_modified_time(zip::DateTime::default());
    zip.start_file("mimetype", stored)
        .map_err(|e| e.to_string())?;
    zip.write_all(b"application/epub+zip")
        .map_err(|e| e.to_string())?;
    zip.start_file("META-INF/container.xml", stored)
        .map_err(|e| e.to_string())?;
    zip.write_all(br#"<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#)
        .map_err(|e| e.to_string())?;
    for (name, bytes) in files {
        zip.start_file(*name, stored).map_err(|e| e.to_string())?;
        zip.write_all(bytes).map_err(|e| e.to_string())?;
    }
    Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
}

/// The same package as `fixtures/book-structure/build_epub.py`.
fn structured_epub(html: &str, illustration: &[u8]) -> Result<Vec<u8>, String> {
    let package = r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>A quieter page</dc:title><dc:creator>simPl fixture</dc:creator></metadata><manifest><item id="main" href="main.xhtml" media-type="application/xhtml+xml"/><item id="notes" href="notes.xhtml" media-type="application/xhtml+xml"/><item id="image" href="illustration.png" media-type="image/png"/></manifest><spine><itemref idref="main"/><itemref idref="notes" linear="no"/></spine></package>"#;
    let main = html.replace("href=\"#note\"", "href=\"notes.xhtml#note\"");
    epub(&[
        ("OPS/book.opf", package.as_bytes()),
        ("OPS/main.xhtml", main.as_bytes()),
        ("OPS/notes.xhtml", br#"<aside id="note" role="doc-footnote"><h2>A note</h2><p>A supplementary note outside the linear spine. <a href="main.xhtml#origin" role="doc-backlink">Return to the book</a>.</p></aside>"#),
        ("OPS/illustration.png", illustration),
    ])
}

/// Three chapters; with `page_list`, a publisher page list in the navigation
/// document (some targets mid-chapter, one at a chapter start).
fn chapters_epub(page_list: bool) -> Result<Vec<u8>, String> {
    let titles = ["Departure", "The Long Afternoon", "Arrival"];
    let mut chapters = Vec::new();
    let mut pages = String::new();
    let mut label = 1;
    for (index, title) in titles.iter().enumerate() {
        let mut body = format!("<h1 id=\"c{index}\">{title}</h1>");
        if index == 2 {
            pages.push_str(&format!(
                "<li><a href=\"c{index}.xhtml#c{index}\">{label}</a></li>"
            ));
            label += 1;
        }
        for paragraph in 0..12 {
            let seed = 300 + (index * 20 + paragraph) as u32;
            if paragraph % 4 == 1 {
                pages.push_str(&format!(
                    "<li><a href=\"c{index}.xhtml#p{index}-{paragraph}\">{label}</a></li>"
                ));
                label += 1;
            }
            let count = if index == 1 && paragraph == 7 {
                70
            } else {
                3 + paragraph % 4
            };
            if paragraph == 9 {
                body.push_str(&format!("<h2>{}</h2>", sentences(seed + 500, 1)));
            }
            body.push_str(&format!(
                "<p id=\"p{index}-{paragraph}\">{} <em>{}</em></p>",
                sentences(seed, count),
                sentences(seed + 900, 1)
            ));
        }
        chapters.push(format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>{title}</title></head><body>{body}</body></html>"
        ));
    }
    let page_nav = if page_list {
        format!("<nav epub:type=\"page-list\"><ol>{pages}</ol></nav>")
    } else {
        String::new()
    };
    let nav = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>Contents</title></head><body><nav epub:type=\"toc\"><ol>{}</ol></nav>{page_nav}</body></html>",
        titles
            .iter()
            .enumerate()
            .map(|(i, t)| format!("<li><a href=\"c{i}.xhtml\">{t}</a></li>"))
            .collect::<String>()
    );
    let package = format!(
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{}</dc:title><dc:creator>simPl fixture</dc:creator></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="c0" href="c0.xhtml" media-type="application/xhtml+xml"/><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="c2.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c0"/><itemref idref="c1"/><itemref idref="c2"/></spine></package>"#,
        if page_list {
            "Printed Pages"
        } else {
            "Three Chapters"
        }
    );
    epub(&[
        ("OPS/book.opf", package.as_bytes()),
        ("OPS/nav.xhtml", nav.as_bytes()),
        ("OPS/c0.xhtml", chapters[0].as_bytes()),
        ("OPS/c1.xhtml", chapters[1].as_bytes()),
        ("OPS/c2.xhtml", chapters[2].as_bytes()),
    ])
}

/// Four Helvetica pages: a heading and wrapped body lines, as a PDF Book input.
fn tides_pdf() -> Vec<u8> {
    let mut objects = vec![
        "<</Type/Catalog/Pages 2 0 R>>".to_owned(),
        String::new(),
        "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>".to_owned(),
        "<</Type/Font/Subtype/Type1/BaseFont/Helvetica-Bold>>".to_owned(),
    ];
    let mut kids = Vec::new();
    for page in 0..4u32 {
        let mut content = format!("BT /F2 18 Tf 72 720 Td (Chapter {}) Tj ET\n", page + 1);
        let prose = sentences(400 + page, 14).replace('é', "e");
        let mut y = 690;
        let mut line = String::new();
        for word in prose.split(' ') {
            if line.len() + word.len() > 78 {
                content.push_str(&format!("BT /F1 11 Tf 72 {y} Td ({line}) Tj ET\n"));
                y -= 14;
                line.clear();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        content.push_str(&format!("BT /F1 11 Tf 72 {y} Td ({line}) Tj ET\n"));
        content.push_str(&format!("BT /F1 10 Tf 300 40 Td ({}) Tj ET\n", page + 1));
        let page_id = objects.len() + 1;
        kids.push(format!("{page_id} 0 R"));
        objects.push(format!(
            "<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]/Contents {} 0 R/Resources<</Font<</F1 3 0 R/F2 4 0 R>>>>>>",
            page_id + 1
        ));
        objects.push(format!(
            "<</Length {}>>\nstream\n{content}endstream",
            content.len()
        ));
    }
    objects[1] = format!(
        "<</Type/Pages/Kids[{}]/Count {}>>",
        kids.join(" "),
        kids.len()
    );
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (index, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{body}\nendobj\n", index + 1));
    }
    let xref = pdf.len();
    pdf.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    ));
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<</Root 1 0 R/Size {}>>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    ));
    pdf.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures_are_deterministic_and_open() {
        let first = std::env::temp_dir().join(format!("simpl-golden-a-{}", std::process::id()));
        let second = std::env::temp_dir().join(format!("simpl-golden-b-{}", std::process::id()));
        let a = write_all(&first).unwrap();
        let b = write_all(&second).unwrap();
        for (a, b) in a.iter().zip(&b) {
            assert_eq!(
                std::fs::read(&a.path).unwrap(),
                std::fs::read(&b.path).unwrap()
            );
            match a.kind {
                Kind::Html => {
                    reader_document::load_html(&a.path).unwrap();
                }
                Kind::Epub => {
                    let epub = reader_document::epub::open(&a.path).unwrap();
                    assert!(!epub.chapters.is_empty());
                    assert_eq!(epub.page_list.is_empty(), a.name != "paged-epub");
                }
                Kind::Pdf => {}
            }
        }
        let _ = std::fs::remove_dir_all(first);
        let _ = std::fs::remove_dir_all(second);
    }
}
