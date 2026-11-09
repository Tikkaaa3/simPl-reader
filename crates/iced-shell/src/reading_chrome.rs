//! The reading bar: one row of icon buttons that floats over the top of a
//! document and hides when idle. [`Bar`] holds the visibility rules; every
//! transition takes an explicit [`Instant`] so the rules are testable without
//! a clock. [`items`] lists what the bar holds, in order: the view draws that
//! list and the keyboard visits it in the same order.

use super::*;
use iced::Animation;
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, overlay, renderer, widget};
use iced::widget::{Space, column, mouse_area, opaque};
use iced::{Point, Rectangle, Vector};
use std::time::{Duration, Instant};

/// Height of the bar's row; a 1px hairline sits under it.
const ROW: f32 = 43.0;
/// The height the bar takes when shown.
pub(super) const HEIGHT: f32 = ROW + 1.0;
/// The pointer reveals the bar anywhere in this band at the top of the window.
pub(super) const REVEAL_ZONE: f32 = 48.0;
const OPEN_HOLD: Duration = Duration::from_millis(2000);
const LEAVE_HOLD: Duration = Duration::from_millis(1500);
const FADE: Duration = Duration::from_millis(150);
const BUTTON: f32 = 32.0;
const GLYPH: f32 = 20.0;
/// Space between neighbouring controls.
const GAP: f32 = 4.0;
/// Space between the window's edges and the bar's outermost controls.
const EDGE: f32 = 8.0;
pub(super) const TITLE_ID: &str = "reading-bar-title";
/// Narrowest windows, in logical pixels, at which the bar fits its one row
/// with every optional control shown and four-digit page counts. Below
/// `title` the book title hides; below `overflow` the light and dark toggle,
/// Search and Settings fold into More. The ignored test
/// `measure_reading_bar_folds` measures them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Folds {
    pub(super) title: f32,
    pub(super) overflow: f32,
}

const BOOK: Folds = Folds {
    title: 722.0,
    overflow: 601.0,
};
/// A PDF read in Book mode adds its Document button and has no Contents.
const PDF_BOOK: Folds = Folds {
    title: 722.0,
    overflow: 601.0,
};
const PDF: Folds = Folds {
    title: 707.0,
    overflow: 587.0,
};
/// Pixels an iced scrollable moves per wheel line.
const WHEEL_LINE: f32 = 60.0;

/// Material Symbols glyphs, one per action.
mod glyph {
    pub(super) const LIBRARY: &str = "\u{e02f}";
    pub(super) const BACK: &str = "\u{e5c4}";
    pub(super) const BOOK_MODE: &str = "\u{ea19}";
    pub(super) const DOCUMENT_MODE: &str = "\u{e415}";
    pub(super) const CONTENTS: &str = "\u{e8de}";
    pub(super) const FIND: &str = "\u{e880}";
    pub(super) const LISTEN: &str = "\u{f01f}";
    pub(super) const STOP: &str = "\u{ef71}";
    pub(super) const NOTES: &str = "\u{e98b}";
    pub(super) const AA: &str = "\u{f6f1}";
    pub(super) const DARK: &str = "\u{e51c}";
    pub(super) const LIGHT: &str = "\u{e518}";
    pub(super) const SEARCH: &str = "\u{e8b6}";
    pub(super) const SETTINGS: &str = "\u{e8b8}";
    pub(super) const MORE: &str = "\u{e5d4}";
}

/// How much a narrow window folds away from the bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Fold {
    All,
    NoTitle,
    Overflow,
}

/// A card that drops from a bar button. At most one is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Menu {
    Aa,
    More,
}

/// One icon button of the bar, described as data.
#[derive(Clone, Debug)]
pub(super) struct Button {
    pub(super) control: Control,
    pub(super) glyph: &'static str,
    /// The action, as the tooltip names it.
    pub(super) label: &'static str,
    pub(super) shortcut: Option<&'static str>,
    /// `None` draws the button disabled and leaves it out of the focus order.
    pub(super) message: Option<Message>,
    /// The panel or mode the button controls is on.
    pub(super) selected: bool,
    /// The button moves into More when the bar folds.
    pub(super) folds: bool,
}

impl Button {
    fn new(
        control: Control,
        glyph: &'static str,
        label: &'static str,
        shortcut: Option<&'static str>,
        message: Option<Message>,
    ) -> Self {
        Self {
            control,
            glyph,
            label,
            shortcut,
            message,
            selected: false,
            folds: false,
        }
    }

