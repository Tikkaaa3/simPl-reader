//! The local workspace: persistent metadata, bounded visible covers, and the reference shelf.
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use iced::widget::{
    Space, button, column, container, image, opaque, progress_bar, row, scrollable, stack, text,
    text_input,
};
use iced::{
    Alignment, Background, Border, Color, ContentFit, Element, Length, Padding, Size, Task,
};
use reader_document::library::{self, Entry};
use reader_document::recent::DocumentKind;
use reader_document::shelves::{self, Shelves};

use crate::ui;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Recent,
    Title,
    Format,
    Shelf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Add,
    Resume(usize),
    Document(usize),
    FavouriteDocument(usize),
    Favourite(usize, bool),
    Remove(usize, bool),
    Sort(Sort),
    /// Show every book (`None`) or only one shelf.
    Filter(Option<u64>),
    NewShelf,
    RenameShelf,
    DeleteShelf,
    /// The shelf menu of a card; the flag marks the Favourites section.
    ShelfMenu(usize, bool),
    ToggleShelf(usize, bool, u64),
    MenuNewShelf(usize, bool),
}

#[derive(Clone, Debug)]
pub enum Message {
    Activate(Control),
    Hover(Option<Control>),
    Loaded(Result<Vec<Entry>, String>),
    Saved(Result<(), String>),
    Covers(Vec<(String, Option<image::Handle>)>),
    Scrolled { offset: f32, viewport: f32 },
    ShelvesLoaded(Result<Shelves, String>),
    ShelvesSaved(Result<(), String>),
    NameInput(String),
    NameSubmit,
    NameCancel,
}

/// What the shelf-name field is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NameTarget {
    /// A new shelf, optionally put straight onto a card's book.
    New {
        assign: Option<(usize, bool)>,
    },
    Rename(u64),
}

#[derive(Clone, Debug)]
struct Naming {
    target: NameTarget,
    value: String,
    error: Option<String>,
}

#[derive(Debug)]
pub struct Shelf {
    pub entries: Vec<Entry>,
    pub notice: Option<String>,
    pub loading: bool,
    pub saving: bool,
    pub dirty: bool,
    pub blocked: bool,
    pub visible: bool,
    pending: Vec<(Entry, Option<std::path::PathBuf>)>,
    covers: HashMap<String, Option<image::Handle>>,
    fetching: HashSet<String>,
    order: Vec<usize>,
    continuing: Vec<usize>,
    favourites: Vec<usize>,
    sort: Sort,
    hovered: Option<Control>,
    size: Size,
    offset: f32,
    viewport: f32,
    shelves: Shelves,
    shelves_loaded: bool,
    shelves_saving: bool,
    shelves_dirty: bool,
    shelves_blocked: bool,
    filter: Option<u64>,
    menu: Option<(usize, bool)>,
    naming: Option<Naming>,
}

impl Default for Shelf {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            notice: None,
            loading: true,
            saving: false,
            dirty: false,
            blocked: false,
            visible: true,
            pending: Vec::new(),
            covers: HashMap::new(),
            fetching: HashSet::new(),
            order: Vec::new(),
            continuing: Vec::new(),
            favourites: Vec::new(),
            sort: Sort::Recent,
            size: Size::new(1280.0, 800.0),
            hovered: None,
            offset: 0.0,
            viewport: 752.0,
            shelves: Shelves::default(),
            shelves_loaded: false,
            shelves_saving: false,
            shelves_dirty: false,
            shelves_blocked: false,
            filter: None,
            menu: None,
            naming: None,
        }
    }
}

impl Shelf {
    pub fn load() -> Task<Message> {
        Task::batch([
            Task::perform(async { library::load() }, Message::Loaded),
            Task::perform(async { shelves::load() }, Message::ShelvesLoaded),
        ])
    }

    /// Whether shelf changes are saved, so the app may exit.
    pub fn shelves_settled(&self) -> bool {
        !self.shelves_saving && !(self.shelves_dirty && !self.shelves_blocked)
    }

    /// Escape closes the shelf-name field, then the shelf menu.
    pub fn dismiss(&mut self) -> bool {
        if self.naming.take().is_some() {
            return true;
        }
        self.menu.take().is_some()
    }

    fn persist_shelves(&mut self) -> Task<Message> {
        if !self.shelves_loaded
            || self.shelves_saving
            || self.shelves_blocked
            || !self.shelves_dirty
        {
            return Task::none();
        }
        self.shelves_saving = true;
        self.shelves_dirty = false;
        let snapshot = self.shelves.clone();
        Task::perform(
            async move { shelves::save(&snapshot) },
            Message::ShelvesSaved,
        )
    }

    fn shelves_editable(&self) -> bool {
        self.shelves_loaded && !self.shelves_blocked
    }

    fn start_naming(&mut self, target: NameTarget) -> Task<Message> {
        if !self.shelves_editable() {
            return Task::none();
        }
        let value = match target {
            NameTarget::Rename(id) => self
                .shelves
                .get(id)
                .map_or_else(String::new, |shelf| shelf.name.clone()),
            NameTarget::New { .. } => String::new(),
        };
        self.naming = Some(Naming {
            target,
            value,
            error: None,
        });
        iced::widget::operation::focus(name_id())
    }

    fn submit_name(&mut self) -> Task<Message> {
        let Some(naming) = self.naming.as_mut() else {
            return Task::none();
        };
        let result = match naming.target {
            NameTarget::New { assign } => self.shelves.create(&naming.value).and_then(|id| {
                match assign.and_then(|(index, _)| self.entries.get(index)) {
                    Some(entry) => self
                        .shelves
                        .toggle(id, &entry.document.fingerprint)
                        .map(|_| ()),
                    None => Ok(()),
                }
            }),
            NameTarget::Rename(id) => self.shelves.rename(id, &naming.value),
        };
        match result {
            Ok(()) => {
                self.naming = None;
                self.shelves_dirty = true;
                self.reorder_keeping_menu();
                Task::batch([self.persist_shelves(), self.ensure_covers()])
            }
            Err(error) => {
                naming.error = Some(error);
                Task::none()
            }
        }
    }

