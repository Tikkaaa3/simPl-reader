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
const INITIAL_CHROME: f32 = 184.0;
const COMPACT_TOOLBAR_WIDTH: f32 = 700.0;

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
    Copy,
}

fn focus_button<'a>(
    label: impl Into<Element<'a, Message>>,
    action: Option<Message>,
    focused: bool,
    selected: bool,
) -> Element<'a, Message> {
    button(label)
        .padding([7, 10])
        .on_press_maybe(action)
        .style(move |_, status| ui::button_style(status, ButtonTone::Quiet, focused, selected))
        .into()
}

#[derive(Debug)]
struct CachedPage {
    handle: iced::widget::image::Handle,
    text: Arc<TextLayer>,
    width: u32,
    bytes: usize,
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
    PageInput(String),
    PageSubmit,
    Previous,
    Next,
    ZoomIn,
    ZoomOut,
    ActualSize,
    FitWidth,
    SelectStart(TextPoint),
    SelectMove(TextPoint),
    ClearSelection,
    Copy,
    CopyFinished(Result<String, String>),
}

#[derive(Debug)]
pub struct RenderReply {
    handle: iced::widget::image::Handle,
    text: Arc<TextLayer>,
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
    viewport_top: f32,
    offset: Point,
    geometry: HeightIndex,
    measurements: iced_shell::virtual_reader::Measurements,
    content_width: f32,
    generation: u64,
    in_flight: Option<(u64, u32, u32)>,
    render_task: Option<iced::task::Handle>,
    copy_task: Option<iced::task::Handle>,
    cached: HashMap<u32, CachedPage>,
    cache_bytes: usize,
    clock: u64,
    failed: HashSet<u32>,
    range: std::ops::Range<usize>,
    error: Option<String>,
    page_input: String,
    editing_page: bool,
    selection: Option<Selection>,
    selection_ready: bool,
    select_all: bool,
    dragging: bool,
    pointer: Option<Point>,
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
            range: 0..0,
            error: None,
            page_input: "1".into(),
            editing_page: false,
            selection: None,
            selection_ready: false,
            select_all: false,
            dragging: false,
            pointer: None,
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

    pub fn copy_ready(&self) -> bool {
        self.copy_task.is_none()
            && self.document.can_copy
            && (self.select_all || self.selection_ready)
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
        self.error = None;
        self.generation = self.generation.wrapping_add(1);
    }

    fn set_zoom(&mut self, zoom: PdfZoom) -> Task<Message> {
        if self.zoom == zoom {
            return Task::none();
        }
        let anchor = self.anchor();
        self.zoom = zoom;
        self.clear_rasters();
        self.rebuild();
        self.set_anchor(anchor);
        Task::batch([scroll_to(self.offset.x, self.offset.y), self.request_next()])
    }