    pub(super) fn tooltip(&self) -> String {
        match self.shortcut {
            Some(shortcut) => format!("{} ({shortcut})", self.label),
            None => self.label.to_owned(),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum Item {
    Button(Box<Button>),
    /// The room the controls leave, with the book title in it while it fits.
    Fill {
        title: Option<String>,
    },
    /// The page field and the page count.
    Pages,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BarState {
    /// F8 keeps the bar on screen until F8 is pressed again.
    Pinned,
    /// On screen; it hides at `hide_at` unless something holds it first.
    Shown {
        hide_at: Option<Instant>,
    },
    Hidden,
}

#[derive(Debug)]
pub(super) struct Bar {
    state: BarState,
    fade: Animation<bool>,
    /// The pointer is in the reveal band or over the bar.
    hovered: bool,
    /// A bar control has focus, or a panel opened from the bar is open.
    held: bool,
    /// The latest instant seen, which the fade is drawn at.
    now: Instant,
    /// The open document, so a new one shows the bar once.
    document: Option<PathBuf>,
    /// Read aloud is on, so starting it shows the bar, and the pill with it, once.
    listening: bool,
    /// The last pointer position in the window.
    pointer: Option<Point>,
    /// The book page field has keyboard focus.
    book_field: bool,
    /// Keyboard modifiers, for the wheel over the bar and the page.
    pub(super) modifiers: keyboard::Modifiers,
}

impl Default for Bar {
    fn default() -> Self {
        Self {
            state: BarState::Shown { hide_at: None },
            fade: Animation::new(true).duration(FADE),
            hovered: false,
            held: false,
            now: Instant::now(),
            document: None,
            listening: false,
            pointer: None,
            book_field: false,
            modifiers: keyboard::Modifiers::default(),
        }
    }
}

#[cfg(test)]
impl Bar {
    pub(super) fn state(&self) -> BarState {
        self.state
    }

    /// A bar that has already faded out.
    pub(super) fn hidden() -> Self {
        Self {
            state: BarState::Hidden,
            fade: Animation::new(false).duration(FADE),
            ..Self::default()
        }
    }

    /// Advances the bar's own clock by `by`, as the next frames would.
    pub(super) fn step_by(&mut self, by: Duration) {
        let at = self.now + by;
        self.tick(at);
    }
}

impl Bar {
    fn advance(&mut self, now: Instant) {
        self.now = self.now.max(now);
    }

    fn idle(&self) -> bool {
        !self.hovered && !self.held
    }

    fn show(&mut self, hide_at: Option<Instant>, now: Instant) {
        self.state = BarState::Shown { hide_at };
        self.fade.go_mut(true, now);
    }

    fn hide(&mut self, now: Instant) {
        self.state = BarState::Hidden;
        self.fade.go_mut(false, now);
    }

    /// A document opened: show the bar briefly so the reader knows where it is.
    pub(super) fn open(&mut self, now: Instant) {
        self.advance(now);
        if self.state != BarState::Pinned {
            self.show(self.idle().then_some(now + OPEN_HOLD), now);
        }
    }

    /// Back on the Library, which has its own static header.
    fn close(&mut self, now: Instant) {
        self.advance(now);
        if self.state != BarState::Pinned {
            self.state = BarState::Shown { hide_at: None };
        }
        self.fade = Animation::new(true).duration(FADE);
    }

    pub(super) fn hover(&mut self, now: Instant, hovered: bool) {
        self.advance(now);
        if self.hovered != hovered {
            self.hovered = hovered;
            self.settle(now);
        }
    }

    pub(super) fn hold(&mut self, now: Instant, held: bool) {
        self.advance(now);
        if self.held != held {
            self.held = held;
            self.settle(now);
        }
    }

    /// Hover or a hold reveals the bar and cancels its deadline; losing the
    /// last of them starts the leave delay.
    fn settle(&mut self, now: Instant) {
        match self.state {
            BarState::Pinned => {}
            _ if !self.idle() => self.show(None, now),
            BarState::Shown { hide_at: None } => {
                self.state = BarState::Shown {
                    hide_at: Some(now + LEAVE_HOLD),
                }
            }
            BarState::Shown { hide_at: Some(_) } | BarState::Hidden => {}
        }
    }

    pub(super) fn tick(&mut self, now: Instant) {
        self.advance(now);
        if let BarState::Shown { hide_at: Some(at) } = self.state
            && now >= at
        {
            self.hide(now);
        }
    }

    pub(super) fn toggle_pin(&mut self, now: Instant) {
        self.advance(now);
        if self.state == BarState::Pinned {
            self.state = BarState::Shown {
                hide_at: self.idle().then_some(now + LEAVE_HOLD),
            };
        } else {
            self.state = BarState::Pinned;
            self.fade.go_mut(true, now);
        }
    }

    /// Escape hides a shown bar unless the pointer is on it or something holds it.
    /// `held` is passed fresh because Escape has just cleared the focus.
    pub(super) fn dismiss(&mut self, now: Instant, held: bool) -> bool {
        self.advance(now);
        self.held = held;
        if matches!(self.state, BarState::Shown { .. }) && self.idle() {
            self.hide(now);
            true
        } else {
            false
        }
    }

    /// The bar's controls are in the widget tree, including while it fades out.
    pub(super) fn visible(&self) -> bool {
        self.state != BarState::Hidden || self.fade.is_animating(self.now)
    }

    fn alpha(&self) -> f32 {
        self.fade.interpolate(0.0, 1.0, self.now)
    }

    /// Frames are needed only to reach a deadline or to run the fade.
    pub(super) fn needs_frames(&self) -> bool {
        matches!(self.state, BarState::Shown { hide_at: Some(_) })
            || self.fade.is_animating(self.now)
    }
}

/// What one message tells the bar about the pointer and the keyboard.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Input {
    /// A new pointer position, or `Some(None)` when it left the window.
    pub(super) pointer: Option<Option<Point>>,
    pub(super) modifiers: Option<keyboard::Modifiers>,
}

impl Input {
    pub(super) fn of(message: &Message) -> Self {
        match message {
            Message::Event(iced::Event::Mouse(mouse::Event::CursorMoved { position }), _) => Self {
                pointer: Some(Some(*position)),
                ..Self::default()
            },
            Message::Event(iced::Event::Mouse(mouse::Event::CursorLeft), _) => Self {
                pointer: Some(None),
                ..Self::default()
            },
            Message::Event(
                iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)),
                _,
            ) => Self {
                modifiers: Some(*modifiers),
                ..Self::default()
            },
            _ => Self::default(),
        }
    }
}

pub(super) fn is_bar_control(control: Control) -> bool {
    matches!(
        control,
        Control::Chrome(_)
            | Control::Close
            | Control::Back
            | Control::BookMode
            | Control::DocumentMode
            | Control::BookPageInput
            | Control::Contents
            | Control::Find
            | Control::ReadAloud
            | Control::Notes(notes::Focus::ToggleList)
            | Control::Menu(_)
            | Control::PdfPage
    )
}

impl Reader {
    fn document_path(&self) -> Option<&Path> {
        self.pdf
            .as_ref()
            .map(|pdf| pdf.document().path.as_path())
            .or_else(|| self.book.as_ref().map(|book| book.path.as_path()))
    }