    fn card_control(menu: (usize, bool)) -> Control {
        if menu.1 {
            Control::FavouriteDocument(menu.0)
        } else {
            Control::Document(menu.0)
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Hover(control) => {
                // The shelf menu belongs to its card: leaving the card closes it,
                // unless a new shelf is being named there.
                let naming_here = matches!(
                    self.naming,
                    Some(Naming {
                        target: NameTarget::New { assign: Some(_) },
                        ..
                    })
                );
                if let Some(menu) = self.menu
                    && control != Some(Self::card_control(menu))
                    && !naming_here
                {
                    self.menu = None;
                }
                self.hovered = control;
                Task::none()
            }
            Message::ShelvesLoaded(result) => {
                self.shelves_loaded = true;
                match result {
                    Ok(loaded) => self.shelves = loaded,
                    Err(error) => {
                        self.shelves_blocked = true;
                        self.notice = Some(format!(
                            "Cannot read your shelves: {error}. The file has not been changed."
                        ));
                    }
                }
                self.reorder();
                self.ensure_covers()
            }
            Message::ShelvesSaved(result) => {
                self.shelves_saving = false;
                if let Err(error) = result {
                    self.notice = Some(format!("Could not save your shelves: {error}"));
                }
                self.persist_shelves()
            }
            Message::NameInput(value) => {
                if let Some(naming) = &mut self.naming {
                    naming.value = value.chars().take(shelves::MAX_NAME_CHARS).collect();
                    naming.error = None;
                }
                Task::none()
            }
            Message::NameSubmit => self.submit_name(),
            Message::NameCancel => {
                self.naming = None;
                Task::none()
            }
            Message::Activate(Control::Filter(filter)) => {
                self.filter = filter;
                self.naming = None;
                self.reorder();
                self.ensure_covers()
            }
            Message::Activate(Control::NewShelf) => {
                self.menu = None;
                self.start_naming(NameTarget::New { assign: None })
            }
            Message::Activate(Control::RenameShelf) => match self.filter {
                Some(id) => self.start_naming(NameTarget::Rename(id)),
                None => Task::none(),
            },
            Message::Activate(Control::DeleteShelf) => {
                let Some(id) = self.filter.filter(|_| self.shelves_editable()) else {
                    return Task::none();
                };
                self.shelves.delete(id);
                self.filter = None;
                self.naming = None;
                self.shelves_dirty = true;
                self.reorder();
                Task::batch([self.persist_shelves(), self.ensure_covers()])
            }
            Message::Activate(Control::ShelfMenu(index, favourites)) => {
                self.menu = if self.menu == Some((index, favourites)) {
                    None
                } else {
                    Some((index, favourites))
                };
                self.naming = None;
                Task::none()
            }
            Message::Activate(Control::MenuNewShelf(index, favourites)) => {
                self.menu = Some((index, favourites));
                self.start_naming(NameTarget::New {
                    assign: Some((index, favourites)),
                })
            }
            Message::Activate(Control::ToggleShelf(index, _, id)) => {
                let Some(entry) = self.entries.get(index).filter(|_| self.shelves_editable())
                else {
                    return Task::none();
                };
                if let Err(error) = self.shelves.toggle(id, &entry.document.fingerprint) {
                    self.notice = Some(error);
                    return Task::none();
                }
                self.shelves_dirty = true;
                if self.filter == Some(id) {
                    self.reorder_keeping_menu();
                }
                Task::batch([self.persist_shelves(), self.ensure_covers()])
            }
            Message::Loaded(result) => {
                self.loading = false;
                match result {
                    Ok(entries) => {
                        self.entries = entries;
                        self.dirty = true;
                    }
                    Err(error) => {
                        self.blocked = true;
                        self.notice = Some(format!(
                            "Cannot read the library: {error}. The original file has not been changed."
                        ));
                    }
                }
                for (entry, old) in std::mem::take(&mut self.pending) {
                    self.remember_entry(entry, old.as_deref());
                }
                self.reorder_closing_menu();
                Task::batch([self.persist(), self.ensure_covers()])
            }
            Message::Saved(result) => {
                self.saving = false;
                if let Err(error) = result {
                    self.notice = Some(format!("Could not save the library: {error}"));
                } else {
                    self.notice = None;
                }
                self.persist()
            }
            Message::Covers(covers) => {
                let wanted = self.cover_keys();
                for (key, image) in covers {
                    self.fetching.remove(&key);
                    if wanted.contains(&key) {
                        self.covers.insert(key, image);
                    }
                }
                Task::none()
            }
            Message::Scrolled { offset, viewport } => {
                self.offset = offset;
                self.viewport = viewport;
                self.ensure_covers()
            }
            Message::Activate(Control::Favourite(index, _)) => {
                if let Some(entry) = self.entries.get_mut(index) {
                    entry.favourite = !entry.favourite;
                    self.dirty = true;
                    self.reorder_closing_menu();
                }
                Task::batch([self.persist(), self.ensure_covers()])
            }
            Message::Activate(Control::Sort(sort)) => {
                self.sort = sort;
                self.reorder();
                self.ensure_covers()
            }
            Message::Activate(_) => Task::none(),
        }
    }

    pub fn persist(&mut self) -> Task<Message> {
        if self.loading || self.saving || self.blocked || !self.dirty {
            return Task::none();
        }
        self.saving = true;
        self.dirty = false;
        let entries = self.entries.clone();
        Task::perform(async move { library::save(&entries) }, Message::Saved)
    }

    pub fn remember(&mut self, entry: Entry, old: Option<&Path>) -> Task<Message> {
        if self.loading {
            self.pending.push((entry, old.map(Path::to_path_buf)));
            return Task::none();
        }
        self.remember_entry(entry, old);
        self.reorder_closing_menu();
        Task::batch([self.persist(), self.ensure_covers()])
    }

    fn remember_entry(&mut self, entry: Entry, old: Option<&Path>) {
        if let Err(error) = library::remember(&mut self.entries, entry, old) {
            self.notice = Some(error);
            return;
        }
        self.dirty = true;
    }

    pub fn remove(&mut self, path: &Path) -> Task<Message> {
        let gone = library::remove(&mut self.entries, path);
        self.dirty = true;
        self.menu = None;
        if self.shelves_editable() {
            for fingerprint in gone {
                self.shelves_dirty |= self.shelves.forget(&fingerprint);
            }
        }
        self.reorder();
        Task::batch([self.persist(), self.persist_shelves(), self.ensure_covers()])
    }

