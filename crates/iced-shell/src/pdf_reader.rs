//! Native PDF presentation: source-relative anchors, virtual pages, and a bounded raster cache.

#[path = "pdf_page.rs"]
mod pdf_page;

use std::collections::{HashMap, HashSet};
use std::mem::size_of;
use std::sync::Arc;

use iced::advanced::widget::{operate, operation::scrollable::AbsoluteOffset};
use iced::keyboard::{self, Key, key};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length, Point, Size, Task, mouse, window};
use reader_document::position::{PdfReadingPosition, PdfZoom};
use reader_pdf::{Document, Selection, TextLayer, TextPoint};

use crate::ui::{self, ButtonTone};
use iced_shell::virtual_reader::{HeightIndex, LayoutReports, VisibleRows};
use pdf_page::Page;

const PAGE_GAP: f32 = 18.0;
const PAGE_MARGIN: f32 = 20.0;
const OVERSCAN: f32 = 320.0;
const CACHE_BUDGET: usize = 32 * 1024 * 1024;
const MAX_CACHED_PAGES: usize = 128;
const MAX_RENDER_PIXELS: f32 = 4_000_000.0;
const MAX_RENDER_EDGE: f32 = 8192.0;
const MIN_ZOOM: f32 = 0.25;
const MAX_ZOOM: f32 = 4.0;
const POINT_TO_DIP: f32 = 96.0 / 72.0;
// Allow for the root header, PDF controls and status. The first on_scroll
// report replaces this conservative viewport estimate with its actual bounds.
const INITIAL_CHROME: f32 = 136.0;
const COMPACT_TOOLBAR_WIDTH: f32 = 840.0;

pub fn page_input_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("pdf-page-input")
}

fn scroll_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("pdf-document")
}

