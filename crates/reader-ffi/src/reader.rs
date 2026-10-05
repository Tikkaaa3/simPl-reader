//! Reader semantics, navigation and positions use the desktop's source model.
use crate::{BookRow, CoreError, LayoutOptions, OpenBook, ReadingFont, layout::book_row};
use reader_document::{BlockKind, Item, LinkKind, position, reading};
use reader_layout::{book::Book, index::HeightIndex, measure, themes};

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SemanticKind {
    Paragraph,
    ListItem,
    Caption,
    Code,
    SceneBreak,
    TableRow,
    Formula,
    Footnote,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum BookLinkKind {
    Reference,
    Note,
    Backlink,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct BookLink {
    pub start_byte: u32,
    pub end_byte: u32,
    pub href: String,
    pub kind: BookLinkKind,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct RowSemantics {
    pub kind: SemanticKind,
    pub quote_depth: u16,
    pub list_depth: u16,
    pub links: Vec<BookLink>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct RowPresentation {
    pub alignment: String,
    pub image_left: f32,
    pub font_size: f32,
    pub line_height: f32,
    pub family: String,
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
    pub gap: f32,
    pub image_width: f32,
    pub image_height: f32,
}
pub(crate) fn semantics(book: &Book, item: &Item) -> RowSemantics {
    let source = book.structure.get(item.id()).cloned().unwrap_or_default();
    RowSemantics {
        kind: match source.kind {
            BlockKind::Paragraph => SemanticKind::Paragraph,
            BlockKind::ListItem => SemanticKind::ListItem,
            BlockKind::Caption => SemanticKind::Caption,
            BlockKind::Preformatted => SemanticKind::Code,
            BlockKind::SceneBreak => SemanticKind::SceneBreak,
            BlockKind::TableRow => SemanticKind::TableRow,
            BlockKind::Formula => SemanticKind::Formula,
            BlockKind::Footnote => SemanticKind::Footnote,
        },
        quote_depth: source.quote_depth,
        list_depth: source.list_depth,
        links: source
            .links
            .into_iter()
            .map(|link| BookLink {
                start_byte: crate::count(link.start_byte),
                end_byte: crate::count(link.end_byte),
                href: link.href,
                kind: match link.kind {
                    LinkKind::Reference => BookLinkKind::Reference,
                    LinkKind::Note => BookLinkKind::Note,
                    LinkKind::Backlink => BookLinkKind::Backlink,
                },
            })
            .collect(),
    }
}
pub(crate) fn presentation(
    book: &Book,
    index: usize,
    item: &Item,
    theme: &'static themes::ReadingTheme,
    options: reading::Options,
) -> RowPresentation {
    let pdf = book.pdf_source.is_some();
    let style = themes::effective_style(theme, pdf, options);
    let body = options.size as f32;
    let width = reader_layout::atlas::PAPER - 2.0 * options.margin as f32;
    let mut padding = style.block_padding(item, book.structure.get(item.id()), body, width);
    let mut alignment = "left";
    let mut image_left = 0.0;
    if let Some(source) = &book.pdf_source {
        let block = &source.conversion.blocks[index];
        match block.layout {
            reader_pdf::book::BlockLayout::Centered => alignment = "center",
            reader_pdf::book::BlockLayout::Right => alignment = "right",
            reader_pdf::book::BlockLayout::List { indent }
            | reader_pdf::book::BlockLayout::Inset { indent }
            | reader_pdf::book::BlockLayout::Toc { indent, .. } => {
                padding.left += width * indent.clamp(0.0, 0.2)
            }
            _ => {}
        }
        let page = source.document.pages[block.sources[0].page as usize];
        padding.top += width * page.height / page.width.max(1.0) * block.top_gap.clamp(0.0, 0.55);
        image_left = width
            * source
                .conversion
                .placements
                .get(item.id())
                .map_or(0.0, |p| p.offset);
    }
    let size = measure::styled_item_size(&style, book, index, item, body);
    let code = book
        .structure
        .get(item.id())
        .is_some_and(|s| matches!(s.kind, BlockKind::Preformatted | BlockKind::Formula));
    let (image_width, image_height) = if let Item::Image { asset_path, .. } = item {
        if let Some(source) = &book.pdf_source {
            source
                .conversion
                .illustrations
                .get(asset_path)
                .map(|rect| {
                    let page = source.document.pages
                        [source.conversion.blocks[index].sources[0].page as usize];
                    let w = width
                        * source
                            .conversion
                            .placements
                            .get(asset_path)
                            .map_or(1.0, |p| p.width);
                    (
                        w,
                        w * page.height * (rect.bottom - rect.top)
                            / (page.width * (rect.right - rect.left)),
                    )
                })
                .unwrap_or((0.0, body * style.line_height))
        } else {
            book.images
                .get(asset_path)
                .map(|asset| {
                    let scale = (width / asset.width.max(1) as f32).min(1.0);
                    (asset.width as f32 * scale, asset.height as f32 * scale)
                })
                .unwrap_or((0.0, body * style.line_height))
        }
    } else {
        (0.0, 0.0)
    };
    RowPresentation {
        alignment: alignment.into(),
        image_left,
        font_size: size,
        line_height: size * style.line_height,
        family: if code {
            "monospace"
        } else {
            themes::effective_family(theme, pdf, options).unwrap_or("Literata")
        }
        .into(),
        top: padding.top,
        bottom: padding.bottom,
        left: padding.left,
        right: padding.right,
        gap: if index + 1 == book.items.len() {
            0.0
        } else {
            style.gap(body)
        },
        image_width,
        image_height,
    }
}

impl From<reading::Options> for LayoutOptions {
    fn from(options: reading::Options) -> Self {
        Self {
            font: match options.font {
                reading::Font::Theme => ReadingFont::Theme,
                reading::Font::Literata => ReadingFont::Literata,
                reading::Font::Spectral => ReadingFont::Spectral,
                reading::Font::FiraSans => ReadingFont::FiraSans,
            },
            size: options.size,
            margin: options.margin,
            spacing: options.spacing,
        }
    }
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ReaderLocation {
    pub page: u32,
    pub section: u32,
    pub row: u32,
    /// Fraction of the complete source row, independent of the current theme.
    pub within: f32,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ReaderInfo {
    pub total: u32,
    pub source_pages: bool,
    pub options: LayoutOptions,
    pub restored: Option<ReaderLocation>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ReaderContents {
    pub label: String,
    pub depth: u32,
    pub location: ReaderLocation,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct LinkDestination {
    pub location: Option<ReaderLocation>,
    pub section: u32,
    /// Auxiliary EPUB notes are outside canonical next/previous order.
    pub note_rows: Vec<BookRow>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ReaderImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ReaderPalette {
    pub paper: u32,
    pub background: u32,
    /// Panels and menus raised above the paper.
    pub raised: u32,
    pub border: u32,
    pub text: u32,
    pub secondary: u32,
    pub muted: u32,
    pub accent: u32,
    pub danger: u32,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ReaderTheme {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub light: ReaderPalette,
    pub dark: ReaderPalette,
}
#[uniffi::export]
pub fn reader_themes() -> Vec<ReaderTheme> {
    fn palette(p: themes::Palette) -> ReaderPalette {
        fn rgba(c: impl Into<[u8; 4]>) -> u32 {
            u32::from_be_bytes(c.into())
        }
        // iced colors convert to sRGBA; return packed RGBA, not platform ARGB.
        ReaderPalette {
            paper: rgba(p.surface.into_rgba8()),
            background: rgba(p.background.into_rgba8()),
            raised: rgba(p.raised.into_rgba8()),
            border: rgba(p.border.into_rgba8()),
            text: rgba(p.text.into_rgba8()),
            secondary: rgba(p.secondary.into_rgba8()),
            muted: rgba(p.muted.into_rgba8()),
            accent: rgba(p.accent.into_rgba8()),
            danger: rgba(p.danger.into_rgba8()),
        }
    }
    themes::THEMES
        .iter()
        .map(|t| ReaderTheme {
            id: t.id.into(),
            name: t.name.into(),
            summary: t.summary.into(),
            light: palette(t.light),
            dark: palette(t.dark),
        })
        .collect()
}

impl OpenBook {
    pub(crate) fn locate(
        &self,
        section: usize,
        row: usize,
        within: f32,
    ) -> Result<ReaderLocation, CoreError> {
        let layout = self
            .atlas
            .sections
            .get(section)
            .ok_or("Target is outside the reading order".to_owned())?;
        if row >= layout.heights.len() {
            return Err("Target row is unavailable".to_owned().into());
        }
        let geometry = HeightIndex::new(layout.heights.clone());
        let offset = geometry.start(row) + geometry.height(row) * within;
        let page = layout
            .pages
            .iter()
            .find(|p| offset >= p.content.start && offset < p.content.end)
            .or_else(|| layout.pages.iter().find(|p| p.rows.contains(&row)))
            .ok_or("Target page is unavailable".to_owned())?;
        Ok(ReaderLocation {
            page: page.number + 1,
            section: crate::count(section),
            row: crate::count(row),
            within,
        })
    }
    fn target(
        &self,
        section: usize,
        fragment: Option<&str>,
    ) -> Result<(std::sync::Arc<Book>, usize), CoreError> {
        let book = self.section(section)?;
        let row = match fragment {
            Some(fragment) => {
                let id = book
                    .anchors
                    .get(fragment)
                    .map(String::as_str)
                    .unwrap_or(fragment);
                book.items
                    .iter()
                    .position(|item| item.id() == id)
                    .ok_or_else(|| format!("Link target is unavailable: {fragment}"))?
            }
            None => 0,
        };
        if book.items.is_empty() {
            return Err("Target section is empty".to_owned().into());
        }
        Ok((book, row))
    }
}

#[uniffi::export]
impl OpenBook {
    pub fn reader_info(&self) -> Result<ReaderInfo, CoreError> {
        let options = reading::load(&self.book.fingerprint)?.unwrap_or_default();
        let saved = if let Some(epub) = &self.book.epub {
            position::load_epub(&self.book.path)?
                .filter(|p| p.fingerprint == self.book.fingerprint)
                .and_then(|p| {
                    epub.document
                        .section_index(&p.chapter)
                        .map(|section| (section, p.item_id, p.within))
                })
        } else if self.book.pdf_source.is_some() {
            position::load_pdf_book(&self.book.path)?
                .filter(|p| p.fingerprint == self.book.fingerprint)
                .map(|p| (0, p.item_id, p.within))
        } else {
            position::load(&self.book.path)?
                .filter(|p| p.fingerprint == self.book.fingerprint)
                .map(|p| (0, p.item_id, p.within))
        };
        let restored = saved.and_then(|(section, id, within)| {
            let book = self.section(section).ok()?;
            let row = book.items.iter().position(|item| item.id() == id)?;
            self.locate(section, row, within).ok()
        });
        Ok(ReaderInfo {
            total: crate::count(self.atlas.total),
            source_pages: self.atlas.source_pages,
            options: options.into(),
            restored,
            warnings: self.book.warnings.clone(),
        })
    }
    pub fn contents(&self) -> Result<Vec<ReaderContents>, CoreError> {
        let entries = if let Some(epub) = &self.book.epub {
            epub.document.contents.clone()
        } else {
            self.book
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Heading { id, text, level } => Some(reader_document::epub::TocEntry {
                        label: text.clone(),
                        chapter: 0,
                        fragment: Some(id.clone()),
                        depth: level.saturating_sub(1) as usize,
                    }),
                    _ => None,
                })
                .collect()
        };
        entries
            .into_iter()
            .map(|entry| {
                let (_, row) = self.target(entry.chapter, entry.fragment.as_deref())?;
                Ok(ReaderContents {
                    label: entry.label,
                    depth: crate::count(entry.depth),
                    location: self.locate(entry.chapter, row, 0.0)?,
                })
            })
            .collect()
    }
    pub fn jump(&self, value: String) -> Result<u32, CoreError> {
        let value = value.trim();
        // Publisher labels take precedence over ordinal numbers.
        if let Some(page) = self
            .atlas
            .sections
            .iter()
            .flat_map(|s| &s.pages)
            .find(|p| p.label.eq_ignore_ascii_case(value))
        {
            return Ok(page.number + 1);
        }
        value
            .parse::<u32>()
            .ok()
            .filter(|p| *p > 0 && *p as usize <= self.atlas.total)
            .ok_or_else(|| {
                CoreError::from(
                    "Enter a page number or a printed page label from this book".to_owned(),
                )
            })
    }
    pub fn follow_link(
        &self,
        section: u32,
        href: String,
        theme: String,
        options: LayoutOptions,
    ) -> Result<LinkDestination, CoreError> {
        if let Some(pdf) = &self.book.pdf_source {
            let page = href
                .strip_prefix("pdf-page:")
                .and_then(|p| p.parse::<u32>().ok())
                .filter(|p| (*p as usize) < pdf.document.pages.len())
                .ok_or("PDF link target is unavailable".to_owned())?;
            let row = pdf
                .conversion
                .blocks
                .iter()
                .position(|b| b.sources.first().is_some_and(|s| s.page == page))
                .ok_or("PDF link page is unavailable".to_owned())?;
            return Ok(LinkDestination {
                location: Some(self.locate(0, row, 0.0)?),
                section: 0,
                note_rows: Vec::new(),
            });
        }
        let (section, fragment) = if let Some(epub) = &self.book.epub {
            epub.document.resolve_link(section as usize, &href)?
        } else {
            if section != 0 {
                return Err("Source section is unavailable".to_owned().into());
            }
            (
                0,
                reader_document::resolve_html_link(&self.book.path, &href)?,
            )
        };
        let (book, row) = self.target(section, fragment.as_deref())?;
        if section < self.atlas.sections.len() {
            return Ok(LinkDestination {
                location: Some(self.locate(section, row, 0.0)?),
                section: crate::count(section),
                note_rows: Vec::new(),
            });
        }
        // Show the targeted note up to the next readable anchor or block kind.
        // A general auxiliary link shows its section, without a page number.
        let source = book.structure.get(book.items[row].id());
        let end = if source.is_some_and(|s| s.kind == BlockKind::Footnote) {
            (row + 1..book.items.len())
                .find(|&i| {
                    book.structure
                        .get(book.items[i].id())
                        .is_none_or(|s| s.kind != BlockKind::Footnote)
                        || book.anchors.values().any(|id| id == book.items[i].id())
                })
                .unwrap_or(book.items.len())
        } else {
            book.items.len()
        };
        Ok(LinkDestination {
            location: None,
            section: crate::count(section),
            note_rows: book.items[row..end]
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    book_row(&book, row + i, item, themes::find(&theme), options.into())
                })
                .collect(),
        })
    }
    pub fn image(&self, section: u32, asset: String) -> Result<Option<ReaderImage>, CoreError> {
        let book = self.section(section as usize)?;
        if let Some(pdf) = &book.pdf_source {
            let Some(rect) = pdf.conversion.illustrations.get(&asset) else {
                return Ok(None);
            };
            let block = pdf
                .conversion
                .blocks
                .iter()
                .find(|b| b.id == asset)
                .ok_or("PDF illustration is unavailable".to_owned())?;
            let raster = crate::complete(pdf.document.session.render(block.sources[0].page, 1200))?;
            let cropped = reader_pdf::book::crop(&raster, *rect)?;
            return Ok(Some(ReaderImage {
                width: cropped.width,
                height: cropped.height,
                rgba: cropped.rgba,
            }));
        }
        Ok(book.images.get(&asset).and_then(|image| {
            image.rgba().map(|pixels| {
                // Bound the FFI copy and Android bitmap; layout retains the
                // native source dimensions and stable document asset key.
                let scale = (2048.0 / image.width.max(image.height).max(1) as f32).min(1.0);
                let width = (image.width as f32 * scale).round().max(1.0) as u32;
                let height = (image.height as f32 * scale).round().max(1.0) as u32;
                let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
                for y in 0..height {
                    for x in 0..width {
                        let source = ((y as u64 * image.height as u64 / height as u64)
                            * image.width as u64
                            + x as u64 * image.width as u64 / width as u64)
                            as usize
                            * 4;
                        rgba.extend_from_slice(&pixels[source..source + 4]);
                    }
                }
                ReaderImage {
                    width,
                    height,
                    rgba,
                }
            })
        }))
    }
    pub fn save_options(&self, options: LayoutOptions) -> Result<LayoutOptions, CoreError> {
        let _guard = crate::backup::read()?;
        crate::backup::current(self.epoch)?;
        let options: reading::Options = options.into();
        reading::save(&self.book.fingerprint, Some(options))?;
        Ok(options.into())
    }
    pub fn save_location(&self, location: ReaderLocation, font_size: u16) -> Result<(), CoreError> {
        let _guard = crate::backup::read()?;
        crate::backup::current(self.epoch)?;
        if !location.within.is_finite()
            || !(0.0..=1.0).contains(&location.within)
            || location.page == 0
            || location.page as usize > self.atlas.total
        {
            return Err("Invalid reading location".to_owned().into());
        }
        let section = location.section as usize;
        let book = self.section(section)?;
        let item = book
            .items
            .get(location.row as usize)
            .ok_or("Reading row is unavailable".to_owned())?;
        let layout = self
            .atlas
            .sections
            .get(section)
            .ok_or("Reading section is unavailable".to_owned())?;
        let page = layout
            .pages
            .iter()
            .find(|p| p.number + 1 == location.page && p.rows.contains(&(location.row as usize)))
            .ok_or("Reading row is outside this page".to_owned())?;
        let geometry = HeightIndex::new(layout.heights.clone());
        let start = geometry.start(location.row as usize);
        let height = geometry.height(location.row as usize).max(0.01);
        // A line ratio can differ slightly from the canonical height ratio due
        // to paragraph padding/gaps. Keep a boundary anchor inside its sheet.
        let offset = (start + height * location.within).clamp(
            page.content.start.max(start),
            (page.content.end - 0.01)
                .max(page.content.start)
                .min(start + height),
        );
        let within = ((offset - start) / height).clamp(0.0, 1.0);
        let size = font_size.clamp(12, 36) as f32;
        if let Some(epub) = &self.book.epub {
            let chapter = epub
                .document
                .section(section)
                .ok_or("Reading chapter is unavailable".to_owned())?;
            position::save_epub(
                &self.book.path,
                &position::EpubReadingPosition {
                    fingerprint: self.book.fingerprint.clone(),
                    chapter: chapter.href.clone(),
                    item_id: item.id().into(),
                    within,
                    font_size: size,
                },
            )?;
        } else if let Some(pdf) = &self.book.pdf_source {
            position::save_pdf_book(
                &self.book.path,
                &position::ReadingPosition {
                    fingerprint: self.book.fingerprint.clone(),
                    item_id: item.id().into(),
                    within,
                    font_size: size,
                },
            )?;
            let mut original = position::load_pdf(&self.book.path)?
                .filter(|p| p.fingerprint == self.book.fingerprint)
                .unwrap_or_else(|| pdf.original_position.clone());
            original.page = location.page - 1;
            original.within = pdf
                .conversion
                .blocks
                .get(location.row as usize)
                .and_then(|b| b.sources.first())
                .map_or(0.0, |s| {
                    (s.top + (s.bottom - s.top) * within).clamp(0.0, 1.0)
                });
            position::save_pdf(&self.book.path, &original)?;
        } else {
            position::save(
                &self.book.path,
                &position::ReadingPosition {
                    fingerprint: self.book.fingerprint.clone(),
                    item_id: item.id().into(),
                    within,
                    font_size: size,
                },
            )?;
        }
        crate::library::save_progress(
            &self.book.fingerprint,
            location.page,
            crate::count(self.atlas.total),
        )
    }
}
