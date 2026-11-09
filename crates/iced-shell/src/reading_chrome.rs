//! The reading bar: the window tools, Library, the page field, zoom, Notes and
//! the caption buttons in one bar that floats over the top of a document and hides
//! when idle. [`Bar`] holds the visibility rules; every transition takes an
//! explicit [`Instant`] so the rules are testable without a clock.

use super::*;
use iced::Animation;
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, overlay, renderer, widget};
use iced::widget::{Space, column, mouse_area, opaque};
use iced::{Point, Rectangle, Vector};
use std::time::{Duration, Instant};

/// Height of one row of the bar.
const ROW: f32 = 43.0;
/// The pointer reveals the bar anywhere in this band at the top of the window.
pub(super) const REVEAL_ZONE: f32 = 48.0;
const OPEN_HOLD: Duration = Duration::from_millis(2000);
const LEAVE_HOLD: Duration = Duration::from_millis(1500);
const FADE: Duration = Duration::from_millis(150);
/// Narrowest windows, in logical pixels, whose bar fits on one row with every
/// optional control shown and four-digit page counts: with tight buttons, and
/// with roomy ones. Below `roomy` the buttons tighten and Library shows only
/// its arrow; below `one_row` the bar takes two rows. The ignored test
/// `measure_reading_bar_thresholds` measures them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Thresholds {
    pub(super) one_row: f32,
    pub(super) roomy: f32,
}

const BOOK: Thresholds = Thresholds {
    one_row: 765.0,
    roomy: 898.0,
};
/// A PDF read in Book mode adds its Document button and has no Contents.
const PDF_BOOK: Thresholds = Thresholds {
    one_row: 776.0,
    roomy: 909.0,
};
const PDF: Thresholds = Thresholds {
    one_row: 793.0,
    roomy: 973.0,
};
/// Space between the bar's groups; compact windows halve it.
const GROUP_GAP: f32 = 8.0;
/// Pixels an iced scrollable moves per wheel line.
const WHEEL_LINE: f32 = 60.0;

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
    /// The book page field has keyboard focus.
    book_field: bool,
    /// Keyboard modifiers, for the wheel over the bar.
    modifiers: keyboard::Modifiers,
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
            | Control::FontDown
            | Control::FitBookWidth
            | Control::FontUp
            | Control::Pdf(_)
    )
}

impl Reader {
    fn document_path(&self) -> Option<&Path> {
        self.pdf
            .as_ref()
            .map(|pdf| pdf.document().path.as_path())
            .or_else(|| self.book.as_ref().map(|book| book.path.as_path()))
    }

    /// A bar control has keyboard focus, a panel the bar opened is open, or a
    /// panel hangs below the bar. The transient loading line does not hold it,
    /// so a slow first layout never stretches the 2 s shown after opening.
    pub(super) fn bar_held(&self) -> bool {
        self.focused.is_some_and(is_bar_control)
            || self.bar.book_field
            || self
                .pdf
                .as_ref()
                .is_some_and(pdf_reader::Reader::page_focused)
            || self.show_settings
            || self.show_search
            || self.has_held_panels()
    }

    /// Something shows in the panel area under the bar.
    pub(super) fn has_panels(&self) -> bool {
        self.has_held_panels() || self.opening.is_some() || self.pagination.is_some()
    }

    /// Find, the read-aloud player, contents, help, recent documents, the
    /// library banner or notices.
    fn has_held_panels(&self) -> bool {
        self.find.is_some()
            || self.read_aloud.active()
            || (self.show_contents && !self.contents().is_empty())
            || self.error.is_some()
            || self.history_notice.is_some()
            || self.show_recent
            || self.show_help
            || self.preferences_notice.is_some()
            || self.shelf.notice.is_some()
            || library_ui::banner_visible(self)
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

    /// Applies what one message changed: an opened or closed document, the
    /// pointer, and whatever holds the bar.
    pub(super) fn sync_bar(&mut self, now: Instant, input: Input) {
        if let Some(modifiers) = input.modifiers {
            self.bar.modifiers = modifiers;
        }
        if self.document_path() != self.bar.document.as_deref() {
            self.bar.document = self.document_path().map(Path::to_path_buf);
            if self.bar.document.is_some() {
                self.bar.open(now);
            } else {
                self.bar.close(now);
            }
        }
        let zone = if self.bar.visible() {
            height(self).max(REVEAL_ZONE)
        } else {
            REVEAL_ZONE
        };
        let hovered = input
            .pointer
            .map(|pointer| pointer.is_some_and(|point| point.y < zone));
        let held = self.bar_held();
        if self.bar.document.is_none() {
            // The Library header is static: only remember the inputs for the
            // next open, so no deadline and no frames start here.
            self.bar.hovered = hovered.unwrap_or(self.bar.hovered);
            self.bar.held = held;
            return;
        }
        if let Some(hovered) = hovered {
            self.bar.hover(now, hovered);
        }
        self.bar.hold(now, held);
    }
}

/// How the bar lays out: tight or roomy buttons, on one row or two.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(super) struct Presentation {
    pub(super) compact: bool,
    pub(super) one_row: bool,
}