    /// A control on the bar or on the read-aloud pill, which shows with it,
    /// has keyboard focus, a menu or panel the bar opened is open, or a panel
    /// hangs below the bar. The transient loading line does not hold it, so a
    /// slow first layout never stretches the 2 s shown after opening.
    pub(super) fn bar_held(&self) -> bool {
        self.focused.is_some_and(|control| {
            is_bar_control(control)
                || (self.read_aloud.active()
                    && matches!(
                        control,
                        Control::ReadAloudSlower
                            | Control::ReadAloudFaster
                            | Control::ReadAloudPause
                    ))
        }) || self.bar.book_field
            || self
                .pdf
                .as_ref()
                .is_some_and(pdf_reader::Reader::page_focused)
            || self.menu.is_some()
            || self.show_settings
            || self.show_search
            || self.has_held_panels()
    }

    /// Something shows in the panel area under the bar. The library banner
    /// does not hold the bar: a library location that stays unavailable
    /// would otherwise keep it on screen for as long as the document is open.
    pub(super) fn has_panels(&self) -> bool {
        self.has_held_panels()
            || self.opening.is_some()
            || self.pagination.is_some()
            || library_ui::banner_visible(self)
    }

    /// Help, recent documents or notices. Find, the read-aloud player and the
    /// drawers float over the page instead, so they never hold the bar.
    fn has_held_panels(&self) -> bool {
        self.error.is_some()
            || self.history_notice.is_some()
            || self.show_recent
            || self.show_help
            || self.preferences_notice.is_some()
            || self.shelf.notice.is_some()
            || self.saving
    }

    /// The book page field gained or lost keyboard focus. Leaving it without
    /// Enter abandons the draft, so the field shows the current page again.
    pub(super) fn book_page_focus(&mut self, focused: bool) {
        self.bar.book_field = focused;
        if !focused {
            self.page_input = None;
        }
    }

    /// Header tooltips stay quiet while a modal covers the header.
    pub(super) fn chrome_tooltips(&self) -> bool {
        !self.show_settings
            && !self.show_search
            && self.confirm_remove.is_none()
            && self.notes.editor.is_none()
    }

    /// A menu closes with its document, behind a modal, and when a wider
    /// window brings More's buttons back to the bar.
    fn settle_menu(&mut self) {
        let modal = self.show_settings || self.show_search || self.confirm_remove.is_some();
        if self.document_path().is_none()
            || modal
            || (self.menu == Some(Menu::More) && fold(self) != Fold::Overflow)
        {
            self.menu = None;
        }
    }