    pub fn progress(&mut self, path: &Path, fraction: f32, current: u32, total: u32) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.document.path == path)
        {
            entry.progress = fraction.clamp(0.0, 1.0);
            entry.current = current;
            entry.total = total;
            self.dirty = true;
        }
        for (entry, _) in &mut self.pending {
            if entry.document.path == path {
                entry.progress = fraction.clamp(0.0, 1.0);
                entry.current = current;
                entry.total = total;
            }
        }
    }

    pub fn resize(&mut self, size: Size) -> Task<Message> {
        self.size = size;
        self.viewport = (size.height - 48.0).max(1.0);
        self.ensure_covers()
    }

    pub fn show(&mut self, visible: bool) -> Task<Message> {
        self.visible = visible;
        self.reorder();
        self.ensure_covers()
    }

    pub fn first_resume(&self) -> Option<usize> {
        self.continuing.first().copied()
    }

    pub fn controls(&self) -> impl Iterator<Item = Control> + Clone + '_ {
        std::iter::once(Control::Add)
            .chain(self.continuing.iter().copied().map(Control::Resume))
            .chain(SORTS.into_iter().map(Control::Sort))
            .chain(std::iter::once(Control::Filter(None)))
            .chain(
                self.shelves
                    .shelves
                    .iter()
                    .map(|shelf| Control::Filter(Some(shelf.id))),
            )
            .chain(self.shelves_editable().then_some(Control::NewShelf))
            .chain(
                self.filter
                    .filter(|_| self.shelves_editable())
                    .into_iter()
                    .flat_map(|_| [Control::RenameShelf, Control::DeleteShelf]),
            )
            .chain(
                self.order
                    .iter()
                    .copied()
                    .flat_map(|i| self.card_controls(i, false)),
            )
            .chain(
                self.favourites
                    .iter()
                    .copied()
                    .flat_map(|i| self.card_controls(i, true)),
            )
    }

    fn card_controls(&self, index: usize, favourites: bool) -> Vec<Control> {
        let mut controls = vec![
            if favourites {
                Control::FavouriteDocument(index)
            } else {
                Control::Document(index)
            },
            Control::ShelfMenu(index, favourites),
        ];
        if self.menu == Some((index, favourites)) {
            controls.extend(
                self.shelves
                    .shelves
                    .iter()
                    .map(|shelf| Control::ToggleShelf(index, favourites, shelf.id)),
            );
            if self.shelves_editable() {
                controls.push(Control::MenuNewShelf(index, favourites));
            }
        }
        controls.push(Control::Favourite(index, favourites));
        controls.push(Control::Remove(index, favourites));
        controls
    }

    pub fn reveal(&mut self, control: Control) -> Task<Message> {
        let metrics = self.metrics();
        let y = match control {
            Control::Document(index)
            | Control::Favourite(index, false)
            | Control::Remove(index, false)
            | Control::ShelfMenu(index, false)
            | Control::ToggleShelf(index, false, _)
            | Control::MenuNewShelf(index, false) => self
                .order
                .iter()
                .position(|value| *value == index)
                .map(|index| {
                    metrics.grid_top + (index / metrics.columns) as f32 * metrics.row_height
                }),
            Control::FavouriteDocument(index)
            | Control::Favourite(index, true)
            | Control::Remove(index, true)
            | Control::ShelfMenu(index, true)
            | Control::ToggleShelf(index, true, _)
            | Control::MenuNewShelf(index, true) => {
                self.favourites.iter().position(|i| *i == index).map(|i| {
                    self.favourites_top(&metrics)
                        + (i / metrics.columns) as f32 * metrics.row_height
                })
            }
            Control::Filter(_)
            | Control::NewShelf
            | Control::RenameShelf
            | Control::DeleteShelf
            | Control::Sort(_) => Some((metrics.grid_top - LIBRARY_HEADING_WIDE - 40.0).max(0.0)),
            _ => Some(0.0),
        };
        let Some(y) = y else {
            return Task::none();
        };
        let height = if matches!(
            control,
            Control::Document(_)
                | Control::FavouriteDocument(_)
                | Control::Favourite(..)
                | Control::Remove(..)
                | Control::ShelfMenu(..)
                | Control::ToggleShelf(..)
                | Control::MenuNewShelf(..)
        ) {
            metrics.card_height
        } else {
            0.0
        };
        let next = if y < self.offset {
            y
        } else if y + height > self.offset + self.viewport {
            (y + height - self.viewport).max(0.0)
        } else {
            return Task::none();
        };
        self.offset = next;
        Task::batch([
            self.ensure_covers(),
            iced::advanced::widget::operate(
                iced::advanced::widget::operation::scrollable::scroll_to(
                    scroll_id(),
                    iced::advanced::widget::operation::scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(next),
                    },
                ),
            ),
        ])
    }

    pub fn search(&self, query: &str) -> Vec<usize> {
        let query = query.trim().to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                (query.is_empty()
                    || entry.document.title.to_lowercase().contains(&query)
                    || entry
                        .author
                        .as_ref()
                        .is_some_and(|author| author.to_lowercase().contains(&query))
                    || entry
                        .document
                        .path
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().to_lowercase().contains(&query)))
                .then_some(index)
            })
            .collect()
    }

    fn reorder(&mut self) {
        self.continuing = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.total > 0 && entry.progress < 1.0)
            .map(|(index, _)| index)
            .take(3)
            .collect();
        self.order = (0..self.entries.len()).collect();
        if self.filter.is_some_and(|id| self.shelves.get(id).is_none()) {
            self.filter = None;
        }
        match self.sort {
            Sort::Recent => {}
            Sort::Title => self
                .order
                .sort_by_cached_key(|index| self.entries[*index].document.title.to_lowercase()),
            Sort::Format => self.order.sort_by_cached_key(|index| {
                (
                    format_name(self.entries[*index].format()),
                    self.entries[*index].document.title.to_lowercase(),
                )
            }),
            // Grouped by shelf in shelf order; books on no shelf come last.
            Sort::Shelf => self.order.sort_by_cached_key(|index| {
                let entry = &self.entries[*index];
                (
                    self.shelves
                        .shelves
                        .iter()
                        .position(|shelf| shelf.books.contains(&entry.document.fingerprint))
                        .unwrap_or(usize::MAX),
                    entry.document.title.to_lowercase(),
                )
            }),
        }
        // Favourites stay whole; only the library section follows the shelf filter.
        self.favourites = self
            .order
            .iter()
            .copied()
            .filter(|i| self.entries[*i].favourite)
            .collect();
        if let Some(shelf) = self.filter.and_then(|id| self.shelves.get(id)) {
            self.order
                .retain(|i| shelf.books.contains(&self.entries[*i].document.fingerprint));
        }
        self.hovered = None;
    }

    /// Reorder after entries or shelves change; card indexes may move, so the menu closes.
    fn reorder_closing_menu(&mut self) {
        self.menu = None;
        if matches!(
            self.naming,
            Some(Naming {
                target: NameTarget::New { assign: Some(_) },
                ..
            })
        ) {
            self.naming = None;
        }
        self.reorder();
    }

    /// Reorder after a shelf edit made from an open card menu.
    fn reorder_keeping_menu(&mut self) {
        let menu = self.menu;
        self.reorder();
        self.menu = menu.filter(|(index, favourites)| {
            if *favourites {
                self.favourites.contains(index)
            } else {
                self.order.contains(index)
            }
        });
    }

    fn metrics(&self) -> Metrics {
        let gutter = if self.size.width >= 1024.0 {
            48.0
        } else {
            24.0
        };
        let width = self.size.width.min(1240.0) - gutter * 2.0;
        let columns = if self.size.width >= 1024.0 {
            6
        } else if self.size.width >= 640.0 {
            3
        } else {
            2
        };
        let card_width = (width - 20.0 * (columns - 1) as f32) / columns as f32;
        let card_height = (card_width - 26.0) * 1.5 + 119.0;
        let wide = self.size.width >= 768.0;
        let continuation = if wide {
            146.0
        } else {
            (self.continuing.len().max(1) as f32 * 170.0) - 24.0
        };
        let grid_top = 40.0
            + if wide { 136.0 } else { 226.0 }
            + 56.0
            + 32.0
            + 20.0
            + continuation
            + 56.0
            + if wide {
                LIBRARY_HEADING_WIDE
            } else {
                LIBRARY_HEADING_WIDE + 40.0
            }
            + 24.0;
        Metrics {
            gutter,
            width,
            columns,
            card_width,
            card_height,
            row_height: card_height + 20.0,
            grid_top,
        }
    }

    fn favourites_top(&self, m: &Metrics) -> f32 {
        let height = (self.order.len().div_ceil(m.columns) as f32 * m.row_height - 20.0).max(0.0);
        m.grid_top + height + 48.0 + 56.0 + 49.0 + 24.0
    }
    fn section_rows(&self, metrics: &Metrics, count: usize, top: f32) -> std::ops::Range<usize> {
        let rows = count.div_ceil(metrics.columns);
        let first = (((self.offset - top).max(0.0) / metrics.row_height) as usize)
            .saturating_sub(1)
            .min(rows);
        let last = (((self.offset + self.viewport - top).max(0.0) / metrics.row_height).ceil()
            as usize
            + 2)
        .min(rows);
        first..last.max(first)
    }
    fn visible_rows(&self, metrics: &Metrics) -> std::ops::Range<usize> {
        self.section_rows(metrics, self.order.len(), metrics.grid_top)
    }

    fn cover_keys(&self) -> HashSet<String> {
        if !self.visible {
            return HashSet::new();
        }
        let metrics = self.metrics();
        let rows = self.visible_rows(&metrics);
        self.continuing
            .iter()
            .copied()
            .chain(
                self.order
                    .iter()
                    .copied()
                    .skip(rows.start * metrics.columns)
                    .take((rows.end - rows.start) * metrics.columns),
            )
            .chain({
                let rows = self.section_rows(
                    &metrics,
                    self.favourites.len(),
                    self.favourites_top(&metrics),
                );
                self.favourites
                    .iter()
                    .copied()
                    .skip(rows.start * metrics.columns)
                    .take((rows.end - rows.start) * metrics.columns)
            })
            .filter(|index| self.entries[*index].cover)
            .map(|index| self.entries[index].document.fingerprint.clone())
            .collect()
    }

    fn ensure_covers(&mut self) -> Task<Message> {
        let wanted = self.cover_keys();
        self.covers.retain(|key, _| wanted.contains(key));
        let missing: Vec<_> = wanted
            .into_iter()
            .filter(|key| !self.covers.contains_key(key) && !self.fetching.contains(key))
            .collect();
        if missing.is_empty() {
            return Task::none();
        }
        self.fetching.extend(missing.iter().cloned());
        Task::perform(
            async move {
                missing
                    .into_iter()
                    .map(|key| {
                        let handle = library::cached_cover(&key).ok().flatten().map(|asset| {
                            image::Handle::from_rgba(asset.width, asset.height, asset.rgba)
                        });
                        (key, handle)
                    })
                    .collect()
            },
            Message::Covers,
        )
    }

    pub fn view(
        &self,
        focused: Option<Control>,
        active: bool,
        dropping: bool,
    ) -> Element<'_, Message> {
        let m = self.metrics();
        let wide = self.size.width >= 768.0;
        let intro = column![
            row![
                container(Space::new().width(8).height(8))
                    .style(|theme| fill(ui::palette(theme).accent, 12.0)),
                label("LOCAL WORKSPACE", 11).style(ui::secondary_text)
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .height(14),
            text("simPl Reader")
                .font(ui::HEADING)
                .size(30)
                .line_height(iced::Pixels(38.0))
                .shaping(text::Shaping::Advanced),
            text("Quick, lightweight offline reader for curated thinking\nand serene study.")
                .font(ui::SANS)
                .size(16)
                .line_height(iced::Pixels(26.0))
                .style(ui::secondary_text)
                .shaping(text::Shaping::Advanced),
        ]
        .spacing(8)
        .width(448.0_f32.min(m.width));
        let add = card_button(
            row![
                container(icon("\u{e145}", 18).style(ui::accent_text))
                    .center(28)
                    .style(|theme| fill(ui::palette(theme).raised, 4.0)),
                column![
                    label("Add document", 13).style(ui::primary_text),
                    label("Drop .epub, .pdf, or .html", 11).style(ui::secondary_text)
                ]
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            Control::Add,
            focused,
            active,
            false,
        )
        .padding([12, 20])
        .width(224)
        .height(58)
        .style(move |theme, status| {
            card_style(
                theme,
                status,
                dropping || focused == Some(Control::Add),
                false,
            )
        });
        let hero: Element<'_, Message> = if wide {
            row![intro, Space::new().width(Length::Fill), add]
                .align_y(Alignment::Center)
                .into()
        } else {
            column![intro, add].spacing(32).into()
        };
        let continue_heading = row![
            text("Continue Reading")
                .font(ui::HEADING)
                .size(22)
                .line_height(iced::Pixels(32.0))
                .shaping(text::Shaping::Advanced),
            pill(format!("{} active", self.continuing.len())),
            Space::new().width(Length::Fill),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let continue_heading: Element<'_, Message> = if wide {
            continue_heading
                .push(label("Press Space to resume current", 11).style(ui::muted_text))
                .into()
        } else {
            continue_heading.into()
        };
        let continuing: Element<'_, Message> = if self.continuing.is_empty() {
            container(
                column![
                    text(if self.loading {
                        "Opening your workspace…"
                    } else {
                        "A quiet place to pick up where you left off."
                    })
                    .font(ui::SANS)
                    .size(18),
                    label(
                        "Add a document, or choose one from your library to begin.",
                        12
                    )
                    .style(ui::muted_text)
                ]
                .spacing(10),
            )
            .center_x(Length::Fill)
            .center_y(146)
            .style(|theme| fill(ui::palette(theme).surface, 8.0))
            .into()
        } else if wide {
            let mut cards = row![].spacing(24);
            let text_width = (m.width - 48.0) / 3.0 - CONTINUE_CHROME;
            for index in &self.continuing {
                cards = cards.push(self.continue_card(*index, focused, active, text_width));
            }
            for _ in self.continuing.len()..3 {
                cards = cards.push(Space::new().width(Length::FillPortion(1)));
            }
            cards.into()
        } else {
            column(self.continuing.iter().map(|index| {
                self.continue_card(*index, focused, active, m.width - CONTINUE_CHROME)
            }))
            .spacing(24)
            .into()
        };
        let continuation = column![continue_heading, continuing].spacing(20);
        let mut sorting = row![label("SORT:", 11).style(ui::muted_text)]
            .spacing(12)
            .align_y(Alignment::Center);
        let mut segments = row![].spacing(2);
        for sort in SORTS {
            let caption = match sort {
                Sort::Recent => "Recently Opened",
                Sort::Title => "Title",
                Sort::Format => "Format",
                Sort::Shelf => "Shelf",
            };
            let control = Control::Sort(sort);
            let focused = focused == Some(control);
            let selected = self.sort == sort;
            let segment = button(label(caption, 12))
                .padding([4, 10])
                .on_press_maybe(active.then_some(Message::Activate(control)))
                .style(move |theme, status| {
                    let tone = if selected {
                        ui::ButtonTone::Quiet
                    } else {
                        ui::ButtonTone::Subtle
                    };
                    let mut style = ui::button_style(theme, status, tone, focused, false);
                    style.border.radius = 2.0.into();
                    style
                });
            segments = segments.push(mark(segment, focused));
        }
        sorting = sorting.push(
            container(segments)
                .padding(2)
                .style(|theme| boxed(theme, ui::palette(theme).lowest, 0.3, 4.0)),
        );
        let heading = row![
            text("Library")
                .font(ui::HEADING)
                .size(22)
                .line_height(iced::Pixels(32.0))
                .shaping(text::Shaping::Advanced),
            pill(match self.filter {
                Some(_) => format!(
                    "{} of {}",
                    self.order.len(),
                    count(self.entries.len(), "item")
                ),
                None => count(self.order.len(), "item"),
            })
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let library_heading: Element<'_, Message> = if wide {
            row![heading, Space::new().width(Length::Fill), sorting]
                .align_y(Alignment::Center)
                .into()
        } else {
            column![heading, sorting].spacing(12).into()
        };
        let library_heading = column![
            library_heading,
            self.shelf_bar(focused, active),
            container(Space::new().height(1))
                .width(Length::Fill)
                .style(|theme| fill(ui::palette(theme).border.scale_alpha(0.2), 0.0))
        ]
        .spacing(16);
        let grid = self.grid(&self.order, &m, m.grid_top, focused, active, false);
        let library = container(column![library_heading, grid].spacing(24)).padding(Padding {
            bottom: 48.0,
            ..Padding::default()
        });
        let mut sections = column![
            container(hero).padding(Padding {
                top: 16.0,
                ..Padding::default()
            }),
            continuation,
            library
        ]
        .spacing(56);
        if !self.favourites.is_empty() {
            let heading = column![
                row![
                    text("Favourites")
                        .font(ui::HEADING)
                        .size(22)
                        .line_height(iced::Pixels(32.0)),
                    pill(count(self.favourites.len(), "item"))
                ]
                .spacing(12)
                .align_y(Alignment::Center),
                container(Space::new().height(1))
                    .width(Length::Fill)
                    .style(|theme| fill(ui::palette(theme).border.scale_alpha(0.2), 0.0))
            ]
            .spacing(16);
            sections = sections.push(
                column![
                    heading,
                    self.grid(
                        &self.favourites,
                        &m,
                        self.favourites_top(&m),
                        focused,
                        active,
                        true
                    )
                ]
                .spacing(24),
            );
        }
        let main = container(sections)
            .padding(Padding {
                top: 40.0,
                bottom: 40.0,
                left: m.gutter,
                right: m.gutter,
            })
            .width(Length::Fill)
            .max_width(1240);
        let footer_left =
            label("simPl Reader — Continuous Distraction-Free Synthesis", 11).style(ui::muted_text);
        let footer_right = row![
            label("Ctrl+K  Quick Switcher", 11).style(ui::muted_text),
            icon("\u{e86f}", 14).style(ui::muted_text),
            label("UTF-8 Engine", 11).style(ui::muted_text)
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let footer: Element<'_, Message> = if wide {
            row![footer_left, Space::new().width(Length::Fill), footer_right].into()
        } else {
            column![footer_left, footer_right].spacing(8).into()
        };
        let footer = container(
            container(footer)
                .padding(Padding {
                    top: 24.0,
                    bottom: 24.0,
                    left: m.gutter,
                    right: m.gutter,
                })
                .width(Length::Fill)
                .max_width(1240),
        )
        .center_x(Length::Fill)
        .style(|theme| fill(ui::palette(theme).lowest.scale_alpha(0.6), 0.0));
        scrollable(column![container(main).center_x(Length::Fill), footer])
            .id(scroll_id())
            .direction(ui::vertical_scrollbar())
            .style(ui::scroll_style)
            .on_scroll(|viewport| Message::Scrolled {
                offset: viewport.absolute_offset().y,
                viewport: viewport.bounds().height,
            })
            .height(Length::Fill)
            .into()
    }

    fn grid<'a>(
        &'a self,
        order: &[usize],
        m: &Metrics,
        top: f32,
        focused: Option<Control>,
        active: bool,
        favourites: bool,
    ) -> Element<'a, Message> {
        let range = self.section_rows(m, order.len(), top);
        let rows = order.len().div_ceil(m.columns);
        let grid_height = (rows as f32 * m.row_height - 20.0).max(0.0);
        let mut grid =
            column![Space::new().height((range.start as f32 * m.row_height).min(grid_height))];
        for row_index in range.clone() {
            let mut cards = row![].spacing(20);
            for index in order.iter().skip(row_index * m.columns).take(m.columns) {
                cards = cards.push(self.library_card(*index, m, focused, active, favourites));
            }
            grid = grid.push(cards);
            if row_index + 1 < rows {
                grid = grid.push(Space::new().height(20));
            }
        }
        let remaining = rows - range.end;
        grid = grid.push(Space::new().height(if remaining == 0 {
            0.0
        } else {
            remaining as f32 * m.row_height - 20.0
        }));
        if order.is_empty() {
            grid = grid.push(
                container(
                    label(
                        if self.entries.is_empty() {
                            "Your library is empty. Add local EPUB, PDF, or HTML files above."
                        } else if self.filter.is_some() {
                            "No books on this shelf yet. Open a book's ⋯ menu to add it."
                        } else {
                            "Your documents are on the reading desk above."
                        },
                        13,
                    )
                    .style(ui::muted_text),
                )
                .padding([32, 0]),
            );
        }
        grid.into()
    }

    fn continue_card(
        &self,
        index: usize,
        focused: Option<Control>,
        active: bool,
        text_width: f32,
    ) -> Element<'_, Message> {
        let entry = &self.entries[index];
        let control = Control::Resume(index);
        let hovered = self.hovered == Some(control);
        let heading = row![
            label(age(entry.opened_at), 11).style(ui::muted_text),
            Space::new().width(Length::Fill),
            label(format!("{}%", (entry.progress * 100.0).round() as u32), 11)
                .style(ui::accent_text)
        ]
        .align_y(Alignment::Center);
        let title = clipped(
            text(ellipsize(&entry.document.title, text_width, 16.0))
                .font(ui::SEMIBOLD)
                .size(16)
                .line_height(iced::Pixels(22.0))
                .style(move |theme| iced::widget::text::Style {
                    color: Some(if hovered {
                        ui::palette(theme).accent
                    } else {
                        ui::palette(theme).text
                    }),
                })
                .shaping(text::Shaping::Advanced)
                .wrapping(text::Wrapping::None),
        );
        let author = clipped(
            text(ellipsize(&author_of(entry), text_width, 14.0))
                .size(14)
                .line_height(iced::Pixels(22.0))
                .style(ui::secondary_text)
                .shaping(text::Shaping::Advanced)
                .wrapping(text::Wrapping::None),
        );
        let metadata = column![heading, title, author].spacing(4);
        let caption = if entry.total > 0 {
            format!("Page {} of {}", entry.current, entry.total)
        } else {
            "Saved reading position".to_owned()
        };
        let progress = column![
            progress_bar(0.0..=1.0, entry.progress)
                .girth(4)
                .style(|theme| progress_bar::Style {
                    background: ui::palette(theme).lowest.into(),
                    bar: ui::palette(theme).accent.into(),
                    border: Border::default().rounded(12)
                }),
            row![
                clipped(label(caption, 12).style(ui::muted_text)),
                icon("\u{e5c8}", 16).style(ui::secondary_text)
            ]
            .align_y(Alignment::Center)
        ]
        .spacing(8);
        let details = column![metadata, Space::new().height(Length::Fill), progress]
            .height(112)
            .width(Length::Fill)
            .padding([2, 0]);
        let content = row![self.cover(entry, 80.0, 112.0, true, hovered), details]
            .spacing(16)
            .align_y(Alignment::Center);
        let button = card_button(content, control, focused, active, false)
            .padding(16)
            .width(Length::FillPortion(1))
            .height(146);
        iced::widget::mouse_area(mark(button, focused == Some(control)))
            .on_enter(Message::Hover(Some(control)))
            .on_exit(Message::Hover(None))
            .into()
    }

    fn library_card(
        &self,
        index: usize,
        m: &Metrics,
        focused: Option<Control>,
        active: bool,
        favourites: bool,
    ) -> Element<'_, Message> {
        let entry = &self.entries[index];
        let control = if favourites {
            Control::FavouriteDocument(index)
        } else {
            Control::Document(index)
        };
        let menu_open = self.menu == Some((index, favourites));
        let hovered = self.hovered == Some(control)
            || menu_open
            || focused.is_some_and(|focused| match focused {
                Control::ShelfMenu(i, f)
                | Control::ToggleShelf(i, f, _)
                | Control::MenuNewShelf(i, f)
                | Control::Favourite(i, f)
                | Control::Remove(i, f) => i == index && f == favourites,
                other => other == control,
            });
        let cover_width = m.card_width - 26.0;
        let title = container(
            text(&entry.document.title)
                .font(ui::MEDIUM)
                .size(14)
                .line_height(iced::Pixels(17.5))
                .style(move |theme| iced::widget::text::Style {
                    color: Some(if hovered {
                        ui::palette(theme).accent
                    } else {
                        ui::palette(theme).text
                    }),
                })
                .shaping(text::Shaping::Advanced),
        )
        .max_height(35)
        .width(Length::Fill)
        .clip(true);
        let author = clipped(
            label(ellipsize(&author_of(entry), m.card_width - 26.0, 12.0), 12)
                .style(ui::secondary_text)
                .wrapping(text::Wrapping::None),
        );
        let on_shelves: Vec<&str> = self
            .shelves
            .of(&entry.document.fingerprint)
            .map(|shelf| shelf.name.as_str())
            .collect();
        let placed: Element<'_, Message> = if on_shelves.is_empty() {
            label(short_date(entry.opened_at), 11)
                .style(ui::muted_text)
                .into()
        } else {
            clipped(
                label(
                    ellipsize(&on_shelves.join(" · "), m.card_width - 26.0 - 56.0, 11.0),
                    11,
                )
                .style(ui::accent_text)
                .wrapping(text::Wrapping::None),
            )
        };
        let footer = row![
            placed,
            Space::new().width(Length::Fill),
            label(file_size(entry.byte_len), 11).style(ui::muted_text)
        ]
        .align_y(Alignment::Center);
        let details = column![title, author, Space::new().height(Length::Fill), footer]
            .spacing(4)
            .height(81);
        let content = column![
            self.cover(entry, cover_width, cover_width * 1.5, false, hovered),
            mark(details, focused == Some(control))
        ]
        .spacing(12);
        let button = card_button(content, control, focused, active, true)
            .padding(13)
            .width(m.card_width)
            .height(m.card_height);
        let mut card = stack![button];
        if hovered {
            let menu_control = Control::ShelfMenu(index, favourites);
            let actions = row![
                mark(
                    iced::widget::tooltip(
                        iced::widget::button(
                            text("⋯")
                                .font(iced::Font::with_name("Segoe UI Symbol"))
                                .size(20)
                        )
                        .padding([3, 7])
                        .on_press_maybe(active.then_some(Message::Activate(menu_control)))
                        .style(move |theme, status| ui::button_style(
                            theme,
                            status,
                            ui::ButtonTone::Surface,
                            focused == Some(menu_control),
                            menu_open
                        )),
                        label("Shelves", 12),
                        iced::widget::tooltip::Position::Bottom,
                    )
                    .gap(4)
                    .padding(6)
                    .style(ui::panel),
                    focused == Some(menu_control)
                ),
                mark(
                    iced::widget::button(
                        text(if entry.favourite { "★" } else { "☆" })
                            .font(iced::Font::with_name("Segoe UI Symbol"))
                            .size(20)
                    )
                    .padding([3, 7])
                    .on_press_maybe(
                        active.then_some(Message::Activate(Control::Favourite(index, favourites)))
                    )
                    .style(move |theme, status| ui::button_style(
                        theme,
                        status,
                        ui::ButtonTone::Surface,
                        focused == Some(Control::Favourite(index, favourites)),
                        entry.favourite
                    )),
                    focused == Some(Control::Favourite(index, favourites))
                ),
                mark(
                    iced::widget::button(text("×").size(20))
                        .padding([3, 7])
                        .on_press_maybe(
                            active.then_some(Message::Activate(Control::Remove(index, favourites)))
                        )
                        .style(move |theme, status| ui::button_style(
                            theme,
                            status,
                            ui::ButtonTone::Surface,
                            focused == Some(Control::Remove(index, favourites)),
                            false
                        )),
                    focused == Some(Control::Remove(index, favourites))
                )
            ]
            .spacing(4);
            card = card.push(
                container(actions)
                    .width(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Right)
                    .padding(8),
            );
        }
        if menu_open {
            card = card.push(
                container(self.shelf_menu(index, favourites, m, focused, active))
                    .width(m.card_width)
                    .padding(Padding {
                        top: 46.0,
                        left: 8.0,
                        right: 8.0,
                        bottom: 8.0,
                    }),
            );
        }
        iced::widget::mouse_area(card)
            .on_enter(Message::Hover(Some(control)))
            .on_exit(Message::Hover(None))
            .into()
    }

    /// The shelf filter strip under the Library heading.
    fn shelf_bar(&self, focused: Option<Control>, active: bool) -> Element<'_, Message> {
        let chip = |control: Control, caption: String, amount: usize, selected: bool| {
            let focus = focused == Some(control);
            mark(
                button(
                    row![
                        label(caption, 12),
                        label(amount.to_string(), 11).style(if selected {
                            ui::accent_text
                        } else {
                            ui::muted_text
                        })
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .padding([5, 12])
                .on_press_maybe(active.then_some(Message::Activate(control)))
                .style(move |theme, status| {
                    let tone = if selected {
                        ui::ButtonTone::Quiet
                    } else {
                        ui::ButtonTone::Surface
                    };
                    let mut style = ui::button_style(theme, status, tone, focus, false);
                    style.border.radius = 14.0.into();
                    style
                }),
                focus,
            )
        };
        let mut chips = row![].spacing(6).align_y(Alignment::Center).push(chip(
            Control::Filter(None),
            "All".into(),
            self.entries.len(),
            self.filter.is_none(),
        ));
        for shelf in &self.shelves.shelves {
            let amount = self
                .entries
                .iter()
                .filter(|entry| shelf.books.contains(&entry.document.fingerprint))
                .count();
            chips = chips.push(chip(
                Control::Filter(Some(shelf.id)),
                shelf.name.clone(),
                amount,
                self.filter == Some(shelf.id),
            ));
        }
        let quiet = |control: Control, caption: &'static str, tone: ui::ButtonTone| {
            let focus = focused == Some(control);
            mark(
                button(label(caption, 12))
                    .padding([5, 10])
                    .on_press_maybe(active.then_some(Message::Activate(control)))
                    .style(move |theme, status| {
                        ui::button_style(theme, status, tone, focus, false)
                    }),
                focus,
            )
        };
        match &self.naming {
            Some(naming) if !matches!(naming.target, NameTarget::New { assign: Some(_) }) => {
                chips = chips.push(self.name_field(naming, 180.0));
            }
            _ if self.shelves_editable() => {
                chips = chips.push(quiet(
                    Control::NewShelf,
                    "+ New shelf",
                    ui::ButtonTone::Subtle,
                ));
                if self.filter.is_some() {
                    chips = chips
                        .push(
                            container(Space::new().width(1).height(16))
                                .style(|theme| fill(ui::palette(theme).border, 0.0)),
                        )
                        .push(quiet(
                            Control::RenameShelf,
                            "Rename",
                            ui::ButtonTone::Subtle,
                        ))
                        .push(quiet(
                            Control::DeleteShelf,
                            "Delete shelf",
                            ui::ButtonTone::Destructive,
                        ));
                }
            }
            _ => {}
        }
        container(
            scrollable(chips)
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::new().width(2).scroller_width(2),
                ))
                .style(ui::scroll_style)
                .width(Length::Fill),
        )
        .height(SHELF_BAR)
        .align_y(Alignment::Center)
        .into()
    }

    /// The shelf-name field with its confirm and cancel actions and any error.
    fn name_field<'a>(&'a self, naming: &'a Naming, width: f32) -> Element<'a, Message> {
        let confirm = match naming.target {
            NameTarget::Rename(_) => "Rename",
            NameTarget::New { .. } => "Create",
        };
        let small = |caption: &'static str, message: Message, tone: ui::ButtonTone| {
            button(label(caption, 12))
                .padding([5, 10])
                .on_press(message)
                .style(move |theme, status| ui::button_style(theme, status, tone, false, false))
        };
        let mut field = row![
            text_input("Shelf name", &naming.value)
                .id(name_id())
                .on_input(Message::NameInput)
                .on_submit(Message::NameSubmit)
                .font(ui::SANS)
                .size(12)
                .padding([5, 10])
                .width(width)
                .style(ui::input_style),
            small(confirm, Message::NameSubmit, ui::ButtonTone::Quiet),
            small("Cancel", Message::NameCancel, ui::ButtonTone::Subtle),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if let Some(error) = &naming.error {
            field = field.push(label(error.as_str(), 11).style(ui::danger_text));
        }
        field.into()
    }

    /// A card's shelf menu: tick shelves on and off, or make a new one for this book.
    fn shelf_menu(
        &self,
        index: usize,
        favourites: bool,
        m: &Metrics,
        focused: Option<Control>,
        active: bool,
    ) -> Element<'_, Message> {
        let Some(entry) = self.entries.get(index) else {
            return Space::new().into();
        };
        let width = m.card_width - 16.0;
        let mut items =
            column![container(label("Shelves", 11).style(ui::muted_text)).padding([4, 8])]
                .spacing(2);
        if self.shelves.shelves.is_empty() {
            items = items.push(
                container(
                    label("No shelves yet. Make one below.", 12)
                        .style(ui::secondary_text)
                        .wrapping(text::Wrapping::WordOrGlyph),
                )
                .padding([4, 8]),
            );
        }
        for shelf in &self.shelves.shelves {
            let control = Control::ToggleShelf(index, favourites, shelf.id);
            let on = shelf.books.contains(&entry.document.fingerprint);
            let focus = focused == Some(control);
            items = items.push(mark(
                button(
                    row![
                        text(if on { "✓" } else { "" })
                            .font(iced::Font::with_name("Segoe UI Symbol"))
                            .size(13)
                            .style(ui::accent_text)
                            .width(16),
                        label(ellipsize(&shelf.name, width - 52.0, 12.0), 12)
                            .wrapping(text::Wrapping::None),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .padding([6, 8])
                .on_press_maybe(active.then_some(Message::Activate(control)))
                .style(move |theme, status| {
                    ui::button_style(theme, status, ui::ButtonTone::Subtle, focus, false)
                }),
                focus,
            ));
        }
        items = items.push(
            container(Space::new().height(1))
                .width(Length::Fill)
                .style(ui::rule),
        );
        match &self.naming {
            Some(naming)
                if naming.target
                    == (NameTarget::New {
                        assign: Some((index, favourites)),
                    }) =>
            {
                items = items.push(self.menu_name_field(naming));
            }
            _ if self.shelves_editable() => {
                let control = Control::MenuNewShelf(index, favourites);
                let focus = focused == Some(control);
                items = items.push(mark(
                    button(label("+ New shelf…", 12).style(ui::accent_text))
                        .width(Length::Fill)
                        .padding([6, 8])
                        .on_press_maybe(active.then_some(Message::Activate(control)))
                        .style(move |theme, status| {
                            ui::button_style(theme, status, ui::ButtonTone::Subtle, focus, false)
                        }),
                    focus,
                ));
            }
            _ => {}
        }
        opaque(
            container(
                scrollable(items)
                    .direction(ui::vertical_scrollbar())
                    .style(ui::scroll_style),
            )
            .padding(6)
            .width(width)
            .max_height(m.card_height - 54.0)
            .style(|theme| container::Style {
                shadow: iced::Shadow::default(),
                ..ui::panel(theme)
            }),
        )
    }

    /// The narrow in-menu variant of the shelf-name field.
    fn menu_name_field<'a>(&'a self, naming: &'a Naming) -> Element<'a, Message> {
        let mut field = column![
            text_input("Shelf name", &naming.value)
                .id(name_id())
                .on_input(Message::NameInput)
                .on_submit(Message::NameSubmit)
                .font(ui::SANS)
                .size(12)
                .padding([5, 8])
                .width(Length::Fill)
                .style(ui::input_style),
            row![
                button(label("Create", 12))
                    .padding([4, 10])
                    .on_press(Message::NameSubmit)
                    .style(|theme, status| ui::button_style(
                        theme,
                        status,
                        ui::ButtonTone::Quiet,
                        false,
                        false
                    )),
                button(label("Cancel", 12))
                    .padding([4, 10])
                    .on_press(Message::NameCancel)
                    .style(|theme, status| ui::button_style(
                        theme,
                        status,
                        ui::ButtonTone::Subtle,
                        false,
                        false
                    )),
            ]
            .spacing(6),
        ]
        .spacing(6)
        .padding([4, 4]);
        if let Some(error) = &naming.error {
            field = field.push(
                label(error.as_str(), 11)
                    .style(ui::danger_text)
                    .wrapping(text::Wrapping::WordOrGlyph),
            );
        }
        field.into()
    }

    fn cover<'a>(
        &'a self,
        entry: &'a Entry,
        width: f32,
        height: f32,
        small: bool,
        hovered: bool,
    ) -> Element<'a, Message> {
        let artwork: Element<'_, Message> =
            if let Some(Some(handle)) = self.covers.get(&entry.document.fingerprint) {
                image(handle.clone())
                    .width(width)
                    .height(height)
                    .content_fit(ContentFit::Cover)
                    .border_radius(4)
                    .scale(if hovered { 1.05_f32 } else { 1.0_f32 })
                    .opacity(if small && !hovered { 0.9_f32 } else { 1.0_f32 })
                    .into()
            } else {
                // A typographic jacket for a document without artwork, not an invented book cover.
                // A tint derived from the title tells jackets apart without inventing art.
                let tint = jacket_tint(&entry.document.title);
                let size = jacket_title_size(
                    &entry.document.title,
                    width - if small { 20.0 } else { 36.0 },
                    if small { 12.0 } else { 19.0 },
                );
                let title = text(&entry.document.title)
                    .font(ui::HEADING)
                    .size(size)
                    .line_height(iced::Pixels(size * if small { 1.33 } else { 1.37 }))
                    .style(ui::primary_text)
                    .shaping(text::Shaping::Advanced)
                    .wrapping(text::Wrapping::WordOrGlyph);
                let mut jacket = column![
                    container(Space::new().height(2))
                        .width(24)
                        .style(move |_| fill(tint, 0.0)),
                    container(title).max_height(height * 0.55).clip(true),
                    Space::new().height(Length::Fill),
                ]
                .spacing(if small { 8 } else { 16 });
                if !small {
                    jacket = jacket.push(
                        container(
                            text(author_of(entry))
                                .font(ui::SANS)
                                .size(12)
                                .style(ui::secondary_text)
                                .shaping(text::Shaping::Advanced),
                        )
                        .max_height(34)
                        .clip(true),
                    );
                }
                container(jacket)
                    .padding(if small { 10 } else { 18 })
                    .width(width)
                    .height(height)
                    .style(move |theme| {
                        let paper = ui::palette(theme).surface;
                        container::Style {
                            background: Some(ui::mix(paper, tint, 0.14).into()),
                            border: Border {
                                color: ui::mix(paper, tint, 0.3),
                                width: 1.0,
                                radius: 4.0.into(),
                            },
                            ..container::Style::default()
                        }
                    })
                    .into()
            };
        let badge = container(
            label(format_name(entry.format()), 10)
                .font(ui::SEMIBOLD)
                .style(move |theme| iced::widget::text::Style {
                    color: Some(if entry.format() == DocumentKind::Epub {
                        ui::palette(theme).accent
                    } else {
                        ui::palette(theme).secondary
                    }),
                }),
        )
        .padding([2, 6])
        .style(|theme| fill(ui::palette(theme).background.scale_alpha(0.9), 2.0));
        let overlay = container(badge)
            .padding(if small { 6 } else { 8 })
            .width(width)
            .height(height)
            .align_y(if small {
                iced::alignment::Vertical::Bottom
            } else {
                iced::alignment::Vertical::Top
            });
        // Put artwork on a clipped renderer layer; Container::clip only narrows its viewport.
        let mut cover = stack![Space::new().width(width).height(height), artwork, overlay]
            .width(width)
            .height(height)
            .clip(true);
        if !small && hovered {
            let open = container(icon("\u{ea19}", 16).style(ui::accent_text))
                .center(28)
                .style(|theme| fill(ui::palette(theme).raised.scale_alpha(0.9), 14.0));
            cover = cover.push(
                container(open)
                    .width(width)
                    .height(height)
                    .padding(8)
                    .align_x(iced::alignment::Horizontal::Right)
                    .align_y(iced::alignment::Vertical::Bottom),
            );
        }
        cover.into()
    }
}