#[cfg(test)]
thread_local! {
    /// Overrides the presentation the window width picks, so a test can
    /// measure the narrowest width at which each presentation still fits.
    pub(super) static FORCED: std::cell::Cell<Option<Presentation>> =
        const { std::cell::Cell::new(None) };
}

/// Narrow windows tighten the reading controls so the bar keeps one row.
pub(super) fn compact(reader: &Reader) -> bool {
    #[cfg(test)]
    if let Some(forced) = FORCED.get() {
        return forced.compact;
    }
    thresholds(reader).is_some_and(|limits| reader.window_size.width < limits.roomy)
}

/// The thresholds for the open document's bar; the Library has none.
pub(super) fn thresholds(reader: &Reader) -> Option<Thresholds> {
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

fn one_row(reader: &Reader) -> bool {
    #[cfg(test)]
    if let Some(forced) = FORCED.get() {
        return forced.one_row;
    }
    thresholds(reader).is_none_or(|limits| reader.window_size.width >= limits.one_row)
}

/// The height the bar takes when shown, which the reading screen keeps free at its top.
pub(super) fn height(reader: &Reader) -> f32 {
    if one_row(reader) {
        ROW + 1.0
    } else {
        2.0 * ROW + 1.0
    }
}

/// The bar while shown or fading; otherwise only an invisible band that
/// reveals it. The band reports no mouse interaction, so the page under it
/// keeps its hover and clicks.
pub(super) fn view(reader: &Reader, active: bool) -> Element<'_, Message> {
    if !reader.bar.visible() {
        return mouse_area(Space::new().width(Length::Fill).height(REVEAL_ZONE))
            .on_enter(Message::RevealBar)
            .into();
    }
    let focused = reader.focused.and_then(|control| match control {
        Control::Chrome(action) => Some(action),
        _ => None,
    });
    let hints = reader.chrome_tooltips();
    let gap = if compact(reader) {
        GROUP_GAP / 2.0
    } else {
        GROUP_GAP
    };
    // Controls and labels are never a drag surface, even when disabled; only
    // the bar's empty background moves the window.
    let tools = container(opaque(
        chrome::tools(focused, hints, reader.appearance).map(Message::Chrome),
    ))
    .padding(iced::Padding {
        left: chrome::TOOLS_EDGE,
        right: gap,
        ..iced::Padding::ZERO
    });
    let captions: Element<'_, Message> = if reader.fullscreen {
        Space::new().width(GROUP_GAP).into()
    } else {
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
            left: gap,
            ..iced::Padding::ZERO
        })
        .into()
    };
    let groups = if reader.pdf.is_some() {
        pdf_groups(reader, active)
    } else {
        book_groups(reader, active)
    };
    let groups = Groups {
        left: opaque(groups.left),
        middle: opaque(groups.middle),
        right: opaque(groups.right),
    };
    let fill = || Space::new().width(Length::Fill);
    let rows: Element<'_, Message> = if one_row(reader) {
        row![
            tools,
            groups.left,
            fill(),
            container(groups.middle).padding([0.0, gap]),
            fill(),
            groups.right,
            captions,
        ]
        .height(ROW)
        .align_y(iced::Alignment::Center)
        .into()
    } else {
        column![
            row![tools, groups.left, fill(), captions]
                .height(ROW)
                .align_y(iced::Alignment::Center),
            row![groups.middle, fill(), groups.right]
                .spacing(GROUP_GAP)
                .height(ROW)
                .padding([0.0, chrome::TOOLS_EDGE])
                .align_y(iced::Alignment::Center),
        ]
        .into()
    };
    let bar = container(column![
        rows,
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
    mouse_area(Element::new(Fade {
        content,
        alpha: reader.bar.alpha(),
    }))
    .on_scroll(Message::ChromeWheel)
    .into()
}

/// The wheel over the bar or a page strip behaves as it does over the page:
/// Ctrl+wheel and touchpad pinch zoom through the same path, anything else
/// scrolls.
pub(super) fn wheel(reader: &mut Reader, delta: mouse::ScrollDelta) -> Task<Message> {
    let modifiers = reader.bar.modifiers;
    if !crate::document_scroll::control_pressed(modifiers) {
        return forward_wheel(reader, delta, modifiers);
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

struct Groups<'a> {
    left: Element<'a, Message>,
    middle: Element<'a, Message>,
    right: Element<'a, Message>,
}

fn library_button(reader: &Reader, active: bool) -> Element<'static, Message> {
    let arrow = text("←").font(ui::SANS).size(16);
    if compact(reader) {
        hinted_control(
            reader,
            Control::Close,
            arrow,
            "Library (Ctrl+W)",
            active.then_some(Message::Close(CloseAction::Document)),
        )
    } else {
        control_button(
            reader,
            Control::Close,
            row![arrow, text("Library").font(ui::MEDIUM).size(13)]
                .spacing(6)
                .align_y(iced::Alignment::Center),
            active.then_some(Message::Close(CloseAction::Document)),
        )
    }
}

fn pdf_groups(reader: &Reader, active: bool) -> Groups<'_> {
    let pdf = reader.pdf.as_ref().unwrap();
    let id = pdf.document().id;
    let mode = hinted_control(
        reader,
        Control::BookMode,
        text("Book").font(ui::MEDIUM).size(13),
        "Read this PDF in Book mode",
        (active && pdf.document().can_copy).then_some(Message::BookMode),
    );
    let focused = reader.focused.and_then(|control| match control {
        Control::Pdf(control) => Some(control),
        _ => None,
    });
    let forward = move |message| Message::Pdf {
        document: id,
        message,
    };
    Groups {
        left: row![library_button(reader, active), mode]
            .spacing(if compact(reader) { 6 } else { 8 })
            .align_y(iced::Alignment::Center)
            .into(),
        middle: pdf.navigation(focused).map(forward),
        right: row![
            pdf.zoom_controls(focused, compact(reader)).map(forward),
            find_button(reader, active),
            listen_button(reader, active),
            notes_button(reader, active),
        ]
        .spacing(if compact(reader) { 6 } else { 8 })
        .align_y(iced::Alignment::Center)
        .into(),
    }
}

