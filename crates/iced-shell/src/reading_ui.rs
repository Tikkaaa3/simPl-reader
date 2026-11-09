//! Typography is independent of zoom. The library defaults live in the
//! preferences; a book's own options, its theme among them, are saved by its
//! fingerprint.
use super::*;
use iced::widget::column;
use reader_document::reading::{self, Font, Options};

/// Whose options a control edits: the library defaults, from Settings, or the
/// open book's own, from its Aa panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Defaults,
    Book,
}

#[derive(Clone, Debug)]
pub(super) enum Action {
    Picker(Picker),
    Font(Font),
    Size(u16),
    Spacing(u16),
    Margin(u16),
    /// An index into the bundled themes.
    Theme(usize),
    Reset,
}
#[derive(Debug, Default)]
pub(super) struct State {
    pub defaults: Options,
    book: Option<String>,
    override_: Option<Options>,
    pub loading: bool,
    pub blocked: bool,
    pub saving: bool,
    pending: Vec<(String, Option<Options>)>,
    flight: Option<(String, Option<Options>)>,
    pub notice: Option<String>,
    pub picker: Option<(Target, Picker)>,
}
impl State {
    pub fn options(&self) -> Options {
        self.override_.unwrap_or(self.defaults).validated()
    }
    fn options_for(&self, target: Target) -> Options {
        match target {
            Target::Defaults => self.defaults.validated(),
            Target::Book => self.options(),
        }
    }
    pub fn settled(&self) -> bool {
        !self.saving && self.pending.is_empty()
    }
    pub fn book_theme(&self) -> Option<reading::ThemeId> {
        self.override_.and_then(|options| options.theme)
    }
}
impl Reader {
    pub(super) fn reading_options(&self) -> Options {
        if self.book.as_ref().is_some_and(|b| b.pdf_source.is_some()) {
            Options::default()
        } else {
            self.reading.options().without_theme()
        }
    }
    /// The theme the open book reads in: its own, else the default theme.
    pub(super) fn book_theme(&self) -> &'static themes::ReadingTheme {
        self.reading
            .book_theme()
            .map_or(self.default_theme, themes::by_id)
    }
    /// What the open book's layout depends on.
    pub(super) fn look(&self) -> (Options, &'static str) {
        (self.reading_options(), self.theme.id)
    }
    /// Puts the open book's theme in place, and lays the book out again when it
    /// looks different from `before`.
    pub(super) fn apply_reading(&mut self, before: (Options, &'static str)) -> Task<Message> {
        self.theme = self.book_theme();
        if self.look() == before {
            return Task::none();
        }
        self.adapted.clear();
        if self.book.is_none() {
            return Task::none();
        }
        self.rebuild_geometry(self.pending_anchor.unwrap_or_else(|| self.anchor()))
    }
    pub(super) fn reading_style(&self, book: &Book) -> BookStyle {
        effective_style(self.theme, book, self.reading_options())
    }
    pub(super) fn reading_family(&self, book: &Book) -> Option<&'static str> {
        effective_family(self.theme, book, self.reading_options())
    }
    pub(super) fn sync_reading_settings(&mut self) -> Task<Message> {
        let before = self.look();
        let load = self.select_reading_book();
        Task::batch([load, self.apply_reading(before)])
    }
    /// Follows the open book: its own options from the save queue, or loaded in
    /// the background, and the theme they call for. Lays nothing out.
    pub(super) fn select_reading_book(&mut self) -> Task<Message> {
        let book = self
            .book
            .as_ref()
            .filter(|b| b.pdf_source.is_none())
            .map(|b| b.fingerprint.clone());
        if self.reading.book == book {
            return Task::none();
        }
        self.reading.picker = None;
        self.reading.book = book.clone();
        self.reading.override_ = None;
        self.reading.blocked = false;
        self.reading.loading = book.is_some();
        let pending = book.as_ref().and_then(|book| {
            self.reading
                .pending
                .iter()
                .find(|(key, _)| key == book)
                .map(|(_, options)| *options)
        });
        if let Some(options) = pending {
            self.reading.override_ = options;
            self.reading.loading = false;
        }
        self.theme = self.book_theme();
        let Some(book) = book.filter(|_| pending.is_none()) else {
            return Task::none();
        };
        let reply = book.clone();
        let profile = self.profile.epoch;
        Task::perform(async move { reading::load(&book) }, move |result| {
            Message::ReadingLoaded {
                profile,
                book: reply.clone(),
                result,
            }
        })
    }
    pub(super) fn reading_loaded(
        &mut self,
        book: String,
        result: Result<Option<Options>, String>,
    ) -> Task<Message> {
        if self.reading.book.as_ref() != Some(&book) {
            return Task::none();
        }
        self.reading.loading = false;
        match result {
            Ok(options) => {
                let before = self.look();
                self.reading.override_ = options;
                self.apply_reading(before)
            }
            Err(error) => {
                self.reading.blocked = true;
                self.reading.notice = Some(error);
                Task::none()
            }
        }
    }
    pub(super) fn reading_action(&mut self, target: Target, action: Action) -> Task<Message> {
        if !enabled(self, target) {
            return Task::none();
        }
        if let (Target::Defaults, Action::Theme(index)) = (target, &action) {
            return self.set_default_theme(*index);
        }
        if let Action::Picker(picker) = action {
            self.reading.picker = if self.reading.picker == Some((target, picker)) {
                None
            } else {
                Some((target, picker))
            };
            return Task::none();
        }
        self.reading.picker = None;
        let before = self.look();
        let current = self.reading.options_for(target);
        let mut options = current;
        let reset = matches!(action, Action::Reset);
        match action {
            Action::Picker(_) => unreachable!(),
            Action::Font(value) => options.font = value,
            Action::Size(value) => options.size = value,
            Action::Spacing(value) => options.spacing = value,
            Action::Margin(value) => options.margin = value,
            Action::Theme(index) => {
                let theme = &themes::THEMES[index.min(themes::THEMES.len() - 1)];
                // The default theme's card means "follow the default theme".
                options.theme = (theme.id != self.default_theme.id).then(|| theme.key());
            }
            Action::Reset => options = Options::default(),
        }
        options = options.validated();
        if options == current && !reset {
            return Task::none();
        }
        self.reading.notice = None;
        let save = match (target, self.reading.book.clone()) {
            (Target::Book, Some(book)) => self.save_book_options(book, (!reset).then_some(options)),
            (Target::Book, None) => return Task::none(),
            (Target::Defaults, _) => {
                self.reading.defaults = options;
                self.preferences_dirty = true;
                self.persist_preferences()
            }
        };
        // A default change that this book overrides keeps its layout.
        Task::batch([save, self.apply_reading(before)])
    }
    fn save_book_options(&mut self, book: String, options: Option<Options>) -> Task<Message> {
        self.reading.override_ = options;
        self.reading.pending.retain(|(key, _)| key != &book);
        self.reading.pending.push((book, options));
        self.persist_reading_settings()
    }
    /// Drops the open book's own theme, keeping its typography, without laying it out.
    pub(super) fn forget_book_theme(&mut self) -> Task<Message> {
        let (Some(book), Some(options)) = (self.reading.book.clone(), self.reading.override_)
        else {
            return Task::none();
        };
        self.save_book_options(
            book,
            Some(Options {
                theme: None,
                ..options
            }),
        )
    }
    pub(super) fn persist_reading_settings(&mut self) -> Task<Message> {
        if self.reading.saving {
            return Task::none();
        }
        let Some((book, options)) = self.reading.pending.first().cloned() else {
            return Task::none();
        };
        self.reading.saving = true;
        self.reading.flight = Some((book.clone(), options));
        Task::perform(
            async move { reading::save(&book, options).map(|()| (book, options)) },
            Message::ReadingSaved,
        )
    }
    pub(super) fn reading_saved(
        &mut self,
        result: Result<(String, Option<Options>), String>,
    ) -> Task<Message> {
        self.reading.saving = false;
        let flight = self.reading.flight.take();
        match result {
            Ok((book, options)) => self
                .reading
                .pending
                .retain(|(key, value)| key != &book || *value != options),
            Err(error) => {
                self.reading.notice = Some(format!("Reading settings could not be saved: {error}"));
                self.error = self.reading.notice.clone();
                if let Some((book, options)) = flight {
                    self.reading
                        .pending
                        .retain(|(key, value)| key != &book || *value != options);
                }
            }
        }
        if self.pending_exit && self.exit_ready() {
            iced::exit()
        } else {
            self.persist_reading_settings()
        }
    }
    pub(super) fn fit_book_width(&mut self) -> Task<Message> {
        if self.book.is_none() || !self.interactive() {
            return Task::none();
        }
        if self.fit_width {
            return update_inner(self, Message::Zoom(self.fit_previous_zoom));
        }
        self.fit_previous_zoom = self.zoom;
        self.fit_width = true;
        self.refit_book()
    }
    pub(super) fn refit_book(&mut self) -> Task<Message> {
        if !self.fit_width || self.book.is_none() || self.pagination.is_some() {
            return Task::none();
        }
        let available = (self.window_size.width - notes_ui::sidebar_width(self)).max(1.0);
        let zoom = ((available - MINIMAL.side_padding(available) * 2.0 - 14.0) / book_map::PAPER)
            .clamp(0.25, 3.0);
        if (zoom - self.zoom).abs() < 0.001 {
            return Task::none();
        }
        let anchor = self.pending_anchor.unwrap_or_else(|| self.anchor());
        self.generation = self.generation.wrapping_add(1);
        self.viewport *= self.zoom / zoom;
        self.zoom = zoom;
        self.offset = self.offset_for(anchor);
        self.pending_anchor = Some(anchor);
        self.restore_book_scroll()
    }
    pub(super) fn toggle_fullscreen(&mut self) -> Task<Message> {
        let Some(id) = self.window else {
            return Task::none();
        };
        if !self.interactive() {
            return Task::none();
        }
        self.fullscreen = !self.fullscreen;
        if self.fullscreen {
            self.before_fullscreen_maximized = self.maximized;
            window::set_mode(id, window::Mode::Fullscreen)
        } else {
            window::set_mode(id, window::Mode::Windowed)
                .chain(window::maximize(id, self.before_fullscreen_maximized))
        }
    }
}
pub(super) fn effective_style(
    theme: &'static themes::ReadingTheme,
    book: &Book,
    options: Options,
) -> BookStyle {
    let mut style = *style_for(theme, book);
    if book.pdf_source.is_none() && options.spacing != 0 {
        style.line_height = options.spacing as f32 / 100.0;
    }
    style
}
pub(super) fn effective_family(
    theme: &'static themes::ReadingTheme,
    book: &Book,
    options: Options,
) -> Option<&'static str> {
    if book.pdf_source.is_some() {
        return None;
    }
    match options.font {
        Font::Theme => family_for(theme, book),
        Font::Literata => None,
        Font::Spectral => Some("Spectral"),
        Font::FiraSans => Some("Fira Sans"),
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Spacing(u16);
impl std::fmt::Display for Spacing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 == 0 {
            f.write_str("Theme spacing")
        } else {
            write!(f, "{:.1}×", self.0 as f32 / 100.0)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Picker {
    Font,
    Spacing,
    Margin,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Focus {
    Picker(Picker),
    Font(Font),
    SizeDown,
    SizeUp,
    Spacing(u16),
    Margin(u16),
    Reset,
}
/// The defaults can always be edited; a book's own options only once they
/// have loaded, and only for a reflowable book.
pub(super) fn enabled(reader: &Reader, target: Target) -> bool {
    reader.interactive()
        && match target {
            Target::Defaults => true,
            Target::Book => {
                !reader.reading.loading
                    && !reader.reading.blocked
                    && reader.reading.book.is_some()
                    && reader.pdf.is_none()
                    && reader.book.as_ref().is_some_and(|b| b.pdf_source.is_none())
            }
        }
}
const SPACING: [u16; 8] = [0, 110, 120, 140, 160, 180, 200, 220];
const MARGINS: [u16; 7] = [16, 24, 32, 48, 64, 80, 96];
const FONTS: [Font; 4] = [Font::Theme, Font::Literata, Font::Spectral, Font::FiraSans];
pub(super) fn controls(
    reader: &Reader,
    target: Target,
) -> impl Iterator<Item = Control> + Clone + '_ {
    let options = reader.reading.options_for(target);
    let picker = reader
        .reading
        .picker
        .and_then(|(owner, picker)| (owner == target).then_some(picker));
    [
        (options.size > 12).then_some(Focus::SizeDown),
        (options.size < 36).then_some(Focus::SizeUp),
        Some(Focus::Picker(Picker::Font)),
        Some(Focus::Picker(Picker::Spacing)),
        Some(Focus::Picker(Picker::Margin)),
        Some(Focus::Reset),
    ]
    .into_iter()
    .flatten()
    .chain(
        FONTS
            .into_iter()
            .filter(move |_| picker == Some(Picker::Font))
            .map(Focus::Font),
    )
    .chain(
        SPACING
            .into_iter()
            .filter(move |_| picker == Some(Picker::Spacing))
            .map(Focus::Spacing),
    )
    .chain(
        MARGINS
            .into_iter()
            .filter(move |_| picker == Some(Picker::Margin))
            .map(Focus::Margin),
    )
    .filter(move |_| enabled(reader, target))
    .map(move |focus| Control::Reading(target, focus))
}
pub(super) fn focus_action(reader: &Reader, target: Target, focus: Focus) -> Action {
    let options = reader.reading.options_for(target);
    match focus {
        Focus::Picker(picker) => Action::Picker(picker),
        Focus::Font(font) => Action::Font(font),
        Focus::SizeDown => Action::Size(options.size.saturating_sub(1)),
        Focus::SizeUp => Action::Size(options.size + 1),
        Focus::Spacing(value) => Action::Spacing(value),
        Focus::Margin(value) => Action::Margin(value),
        Focus::Reset => Action::Reset,
    }
}
fn setting_button<'a>(
    reader: &Reader,
    target: Target,
    focus: Focus,
    label: String,
    selected: bool,
) -> Element<'a, Message> {
    toned_button(
        reader,
        Control::Reading(target, focus),
        text(label).size(12),
        enabled(reader, target)
            .then(|| Message::Reading(target, focus_action(reader, target, focus))),
        ui::ButtonTone::Surface,
        selected,
    )
}
/// The Settings section: the library defaults, which every book without its
/// own options follows.
pub(super) fn settings(reader: &Reader) -> Element<'_, Message> {
    let mut content = column![
        text("Reading defaults").font(ui::MEDIUM).size(13),
        typography(reader, Target::Defaults),
        text("Font, text size, spacing and margins change independently of zoom. Page numbers and annotations stay stable. A book follows these defaults until you change it from Aa in its reading bar.")
            .size(12)
            .style(ui::muted_text),
    ]
    .spacing(8);
    if let Some(notice) = &reader.reading.notice {
        content = content.push(text(notice).size(12).style(ui::secondary_text));
    }
    content.into()
}
/// Font, text size, line spacing, side margins and the reset for `target`.
pub(super) fn typography(reader: &Reader, target: Target) -> Element<'_, Message> {
    let options = reader.reading.options_for(target);
    let picker = reader
        .reading
        .picker
        .and_then(|(owner, picker)| (owner == target).then_some(picker));
    let setting_button =
        |focus, label, selected| setting_button(reader, target, focus, label, selected);
    let mut content = column![].spacing(8);
    if enabled(reader, target) {
        content = content
            .push(
                row![
                    text("Text size").size(12).width(Length::Fill),
                    setting_button(Focus::SizeDown, "−".into(), false),
                    text(format!("{}", options.size)).size(12),
                    setting_button(Focus::SizeUp, "+".into(), false)
                ]
                .spacing(12)
                .align_y(iced::Alignment::Center),
            )
            .push(
                row![
                    text("Font").size(12).width(Length::Fill),
                    setting_button(
                        Focus::Picker(Picker::Font),
                        format!("{} ▾", options.font),
                        picker == Some(Picker::Font)
                    )
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            )
            .push(
                row![
                    text("Line spacing").size(12).width(Length::Fill),
                    setting_button(
                        Focus::Picker(Picker::Spacing),
                        format!("{} ▾", Spacing(options.spacing)),
                        picker == Some(Picker::Spacing)
                    )
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            )
            .push(
                row![
                    text("Side margins").size(12).width(Length::Fill),
                    setting_button(
                        Focus::Picker(Picker::Margin),
                        format!("{} px ▾", options.margin),
                        picker == Some(Picker::Margin)
                    )
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            );
        let choices: Vec<(Focus, String, bool)> = match picker {
            Some(Picker::Font) => FONTS
                .into_iter()
                .map(|v| (Focus::Font(v), v.to_string(), v == options.font))
                .collect(),
            Some(Picker::Spacing) => SPACING
                .into_iter()
                .map(|v| {
                    (
                        Focus::Spacing(v),
                        Spacing(v).to_string(),
                        v == options.spacing,
                    )
                })
                .collect(),
            Some(Picker::Margin) => MARGINS
                .into_iter()
                .map(|v| (Focus::Margin(v), format!("{v} px"), v == options.margin))
                .collect(),
            None => Vec::new(),
        };
        if !choices.is_empty() {
            let mut grid = column![].spacing(6);
            for pair in choices.chunks(2) {
                let mut line = row![].spacing(6);
                for (focus, label, selected) in pair {
                    line = line.push(setting_button(*focus, label.clone(), *selected));
                }
                grid = grid.push(line);
            }
            content = content.push(
                container(grid)
                    .padding(8)
                    .width(Length::Fill)
                    .style(ui::panel),
            );
        }
        content = content.push(setting_button(
            Focus::Reset,
            match target {
                Target::Defaults => "Reset reading defaults",
                Target::Book => "Use reading defaults",
            }
            .into(),
            false,
        ));
    } else {
        content = content.push(
            text(if reader.reading.loading {
                "Loading this book’s reading settings…"
            } else {
                "Typography controls apply to reflowable books. PDF Book retains its source layout."
            })
            .size(12)
            .style(ui::muted_text),
        );
    }
    content.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reader() -> Reader {
        let mut reader = super::super::tests::themed_reader(30);
        reader.reading.book = reader.book.as_ref().map(|b| b.fingerprint.clone());
        reader
    }
    #[test]
    fn typography_changes_preserve_canonical_pages_selection_and_defaults() {
        let mut reader = reader();
        let canonical = serde_json::to_vec(&**reader.atlas.as_ref().unwrap()).unwrap();
        let pages: Vec<_> = reader
            .pages()
            .iter()
            .map(|p| (p.number, p.label.clone()))
            .collect();
        let original = reader.heights.total();
        let defaults = reader.reading.defaults;
        let _ = reader.go_to_local_page(2);
        let page = reader.active_page().unwrap().number;
        reader.selection.begin(Endpoint {
            item_id: "paragraph-3".into(),
            byte_offset: 0,
        });
        reader.selection.extend(Endpoint {
            item_id: "paragraph-3".into(),
            byte_offset: 5,
        });
        reader.selection.end_drag();
        let selection = reader
            .selection
            .copy_text(&reader.book.as_ref().unwrap().items);
        for action in [
            Action::Font(Font::FiraSans),
            Action::Size(28),
            Action::Spacing(200),
            Action::Margin(80),
        ] {
            let _ = reader.reading_action(Target::Book, action);
            settle_theme(&mut reader);
            assert_eq!(
                serde_json::to_vec(&**reader.atlas.as_ref().unwrap()).unwrap(),
                canonical
            );
            assert_eq!(
                reader
                    .pages()
                    .iter()
                    .map(|p| (p.number, p.label.clone()))
                    .collect::<Vec<_>>(),
                pages
            );
            assert_eq!(reader.active_page().unwrap().number, page);
            assert_eq!(
                reader
                    .selection
                    .copy_text(&reader.book.as_ref().unwrap().items),
                selection
            );
            assert_eq!(reader.reading.defaults, defaults);
        }
        assert!(reader.heights.total() > original);
        assert_eq!(reader.font_size, 28.0);
        assert_eq!(reader.width, 560.0);
        assert_eq!(reader.line_height(), 2.0);
        assert_eq!(
            reader.reading_family(reader.book.as_ref().unwrap()),
            Some("Fira Sans")
        );
        // A stale finished write cannot discard the newer queued override.
        let flight = reader.reading.flight.clone().unwrap();
        let _ = reader.reading_saved(Ok(flight));
        assert!(reader.reading.saving && !reader.reading.pending.is_empty());
        let flight = reader.reading.flight.clone().unwrap();
        let _ = reader.reading_saved(Ok(flight));
        assert!(reader.reading.settled());
        let _ = reader.reading_action(Target::Book, Action::Reset);
        settle_theme(&mut reader);
        assert_eq!(reader.reading.options(), defaults);
        assert!(!reader.needs_adaptation());
        assert!((reader.heights.total() - original).abs() < 0.1);
    }
    #[test]
    fn stale_loads_and_save_failures_do_not_overwrite_other_books_or_loop() {
        let mut reader = reader();
        let generation = reader.generation;
        let _ = reader.reading_loaded(
            "other".into(),
            Ok(Some(Options {
                size: 36,
                ..Default::default()
            })),
        );
        assert_eq!(reader.generation, generation);
        assert_eq!(reader.font_size, 20.0);
        let _ = reader.reading_action(Target::Book, Action::Size(26));
        let _ = reader.reading_saved(Err("disk unavailable".into()));
        assert!(reader.reading.settled());
        assert!(reader.error.as_ref().unwrap().contains("disk unavailable"));
        reader.book = None;
        let _ = reader.sync_reading_settings();
        let _ = reader.reading_action(Target::Book, Action::Font(Font::Spectral));
        assert_eq!(
            reader.reading.defaults.font,
            Font::Theme,
            "with no book open there is no book to change"
        );
        let _ = reader.reading_action(Target::Defaults, Action::Font(Font::Spectral));
        assert_eq!(reader.reading.defaults.font, Font::Spectral);
        assert!(reader.preferences_saving);
    }
    #[test]
    fn settings_change_the_defaults_that_a_book_without_its_own_follows() {
        let mut reader = reader();
        reader.show_settings = true;
        let size_up = Control::Reading(Target::Defaults, Focus::SizeUp);
        assert!(reader.controls().any(|control| control == size_up));
        let _ = reader.activate(size_up);
        settle_theme(&mut reader);
        assert_eq!(reader.reading.defaults.size, 21);
        assert_eq!(reader.reading.options().size, 21);
        assert_eq!(
            reader.font_size, 21.0,
            "the open book reads at the new default"
        );

        let _ = reader.reading_action(Target::Book, Action::Size(30));
        let _ = reader.reading_action(Target::Defaults, Action::Size(24));
        settle_theme(&mut reader);
        assert_eq!(reader.reading.defaults.size, 24);
        assert_eq!(reader.font_size, 30.0, "a book with its own size keeps it");
    }
    #[test]
    fn fit_width_tracks_resizing_and_explicit_zoom_does_not_cancel_pagination() {
        let mut reader = reader();
        let total = reader.heights.total();
        let page = reader.active_page().unwrap().number;
        reader.window_size.width = 540.0;
        let _ = reader.fit_book_width();
        assert!(reader.fit_width && reader.zoom < 1.0);
        assert!(book_map::PAPER * reader.zoom + 46.0 <= reader.window_size.width + 0.1);
        reader.notes.list = Some(notes::ListTab::Bookmarks);
        let _ = reader.refit_book();
        let available = reader.window_size.width - notes_ui::sidebar_width(&reader);
        assert!(book_map::PAPER * reader.zoom + 46.0 <= available + 0.1);
        reader.notes.list = None;
        let _ = reader.fit_book_width();
        assert!(!reader.fit_width && reader.zoom == 1.0);
        let _ = reader.fit_book_width();
        reader.window_size.width = 1280.0;
        let _ = reader.refit_book();
        assert!(reader.zoom > 1.0);
        assert_eq!(reader.heights.total(), total);
        assert_eq!(reader.active_page().unwrap().number, page);
        reader.atlas = None;
        let _ = reader.rebuild_geometry(reader.anchor());
        let generation = reader.generation;
        let _ = update_inner(&mut reader, Message::Zoom(1.2));
        assert!(!reader.fit_width);
        assert_eq!(reader.generation, generation);
        settle_pagination(&mut reader);
        assert!(!reader.pages().is_empty() && reader.pagination.is_none());
    }
    #[test]
    fn fullscreen_restores_maximized_state_and_escape_dismisses_settings_first() {
        let mut reader = reader();
        let id = window::Id::unique();
        reader.window = Some(id);
        reader.maximized = true;
        let _ = reader.toggle_fullscreen();
        assert!(reader.fullscreen && reader.before_fullscreen_maximized);
        reader.show_settings = true;
        let escape = || {
            Message::Event(
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key: Key::Named(key::Named::Escape),
                    modified_key: Key::Named(key::Named::Escape),
                    physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Escape),
                    location: keyboard::Location::Standard,
                    modifiers: keyboard::Modifiers::default(),
                    text: None,
                    repeat: false,
                }),
                id,
            )
        };
        let _ = update_inner(&mut reader, escape());
        assert!(!reader.show_settings && reader.fullscreen);
        let _ = update_inner(&mut reader, escape());
        assert!(!reader.fullscreen && reader.before_fullscreen_maximized);
    }
    #[test]
    #[ignore = "Production reading comfort visual QA: writes target/reader-comfort-previews"]
    fn render_reading_comfort_previews() {
        let output = PathBuf::from("../../target/reader-comfort-previews");
        std::fs::create_dir_all(&output).unwrap();
        let mut reader = reader();
        for (name, width, appearance) in [
            ("wide-light", 1280.0, Appearance::Light),
            ("narrow-dark", 540.0, Appearance::Dark),
        ] {
            reader.window_size = Size::new(width, 800.0);
            reader.appearance = appearance;
            let _ = reader.reading_action(Target::Book, Action::Font(Font::Spectral));
            let _ = reader.reading_action(Target::Book, Action::Size(24));
            let _ = reader.reading_action(Target::Book, Action::Spacing(180));
            let _ = reader.reading_action(Target::Book, Action::Margin(64));
            settle_theme(&mut reader);
            if reader.fit_width {
                let _ = reader.refit_book();
            } else {
                let _ = reader.fit_book_width();
            }
            super::super::book_preview::render(
                &mut reader,
                &output.join(format!("reading-{name}.png")),
            );
            reader.show_settings = true;
            reader.focused = Some(Control::Reading(
                Target::Defaults,
                Focus::Picker(Picker::Font),
            ));
            super::super::book_preview::render(
                &mut reader,
                &output.join(format!("settings-{name}.png")),
            );
            reader.show_settings = false;
            reader.focused = None;
            reader.fullscreen = true;
            super::super::book_preview::render(
                &mut reader,
                &output.join(format!("fullscreen-{name}.png")),
            );
            reader.fullscreen = false;
        }
    }
}