struct Metrics {
    gutter: f32,
    width: f32,
    columns: usize,
    card_width: f32,
    card_height: f32,
    row_height: f32,
    grid_top: f32,
}

pub fn format_name(kind: DocumentKind) -> &'static str {
    match kind {
        DocumentKind::Html => "HTML",
        DocumentKind::Pdf => "PDF",
        DocumentKind::Epub => "EPUB",
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn age(opened: u64) -> String {
    if opened == 0 {
        return "Previously opened".into();
    }
    match now().saturating_sub(opened) / 86400 {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        days if days < 7 => format!("{days} days ago"),
        _ => short_date(opened),
    }
}

fn short_date(timestamp: u64) -> String {
    if timestamp == 0 {
        return "Not recorded".into();
    }
    // Gregorian civil date from whole UTC days; no locale/runtime dependency.
    let z = timestamp / 86400 + 719468;
    let era = z / 146097;
    let day = z - era * 146097;
    let year = (day - day / 1460 + day / 36524 - day / 146096) / 365;
    let day_of_year = day - (365 * year + year / 4 - year / 100);
    let month = (5 * day_of_year + 2) / 153;
    let d = day_of_year - (153 * month + 2) / 5 + 1;
    let m = if month < 10 { month + 3 } else { month - 9 };
    format!(
        "{} {d:02}",
        [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"
        ][(m - 1) as usize]
    )
}

fn file_size(bytes: u64) -> String {
    if bytes == 0 {
        String::new()
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

const SORTS: [Sort; 4] = [Sort::Recent, Sort::Title, Sort::Format, Sort::Shelf];
/// Height of the shelf filter strip.
const SHELF_BAR: f32 = 30.0;
/// Library heading, shelf strip and rule, with their spacing, on a wide window.
const LIBRARY_HEADING_WIDE: f32 = 32.0 + 16.0 + SHELF_BAR + 16.0 + 1.0;

fn name_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("shelf-name")
}

/// Card padding, cover and gap beside the text of a Continue Reading card.
const CONTINUE_CHROME: f32 = 32.0 + 80.0 + 16.0;

fn author_of(entry: &Entry) -> String {
    entry
        .author
        .as_deref()
        .map_or_else(|| "Local document".to_owned(), display_author)
}

/// Catalog names such as "Austen, Jane, 1775-1817" read as "Jane Austen".
fn display_author(author: &str) -> String {
    let parts: Vec<&str> = author.split(',').map(str::trim).collect();
    let dates = |part: &str| {
        part.chars().any(|c| c.is_ascii_digit())
            && part
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '-' | '–' | '?' | ' '))
    };
    match parts.as_slice() {
        [last, first, years] if dates(years) && !first.is_empty() && !last.is_empty() => {
            format!("{first} {last}")
        }
        [name, years] if dates(years) && !name.is_empty() => (*name).to_owned(),
        _ => author.to_owned(),
    }
}