fn book_groups(reader: &Reader, active: bool) -> Groups<'_> {
    let mut left = row![library_button(reader, active)]
        .spacing(6)
        .align_y(iced::Alignment::Center);
    if !reader.returns.is_empty() {
        left = left.push(hinted_control(
            reader,
            Control::Back,
            text("↶").size(16),
            "Back to previous passage (Alt+Left)",
            active.then_some(Message::Back),
        ));
    }
    if reader.book.as_ref().is_some_and(|b| b.pdf_source.is_some()) {
        left = left.push(control_button(
            reader,
            Control::DocumentMode,
            text("Document").font(ui::MEDIUM).size(13),
            active.then_some(Message::DocumentMode { source_page: true }),
        ));
    }
    let pages = reader.pages();
    let current = book_pages::current(pages, reader.offset)
        .map_or_else(|| "1".to_owned(), |p| p.label.clone());
    let mut navigation = row![
        ui::watch_focus(
            iced::widget::text_input(
                if current.is_empty() { "—" } else { "Page" },
                reader.page_input.as_deref().unwrap_or(&current),
            )
            .id(iced::advanced::widget::Id::new("book-page"))
            .on_input(Message::PageInput)
            .on_submit(Message::PageSubmit)
            .width(64)
            .size(13)
            .style(ui::input_style),
            Message::BookPageFocus,
        ),
        text(format!("/ {}", reader.page_total()))
            .size(12)
            .style(ui::muted_text),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);
    if !reader.contents().is_empty() {
        navigation = navigation.push(toned_button(
            reader,
            Control::Contents,
            text("Contents").font(ui::MEDIUM).size(13),
            active.then_some(Message::ToggleContents),
            ui::ButtonTone::Surface,
            reader.show_contents,
        ));
    }
    let right = row![
        find_button(reader, active),
        listen_button(reader, active),
        notes_button(reader, active),
        hinted_control(
            reader,
            Control::FontDown,
            text("−").size(16),
            "Zoom out",
            (active && reader.zoom > 0.25).then_some(Message::Zoom(reader.zoom - 0.1))
        ),
        hinted_control(
            reader,
            Control::FitBookWidth,
            text(if reader.fit_width {
                "Fit".to_owned()
            } else {
                format!("{:.0}%", reader.zoom * 100.0)
            })
            .size(12),
            "Fit width / restore zoom (Ctrl+Shift+F)",
            active.then_some(Message::FitBookWidth)
        ),
        hinted_control(
            reader,
            Control::FontUp,
            text("+").size(16),
            "Zoom in",
            (active && reader.zoom < 3.0).then_some(Message::Zoom(reader.zoom + 0.1))
        ),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);
    Groups {
        left: left.into(),
        middle: navigation.into(),
        right: right.into(),
    }
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