    /// Applies what one message changed: an opened or closed document, the
    /// pointer, and whatever holds the bar.
    pub(super) fn sync_bar(&mut self, now: Instant, input: Input) {
        self.settle_menu();
        if let Some(modifiers) = input.modifiers {
            self.bar.modifiers = modifiers;
        }
        if self.document_path() != self.bar.document.as_deref() {
            if self.bar.document.is_some() {
                // Another document replaced this one under an open menu.
                self.menu = None;
            }
            self.bar.document = self.document_path().map(Path::to_path_buf);
            if self.bar.document.is_some() {
                self.bar.open(now);
            } else {
                self.bar.close(now);
            }
        }
        let listening = self.read_aloud.active();
        if listening && !self.bar.listening {
            self.bar.open(now);
        }
        self.bar.listening = listening;
        if let Some(pointer) = input.pointer {
            self.bar.pointer = pointer;
        }
        // Checked against the last pointer on every message, so a pill that
        // goes away under a resting pointer stops holding the bar.
        let hovered = self.bar.pointer.is_some_and(|point| {
            point.y < HEIGHT.max(REVEAL_ZONE)
                || pill_zone(self).is_some_and(|zone| zone.contains(point))
        });
        let held = self.bar_held();
        if self.bar.document.is_none() {
            // The Library header is static: only remember the inputs for the
            // next open, so no deadline and no frames start here.
            self.bar.hovered = hovered;
            self.bar.held = held;
            return;
        }
        self.bar.hover(now, hovered);
        self.bar.hold(now, held);
    }
}

/// The fold widths for the open document's bar; the Library has none.
pub(super) fn folds(reader: &Reader) -> Option<Folds> {
    if reader.pdf.is_some() {
        Some(PDF)
    } else {
        let book = reader.book.as_ref()?;
        Some(if book.pdf_source.is_some() {
            PDF_BOOK
        } else {
            BOOK
        })
    }
}

pub(super) fn fold(reader: &Reader) -> Fold {
    folds(reader).map_or(Fold::All, |folds| {
        let width = reader.window_size.width;
        if width >= folds.title {
            Fold::All
        } else if width >= folds.overflow {
            Fold::NoTitle
        } else {
            Fold::Overflow
        }
    })
}

/// What the bar holds at the window's width, in order.
pub(super) fn items(reader: &Reader, active: bool) -> Vec<Item> {
    items_at(reader, active, fold(reader))
}

pub(super) fn items_at(reader: &Reader, active: bool, fold: Fold) -> Vec<Item> {
    let book = reader.book.as_ref();
    let mut items = vec![Button::new(
        Control::Close,
        glyph::LIBRARY,
        "Library",
        Some("Ctrl+W"),
        active.then_some(Message::Close(CloseAction::Document)),
    )];
    if book.is_some() && !reader.returns.is_empty() {
        items.push(Button::new(
            Control::Back,
            glyph::BACK,
            "Back to the previous passage",
            Some("Alt+Left"),
            active.then_some(Message::Back),
        ));
    }
    if let Some(pdf) = &reader.pdf {
        items.push(Button::new(
            Control::BookMode,
            glyph::BOOK_MODE,
            "Read this PDF in Book mode",
            None,
            (active && pdf.document().can_copy).then_some(Message::BookMode),
        ));
    }
    if book.is_some_and(|book| book.pdf_source.is_some()) {
        items.push(Button::new(
            Control::DocumentMode,
            glyph::DOCUMENT_MODE,
            "Show the original PDF",
            None,
            active.then_some(Message::DocumentMode { source_page: true }),
        ));
    }
    let left = items.len();
    if !reader.contents().is_empty() {
        items.push(Button {
            selected: reader.show_contents,
            ..Button::new(
                Control::Contents,
                glyph::CONTENTS,
                "Contents",
                Some("Ctrl+T"),
                active.then_some(Message::ToggleContents),
            )
        });
    }
    let listening = reader.read_aloud.active();
    let appearance = reader.appearance;
    items.extend([
        Button::new(
            Control::Find,
            glyph::FIND,
            "Find in document",
            Some("Ctrl+F"),
            active.then_some(Message::FindOpen),
        ),
        Button {
            selected: listening,
            ..Button::new(
                Control::ReadAloud,
                if listening {
                    glyph::STOP
                } else {
                    glyph::LISTEN
                },
                if listening {
                    "Stop reading aloud"
                } else {
                    "Read aloud the selection or from this page"
                },
                Some("Ctrl+Shift+U"),
                active.then_some(Message::ReadAloud(read_aloud::Action::Toggle)),
            )
        },
        Button {
            selected: reader.notes.list.is_some(),
            ..Button::new(
                Control::Notes(notes::Focus::ToggleList),
                glyph::NOTES,
                if reader.notes.list.is_some() {
                    "Hide bookmarks and notes"
                } else {
                    "Show bookmarks and notes"
                },
                Some("Ctrl+B"),
                active.then_some(Message::Notes(notes::Action::ToggleList)),
            )
        },
        Button {
            selected: reader.menu == Some(Menu::Aa),
            ..Button::new(
                Control::Menu(Menu::Aa),
                glyph::AA,
                "Text, theme and zoom",
                None,
                active.then_some(Message::ToggleMenu(Menu::Aa)),
            )
        },
        Button {
            folds: true,
            ..Button::new(
                Control::Chrome(chrome::Action::ToggleAppearance),
                if appearance == Appearance::Dark {
                    glyph::LIGHT
                } else {
                    glyph::DARK
                },
                appearance.toggle_label(),
                None,
                Some(Message::Chrome(chrome::Action::ToggleAppearance)),
            )
        },
        Button {
            folds: true,
            ..Button::new(
                Control::Chrome(chrome::Action::Search),
                glyph::SEARCH,
                "Search or switch",
                Some("Ctrl+K"),
                Some(Message::Chrome(chrome::Action::Search)),
            )
        },
        Button {
            folds: true,
            ..Button::new(
                Control::Chrome(chrome::Action::Settings),
                glyph::SETTINGS,
                "Settings",
                None,
                Some(Message::Chrome(chrome::Action::Settings)),
            )
        },
    ]);
    if fold == Fold::Overflow {
        items.retain(|button| !button.folds);
        items.push(Button {
            selected: reader.menu == Some(Menu::More),
            ..Button::new(
                Control::Menu(Menu::More),
                glyph::MORE,
                "More",
                None,
                active.then_some(Message::ToggleMenu(Menu::More)),
            )
        });
    }
    let mut items: Vec<Item> = items
        .into_iter()
        .map(|button| Item::Button(Box::new(button)))
        .collect();
    let title = reader
        .pdf
        .as_ref()
        .map(|pdf| pdf.document().title.clone())
        .or_else(|| book.map(|book| book.title.clone()));
    items.splice(
        left..left,
        [
            Item::Fill {
                title: title.filter(|_| fold == Fold::All),
            },
            Item::Pages,
        ],
    );
    items
}

/// The buttons More holds once the bar folds.
pub(super) fn overflow(reader: &Reader, active: bool) -> Vec<Button> {
    items_at(reader, active, Fold::All)
        .into_iter()
        .filter_map(|item| match item {
            Item::Button(button) if button.folds => Some(*button),
            _ => None,
        })
        .collect()
}

/// The bar's controls in keyboard order: its items, an open menu's controls
/// right after the button that opened it, then the caption buttons.
pub(super) fn controls(reader: &Reader) -> Vec<Control> {
    let mut controls = Vec::new();
    for item in items(reader, true) {
        match item {
            Item::Button(button) => {
                if button.message.is_some() {
                    controls.push(button.control);
                }
                if let Control::Menu(menu) = button.control
                    && reader.menu == Some(menu)
                {
                    controls.extend(menu_controls(reader, menu));
                }
            }
            Item::Pages => controls.push(if reader.pdf.is_some() {
                Control::PdfPage
            } else {
                Control::BookPageInput
            }),
            Item::Fill { .. } => {}
        }
    }
    if !reader.fullscreen {
        controls.extend(
            [
                chrome::Action::Minimize,
                chrome::Action::Maximize,
                chrome::Action::Close,
            ]
            .map(Control::Chrome),
        );
    }
    controls
}

fn menu_controls(reader: &Reader, menu: Menu) -> Vec<Control> {
    match menu {
        Menu::Aa => reading_panel::controls(reader),
        Menu::More => overflow(reader, true)
            .into_iter()
            .filter(|button| button.message.is_some())
            .map(|button| button.control)
            .collect(),
    }
}

/// Where keyboard focus goes back to after a panel `control` opened closes:
/// the control, or More while the bar keeps it folded away.
pub(super) fn return_focus(reader: &Reader, control: Control) -> Control {
    let folded = fold(reader) == Fold::Overflow
        && overflow(reader, true)
            .iter()
            .any(|button| button.control == control);
    if folded {
        Control::Menu(Menu::More)
    } else {
        control
    }
}

/// The bar while shown or fading; otherwise only an invisible band that
/// reveals it. The band reports no mouse interaction, so the page under it
/// keeps its hover and clicks.
pub(super) fn view(reader: &Reader, active: bool) -> Element<'_, Message> {
    view_at(reader, active, fold(reader))
}

