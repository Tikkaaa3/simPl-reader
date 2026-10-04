//! PDF Document mode stays file-backed; only requested pages are rasterized.

use crate::{CoreError, complete, library};
use reader_document::position::{self, PdfReadingPosition, PdfZoom};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Debug, uniffi::Record)]
pub struct PdfPageSize {
    pub width: f32,
    pub height: f32,
}

/// One-based page and normalized offsets. Scale uses the desktop's 96 DPI
/// zoom units; fit width remains a distinct mode in the shared position schema.
#[derive(Clone, Debug, uniffi::Record)]
pub struct PdfLocation {
    pub page: u32,
    pub within: f32,
    pub horizontal: f32,
    pub zoom: f32,
    pub fit_width: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PdfInfo {
    pub title: String,
    pub fingerprint: String,
    pub pages: Vec<PdfPageSize>,
    pub can_copy: bool,
    pub restored: PdfLocation,
    pub warnings: Vec<String>,
}

#[derive(Debug, uniffi::Record)]
pub struct PdfBitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PdfGlyphRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PdfGlyph {
    /// UTF-8 byte offsets; copy operations address glyph ordinals instead.
    pub start: u32,
    pub end: u32,
    pub bounds: Option<PdfGlyphRect>,
}

#[derive(Debug, uniffi::Record)]
pub struct PdfPageText {
    pub text: String,
    pub glyphs: Vec<PdfGlyph>,
}

#[derive(uniffi::Object)]
pub struct PdfDocument {
    pub(crate) epoch: u64,
    pub(crate) document: Arc<reader_pdf::Document>,
}

/// Call on a background thread. Opening hashes the file and reads dimensions,
/// without converting the book or allocating page rasters.
#[uniffi::export]
pub fn open_pdf_document(path: String) -> Result<Arc<PdfDocument>, CoreError> {
    let epoch = crate::backup::epoch();
    Ok(Arc::new(PdfDocument {
        epoch,
        document: complete(reader_pdf::open(PathBuf::from(path)))?,
    }))
}

#[uniffi::export]
pub fn pdf_book_mode(path: String, fingerprint: String) -> Result<bool, CoreError> {
    let _guard = crate::backup::read()?;
    Ok(
        position::load_pdf_mode(std::path::Path::new(&path), &fingerprint)?
            == position::PdfMode::Book,
    )
}
#[uniffi::export]
pub fn save_pdf_book_mode(path: String, fingerprint: String, book: bool) -> Result<(), CoreError> {
    let _guard = crate::backup::read()?;
    position::save_pdf_mode(
        std::path::Path::new(&path),
        &fingerprint,
        if book {
            position::PdfMode::Book
        } else {
            position::PdfMode::Document
        },
    )?;
    Ok(())
}

#[uniffi::export]
impl PdfDocument {
    pub fn info(&self) -> Result<PdfInfo, CoreError> {
        let mut warnings = Vec::new();
        let restored = match position::load_pdf(&self.document.path) {
            Ok(saved) => saved.filter(|p| p.fingerprint == self.document.fingerprint),
            Err(error) => {
                warnings.push(error);
                None
            }
        };
        let restored = restored.map_or(
            PdfLocation {
                page: 1,
                within: 0.0,
                horizontal: 0.0,
                zoom: 1.0,
                fit_width: true,
            },
            |p| PdfLocation {
                page: (p.page + 1).min(self.document.pages.len() as u32),
                within: p.within,
                horizontal: p.horizontal,
                zoom: match p.zoom {
                    PdfZoom::FitWidth => 1.0,
                    PdfZoom::Scale(scale) => scale,
                },
                fit_width: p.zoom == PdfZoom::FitWidth,
            },
        );
        Ok(PdfInfo {
            title: self.document.title.clone(),
            fingerprint: self.document.fingerprint.clone(),
            pages: self
                .document
                .pages
                .iter()
                .map(|p| PdfPageSize {
                    width: p.width,
                    height: p.height,
                })
                .collect(),
            can_copy: self.document.can_copy,
            restored,
            warnings,
        })
    }

    /// The shared worker caps each raster at four million pixels / 8192 px.
    pub fn render(&self, page: u32, width: u32) -> Result<PdfBitmap, CoreError> {
        let raster = complete(self.document.session.render(self.page_index(page)?, width))?;
        Ok(PdfBitmap {
            width: raster.width,
            height: raster.height,
            rgba: raster.rgba,
        })
    }

    /// Restricted documents expose no text. Bounds are normalized top-down
    /// coordinates from the same PDFium page transform as the raster.
    pub fn text(&self, page: u32, width: u32) -> Result<PdfPageText, CoreError> {
        let layer = complete(self.document.session.text(self.page_index(page)?, width))?;
        Ok(PdfPageText {
            text: layer.text,
            glyphs: layer
                .glyphs
                .into_iter()
                .map(|g| PdfGlyph {
                    start: g.start as u32,
                    end: g.end as u32,
                    bounds: g.bounds.map(|r| PdfGlyphRect {
                        left: r.left,
                        top: r.top,
                        right: r.right,
                        bottom: r.bottom,
                    }),
                })
                .collect(),
        })
    }

    /// Inclusive source glyph indices. The worker rechecks PDF permissions.
    pub fn copy(&self, page: u32, anchor: u32, focus: u32) -> Result<String, CoreError> {
        let page = self.page_index(page)?;
        Ok(complete(self.document.session.copy(
            reader_pdf::Selection {
                anchor: reader_pdf::TextPoint {
                    page,
                    index: anchor as usize,
                },
                focus: reader_pdf::TextPoint {
                    page,
                    index: focus as usize,
                },
            },
        ))?)
    }

    pub fn save_location(&self, location: PdfLocation) -> Result<(), CoreError> {
        let _guard = crate::backup::read()?;
        crate::backup::current(self.epoch)?;
        if position::load_pdf_mode(&self.document.path, &self.document.fingerprint)?
            == position::PdfMode::Book
        {
            return Ok(());
        }
        let page = self.page_index(location.page)?;
        position::save_pdf(
            &self.document.path,
            &PdfReadingPosition {
                fingerprint: self.document.fingerprint.clone(),
                page,
                within: location.within,
                horizontal: location.horizontal,
                zoom: if location.fit_width {
                    PdfZoom::FitWidth
                } else {
                    PdfZoom::Scale(location.zoom)
                },
            },
        )?;
        library::save_progress(
            &self.document.fingerprint,
            location.page,
            self.document.pages.len() as u32,
        )?;
        Ok(())
    }
}

impl PdfDocument {
    pub(crate) fn page_index(&self, page: u32) -> Result<u32, CoreError> {
        if page == 0 || page as usize > self.document.pages.len() {
            return Err(format!("PDF page {page} is out of range").into());
        }
        Ok(page - 1)
    }
}
