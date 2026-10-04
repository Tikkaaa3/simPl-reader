//! Shared book sections and assets. Image handles are window-free Iced data.
use iced_core::image;
use reader_document::position::{PdfReadingPosition, ReadingPosition};
use reader_document::{BaseDirection, Item};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
#[derive(Debug)]
pub struct DisplayImage {
    pub handle: image::Handle,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub struct EpubChapter {
    pub document: Arc<reader_document::epub::Epub>,
    pub index: usize,
}

#[derive(Debug)]
pub struct PdfBook {
    pub document: Arc<reader_pdf::Document>,
    pub conversion: reader_pdf::book::Conversion,
    pub original_position: PdfReadingPosition,
    pub illustration_pages: HashSet<u32>,
}

#[derive(Debug)]
pub struct Book {
    pub path: PathBuf,
    pub title: String,
    pub author: Option<String>,
    pub cover: bool,
    pub fingerprint: String,
    pub items: Vec<Item>,
    pub images: HashMap<String, DisplayImage>,
    pub structure: HashMap<String, reader_document::BlockSemantics>,
    pub anchors: HashMap<String, String>,
    pub page_breaks: Vec<(String, String)>,
    pub warnings: Vec<String>,
    pub restored: Option<ReadingPosition>,
    pub epub: Option<EpubChapter>,
    pub pdf_source: Option<PdfBook>,
    pub contents: OnceLock<Vec<reader_document::epub::TocEntry>>,
}

pub fn display_book(
    document: reader_document::Document,
    restored: Option<ReadingPosition>,
    epub: Option<EpubChapter>,
) -> Book {
    let reader_document::Document {
        path,
        title,
        author,
        fingerprint,
        items,
        structure,
        anchors,
        page_breaks,
        images,
        warnings,
    } = document;
    let images = images
        .into_iter()
        .map(|(key, asset)| {
            let image = DisplayImage {
                handle: image::Handle::from_rgba(asset.width, asset.height, asset.rgba),
                width: asset.width,
                height: asset.height,
            };
            (key, image)
        })
        .collect();
    Book {
        path,
        title: epub
            .as_ref()
            .map_or(title, |chapter| chapter.document.title.clone()),
        author: epub
            .as_ref()
            .and_then(|chapter| chapter.document.author.clone())
            .or(author),
        cover: false,
        fingerprint,
        items,
        structure,
        anchors,
        page_breaks,
        images,
        warnings,
        restored,
        epub,
        pdf_source: None,
        contents: OnceLock::new(),
    }
}

pub fn load_epub_chapter(
    document: Arc<reader_document::epub::Epub>,
    index: usize,
    restored: Option<ReadingPosition>,
) -> Result<Book, String> {
    let mut chapter = document.load_chapter(index)?;
    chapter
        .document
        .warnings
        .extend(document.warnings.iter().cloned());
    Ok(display_book(
        chapter.document,
        restored,
        Some(EpubChapter { document, index }),
    ))
}

pub fn pdf_book(
    document: Arc<reader_pdf::Document>,
    conversion: reader_pdf::book::Conversion,
    original_position: PdfReadingPosition,
    restored: Option<ReadingPosition>,
    warnings: Vec<String>,
) -> Result<Book, String> {
    let structure: HashMap<_, _> = conversion
        .blocks
        .iter()
        .filter_map(|block| {
            let links: Vec<_> = block
                .links
                .iter()
                .filter(|link| {
                    link.start < link.end
                        && link.end <= block.text.len()
                        && block.text.is_char_boundary(link.start)
                        && block.text.is_char_boundary(link.end)
                })
                .map(|link| reader_document::Link {
                    start_byte: link.start,
                    end_byte: link.end,
                    href: link.href.clone(),
                    kind: reader_document::LinkKind::Reference,
                })
                .collect();
            (!links.is_empty()).then(|| {
                (
                    block.id.clone(),
                    reader_document::BlockSemantics {
                        links,
                        ..Default::default()
                    },
                )
            })
        })
        .collect();
    let items = conversion
        .blocks
        .iter()
        .map(|block| {
            if conversion.illustrations.contains_key(&block.id) {
                Item::Image {
                    id: block.id.clone(),
                    asset_path: block.id.clone(),
                }
            } else if block.heading {
                Item::Heading {
                    id: block.id.clone(),
                    text: block.text.clone(),
                    level: 2,
                }
            } else {
                Item::Paragraph {
                    id: block.id.clone(),
                    text: block.text.clone(),
                    base_direction: BaseDirection::Ltr,
                    style_runs: block
                        .styles
                        .iter()
                        .filter_map(|run| {
                            let style = match (run.bold, run.italic) {
                                (true, true) => reader_document::InlineStyle::BoldItalic,
                                (true, false) => reader_document::InlineStyle::Bold,
                                (false, true) => reader_document::InlineStyle::Italic,
                                (false, false) => return None,
                            };
                            (run.start < run.end && run.end <= block.text.len()).then_some(
                                reader_document::StyleRun {
                                    start_byte: run.start,
                                    end_byte: run.end,
                                    style,
                                },
                            )
                        })
                        .collect(),
                }
            }
        })
        .collect();
    let illustration_pages = conversion
        .blocks
        .iter()
        .filter(|block| conversion.illustrations.contains_key(&block.id))
        .filter_map(|block| block.sources.first().map(|source| source.page))
        .collect();
    Ok(Book {
        path: document.path.clone(),
        title: document.title.clone(),
        author: document.author.clone(),
        cover: false,
        fingerprint: document.fingerprint.clone(),
        items,
        images: HashMap::new(),
        structure,
        anchors: HashMap::new(),
        page_breaks: Vec::new(),
        warnings,
        restored,
        epub: None,
        contents: OnceLock::new(),
        pdf_source: Some(PdfBook {
            document,
            conversion,
            original_position,
            illustration_pages,
        }),
    })
}

/// Open a reflowable document without reading or changing user positions.
pub fn open(path: &Path) -> Result<Book, String> {
    match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "epub" => load_epub_chapter(Arc::new(reader_document::epub::open(path)?), 0, None),
        "html" | "htm" | "xhtml" => Ok(display_book(reader_document::load_html(path)?, None, None)),
        _ => Err(format!("Unsupported reflowable book: {}", path.display())),
    }
}

/// Open PDF Book using the same conversion and physical pages as the desktop.
pub async fn open_pdf(path: PathBuf) -> Result<Book, String> {
    let document = reader_pdf::open(path).await?;
    let conversion = document.session.book().await?;
    let warnings = conversion.warnings.clone();
    let original_position = PdfReadingPosition {
        fingerprint: document.fingerprint.clone(),
        page: 0,
        within: 0.0,
        horizontal: 0.0,
        zoom: reader_document::position::PdfZoom::FitWidth,
    };
    pdf_book(document, conversion, original_position, None, warnings)
}