/// Shortens `value` with an ellipsis when it would overflow `width` at `size`.
/// iced 0.14 cannot ellipsize text, so this estimates an average glyph width.
fn ellipsize(value: &str, width: f32, size: f32) -> String {
    let fits = ((width / (size * 0.56)).floor() as usize).max(2);
    if value.chars().count() <= fits {
        return value.to_owned();
    }
    let kept: String = value.chars().take(fits - 1).collect();
    format!("{}…", kept.trim_end())
}

/// The jacket title size: the preferred size, smaller when the longest word
/// would otherwise break mid-word. Uses the same glyph estimate as [`ellipsize`].
fn jacket_title_size(title: &str, width: f32, preferred: f32) -> f32 {
    let longest = title
        .split_whitespace()
        .map(|word| word.chars().count())
        .max()
        .unwrap_or(0);
    if longest == 0 {
        return preferred;
    }
    (width / (longest as f32 * 0.58)).clamp(preferred * 0.6, preferred)
}

/// A stable, quiet hue per title for jackets without artwork.
fn jacket_tint(title: &str) -> Color {
    const HUES: [u32; 7] = [
        0x58a6ff, 0xd29922, 0x3fb950, 0xdb61a2, 0xa371f7, 0xf0883e, 0x39c5cf,
    ];
    // FNV-1a keeps the choice identical across runs and platforms.
    let hash = title.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    let hue = HUES[hash as usize % HUES.len()];
    Color::from_rgb8((hue >> 16) as u8, (hue >> 8) as u8, hue as u8)
}