pub(super) fn view_at(reader: &Reader, active: bool, fold: Fold) -> Element<'_, Message> {
    if !reader.bar.visible() {
        return mouse_area(Space::new().width(Length::Fill).height(REVEAL_ZONE))
            .on_enter(Message::RevealBar)
            .into();
    }
    let hints = reader.chrome_tooltips();
    let mut left = row![].spacing(GAP).align_y(iced::Alignment::Center);
    let mut right = row![].spacing(GAP).align_y(iced::Alignment::Center);
    let mut fill: Element<'_, Message> = Space::new().width(Length::Fill).into();
    let mut past_fill = false;
    for item in items_at(reader, active, fold) {
        match item {
            Item::Button(button) => {
                let button = icon_button(reader, *button, hints);
                if past_fill {
                    right = right.push(button);
                } else {
                    left = left.push(button);
                }
            }
            Item::Fill { title } => {
                past_fill = true;
                fill = title_view(title);
            }
            Item::Pages => right = right.push(pages(reader)),
        }
    }
    let captions: Element<'_, Message> = if reader.fullscreen {
        Space::new().width(EDGE).into()
    } else {
        let focused = reader.focused.and_then(|control| match control {
            Control::Chrome(action) => Some(action),
            _ => None,
        });
        container(opaque(
            chrome::captions(
                focused,
                chrome::Controls {
                    maximized: reader.maximized,
                    tooltips: hints,
                },
            )
            .map(Message::Chrome),
        ))
        .padding(iced::Padding {
            left: 2.0 * GAP,
            ..iced::Padding::ZERO
        })
        .into()
    };
    // Controls and labels are never a drag surface, even when disabled; only
    // the bar's empty background and the title move the window.
    let bar = container(column![
        row![
            Space::new().width(EDGE),
            opaque(left),
            fill,
            opaque(right),
            captions
        ]
        .height(ROW)
        .align_y(iced::Alignment::Center),
        container(Space::new().width(Length::Fill).height(1)).style(|theme| container::Style {
            background: Some(ui::palette(theme).border.into()),
            ..container::Style::default()
        }),
    ])
    .width(Length::Fill)
    .style(ui::header);
    // Fullscreen has no window to drag, maximize or open a system menu for;
    // the bar still keeps presses away from the page under it.
    let content = if reader.fullscreen {
        opaque(bar)
    } else {
        chrome::drag_region(bar.into(), Message::Chrome)
    };
    // The wheel over the bar acts on the document under it.
    mouse_area(fade(reader, content))
        .on_scroll(Message::ChromeWheel)
        .into()
}

fn icon_button(reader: &Reader, button: Button, hints: bool) -> Element<'static, Message> {
    let focused = reader.focused == Some(button.control);
    let selected = button.selected;
    // An open menu's card sits where its button's tooltip would.
    let open = matches!(button.control, Control::Menu(menu) if reader.menu == Some(menu));
    let control = iced::widget::button(
        container(
            text(button.glyph)
                .font(ui::ICONS)
                .size(GLYPH)
                .line_height(1.0),
        )
        .center(Length::Fill),
    )
    .width(BUTTON)
    .height(BUTTON)
    .padding(0)
    .on_press_maybe(button.message.clone())
    .style(move |theme, status| icon_style(theme, status, focused, selected));
    let control: Element<'static, Message> = if focused {
        container(control)
            .id(iced::advanced::widget::Id::new(ui::FOCUSED_CONTROL))
            .into()
    } else {
        control.into()
    };
    chrome::hint(control, button.tooltip(), hints && !open)
}

fn icon_style(
    theme: &iced::Theme,
    status: iced::widget::button::Status,
    focused: bool,
    selected: bool,
) -> iced::widget::button::Style {
    use iced::widget::button::Status;
    let palette = ui::palette(theme);
    let hovered = matches!(status, Status::Hovered | Status::Pressed);
    let disabled = matches!(status, Status::Disabled);
    iced::widget::button::Style {
        background: if selected && !disabled {
            Some(palette.accent.scale_alpha(0.14).into())
        } else if hovered {
            Some(palette.raised.into())
        } else {
            None
        },
        text_color: if disabled {
            palette.muted
        } else if selected {
            palette.accent
        } else if hovered {
            palette.text
        } else {
            palette.secondary
        },
        border: iced::Border {
            color: if focused {
                palette.accent
            } else {
                iced::Color::TRANSPARENT
            },
            width: if focused { 2.0 } else { 0.0 },
            radius: 6.0.into(),
        },
        ..iced::widget::button::Style::default()
    }
}

fn title_view(title: Option<String>) -> Element<'static, Message> {
    let content: Element<'static, Message> = match title {
        Some(title) => text(title)
            .size(13)
            .wrapping(iced::widget::text::Wrapping::None)
            .style(ui::muted_text)
            .into(),
        None => Space::new().into(),
    };
    container(content)
        .id(iced::advanced::widget::Id::new(TITLE_ID))
        .width(Length::Fill)
        .padding([0.0, 3.0 * GAP])
        .clip(true)
        .into()
}