    fn adjust_zoom(&mut self, multiplier: f32) -> Task<Message> {
        let scale = match self.zoom {
            PdfZoom::Scale(scale) => scale,
            PdfZoom::FitWidth => {
                self.scale_for(self.document.pages[self.anchor().page as usize].width)
                    / POINT_TO_DIP
            }
        };
        self.set_zoom(PdfZoom::Scale(
            (scale * multiplier).clamp(MIN_ZOOM, MAX_ZOOM),
        ))
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
        if self.in_flight.is_some() {
            return Task::none();
        }
        let visible = self.visible_range();
        self.range = visible.clone();
        let center = self.offset.y + self.viewport.height / 2.0;
        let wanted = visible
            .filter(|&page| {
                !self.failed.contains(&(page as u32))
                    && self
                        .cached
                        .get(&(page as u32))
                        .is_none_or(|entry| entry.width != self.target_width(page))
            })
            .min_by(|&a, &b| {
                let da = (self.geometry.start(a) + self.page_size(a).height / 2.0 - center).abs();
                let db = (self.geometry.start(b) + self.page_size(b).height / 2.0 - center).abs();
                da.total_cmp(&db)
            });
        let Some(page) = wanted else {
            return Task::none();
        };
        let page = page as u32;
        let width = self.target_width(page as usize);
        let generation = self.generation;
        self.in_flight = Some((generation, page, width));
        let session = self.document.session.clone();
        let (task, handle) = Task::perform(
            async move {
                session.render(page, width).await.map(|rendered| {
                    let bytes = rendered.rgba.capacity()
                        + rendered.text.text.capacity()
                        + rendered.text.glyphs.capacity() * size_of::<reader_pdf::Glyph>();
                    Arc::new(RenderReply {
                        handle: iced::widget::image::Handle::from_rgba(
                            rendered.width,
                            rendered.height,
                            rendered.rgba,
                        ),
                        text: rendered.text,
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
        .abortable();
        self.render_task = Some(handle.abort_on_drop());
        task
    }

    fn cache(&mut self, page: u32, requested_width: u32, reply: &RenderReply) {
        if reply.bytes > CACHE_BUDGET {
            self.failed.insert(page);
            self.error = Some(format!(
                "Page {} exceeds the 32 MiB display cache limit",
                page + 1
            ));
            return;
        }
        if let Some(old) = self.cached.remove(&page) {
            self.cache_bytes -= old.bytes;
        }
        while self.cache_bytes + reply.bytes > CACHE_BUDGET || self.cached.len() >= MAX_CACHED_PAGES
        {
            let victim = self
                .cached
                .iter()
                .min_by_key(|(page, entry)| {
                    // Evict offscreen images before visible images, then least recently used.
                    (self.range.contains(&(**page as usize)), entry.used)
                })
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
        self.cached.insert(
            page,
            CachedPage {
                handle: reply.handle.clone(),
                text: reply.text.clone(),
                width: requested_width,
                bytes: reply.bytes,
                used: self.clock,
            },
        );
        self.cache_bytes += reply.bytes;
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
        self.error = None;
        self.range = self.visible_range();
        self.page_input = (page + 1).to_string();
        self.editing_page = false;
        Task::batch([scroll_to(self.offset.x, self.offset.y), self.request_next()])
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
                top,
                width,
                height,
            } if generation == self.generation => {
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
                if self.in_flight != Some((generation, page, width)) {
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
                            self.cache(page, width, &reply);
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
            Message::ZoomIn => self.adjust_zoom(1.25),
            Message::ZoomOut => self.adjust_zoom(0.8),
            Message::ActualSize => self.set_zoom(PdfZoom::Scale(1.0)),
            Message::FitWidth => self.set_zoom(PdfZoom::FitWidth),
            Message::SelectStart(point) => {
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
                if self.dragging
                    && let Some(selection) = &mut self.selection
                {
                    selection.focus = point;
                    self.selection_ready = true;
                }
                Task::none()
            }
            Message::ClearSelection => {
                self.selection = None;
                self.selection_ready = false;
                self.select_all = false;
                self.dragging = false;
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
        let Some(entry) = self.cached.get(&(page as u32)) else {
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
        if let Some(point) = pdf_page::hit(&entry.text, page as u32, bounds, pointer, false)
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
                            "f" => self.update(Message::FitWidth),
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
        self.error = None;
        Task::batch([scroll_to(self.offset.x, self.offset.y), self.request_next()])
    }

    pub fn view(&self, focused: Option<FocusControl>) -> Element<'_, Message> {
        let page = self.anchor().page as usize;
        let zoom_label = match self.zoom {
            PdfZoom::FitWidth => "Fit".to_owned(),
            PdfZoom::Scale(value) => format!("{:.0}%", value * 100.0),
        };
        let navigation = row![
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
                        style.border.color = ui::ACCENT;
                        style.border.width = 2.0;
                    }
                    style
                }),
            text(format!("of {}", self.document.pages.len()))
                .size(13)
                .color(ui::MUTED),
            focus_button(
                "Next",
                (page + 1 < self.document.pages.len()).then_some(Message::Next),
                focused == Some(FocusControl::Next),
                false,
            ),
        ]
        .spacing(7)
        .align_y(iced::Alignment::Center);
        let zoom = row![
            focus_button(
                "−",
                Some(Message::ZoomOut),
                focused == Some(FocusControl::ZoomOut),
                false,
            ),
            text(zoom_label).size(13).color(ui::MUTED),
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
                "Fit width",
                Some(Message::FitWidth),
                focused == Some(FocusControl::FitWidth),
                self.zoom == PdfZoom::FitWidth,
            ),
            iced::widget::Space::new().width(Length::Fill),
            focus_button(
                "Copy",
                self.copy_ready().then_some(Message::Copy),
                focused == Some(FocusControl::Copy),
                false,
            ),
        ]
        .spacing(7)
        .align_y(iced::Alignment::Center)
        .width(Length::Fill);
        // Keep one toolbar slot above the scrollable in both layouts. Only its
        // children change when the window crosses the compact breakpoint.
        let toolbar: Element<'_, Message> = if self.size.width < COMPACT_TOOLBAR_WIDTH {
            column![navigation, zoom].spacing(5).into()
        } else {
            row![navigation, zoom]
                .spacing(16)
                .align_y(iced::Alignment::Center)
                .into()
        };
        let mut content = column![
            container(toolbar)
                .width(Length::Fill)
                .padding([7, 12])
                .style(ui::header)
        ];
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
                    image: cached.map(|entry| &entry.handle),
                    text: cached.map(|entry| &entry.text),
                    selection: selected,
                    dragging: self.dragging,
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
        content = content.push(
            scrollable(visible)
                .id(scroll_id())
                .style(ui::scroll_style)
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
                        top: bounds.y,
                        width: bounds.width,
                        height: bounds.height,
                    }
                }),
        );
        let status = if let Some(error) = &self.error {
            error.as_str()
        } else if self.copy_task.is_some() {
            "Copying PDF text…"
        } else if !self.document.can_copy {
            "Text copying is not permitted by this PDF"
        } else if self.failed.contains(&(page as u32)) {
            "Page is unavailable at this zoom; navigate or zoom to retry"
        } else if let Some(entry) = self.cached.get(&(page as u32)) {
            if entry.text.glyphs.is_empty() {
                "No selectable text on this page (scanned pages need OCR)"
            } else {
                ""
            }
        } else {
            "Rendering page and loading text…"
        };
        let status_color = if self.error.is_some() || self.failed.contains(&(page as u32)) {
            ui::DANGER
        } else {
            ui::MUTED
        };
        content = content.push(
            container(text(status).size(12).color(status_color))
                .width(Length::Fill)
                .padding([5, 14])
                .style(ui::header),
        );
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(ui::BACKGROUND.into()),
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