pub fn icon<'a>(glyph: &'a str, size: u32) -> iced::widget::Text<'a> {
    text(glyph)
        .font(ui::ICONS)
        .size(size)
        .line_height(text::LineHeight::Relative(1.0))
        .shaping(text::Shaping::Advanced)
}

fn label<'a>(value: impl Into<std::borrow::Cow<'a, str>>, size: u32) -> iced::widget::Text<'a> {
    text(value.into())
        .font(ui::MEDIUM)
        .size(size)
        .line_height(iced::Pixels(if size == 13 {
            18.0
        } else if size == 12 {
            16.0
        } else {
            14.0
        }))
        .shaping(text::Shaping::Advanced)
}

fn clipped<'a>(value: iced::widget::Text<'a>) -> Element<'a, Message> {
    container(value).width(Length::Fill).clip(true).into()
}
fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

fn pill(value: String) -> Element<'static, Message> {
    container(label(value, 11).style(ui::secondary_text))
        .padding([2, 8])
        .style(|theme| boxed(theme, ui::palette(theme).surface, 0.8, 12.0))
        .into()
}
fn scroll_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("workspace-library")
}

fn fill(color: Color, radius: f32) -> container::Style {
    container::Style {
        background: Some(color.into()),
        border: Border::default().rounded(radius),
        ..container::Style::default()
    }
}
fn boxed(theme: &iced::Theme, color: Color, alpha: f32, radius: f32) -> container::Style {
    container::Style {
        border: Border {
            color: ui::palette(theme).border.scale_alpha(alpha),
            width: 1.0,
            radius: radius.into(),
        },
        ..fill(color, radius)
    }
}