fn scroll_to(x: f32, y: f32) -> Task<Message> {
    operate(iced::advanced::widget::operation::scrollable::scroll_to(
        scroll_id(),
        AbsoluteOffset {
            x: Some(x),
            y: Some(y),
        },
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusControl {
    Previous,
    Page,
    Next,
    ZoomOut,
    ZoomIn,
    ActualSize,
    FitWidth,
}

fn focus_button<'a>(
    label: &'a str,
    action: Option<Message>,
    focused: bool,
    selected: bool,
) -> Element<'a, Message> {
    button(text(label).font(ui::MEDIUM))
        .padding([7, 10])
        .on_press_maybe(action)
        .style(move |theme, status| {
            ui::button_style(theme, status, ButtonTone::Surface, focused, selected)
        })
        .into()
}

#[derive(Debug)]
struct CachedPage {
    handle: Option<iced::widget::image::Handle>,
    text: Option<Arc<TextLayer>>,
    width: u32,
    bytes: usize,
    text_bytes: usize,
    used: u64,
}

#[derive(Clone, Copy, Debug)]
struct Anchor {
    page: u32,
    within: f32,
    horizontal: f32,
}

#[derive(Clone, Debug)]
pub enum Message {
    Scroll {
        generation: u64,
        x: f32,
        y: f32,
        left: f32,
        top: f32,
        width: f32,
        height: f32,
    },
    Rendered {
        generation: u64,
        page: u32,
        width: u32,
        result: Result<Arc<RenderReply>, String>,
    },
    TextReady {
        generation: u64,
        page: u32,
        width: u32,
        result: Result<Arc<TextLayer>, String>,
    },
    PageInput(String),
    PageSubmit,
    Previous,
    Next,
    ZoomIn,
    ZoomOut,
    ActualSize,
    FitWidth,
    /// Ctrl+wheel or touchpad pinch, in wheel notches (positive zooms in).
    ZoomBy(f32),
    SelectStart(TextPoint),
    SelectMove(TextPoint),
    /// Right click on a page; the point is set when it is on text.
    Context(Option<TextPoint>),
    ClearSelection,
    Copy,
    CopyFinished(Result<String, String>),
}

/// A saved highlight drawn on the pages: inclusive glyph range from `from` to `to`.
#[derive(Clone, Debug)]
pub struct Mark {
    pub id: u64,
    pub from: reader_document::annotations::PdfPoint,
    pub to: reader_document::annotations::PdfPoint,
    pub tint: iced::Color,
    /// A note is attached, so the highlight also gets an underline.
    pub note: bool,
}

#[derive(Debug)]
pub struct RenderReply {
    handle: iced::widget::image::Handle,
    width: u32,
    bytes: usize,
}

/// PDF-specific UI and state. Dropping this value drops the bounded cache and
/// releases the session once outstanding native operations have completed.
#[derive(Debug)]
pub struct Reader {
    document: Arc<Document>,
    zoom: PdfZoom,
    size: Size,
    scale_factor: f64,
    viewport: Size,
    viewport_left: f32,
    viewport_top: f32,
    offset: Point,
    geometry: HeightIndex,
    measurements: iced_shell::virtual_reader::Measurements,
    content_width: f32,
    generation: u64,
    in_flight: Option<(u64, u32, u32, bool)>,
    render_task: Option<iced::task::Handle>,
    copy_task: Option<iced::task::Handle>,
    cached: HashMap<u32, CachedPage>,
    cache_bytes: usize,
    clock: u64,
    failed: HashSet<u32>,
    text_failed: HashSet<u32>,
    range: std::ops::Range<usize>,
    error: Option<String>,
    page_input: String,
    editing_page: bool,
    selection: Option<Selection>,
    word_selection: Option<Selection>,
    selection_ready: bool,
    select_all: bool,
    dragging: bool,
    pointer: Option<Point>,
    /// A search match whose page text is still loading: (page, first glyph, last glyph).
    pending_reveal: Option<(u32, usize, usize)>,
    /// The zoom to return to when Ctrl+Shift+F is pressed again.
    zoom_before_fit: Option<PdfZoom>,
    marks: Vec<Mark>,
    /// Pages that carry a bookmark ribbon.
    bookmarked: Vec<u32>,
}

impl Reader {
    pub fn new(
        document: Arc<Document>,
        restored: Option<PdfReadingPosition>,
        window_size: Size,
        scale_factor: f64,
    ) -> (Self, Task<Message>) {
        let restored = restored.filter(|state| {
            state.fingerprint == document.fingerprint
                && (state.page as usize) < document.pages.len()
        });
        let zoom = restored
            .as_ref()
            .map_or(PdfZoom::FitWidth, |state| match state.zoom {
                PdfZoom::Scale(scale)
                    if scale.is_finite() && (MIN_ZOOM..=MAX_ZOOM).contains(&scale) =>
                {
                    PdfZoom::Scale(scale)
                }
                PdfZoom::FitWidth => PdfZoom::FitWidth,
                _ => PdfZoom::FitWidth,
            });
        let mut reader = Self {
            document,
            zoom,
            size: window_size,
            scale_factor: valid_dpi(scale_factor),
            viewport: Size::new(
                (window_size.width - 32.0).max(1.0),
                (window_size.height - INITIAL_CHROME).max(1.0),
            ),
            viewport_left: 16.0,
            viewport_top: INITIAL_CHROME,
            offset: Point::ORIGIN,
            geometry: HeightIndex::new(Vec::new()),
            content_width: 0.0,
            measurements: Arc::new(parking_lot::Mutex::new(Vec::new())),
            generation: 0,
            in_flight: None,
            render_task: None,
            copy_task: None,
            cached: HashMap::new(),
            cache_bytes: 0,
            clock: 0,
            failed: HashSet::new(),
            text_failed: HashSet::new(),
            range: 0..0,
            error: None,
            page_input: "1".into(),
            editing_page: false,
            selection: None,
            word_selection: None,
            selection_ready: false,
            select_all: false,
            dragging: false,
            pointer: None,
            pending_reveal: None,
            zoom_before_fit: None,
            marks: Vec::new(),
            bookmarked: Vec::new(),
        };
        reader.rebuild();
        let anchor = restored.map_or(
            Anchor {
                page: 0,
                within: 0.0,
                horizontal: 0.0,
            },
            |state| Anchor {
                page: state.page,
                within: valid_fraction(state.within),
                horizontal: valid_fraction(state.horizontal),
            },
        );
        reader.set_anchor(anchor);
        let task = Task::batch([
            scroll_to(reader.offset.x, reader.offset.y),
            reader.request_next(),
        ]);
        (reader, task)
    }

    pub fn document(&self) -> &Arc<Document> {
        &self.document
    }

    pub fn page_index(&self) -> usize {
        self.anchor().page as usize
    }

    pub fn position(&self) -> PdfReadingPosition {
        let anchor = self.anchor();
        PdfReadingPosition {
            fingerprint: self.document.fingerprint.clone(),
            page: anchor.page,
            within: anchor.within,
            horizontal: anchor.horizontal,
            zoom: self.zoom,
        }
    }

    /// Reassert the actual native scroll offset after shell chrome changes.
    pub fn restore_scroll(&self) -> Task<Message> {
        scroll_to(self.offset.x, self.offset.y)
    }

    fn scale_for(&self, page_width: f32) -> f32 {
        match self.zoom {
            PdfZoom::Scale(scale) => scale * POINT_TO_DIP,
            PdfZoom::FitWidth => ((self.viewport.width - PAGE_MARGIN * 2.0 - 12.0).max(1.0)
                / page_width)
                .min(MAX_ZOOM * POINT_TO_DIP),
        }
    }

    fn page_size(&self, index: usize) -> Size {
        let page = &self.document.pages[index];
        let scale = self.scale_for(page.width);
        Size::new(page.width * scale, page.height * scale)
    }

    fn rebuild(&mut self) {
        let mut width = self.viewport.width;
        let heights = self
            .document
            .pages
            .iter()
            .enumerate()
            .map(|(index, info)| {
                let scale = self.scale_for(info.width);
                width = width.max(info.width * scale + PAGE_MARGIN * 2.0);
                info.height * scale
                    + if index + 1 == self.document.pages.len() {
                        0.0
                    } else {
                        PAGE_GAP
                    }
            })
            .collect();
        self.geometry = HeightIndex::new(heights);
        self.content_width = width;
        self.range = self.visible_range();
    }

    fn anchor(&self) -> Anchor {
        if self.geometry.is_empty() {
            return Anchor {
                page: 0,
                within: 0.0,
                horizontal: 0.0,
            };
        }
        let page = self.geometry.window(self.offset.y, 0.0, 0.0).start;
        let page_height = self.page_size(page).height.max(1.0);
        let horizontal_extent = (self.content_width - self.viewport.width).max(0.0);
        Anchor {
            page: page as u32,
            within: ((self.offset.y - self.geometry.start(page)) / page_height).clamp(0.0, 1.0),
            horizontal: if horizontal_extent > 0.0 {
                (self.offset.x / horizontal_extent).clamp(0.0, 1.0)
            } else {
                0.0
            },
        }
    }

    fn set_anchor(&mut self, anchor: Anchor) {
        if self.geometry.is_empty() {
            return;
        }
        let page = (anchor.page as usize).min(self.geometry.len() - 1);
        self.offset.y = (self.geometry.start(page)
            + valid_fraction(anchor.within) * self.page_size(page).height)
            .min((self.geometry.total() - self.viewport.height).max(0.0));
        self.offset.x =
            valid_fraction(anchor.horizontal) * (self.content_width - self.viewport.width).max(0.0);
        self.range = self.visible_range();
        if !self.editing_page {
            self.page_input = (self.anchor().page + 1).to_string();
        }
    }

    fn visible_range(&self) -> std::ops::Range<usize> {
        self.geometry
            .window(self.offset.y, self.viewport.height, OVERSCAN)
    }

    fn clear_rasters(&mut self) {
        self.cached.clear();
        self.cache_bytes = 0;
        self.failed.clear();
        self.text_failed.clear();
        self.error = None;
        self.generation = self.generation.wrapping_add(1);
    }

    fn set_zoom(&mut self, zoom: PdfZoom) -> Task<Message> {
        self.zoom_around(zoom, None)
    }

    /// Changes zoom while keeping the document point under `focus` (a window
    /// position, by default the viewport centre) in place. Existing rasters stay
    /// visible, scaled, until pages are rendered at the new size.
    fn zoom_around(&mut self, zoom: PdfZoom, focus: Option<Point>) -> Task<Message> {
        if self.zoom == zoom {
            return Task::none();
        }
        let anchor = self.anchor();
        // Zooming about the left edge would show blank margin beside narrower
        // pages (content is as wide as the widest page); keep the centre instead.
        let focus = focus.or(Some(Point::new(
            self.viewport_left + self.viewport.width / 2.0,
            self.viewport_top + self.viewport.height / 2.0,
        )));
        let focus = focus.and_then(|point| {
            let x = point.x - self.viewport_left;
            let y = point.y - self.viewport_top;
            if self.geometry.is_empty()
                || !(0.0..=self.viewport.width).contains(&x)
                || !(0.0..=self.viewport.height).contains(&y)
            {
                return None;
            }
            let document_y = self.offset.y + y;
            let page = self.geometry.window(document_y, 0.0, 0.0).start;
            let size = self.page_size(page);
            let left = (self.content_width - size.width) / 2.0;
            Some((
                point,
                page,
                (self.offset.x + x - left) / size.width.max(1.0),
                (document_y - self.geometry.start(page)) / size.height.max(1.0),
            ))
        });
        self.zoom = zoom;
        self.generation = self.generation.wrapping_add(1);
        self.failed.clear();
        self.text_failed.clear();
        self.error = None;
        self.rebuild();
        if let Some((point, page, fx, fy)) = focus {
            let size = self.page_size(page);
            let left = (self.content_width - size.width) / 2.0;
            self.offset.x = (left + fx * size.width - (point.x - self.viewport_left))
                .clamp(0.0, (self.content_width - self.viewport.width).max(0.0));
            self.offset.y = (self.geometry.start(page) + fy * size.height
                - (point.y - self.viewport_top))
                .clamp(0.0, (self.geometry.total() - self.viewport.height).max(0.0));
            self.range = self.visible_range();
            if !self.editing_page {
                self.page_input = (self.anchor().page + 1).to_string();
            }
        } else {
            self.set_anchor(anchor);
        }
        Task::batch([scroll_to(self.offset.x, self.offset.y), self.request_next()])
    }

    /// The zoom currently shown, including the scale Fit width resolves to.
    fn effective_scale(&self) -> f32 {
        match self.zoom {
            PdfZoom::Scale(scale) => scale,
            PdfZoom::FitWidth => {
                self.scale_for(self.document.pages[self.anchor().page as usize].width)
                    / POINT_TO_DIP
            }
        }
    }

    fn adjust_zoom(&mut self, multiplier: f32, focus: Option<Point>) -> Task<Message> {
        let scale = (self.effective_scale() * multiplier).clamp(MIN_ZOOM, MAX_ZOOM);
        self.zoom_around(PdfZoom::Scale(scale), focus)
    }

    pub fn resize(&mut self, window_size: Size, scale_factor: f64) -> Task<Message> {
        let dpi = valid_dpi(scale_factor);
        let width_changed = (window_size.width - self.size.width).abs() > 0.5;
        let height_changed = (window_size.height - self.size.height).abs() > 0.5;
        let dpi_changed = (dpi - self.scale_factor).abs() > 0.001;
        if !width_changed && !height_changed && !dpi_changed {
            return Task::none();
        }
        let anchor = self.anchor();
        self.size = window_size;
        self.scale_factor = dpi;
        self.viewport = Size::new(
            (window_size.width - 32.0).max(1.0),
            (window_size.height - INITIAL_CHROME).max(1.0),
        );
        if dpi_changed || (width_changed && matches!(self.zoom, PdfZoom::FitWidth)) {
            self.clear_rasters();
        } else {
            self.generation = self.generation.wrapping_add(1);
        }
        self.rebuild();
        self.set_anchor(anchor);
        Task::batch([scroll_to(self.offset.x, self.offset.y), self.request_next()])
    }

    fn target_width(&self, page: usize) -> u32 {
        let size = self.page_size(page);
        let dpi = self.scale_factor as f32;
        let factor = (MAX_RENDER_EDGE / (size.width * dpi).max(1.0))
            .min(MAX_RENDER_EDGE / (size.height * dpi).max(1.0))
            .min((MAX_RENDER_PIXELS / (size.width * size.height * dpi * dpi).max(1.0)).sqrt())
            .min(1.0);
        (size.width * dpi * factor).round().max(1.0) as u32
    }

    fn request_next(&mut self) -> Task<Message> {
        let visible = self.visible_range();
        if let Some((generation, page, width, _)) = self.in_flight {
            if generation == self.generation
                && visible.contains(&(page as usize))
                && width == self.target_width(page as usize)
            {
                return Task::none();
            }
            // Cancel queued work for pages no longer needed. A PDFium call
            // already executing still finishes on its serialized worker.
            self.render_task.take();
            self.in_flight = None;
        }
        self.range = visible.clone();
        let center = self.offset.y + self.viewport.height / 2.0;
        let distance = |page: usize| {
            (self.geometry.start(page) + self.page_size(page).height / 2.0 - center).abs()
        };
        // Request text before raster pixels; selection must not wait for a
        // render of this page (or any of its neighbors).
        let wanted_text = visible
            .clone()
            .filter(|&page| {
                !self.failed.contains(&(page as u32))
                    && !self.text_failed.contains(&(page as u32))
                    && self
                        .cached
                        .get(&(page as u32))
                        .is_none_or(|entry| entry.text.is_none())
            })
            .min_by(|&a, &b| distance(a).total_cmp(&distance(b)));
        let (page, text_request) = if let Some(page) = wanted_text {
            (page, true)
        } else if let Some(page) = visible
            .filter(|&page| {
                !self.failed.contains(&(page as u32))
                    && self.cached.get(&(page as u32)).is_none_or(|entry| {
                        entry.handle.is_none() || entry.width != self.target_width(page)
                    })
            })
            .min_by(|&a, &b| distance(a).total_cmp(&distance(b)))
        {
            (page, false)
        } else {
            return Task::none();
        };
        let page = page as u32;
        let width = self.target_width(page as usize);
        let generation = self.generation;
        self.in_flight = Some((generation, page, width, text_request));
        let session = self.document.session.clone();
        let (task, handle) = if text_request {
            Task::perform(
                async move { session.text(page, width).await.map(Arc::new) },
                move |result| Message::TextReady {
                    generation,
                    page,
                    width,
                    result,
                },
            )
            .abortable()
        } else {
            Task::perform(
                async move {
                    session.render(page, width).await.map(|rendered| {
                        let bytes = rendered.rgba.capacity();
                        Arc::new(RenderReply {
                            handle: iced::widget::image::Handle::from_rgba(
                                rendered.width,
                                rendered.height,
                                rendered.rgba,
                            ),
                            width: rendered.width,
                            bytes,
                        })
                    })
                },
                move |result| Message::Rendered {
                    generation,
                    page,
                    width,
                    result,
                },
            )
            .abortable()
        };
        self.render_task = Some(handle.abort_on_drop());
        task
    }

    fn cache(
        &mut self,
        page: u32,
        requested_width: u32,
        image: Option<&RenderReply>,
        text: Option<Arc<TextLayer>>,
    ) {
        let mut entry = self.cached.remove(&page).unwrap_or(CachedPage {
            handle: None,
            text: None,
            width: 0,
            bytes: 0,
            text_bytes: 0,
            used: 0,
        });
        self.cache_bytes -= entry.bytes;
        if let Some(image) = image {
            entry.bytes = entry.text_bytes + image.bytes;
            entry.handle = Some(image.handle.clone());
            entry.width = requested_width;
        }
        if let Some(text) = text {
            let text_bytes = text.text.capacity()
                + text.glyphs.capacity() * size_of::<reader_pdf::Glyph>()
                + text.hit_bytes();
            entry.bytes = entry.bytes - entry.text_bytes + text_bytes;
            entry.text_bytes = text_bytes;
            entry.text = Some(text);
        }
        if entry.bytes > CACHE_BUDGET {
            self.failed.insert(page);
            self.error = Some(format!(
                "Page {} exceeds the 32 MiB display cache limit",
                page + 1
            ));
            return;
        }
        while self.cache_bytes + entry.bytes > CACHE_BUDGET || self.cached.len() >= MAX_CACHED_PAGES
        {
            let victim = self
                .cached
                .iter()
                .min_by_key(|(page, entry)| (self.range.contains(&(**page as usize)), entry.used))
                .map(|(page, _)| *page);
            let Some(victim) = victim else { break };
            if let Some(old) = self.cached.remove(&victim) {
                self.cache_bytes -= old.bytes;
                if self.range.contains(&(victim as usize)) {
                    self.failed.insert(victim);
                }
            }
        }
        self.clock = self.clock.wrapping_add(1);
        entry.used = self.clock;
        self.cache_bytes += entry.bytes;
        self.cached.insert(page, entry);
    }

    fn jump(&mut self, page: usize) -> Task<Message> {
        if page >= self.geometry.len() {
            return Task::none();
        }
        let anchor = self.anchor();
        self.offset.y = self
            .geometry
            .start(page)
            .min((self.geometry.total() - self.viewport.height).max(0.0));
        self.offset.x = anchor.horizontal * (self.content_width - self.viewport.width).max(0.0);
        self.failed.clear();
        self.text_failed.clear();
        self.error = None;
        self.range = self.visible_range();
        self.page_input = (page + 1).to_string();
        self.editing_page = false;
        Task::batch([scroll_to(self.offset.x, self.offset.y), self.request_next()])
    }

    /// Highlights glyphs `first..=last` of `page` and brings them into view.
    pub fn show_match(&mut self, page: u32, first: usize, last: usize) -> Task<Message> {
        self.word_selection = None;
        if page as usize >= self.document.pages.len() {
            return Task::none();
        }
        self.selection = Some(Selection {
            anchor: TextPoint { page, index: first },
            focus: TextPoint { page, index: last },
        });
        self.selection_ready = true;
        self.select_all = false;
        self.dragging = false;
        self.pending_reveal = None;
        if self.glyph_position(page, first, last).is_some() {
            return self.scroll_to_glyph(page, first, last);
        }
        // Positions come from the page text layer: go to the page, then refine.
        self.pending_reveal = Some((page, first, last));
        self.jump(page as usize)
    }

    pub(crate) fn selection(&self) -> Option<Selection> {
        self.selection.filter(|_| self.selection_ready)
    }

    /// A selection is being dragged now.
    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    pub fn set_marks(&mut self, marks: Vec<Mark>, bookmarked: Vec<u32>) {
        self.marks = marks;
        self.bookmarked = bookmarked;
    }

    /// The topmost saved highlight that contains this glyph.
    pub fn mark_at(&self, point: TextPoint) -> Option<u64> {
        self.marks
            .iter()
            .rev()
            .find(|mark| {
                let key = (point.page, point.index);
                (mark.from.page, mark.from.index) <= key && key <= (mark.to.page, mark.to.index)
            })
            .map(|mark| mark.id)
    }

    /// Turns to a page.
    pub fn go_to_page(&mut self, page: usize) -> Task<Message> {
        self.jump(page.min(self.document.pages.len().saturating_sub(1)))
    }

    /// Brings glyphs `first..=last` of `page` into view without selecting them.
    pub fn reveal(&mut self, page: u32, first: usize, last: usize) -> Task<Message> {
        if page as usize >= self.document.pages.len() {
            return Task::none();
        }
        self.pending_reveal = None;
        if self.glyph_position(page, first, last).is_some() {
            return self.scroll_to_glyph(page, first, last);
        }
        self.pending_reveal = Some((page, first, last));
        self.jump(page as usize)
    }

    pub fn clear_selection(&mut self) {
        self.word_selection = None;
        self.selection = None;
        self.selection_ready = false;
        self.select_all = false;
        self.dragging = false;
        self.pending_reveal = None;
    }

    /// Expand the hit glyph to its Unicode word without crossing a page boundary.
    pub(crate) fn select_word(&mut self, point: TextPoint) {
        let Some(layer) = self
            .cached
            .get(&point.page)
            .and_then(|entry| entry.text.as_ref())
        else {
            return;
        };
        let Some(glyph) = layer.glyphs.get(point.index) else {
            return;
        };
        let Some(range) = crate::app::word_translation::word_range(&layer.text, glyph.start) else {
            return;
        };
        let Some(first) = layer
            .glyphs
            .iter()
            .position(|glyph| glyph.end > range.start && glyph.start < range.end)
        else {
            return;
        };
        let Some(last) = layer
            .glyphs
            .iter()
            .rposition(|glyph| glyph.end > range.start && glyph.start < range.end)
        else {
            return;
        };
        let selection = Selection {
            anchor: TextPoint {
                page: point.page,
                index: first,
            },
            focus: TextPoint {
                page: point.page,
                index: last,
            },
        };
        self.selection = Some(selection);
        self.word_selection = Some(selection);
        self.selection_ready = true;
    }

    fn keep_word(&mut self, point: TextPoint) -> bool {
        if self
            .word_selection
            .is_some_and(|word| word.anchor <= point && point <= word.focus)
        {
            return true;
        }
        self.word_selection = None;
        false
    }

    /// Document-space top of a match, its height, and its left edge, once the page text is loaded.
    fn glyph_position(&self, page: u32, first: usize, last: usize) -> Option<(f32, f32, f32)> {
        let layer = self.cached.get(&page)?.text.as_ref()?;
        let last = last.min(layer.glyphs.len().checked_sub(1)?);
        let rect = layer
            .glyphs
            .get(first..=last)?
            .iter()
            .find_map(|g| g.bounds)?;
        let size = self.page_size(page as usize);
        let left = (self.content_width - size.width) / 2.0;
        Some((
            self.geometry.start(page as usize) + rect.top * size.height,
            (rect.bottom - rect.top) * size.height,
            left + rect.left * size.width,
        ))
    }

    /// Scrolls so the match sits about a third of the way down; a match already
    /// comfortably in view is left where it is.
    fn scroll_to_glyph(&mut self, page: u32, first: usize, last: usize) -> Task<Message> {
        let Some((y, height, x)) = self.glyph_position(page, first, last) else {
            return Task::none();
        };
        let visible_height = self.viewport.height;
        let comfortably_visible = y >= self.offset.y + visible_height * 0.08
            && y + height <= self.offset.y + visible_height * 0.85;
        if !comfortably_visible {
            self.offset.y = (y - visible_height * 0.3)
                .clamp(0.0, (self.geometry.total() - visible_height).max(0.0));
        }
        let horizontal_extent = (self.content_width - self.viewport.width).max(0.0);
        if horizontal_extent > 0.0
            && (x < self.offset.x + 24.0 || x > self.offset.x + self.viewport.width - 96.0)
        {
            self.offset.x = (x - self.viewport.width / 2.0).clamp(0.0, horizontal_extent);
        }
        self.range = self.visible_range();
        self.failed.clear();
        self.text_failed.clear();
        self.error = None;
        if !self.editing_page {
            self.page_input = (self.anchor().page + 1).to_string();
        }
        Task::batch([scroll_to(self.offset.x, self.offset.y), self.request_next()])
    }

    /// Ctrl+Shift+F: fit the width, or go back to the zoom used before (100% if none).
    fn toggle_fit_width(&mut self) -> Task<Message> {
        if self.zoom == PdfZoom::FitWidth {
            let back = self.zoom_before_fit.take().unwrap_or(PdfZoom::Scale(1.0));
            self.set_zoom(back)
        } else {
            self.zoom_before_fit = Some(self.zoom);
            self.set_zoom(PdfZoom::FitWidth)
        }
    }

    fn copy(&mut self) -> Task<Message> {
        if self.copy_task.is_some() {
            return Task::none();
        }
        if !self.document.can_copy {
            self.error = Some("This PDF does not permit copying text".into());
            return Task::none();
        }
        let session = self.document.session.clone();
        let task = if self.select_all {
            Task::perform(
                async move { session.copy_all().await },
                Message::CopyFinished,
            )
        } else if let Some(selection) = self.selection.filter(|_| self.selection_ready) {
            Task::perform(
                async move { session.copy(selection).await },
                Message::CopyFinished,
            )
        } else {
            return Task::none();
        };
        let (task, handle) = task.abortable();
        self.copy_task = Some(handle.abort_on_drop());
        task
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Scroll {
                generation,
                x,
                y,
                left,
                top,
                width,
                height,
            } if generation == self.generation => {
                self.viewport_left = left;
                self.viewport_top = top;
                if (width - self.viewport.width).abs() > 0.5
                    || (height - self.viewport.height).abs() > 0.5
                {
                    let anchor = self.anchor();
                    let width_changed = (width - self.viewport.width).abs() > 0.5;
                    self.viewport = Size::new(width.max(1.0), height.max(1.0));
                    if width_changed && matches!(self.zoom, PdfZoom::FitWidth) {
                        self.clear_rasters();
                    } else {
                        self.generation = self.generation.wrapping_add(1);
                    }
                    self.rebuild();
                    self.set_anchor(anchor);
                    return Task::batch([
                        scroll_to(self.offset.x, self.offset.y),
                        self.request_next(),
                    ]);
                }
                self.offset = Point::new(
                    x.clamp(0.0, (self.content_width - self.viewport.width).max(0.0)),
                    y.clamp(0.0, (self.geometry.total() - self.viewport.height).max(0.0)),
                );
                let range = self.visible_range();
                if range != self.range {
                    self.failed.clear();
                    self.text_failed.clear();
                    self.error = None;
                }
                self.range = range.clone();
                self.clock = self.clock.wrapping_add(1);
                for (page, entry) in &mut self.cached {
                    if range.contains(&(*page as usize)) {
                        entry.used = self.clock;
                    }
                }
                if !self.editing_page {
                    self.page_input = (self.anchor().page + 1).to_string();
                }
                self.continue_drag();
                self.request_next()
            }
            Message::Rendered {
                generation,
                page,
                width,
                result,
            } => {
                if self.in_flight != Some((generation, page, width, false)) {
                    return Task::none();
                }
                self.in_flight = None;
                self.render_task = None;
                if generation == self.generation
                    && self.visible_range().contains(&(page as usize))
                    && width == self.target_width(page as usize)
                {
                    match result {
                        Ok(reply) if reply.width > 0 => {
                            self.cache(page, width, Some(&reply), None);
                        }
                        Ok(_) => {
                            self.failed.insert(page);
                            self.error =
                                Some(format!("Page {} returned an empty raster", page + 1));
                        }
                        Err(error) => {
                            self.failed.insert(page);
                            self.error = Some(format!("Page {}: {error}", page + 1));
                        }
                    }
                }
                self.continue_drag();
                self.request_next()
            }
            Message::TextReady {
                generation,
                page,
                width,
                result,
            } => {
                if self.in_flight != Some((generation, page, width, true)) {
                    return Task::none();
                }
                self.in_flight = None;
                self.render_task = None;
                if generation == self.generation
                    && self.visible_range().contains(&(page as usize))
                    && width == self.target_width(page as usize)
                {
                    match result {
                        Ok(text) => self.cache(page, width, None, Some(text)),
                        Err(error) => {
                            self.text_failed.insert(page);
                            self.error =
                                Some(format!("Page {} text is unavailable: {error}", page + 1));
                        }
                    }
                }
                self.continue_drag();
                if let Some((wanted, first, last)) = self.pending_reveal
                    && wanted == page
                    && self.glyph_position(page, first, last).is_some()
                {
                    self.pending_reveal = None;
                    return self.scroll_to_glyph(page, first, last);
                }
                self.request_next()
            }
            Message::PageInput(value) => {
                self.page_input = value;
                self.editing_page = true;
                Task::none()
            }
            Message::PageSubmit => match self.page_input.trim().parse::<usize>() {
                Ok(page) if page > 0 && page <= self.document.pages.len() => self.jump(page - 1),
                _ => {
                    self.error = Some(format!(
                        "Enter a page from 1 to {}",
                        self.document.pages.len()
                    ));
                    Task::none()
                }
            },
            Message::Previous => self.jump((self.anchor().page as usize).saturating_sub(1)),
            Message::Next => self.jump(
                (self.anchor().page as usize + 1).min(self.document.pages.len().saturating_sub(1)),
            ),
            Message::ZoomIn => self.adjust_zoom(1.25, None),
            Message::ZoomOut => self.adjust_zoom(0.8, None),
            Message::ZoomBy(steps) if steps.is_finite() => {
                self.adjust_zoom(1.1_f32.powf(steps.clamp(-10.0, 10.0)), self.pointer)
            }
            Message::ActualSize => self.set_zoom(PdfZoom::Scale(1.0)),
            Message::FitWidth => self.set_zoom(PdfZoom::FitWidth),
            Message::SelectStart(point) => {
                self.word_selection = None;
                self.selection = Some(Selection {
                    anchor: point,
                    focus: point,
                });
                self.selection_ready = false;
                self.select_all = false;
                self.dragging = true;
                self.error = None;
                Task::none()
            }
            Message::SelectMove(point) => {
                if self.keep_word(point) {
                    return Task::none();
                }
                if self.dragging
                    && let Some(selection) = &mut self.selection
                {
                    selection.focus = point;
                    self.selection_ready = true;
                }
                Task::none()
            }
            Message::ClearSelection => {
                self.clear_selection();
                Task::none()
            }
            Message::Copy => self.copy(),
            Message::CopyFinished(result) => {
                self.copy_task = None;
                match result {
                    Ok(text) if text.is_empty() => {
                        self.error =
                            Some("No selectable text in this PDF (scanned pages need OCR)".into());
                        Task::none()
                    }
                    Ok(text) => {
                        self.error = None;
                        iced::clipboard::write(text)
                    }
                    Err(error) => {
                        self.error = Some(format!("Could not copy PDF text: {error}"));
                        Task::none()
                    }
                }
            }
            _ => Task::none(),
        }
    }

    fn continue_drag(&mut self) {
        if !self.dragging {
            return;
        }
        let Some(pointer) = self.pointer else { return };
        // The native scrollbar can move while the cursor is stationary. Continue
        // selection from source glyphs at the pointer's new page location.
        let y = (pointer.y - self.viewport_top + self.offset.y).max(0.0);
        if self.geometry.is_empty() {
            return;
        }
        let page = self.geometry.window(y, 0.0, 0.0).start;
        let Some(text) = self
            .cached
            .get(&(page as u32))
            .and_then(|entry| entry.text.as_ref())
        else {
            return;
        };
        let page_size = self.page_size(page);
        let x = (self.content_width - page_size.width) / 2.0 - self.offset.x;
        let bounds = iced::Rectangle::new(
            Point::new(
                x,
                self.geometry.start(page) - self.offset.y + self.viewport_top,
            ),
            page_size,
        );
        if let Some(point) = pdf_page::hit(text, page as u32, bounds, pointer, false)
            && !self.keep_word(point)
            && let Some(selection) = &mut self.selection
            && selection.focus != point
        {
            selection.focus = point;
            self.selection_ready = true;
        }
    }

    pub fn event(&mut self, event: &iced::Event) -> Task<Message> {
        match event {
            iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                self.pointer = Some(*position);
                Task::none()
            }
            iced::Event::Mouse(mouse::Event::CursorLeft)
            | iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | iced::Event::Window(window::Event::Unfocused) => {
                self.dragging = false;
                Task::none()
            }
            iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                if modifiers.control() {
                    match key.as_ref() {
                        Key::Character(value) => match value.to_ascii_lowercase().as_str() {
                            "+" | "=" => self.update(Message::ZoomIn),
                            "-" => self.update(Message::ZoomOut),
                            "0" => self.update(Message::ActualSize),
                            // Plain Ctrl+F belongs to Find in the app.
                            "f" if modifiers.shift() => self.toggle_fit_width(),
                            "c" => self.update(Message::Copy),
                            "a" => {
                                self.select_all = true;
                                self.selection = None;
                                self.selection_ready = false;
                                self.dragging = false;
                                Task::none()
                            }
                            _ => Task::none(),
                        },
                        Key::Named(key::Named::Home) => self.jump(0),
                        Key::Named(key::Named::End) => {
                            self.jump(self.document.pages.len().saturating_sub(1))
                        }
                        _ => Task::none(),
                    }
                } else {
                    match key.as_ref() {
                        Key::Named(key::Named::PageDown | key::Named::Space) => {
                            self.scroll_page(0.9)
                        }
                        Key::Named(key::Named::PageUp) => self.scroll_page(-0.9),
                        Key::Named(key::Named::ArrowLeft) => self.update(Message::Previous),
                        Key::Named(key::Named::ArrowRight) => self.update(Message::Next),
                        Key::Named(key::Named::ArrowUp) => {
                            self.scroll_page(-40.0 / self.viewport.height.max(1.0))
                        }
                        Key::Named(key::Named::ArrowDown) => {
                            self.scroll_page(40.0 / self.viewport.height.max(1.0))
                        }
                        Key::Named(key::Named::Escape) => self.update(Message::ClearSelection),
                        _ => Task::none(),
                    }
                }
            }
            _ => Task::none(),
        }
    }

    fn scroll_page(&mut self, amount: f32) -> Task<Message> {
        self.offset.y = (self.offset.y + amount * self.viewport.height)
            .clamp(0.0, (self.geometry.total() - self.viewport.height).max(0.0));
        self.range = self.visible_range();
        self.failed.clear();
        self.text_failed.clear();
        self.error = None;
        Task::batch([scroll_to(self.offset.x, self.offset.y), self.request_next()])
    }

    /// Previous / page number / next, sized for the centre of the app toolbar.
    pub fn navigation(&self, focused: Option<FocusControl>) -> Element<'_, Message> {
        let page = self.anchor().page as usize;
        row![
            focus_button(
                "Prev",
                (page > 0).then_some(Message::Previous),
                focused == Some(FocusControl::Previous),
                false,
            ),
            text_input("Page", &self.page_input)
                .id(page_input_id())
                .on_input(Message::PageInput)
                .on_submit(Message::PageSubmit)
                .size(13)
                .padding([6, 8])
                .width(65)
                .style(move |theme, status| {
                    let mut style = ui::input_style(theme, status);
                    if focused == Some(FocusControl::Page) {
                        style.border.color = ui::palette(theme).accent;
                        style.border.width = 2.0;
                    }
                    style
                }),
            text(format!("of {}", self.document.pages.len()))
                .size(13)
                .style(ui::muted_text),
            focus_button(
                "Next",
                (page + 1 < self.document.pages.len()).then_some(Message::Next),
                focused == Some(FocusControl::Next),
                false,
            ),
        ]
        .spacing(7)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// Zoom and fit controls, sized for the right side of the app toolbar.
    pub fn zoom_controls(&self, focused: Option<FocusControl>) -> Element<'_, Message> {
        // Show the size in use; Fit width stays marked on its own button.
        let zoom_label = format!("{:.0}%", self.effective_scale() * 100.0);
        row![
            focus_button(
                "−",
                Some(Message::ZoomOut),
                focused == Some(FocusControl::ZoomOut),
                false,
            ),
            text(zoom_label).size(13).style(ui::muted_text),
            focus_button(
                "+",
                Some(Message::ZoomIn),
                focused == Some(FocusControl::ZoomIn),
                false,
            ),
            focus_button(
                "100%",
                Some(Message::ActualSize),
                focused == Some(FocusControl::ActualSize),
                self.zoom == PdfZoom::Scale(1.0),
            ),
            focus_button(
                if self.size.width < 520.0 {
                    "Fit"
                } else {
                    "Fit width"
                },
                Some(Message::FitWidth),
                focused == Some(FocusControl::FitWidth),
                self.zoom == PdfZoom::FitWidth,
            ),
        ]
        .spacing(7)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// Both control groups in one strip, for windows too narrow for the three-column layout.
    pub fn toolbar(&self, focused: Option<FocusControl>) -> Element<'_, Message> {
        let navigation = self.navigation(focused);
        let zoom = container(self.zoom_controls(focused)).width(Length::Fill);
        // The app supplies the one collapsible toolbar slot around these controls.
        let toolbar: Element<'_, Message> = if self.size.width < COMPACT_TOOLBAR_WIDTH {
            column![navigation, zoom].spacing(5).into()
        } else {
            row![navigation, zoom]
                .spacing(16)
                .align_y(iced::Alignment::Center)
                .into()
        };
        toolbar
    }

    pub fn view(&self) -> Element<'_, Message> {
        let page = self.anchor().page as usize;
        let mut content = column![];
        let range = self.visible_range();
        let selected = if self.select_all {
            Some(Selection {
                anchor: TextPoint { page: 0, index: 0 },
                focus: TextPoint {
                    page: self.document.pages.len().saturating_sub(1) as u32,
                    index: usize::MAX,
                },
            })
        } else if self.selection_ready {
            self.selection
        } else {
            None
        };
        let rows: Vec<Element<'_, Message>> = range
            .clone()
            .map(|index| {
                let size = self.page_size(index);
                let cached = self.cached.get(&(index as u32));
                let page_widget: Element<'_, Message> = Page {
                    number: index as u32,
                    width: size.width,
                    height: size.height,
                    image: cached.and_then(|entry| entry.handle.as_ref()),
                    text: cached.and_then(|entry| entry.text.as_ref()),
                    selection: selected,
                    dragging: self.dragging,
                    marks: &self.marks,
                    bookmarked: self.bookmarked.contains(&(index as u32)),
                }
                .into();
                container(page_widget)
                    .width(self.content_width)
                    .center_x(Length::Fill)
                    .into()
            })
            .collect();
        let visible = VisibleRows::new(
            range,
            &self.geometry,
            self.content_width,
            PAGE_GAP,
            rows,
            LayoutReports {
                measurements: self.measurements.clone(),
                generation: self.generation,
                counters: None,
            },
        );
        let generation = self.generation;
        content = content.push(crate::document_scroll::wrap(
            Message::ZoomBy,
            scrollable(visible)
                .id(scroll_id())
                .direction(scrollable::Direction::Both {
                    vertical: ui::scrollbar(),
                    horizontal: ui::scrollbar(),
                })
                .width(Length::Fill)
                .height(Length::Fill)
                .on_scroll(move |viewport| {
                    let offset = viewport.absolute_offset();
                    let bounds = viewport.bounds();
                    Message::Scroll {
                        generation,
                        x: offset.x,
                        y: offset.y,
                        left: bounds.x,
                        top: bounds.y,
                        width: bounds.width,
                        height: bounds.height,
                    }
                }),
        ));
        let status = if let Some(error) = &self.error {
            error.as_str()
        } else if self.copy_task.is_some() {
            "Copying PDF text…"
        } else if !self.document.can_copy {
            "Text copying is not permitted by this PDF"
        } else if self.failed.contains(&(page as u32)) {
            "Page is unavailable at this zoom; navigate or zoom to retry"
        } else if let Some(text) = self
            .cached
            .get(&(page as u32))
            .and_then(|entry| entry.text.as_ref())
        {
            if text.glyphs.is_empty() {
                "No selectable text on this page (scanned pages need OCR)"
            } else {
                ""
            }
        } else {
            "Loading PDF text…"
        };
        let status_style = if self.error.is_some() || self.failed.contains(&(page as u32)) {
            ui::danger_text
        } else {
            ui::muted_text
        };
        content = content.push(
            container(text(status).size(12).style(status_style))
                .width(Length::Fill)
                .padding([5, 14])
                .style(ui::header),
        );
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|theme| container::Style {
                background: Some(ui::palette(theme).background.into()),
                ..container::Style::default()
            })
            .into()
    }
}