/// The page field and the page count.
fn pages(reader: &Reader) -> Element<'_, Message> {
    let field: Element<'_, Message> = if let Some(pdf) = &reader.pdf {
        let document = pdf.document().id;
        pdf.navigation(reader.focused == Some(Control::PdfPage))
            .map(move |message| Message::Pdf { document, message })
    } else {
        let current = book_pages::current(reader.pages(), reader.offset)
            .map_or_else(|| "1".to_owned(), |page| page.label.clone());
        row![
            ui::watch_focus(
                iced::widget::text_input(
                    if current.is_empty() { "—" } else { "Page" },
                    reader.page_input.as_deref().unwrap_or(&current),
                )
                .id(iced::advanced::widget::Id::new("book-page"))
                .on_input(Message::PageInput)
                .on_submit(Message::PageSubmit)
                .width(56)
                .size(13)
                .style(ui::input_style),
                Message::BookPageFocus,
            ),
            text(format!("/ {}", reader.page_total()))
                .size(12)
                .style(ui::muted_text),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center)
        .into()
    };
    container(field)
        .padding(iced::Padding {
            right: 2.0 * GAP,
            ..iced::Padding::ZERO
        })
        .into()
}

/// The open menu's card, right-aligned under the bar, over a layer that
/// closes it on a press anywhere else below the bar. That layer reports no
/// mouse interaction, so the page under it keeps its hover and its wheel.
pub(super) fn menu(reader: &Reader, active: bool) -> Option<Element<'_, Message>> {
    let menu = reader.menu?;
    let (content, width) = match menu {
        Menu::Aa => (reading_panel::view(reader, active), reading_panel::WIDTH),
        Menu::More => (more(reader, active), 240.0),
    };
    let card = container(
        iced::widget::scrollable(container(content).padding(iced::Padding {
            right: 10.0,
            ..iced::Padding::ZERO
        }))
        .id(iced::advanced::widget::Id::new("reading-bar-menu"))
        .direction(ui::vertical_scrollbar())
        .style(ui::scroll_style)
        .height(Length::Shrink),
    )
    .padding(iced::Padding {
        top: 12.0,
        right: 2.0,
        bottom: 12.0,
        left: 12.0,
    })
    .width(width.min(reader.window_size.width - 2.0 * EDGE))
    .max_height((reader.window_size.height - HEIGHT - 2.0 * EDGE).max(120.0))
    .style(ui::panel);
    Some(
        column![
            Space::new().height(HEIGHT),
            iced::widget::stack![
                mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
                    .on_press(Message::CloseMenu),
                container(opaque(card))
                    .width(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Right)
                    .padding(iced::Padding {
                        top: EDGE,
                        right: EDGE,
                        ..iced::Padding::ZERO
                    }),
            ]
        ]
        .into(),
    )
}

/// More's rows: the folded buttons with their names and shortcuts.
fn more(reader: &Reader, active: bool) -> Element<'_, Message> {
    let rows = overflow(reader, active).into_iter().map(|button| {
        let focused = reader.focused == Some(button.control);
        let label = row![
            text(button.glyph).font(ui::ICONS).size(18).line_height(1.0),
            text(button.label).size(13),
            Space::new().width(Length::Fill),
            text(button.shortcut.unwrap_or_default())
                .size(12)
                .style(ui::muted_text),
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center);
        let row = iced::widget::button(label)
            .width(Length::Fill)
            .padding([7, 10])
            .on_press_maybe(button.message)
            .style(move |theme, status| {
                ui::button_style(theme, status, ui::ButtonTone::Subtle, focused, false)
            });
        if focused {
            container(row)
                .id(iced::advanced::widget::Id::new(ui::FOCUSED_CONTROL))
                .into()
        } else {
            row.into()
        }
    });
    column(rows).spacing(2).into()
}

/// The wheel over the bar or a page strip behaves as it does over the page:
/// Ctrl+wheel and touchpad pinch zoom through the same path, anything else
/// scrolls.
pub(super) fn wheel(reader: &mut Reader, delta: mouse::ScrollDelta) -> Task<Message> {
    let modifiers = reader.bar.modifiers;
    if !crate::document_scroll::control_pressed(modifiers) {
        // At the page's end the forwarded scroll moves nothing, so the push
        // toward the next page counts here. A turn replaces the scroll, which
        // would otherwise move the new page.
        return reader
            .overscroll(delta, Instant::now())
            .unwrap_or_else(|| forward_wheel(reader, delta, modifiers));
    }
    let notches = crate::document_scroll::zoom_notches(delta);
    if notches == 0.0 {
        Task::none()
    } else if let Some(pdf) = &reader.pdf {
        let document = pdf.document().id;
        update_inner(
            reader,
            Message::Pdf {
                document,
                message: pdf_reader::Message::ZoomBy(notches),
            },
        )
    } else if reader.book.is_some() {
        update_inner(reader, Message::ZoomBy(notches))
    } else {
        Task::none()
    }
}

/// How far an iced scrollable moves for a wheel `delta`: 60 px per line, with
/// Shift turning line scrolling sideways, and pixel deltas as they come.
pub(super) fn wheel_offset(
    delta: mouse::ScrollDelta,
    shift: bool,
) -> iced::widget::scrollable::AbsoluteOffset {
    let (x, y) = match delta {
        mouse::ScrollDelta::Lines { x, y } => {
            let (x, y) = if shift { (y, x) } else { (x, y) };
            (-x * WHEEL_LINE, -y * WHEEL_LINE)
        }
        mouse::ScrollDelta::Pixels { x, y } => (-x, -y),
    };
    iced::widget::scrollable::AbsoluteOffset { x, y }
}

/// Scrolls the open document by a wheel `delta` received over the bar or a
/// page strip, by the same distance its scrollable moves for that delta.
pub(super) fn forward_wheel(
    reader: &Reader,
    delta: mouse::ScrollDelta,
    modifiers: keyboard::Modifiers,
) -> Task<Message> {
    let offset = wheel_offset(delta, crate::document_scroll::shift_pressed(modifiers));
    if reader.pdf.is_some() {
        iced::widget::operation::scroll_by(pdf_reader::scroll_id(), offset)
    } else if reader.book.is_some() {
        iced::widget::operation::scroll_by(scroll_id(), offset)
    } else {
        Task::none()
    }
}