fn card_style(
    theme: &iced::Theme,
    status: button::Status,
    focused: bool,
    muted: bool,
) -> button::Style {
    let hover = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(if hover {
            ui::palette(theme).raised
        } else if muted {
            ui::palette(theme).background
        } else {
            ui::palette(theme).surface
        })),
        text_color: ui::palette(theme).text,
        border: Border {
            radius: 8.0.into(),
            width: 1.0,
            color: if focused {
                ui::palette(theme).accent
            } else {
                ui::palette(theme)
                    .border
                    .scale_alpha(if hover { 0.5 } else { 0.2 })
            },
        },
        ..button::Style::default()
    }
}

fn card_button<'a>(
    content: impl Into<Element<'a, Message>>,
    control: Control,
    focused: Option<Control>,
    active: bool,
    muted: bool,
) -> iced::widget::Button<'a, Message> {
    button(content)
        .on_press_maybe(active.then_some(Message::Activate(control)))
        .style(move |theme, status| card_style(theme, status, focused == Some(control), muted))
}

fn mark<'a>(content: impl Into<Element<'a, Message>>, focused: bool) -> Element<'a, Message> {
    let content = content.into();
    if focused {
        container(content)
            .id(iced::advanced::widget::Id::new(ui::FOCUSED_CONTROL))
            .into()
    } else {
        content
    }
}