fn valid_fraction(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
fn valid_dpi(value: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        1.0
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::future::Future;
    use std::task::{Context, Poll, Wake, Waker};

    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    pub(crate) fn complete<T>(future: impl Future<Output = T>) -> T {
        let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::park(),
            }
        }
    }

    /// One tall page with a line of text near its bottom.
    pub(crate) fn tall_pdf() -> Vec<u8> {
        let content = "BT /F1 18 Tf 20 300 Td (The old Lighthouse keeper) Tj ET";
        let objects = [
            "<</Type/Catalog/Pages 2 0 R>>".to_owned(),
            "<</Type/Pages/Kids[3 0 R]/Count 1>>".to_owned(),
            "<</Type/Page/Parent 2 0 R/MediaBox[0 0 300 3000]/Contents 4 0 R/Resources<</Font<</F1 5 0 R>>>>>>".to_owned(),
            format!("<</Length {}>>\nstream\n{content}\nendstream", content.len()),
            "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>".to_owned(),
        ];
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, body) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend(format!("{} 0 obj\n{body}\nendobj\n", index + 1).bytes());
        }
        let xref = pdf.len();
        pdf.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
        for offset in offsets {
            pdf.extend(format!("{offset:010} 00000 n \n").bytes());
        }
        pdf.extend(
            format!(
                "trailer\n<</Root 1 0 R/Size {}>>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .bytes(),
        );
        pdf
    }

    #[test]
    fn double_click_uses_source_word_boundaries_and_keeps_the_word_during_small_movements() {
        if !std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("pdfium.dll")
            .exists()
        {
            eprintln!("skipped: pdfium.dll is not beside the test executable");
            return;
        }
        let path = std::env::temp_dir().join(format!("simpl-pdf-word-{}.pdf", std::process::id()));
        std::fs::write(&path, tall_pdf()).unwrap();
        let document = complete(reader_pdf::open(path.clone())).unwrap();
        let (mut reader, _) = Reader::new(document.clone(), None, Size::new(1280.0, 800.0), 1.0);
        let width = reader.target_width(0);
        let layer = complete(document.session.text(0, width)).unwrap();
        let first = layer
            .glyphs
            .iter()
            .position(|glyph| layer.text[glyph.start..glyph.end] == *"L")
            .unwrap();
        reader.cache(0, width, None, Some(Arc::new(layer)));
        let _ = reader.update(Message::SelectStart(TextPoint {
            page: 0,
            index: first + 3,
        }));
        reader.select_word(TextPoint {
            page: 0,
            index: first + 3,
        });
        let _ = reader.update(Message::SelectMove(TextPoint {
            page: 0,
            index: first + 1,
        }));
        let selection = reader.selection().unwrap();
        assert_eq!(selection.anchor.index, first);
        assert_eq!(selection.focus.index, first + "Lighthouse".len() - 1);
        assert_eq!(
            complete(document.session.copy(selection)).unwrap(),
            "Lighthouse"
        );

        let text = "café 日本語 책";
        let glyphs = text
            .char_indices()
            .map(|(start, character)| reader_pdf::Glyph {
                start,
                end: start + character.len_utf8(),
                bounds: None,
                style: None,
            })
            .collect();
        reader.cache(
            0,
            width,
            None,
            Some(Arc::new(TextLayer::new(text.into(), glyphs))),
        );
        reader.select_word(TextPoint { page: 0, index: 3 });
        assert_eq!(reader.selection().unwrap().anchor.index, 0);
        assert_eq!(reader.selection().unwrap().focus.index, 3);
        let _ = reader.update(Message::SelectMove(TextPoint { page: 0, index: 2 }));
        assert_eq!(reader.selection().unwrap().focus.index, 3);
        let _ = reader.update(Message::SelectMove(TextPoint { page: 0, index: 9 }));
        assert_eq!(reader.selection().unwrap().focus.index, 9);
        assert!(reader.word_selection.is_none());
        reader.clear_selection();
        assert!(reader.selection().is_none());
        drop(reader);
        drop(document);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn text_failure_still_renders_the_page_and_does_not_retry_forever() {
        let dll = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("pdfium.dll");
        if !dll.exists() {
            return;
        }
        let path =
            std::env::temp_dir().join(format!("simpl-text-failure-{}.pdf", std::process::id()));
        std::fs::write(&path, tall_pdf()).unwrap();
        let document = complete(reader_pdf::open(path.clone())).unwrap();
        let (mut reader, _) = Reader::new(document.clone(), None, Size::new(1280.0, 800.0), 1.0);
        let (generation, page, width, text_request) = reader.in_flight.unwrap();
        assert!(text_request);
        let _ = reader.update(Message::TextReady {
            generation,
            page,
            width,
            result: Err("unreadable text layer".into()),
        });
        assert!(reader.text_failed.contains(&page));
        assert!(!reader.failed.contains(&page));
        assert_eq!(reader.in_flight, Some((generation, page, width, false)));
        let raster = complete(document.session.render(page, width)).unwrap();
        let reply = Arc::new(RenderReply {
            width: raster.width,
            bytes: raster.rgba.capacity(),
            handle: iced::widget::image::Handle::from_rgba(
                raster.width,
                raster.height,
                raster.rgba,
            ),
        });
        let _ = reader.update(Message::Rendered {
            generation,
            page,
            width,
            result: Ok(reply),
        });
        assert!(reader.cached.get(&page).unwrap().handle.is_some());
        assert!(reader.in_flight.is_none());
        let _ = reader.request_next();
        assert!(reader.in_flight.is_none());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn showing_a_match_highlights_it_and_scrolls_to_its_line() {
        let dll = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("pdfium.dll");
        if !dll.exists() {
            eprintln!("skipped: pdfium.dll is not beside the test executable");
            return;
        }
        let path = std::env::temp_dir().join(format!("simpl-pdf-find-{}.pdf", std::process::id()));
        std::fs::write(&path, tall_pdf()).unwrap();
        let document = complete(reader_pdf::open(path.clone())).unwrap();
        let (mut reader, _) = Reader::new(document.clone(), None, Size::new(1280.0, 800.0), 1.0);
        let text = complete(document.session.page_text(0)).unwrap();
        let first = text.chars().position(|c| c == 'L').unwrap();
        let last = first + "Lighthouse".len() - 1;

        // Before the page text is loaded the reader stays on the page and waits for it.
        let _ = reader.show_match(0, first, last);
        assert_eq!(reader.pending_reveal, Some((0, first, last)));
        assert_eq!(reader.offset.y, 0.0);

        let width = reader.target_width(0);
        let layer = complete(document.session.text(0, width)).unwrap();
        reader.cache(0, width, None, Some(Arc::new(layer)));
        let _ = reader.show_match(0, first, last);
        let _ = std::fs::remove_file(path);

        let selection = reader.selection.expect("the match is highlighted");
        assert_eq!(
            selection.anchor,
            TextPoint {
                page: 0,
                index: first
            }
        );
        assert_eq!(
            selection.focus,
            TextPoint {
                page: 0,
                index: last
            }
        );
        assert!(reader.selection_ready);
        let (y, height, _) = reader.glyph_position(0, first, last).unwrap();
        assert!(reader.offset.y > 0.0, "the page must scroll to the line");
        assert!(
            y >= reader.offset.y && y + height <= reader.offset.y + reader.viewport.height,
            "line at {y} must be inside the viewport starting at {}",
            reader.offset.y
        );
        // A match that is already in view is left where it is.
        let before = reader.offset.y;
        let _ = reader.show_match(0, first, last);
        assert_eq!(reader.offset.y, before);
        reader.clear_selection();
        assert!(reader.selection.is_none());
    }

    #[test]
    fn fit_width_shortcut_toggles_back_to_the_previous_zoom() {
        let dll = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("pdfium.dll");
        if !dll.exists() {
            eprintln!("skipped: pdfium.dll is not beside the test executable");
            return;
        }
        let path = std::env::temp_dir().join(format!("simpl-pdf-fit-{}.pdf", std::process::id()));
        std::fs::write(&path, tall_pdf()).unwrap();
        let document = complete(reader_pdf::open(path.clone())).unwrap();
        let _ = std::fs::remove_file(path);
        let (mut reader, _) = Reader::new(document, None, Size::new(1280.0, 800.0), 1.0);
        let _ = reader.update(Message::ZoomIn);
        let zoomed = reader.zoom;
        assert!(matches!(zoomed, PdfZoom::Scale(_)));
        let _ = reader.toggle_fit_width();
        assert_eq!(reader.zoom, PdfZoom::FitWidth);
        let _ = reader.toggle_fit_width();
        assert_eq!(reader.zoom, zoomed);
        // Already fitting when opened: the second press goes to 100%.
        let _ = reader.set_zoom(PdfZoom::FitWidth);
        reader.zoom_before_fit = None;
        let _ = reader.toggle_fit_width();
        assert_eq!(reader.zoom, PdfZoom::Scale(1.0));
    }
}