/// `content` fading in and out with the bar.
pub(super) fn fade<'a>(reader: &Reader, content: Element<'a, Message>) -> Element<'a, Message> {
    Element::new(Fade {
        content,
        alpha: reader.bar.alpha(),
    })
}

/// Draws its content with every palette colour at `alpha`, so the bar fades as one.
struct Fade<'a> {
    content: Element<'a, Message>,
    alpha: f32,
}

impl Widget<Message, iced::Theme, iced::Renderer> for Fade<'_> {
    fn children(&self) -> Vec<widget::Tree> {
        vec![widget::Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut widget::Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }
    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let style = renderer::Style {
            text_color: style.text_color.scale_alpha(self.alpha),
        };
        ui::with_alpha(self.alpha, || {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                &style,
                layout,
                cursor,
                viewport,
            );
        });
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut widget::Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seconds(value: f32) -> Duration {
        Duration::from_secs_f32(value)
    }

    fn pointer_at(y: f32) -> Input {
        Input {
            pointer: Some(Some(Point::new(200.0, y))),
            ..Input::default()
        }
    }

    #[test]
    fn opened_then_idle_hides_after_two_seconds() {
        let mut bar = Bar::default();
        let opened = Instant::now();
        bar.open(opened);
        assert_eq!(
            bar.state(),
            BarState::Shown {
                hide_at: Some(opened + OPEN_HOLD)
            }
        );
        assert!(bar.needs_frames());
        bar.tick(opened + seconds(1.9));
        assert!(matches!(bar.state(), BarState::Shown { .. }));
        bar.tick(opened + seconds(2.1));
        assert_eq!(bar.state(), BarState::Hidden);
        assert!(bar.visible(), "the fade-out keeps the bar drawn");
        assert!(bar.needs_frames());
        bar.tick(opened + seconds(2.1) + FADE);
        assert!(!bar.visible());
        assert!(!bar.needs_frames(), "a hidden bar needs no frames");
    }

    #[test]
    fn focus_holds_the_bar() {
        let mut bar = Bar::default();
        let opened = Instant::now();
        bar.open(opened);
        bar.tick(opened + seconds(2.5));
        assert_eq!(bar.state(), BarState::Hidden);
        bar.hold(opened + seconds(3.0), true);
        assert_eq!(bar.state(), BarState::Shown { hide_at: None });
        bar.tick(opened + seconds(30.0));
        assert_eq!(bar.state(), BarState::Shown { hide_at: None });
        assert!(!bar.needs_frames(), "a held bar waits without frames");
        bar.hold(opened + seconds(31.0), false);
        bar.tick(opened + seconds(32.6));
        assert_eq!(bar.state(), BarState::Hidden);

        let mut reader = Reader {
            book: Some(super::super::tests::book("bar-focus")),
            ..Reader::default()
        };
        reader.sync_bar(opened, Input::default());
        reader.bar.tick(opened + seconds(2.5));
        assert_eq!(reader.bar.state(), BarState::Hidden);
        assert!(
            reader.controls().any(|control| control == Control::Find),
            "bar controls stay in the focus order while hidden"
        );
        reader.focused = Some(Control::Find);
        reader.sync_bar(opened + seconds(3.0), Input::default());
        reader.bar.tick(opened + seconds(9.0));
        assert_eq!(reader.bar.state(), BarState::Shown { hide_at: None });
    }

    #[test]
    fn pin_survives_idle() {
        let mut bar = Bar::default();
        let opened = Instant::now();
        bar.open(opened);
        bar.toggle_pin(opened + seconds(0.5));
        bar.tick(opened + seconds(60.0));
        assert_eq!(bar.state(), BarState::Pinned);
        assert!(!bar.dismiss(opened + seconds(60.0), false));
        assert_eq!(bar.state(), BarState::Pinned);
        bar.toggle_pin(opened + seconds(61.0));
        bar.tick(opened + seconds(62.6));
        assert_eq!(bar.state(), BarState::Hidden);
    }

    #[test]
    fn pointer_reveals_and_leaving_hides_after_the_delay() {
        let mut bar = Bar::default();
        let opened = Instant::now();
        bar.open(opened);
        bar.tick(opened + seconds(2.5));
        assert_eq!(bar.state(), BarState::Hidden);
        bar.hover(opened + seconds(3.0), true);
        assert_eq!(bar.state(), BarState::Shown { hide_at: None });
        bar.tick(opened + seconds(13.0));
        assert_eq!(bar.state(), BarState::Shown { hide_at: None });
        bar.hover(opened + seconds(14.0), false);
        bar.tick(opened + seconds(15.0));
        assert!(matches!(bar.state(), BarState::Shown { .. }));
        bar.tick(opened + seconds(15.6));
        assert_eq!(bar.state(), BarState::Hidden);
    }

    #[test]
    fn the_library_never_starts_bar_frames() {
        let mut reader = Reader::default();
        let now = Instant::now();
        reader.sync_bar(now, pointer_at(10.0));
        reader.sync_bar(now, pointer_at(300.0));
        reader.show_settings = true;
        reader.sync_bar(now, Input::default());
        reader.show_settings = false;
        reader.sync_bar(now, Input::default());
        assert!(!reader.bar.needs_frames());

        reader.book = Some(super::super::tests::book("bar-library"));
        reader.sync_bar(now, Input::default());
        assert!(
            reader.bar.needs_frames(),
            "opening a book starts the open delay"
        );
        reader.book = None;
        reader.sync_bar(now + seconds(0.5), Input::default());
        assert!(!reader.bar.needs_frames(), "closing it stops the frames");
    }

    #[test]
    fn a_slow_first_layout_does_not_stretch_the_open_delay() {
        let mut reader = Reader {
            book: Some(super::super::tests::book("bar-paginating")),
            pagination: Some(Arc::new(AtomicBool::new(false))),
            ..Reader::default()
        };
        let opened = Instant::now();
        reader.sync_bar(opened, Input::default());
        assert!(reader.has_panels(), "the loading line shows below the bar");
        reader.bar.tick(opened + seconds(2.1));
        assert_eq!(reader.bar.state(), BarState::Hidden);
    }

    #[test]
    fn find_listen_and_contents_float_without_holding_the_bar() {
        let mut reader = super::super::book_preview::full_bar_book(1400.0, 900.0);
        let opened = Instant::now();
        reader.sync_bar(opened, Input::default());
        reader.begin_silent_listening();
        let _ = update_inner(&mut reader, Message::FindOpen);
        let _ = update_inner(&mut reader, Message::ToggleContents);
        assert!(reader.find.is_some() && reader.read_aloud.active() && reader.show_contents);
        reader.focused = None;
        // Listening started at 0.5 s shows the bar for its own 2 s.
        reader.sync_bar(opened + seconds(0.5), Input::default());
        reader.bar.tick(opened + seconds(2.6));
        assert_eq!(
            reader.bar.state(),
            BarState::Hidden,
            "the bar idles away over open panels that float"
        );
        reader.show_help = true;
        reader.sync_bar(opened + seconds(3.0), Input::default());
        assert_eq!(
            reader.bar.state(),
            BarState::Shown { hide_at: None },
            "help still hangs below the bar and holds it"
        );
    }

    fn escape() -> Message {
        Message::Event(
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                modified_key: keyboard::Key::Named(keyboard::key::Named::Escape),
                physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Escape),
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::empty(),
                text: None,
                repeat: false,
            }),
            iced::window::Id::unique(),
        )
    }

    #[test]
    fn an_open_menu_holds_the_bar_and_escape_returns_to_its_button() {
        let mut reader = Reader {
            book: Some(super::super::tests::book("bar-menu")),
            ..Reader::default()
        };
        let opened = Instant::now();
        reader.sync_bar(opened, Input::default());
        let _ = update_inner(&mut reader, Message::ToggleMenu(Menu::Aa));
        reader.sync_bar(opened + seconds(0.5), Input::default());
        reader.bar.tick(opened + seconds(30.0));
        assert_eq!(
            reader.bar.state(),
            BarState::Shown { hide_at: None },
            "the open panel holds the bar past its hide deadline"
        );

        let _ = update_inner(&mut reader, escape());
        assert_eq!(reader.menu, None);
        assert_eq!(reader.focused, Some(Control::Menu(Menu::Aa)));
        let _ = update_inner(&mut reader, escape());
        assert_eq!(reader.focused, None, "a second Escape leaves the bar");
    }

    #[test]
    fn opening_a_menu_closes_the_other_and_closing_the_book_closes_both() {
        let mut reader = Reader {
            book: Some(super::super::tests::book("bar-menus")),
            window_size: Size::new(540.0, 600.0),
            ..Reader::default()
        };
        let _ = update_inner(&mut reader, Message::ToggleMenu(Menu::More));
        assert_eq!(reader.menu, Some(Menu::More));
        let _ = update_inner(&mut reader, Message::ToggleMenu(Menu::Aa));
        assert_eq!(reader.menu, Some(Menu::Aa));
        let _ = update_inner(&mut reader, Message::ToggleMenu(Menu::Aa));
        assert_eq!(reader.menu, None, "its button closes it again");

        let _ = update_inner(&mut reader, Message::ToggleMenu(Menu::More));
        let _ = update_inner(
            &mut reader,
            Message::Chrome(chrome::Action::ToggleAppearance),
        );
        assert_eq!(reader.menu, None, "choosing an item in More closes it");
        assert_eq!(reader.appearance, Appearance::Dark);
        let _ = update_inner(&mut reader, Message::ToggleMenu(Menu::More));
        let _ = update_inner(&mut reader, Message::Chrome(chrome::Action::Settings));
        assert!(reader.show_settings && reader.menu.is_none());
        let _ = update_inner(&mut reader, Message::Chrome(chrome::Action::Settings));
        assert_eq!(
            reader.focused,
            Some(Control::Menu(Menu::More)),
            "closing Settings returns focus to More, which holds it"
        );

        let _ = update_inner(&mut reader, Message::ToggleMenu(Menu::More));
        reader.window_size.width = 1280.0;
        reader.sync_bar(Instant::now(), Input::default());
        assert_eq!(reader.menu, None, "a wide bar has nothing in More");

        let _ = update_inner(&mut reader, Message::ToggleMenu(Menu::Aa));
        reader.book = None;
        reader.sync_bar(Instant::now(), Input::default());
        assert_eq!(reader.menu, None, "closing the book closes the panel");
    }

    #[test]
    fn escape_hides_only_an_idle_bar() {
        let mut bar = Bar::default();
        let opened = Instant::now();
        bar.open(opened);
        bar.hover(opened, true);
        assert!(
            !bar.dismiss(opened + seconds(0.5), false),
            "the pointer is on it"
        );
        bar.hover(opened + seconds(0.6), false);
        assert!(
            !bar.dismiss(opened + seconds(0.7), true),
            "a panel holds it"
        );
        assert!(bar.dismiss(opened + seconds(0.8), false));
        assert_eq!(bar.state(), BarState::Hidden);
        assert!(!bar.dismiss(opened + seconds(0.9), false), "already hidden");
    }
}