#[cfg(test)]
pub(crate) fn sample_entries(titles: &[(&str, DocumentKind)]) -> Vec<Entry> {
    titles
        .iter()
        .enumerate()
        .map(|(index, (title, kind))| Entry {
            document: reader_document::recent::Entry {
                path: std::env::temp_dir().join(format!("{title}.book")),
                title: (*title).into(),
                fingerprint: format!("{:064x}", index + 1),
                kind: *kind,
            },
            author: Some("Sample Author".into()),
            byte_len: 2048,
            opened_at: 0,
            progress: 0.0,
            current: 0,
            total: 0,
            cover: false,
            favourite: false,
            source_kind: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shelf_with(titles: &[&str]) -> Shelf {
        let mut shelf = Shelf::default();
        let entries = sample_entries(
            &titles
                .iter()
                .map(|title| (*title, DocumentKind::Epub))
                .collect::<Vec<_>>(),
        );
        let _ = shelf.update(Message::ShelvesLoaded(Ok(Shelves::default())));
        let _ = shelf.update(Message::Loaded(Ok(entries)));
        shelf
    }

    fn name(shelf: &mut Shelf, start: Control, value: &str) {
        let _ = shelf.update(Message::Activate(start));
        let _ = shelf.update(Message::NameInput(value.into()));
        let _ = shelf.update(Message::NameSubmit);
    }

    fn id_of(shelf: &Shelf, name: &str) -> u64 {
        shelf
            .shelves
            .shelves
            .iter()
            .find(|shelf| shelf.name == name)
            .unwrap()
            .id
    }

    #[test]
    fn shelves_are_made_assigned_filtered_and_sorted() {
        let mut shelf = shelf_with(&["Alpha", "Beta", "Gamma"]);
        name(&mut shelf, Control::NewShelf, "Ders");
        let ders = id_of(&shelf, "Ders");
        assert!(shelf.naming.is_none());

        // A new shelf made from a card's menu takes that book at once.
        let _ = shelf.update(Message::Activate(Control::ShelfMenu(1, false)));
        assert!(
            shelf
                .controls()
                .any(|c| c == Control::MenuNewShelf(1, false))
        );
        name(&mut shelf, Control::MenuNewShelf(1, false), "Okunacaklar");
        let later = id_of(&shelf, "Okunacaklar");
        assert!(
            shelf
                .shelves
                .contains(later, &shelf.entries[1].document.fingerprint)
        );
        assert_eq!(shelf.menu, Some((1, false)));
        assert!(
            shelf
                .controls()
                .any(|c| c == Control::ToggleShelf(1, false, ders))
        );

        let _ = shelf.update(Message::Activate(Control::ToggleShelf(0, false, ders)));
        let _ = shelf.update(Message::Activate(Control::Filter(Some(ders))));
        assert_eq!(shelf.order, [0]);
        assert!(shelf.controls().any(|c| c == Control::DeleteShelf));

        let _ = shelf.update(Message::Activate(Control::Filter(None)));
        let _ = shelf.update(Message::Activate(Control::Sort(Sort::Shelf)));
        assert_eq!(shelf.order, [0, 1, 2]);
        let _ = shelf.update(Message::Activate(Control::ToggleShelf(2, false, ders)));
        let _ = shelf.update(Message::Activate(Control::ToggleShelf(0, false, ders)));
        let _ = shelf.update(Message::Activate(Control::Sort(Sort::Title)));
        let _ = shelf.update(Message::Activate(Control::Sort(Sort::Shelf)));
        assert_eq!(shelf.order, [2, 1, 0]);
    }

    #[test]
    fn names_are_checked_and_shelves_renamed_deleted_and_emptied() {
        let mut shelf = shelf_with(&["Alpha", "Beta"]);
        name(&mut shelf, Control::NewShelf, "Bitenler");
        name(&mut shelf, Control::NewShelf, "bitenler");
        assert!(shelf.naming.as_ref().unwrap().error.is_some());
        assert!(shelf.dismiss());
        assert!(shelf.naming.is_none());
        let done = id_of(&shelf, "Bitenler");
        let _ = shelf.update(Message::Activate(Control::Filter(Some(done))));
        name(&mut shelf, Control::RenameShelf, "Finished");
        assert_eq!(shelf.shelves.get(done).unwrap().name, "Finished");

        let _ = shelf.update(Message::Activate(Control::ToggleShelf(1, false, done)));
        let fingerprint = shelf.entries[1].document.fingerprint.clone();
        let path = shelf.entries[1].document.path.clone();
        let _ = shelf.remove(&path);
        assert!(!shelf.shelves.contains(done, &fingerprint));

        let _ = shelf.update(Message::Activate(Control::DeleteShelf));
        assert!(shelf.shelves.shelves.is_empty());
        assert_eq!(shelf.filter, None);
        assert_eq!(shelf.order, [0]);
        assert!(shelf.shelves_dirty || shelf.shelves_saving);
    }

    #[test]
    fn leaving_a_card_closes_its_shelf_menu() {
        let mut shelf = shelf_with(&["Alpha", "Beta"]);
        let _ = shelf.update(Message::Activate(Control::ShelfMenu(0, false)));
        let _ = shelf.update(Message::Hover(Some(Control::Document(0))));
        assert_eq!(shelf.menu, Some((0, false)));
        let _ = shelf.update(Message::Hover(None));
        assert_eq!(shelf.menu, None);
    }

    #[test]
    fn catalog_author_names_read_naturally() {
        assert_eq!(display_author("Austen, Jane, 1775-1817"), "Jane Austen");
        assert_eq!(
            display_author("Shelley, Mary Wollstonecraft, 1797-1851"),
            "Mary Wollstonecraft Shelley"
        );
        assert_eq!(display_author("Homer, 750"), "Homer");
        assert_eq!(display_author("Arthur Conan Doyle"), "Arthur Conan Doyle");
        assert_eq!(display_author("Dunne, Anthony"), "Dunne, Anthony");
    }

    #[test]
    fn long_titles_end_in_an_ellipsis_and_short_ones_are_kept() {
        assert_eq!(ellipsize("Let's Go", 300.0, 16.0), "Let's Go");
        let short = ellipsize("The Adventures of Sherlock Holmes", 160.0, 16.0);
        assert!(short.ends_with('…'), "{short}");
        assert!(short.chars().count() <= 17, "{short}");
        assert_eq!(jacket_tint("Frankenstein"), jacket_tint("Frankenstein"));
        assert_eq!(jacket_title_size("Let's Go", 150.0, 19.0), 19.0);
        let long = jacket_title_size("Frankenstein", 112.0, 19.0);
        assert!(long < 19.0 && long * 0.58 * 12.0 <= 112.0, "{long}");
    }

    #[test]
    fn continuing_documents_remain_in_every_library_sort() {
        let mut shelf = Shelf {
            entries: [
                ("Zulu", DocumentKind::Epub, 0.5),
                ("Alpha", DocumentKind::Pdf, 1.0),
                ("Beta", DocumentKind::Html, 0.25),
            ]
            .into_iter()
            .map(|(title, kind, progress)| Entry {
                document: reader_document::recent::Entry {
                    path: format!("{title}.book").into(),
                    title: title.into(),
                    fingerprint: String::new(),
                    kind,
                },
                author: None,
                byte_len: 0,
                opened_at: 0,
                progress,
                current: 1,
                total: 2,
                cover: false,
                favourite: false,
                source_kind: None,
            })
            .collect(),
            ..Shelf::default()
        };
        for (sort, expected) in [
            (Sort::Recent, [0, 1, 2]),
            (Sort::Title, [1, 2, 0]),
            (Sort::Format, [0, 2, 1]),
        ] {
            shelf.sort = sort;
            shelf.reorder();
            assert_eq!(shelf.continuing, [0, 2]);
            assert_eq!(shelf.order, expected);
        }
        shelf.entries[0].progress = 1.0;
        shelf.reorder();
        assert_eq!(shelf.continuing, [2]);
        assert_eq!(shelf.order, [0, 2, 1]);
    }
}
