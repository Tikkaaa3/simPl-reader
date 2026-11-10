//! The Aa panel that drops from the reading bar: the open book's own theme and
//! typography, and zoom. PDF views keep their source layout, so there it
//! offers only zoom.

use super::*;
use iced::widget::{Space, column};
use reading_ui::Target;

pub(super) const WIDTH: f32 = 360.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Zoom {
    Out,
    In,
    Fit,
}

/// A reflowable book, the only kind whose typography the reader sets.
fn reflowable(reader: &Reader) -> bool {
    reader.pdf.is_none()
        && reader
            .book
            .as_ref()
            .is_some_and(|book| book.pdf_source.is_none())
}

/// What a zoom control does in the open view, or `None` at its limit.
pub(super) fn message(reader: &Reader, zoom: Zoom) -> Option<Message> {
    if let Some(pdf) = &reader.pdf {
        let document = pdf.document().id;
        let message = match zoom {
            Zoom::Out => pdf_reader::Message::ZoomOut,
            Zoom::In => pdf_reader::Message::ZoomIn,
            Zoom::Fit => pdf_reader::Message::FitWidth,
        };
        return Some(Message::Pdf { document, message });
    }
    reader.book.as_ref()?;
    match zoom {
        Zoom::Out => (reader.zoom > 0.25).then(|| Message::Zoom(reader.zoom - 0.1)),
        Zoom::In => (reader.zoom < 3.0).then(|| Message::Zoom(reader.zoom + 0.1)),
        Zoom::Fit => Some(Message::FitBookWidth),
    }
}

/// The panel's controls in the order it shows them.
pub(super) fn controls(reader: &Reader) -> Vec<Control> {
    let mut controls = Vec::new();
    if reflowable(reader) {
        controls.extend(
            (0..themes::THEMES.len())
                .filter(|_| reading_ui::enabled(reader, Target::Book))
                .map(|index| Control::Theme(Target::Book, index)),
        );
        controls.extend(reading_ui::controls(reader, Target::Book));
    }
    controls.extend(
        [Zoom::Out, Zoom::In, Zoom::Fit]
            .into_iter()
            .filter(|zoom| message(reader, *zoom).is_some())
            .map(Control::Zoom),
    );
    controls
}

pub(super) fn view(reader: &Reader, active: bool) -> Element<'_, Message> {
    let section = |title: &'static str| {
        text(title.to_uppercase())
            .font(ui::MEDIUM)
            .size(11)
            .style(ui::muted_text)
    };
    let rule = || {
        container(Space::new().height(1))
            .width(Length::Fill)
            .style(ui::rule)
    };
    let mut content = column![].spacing(10);
    if reflowable(reader) {
        content = content
            .push(section("Theme"))
            .push(theme_picker(reader, Target::Book))
            .push(rule())
            .push(section("This book"))
            .push(reading_ui::typography(reader, Target::Book))
            .push(rule());
    }
    let (percent, fits) = match &reader.pdf {
        Some(pdf) => (pdf.zoom_percent(), pdf.fits_width()),
        None => (reader.zoom * 100.0, reader.fit_width),
    };
    let button = |zoom, label: &'static str, selected| {
        toned_button(
            reader,
            Control::Zoom(zoom),
            text(label).size(12),
            message(reader, zoom).filter(|_| active),
            ui::ButtonTone::Surface,
            selected,
        )
    };
    content = content.push(section("Zoom")).push(
        row![
            button(Zoom::Out, "−", false),
            text(format!("{percent:.0}%"))
                .size(12)
                .width(44)
                .align_x(iced::alignment::Horizontal::Center),
            button(Zoom::In, "+", false),
            Space::new().width(Length::Fill),
            button(Zoom::Fit, "Fit width", fits),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    );
    if !reflowable(reader) {
        content = content.push(
            text("A PDF keeps its own fonts and layout, so only zoom applies.")
                .size(12)
                .style(ui::muted_text),
        );
    }
    content.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aa_panel_changes_the_open_book_only() {
        let mut reader = super::super::tests::themed_reader(30);
        let _ = reader.sync_reading_settings();
        let fingerprint = reader.book.as_ref().unwrap().fingerprint.clone();
        let _ = reader.reading_loaded(fingerprint, Ok(None));
        let _ = update(&mut reader, Message::ToggleMenu(reading_chrome::Menu::Aa));
        let size_up = Control::Reading(Target::Book, reading_ui::Focus::SizeUp);
        let controls: Vec<_> = reader.controls().collect();
        let aa = controls
            .iter()
            .position(|control| *control == Control::Menu(reading_chrome::Menu::Aa))
            .unwrap();
        assert_eq!(
            controls[aa + 1..aa + 3],
            [
                Control::Theme(Target::Book, 0),
                Control::Theme(Target::Book, 1)
            ],
            "Tab moves from Aa into its panel"
        );
        assert!(controls.contains(&size_up));
        for _ in 0..2 {
            let _ = reader.activate(size_up);
        }
        settle_theme(&mut reader);
        assert_eq!(reader.reading.options().size, 22);
        assert_eq!(reader.font_size, 22.0);
        assert_eq!(reader.reading.defaults.size, 20, "the defaults stay");
    }

    fn aa_book_with_size_22() -> Reader {
        let mut reader = super::super::tests::themed_reader(30);
        let _ = reader.sync_reading_settings();
        let fingerprint = reader.book.as_ref().unwrap().fingerprint.clone();
        let _ = reader.reading_loaded(fingerprint, Ok(None));
        let _ = update(&mut reader, Message::ToggleMenu(reading_chrome::Menu::Aa));
        let size_up = Control::Reading(Target::Book, reading_ui::Focus::SizeUp);
        for _ in 0..2 {
            let _ = reader.activate(size_up);
        }
        settle_theme(&mut reader);
        reader
    }

    fn escape() -> Message {
        Message::Event(
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(key::Named::Escape),
                modified_key: Key::Named(key::Named::Escape),
                physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::empty(),
                text: None,
                repeat: false,
            }),
            window::Id::unique(),
        )
    }

    #[test]
    fn a_default_the_book_overrides_keeps_its_pages() {
        let mut reader = aa_book_with_size_22();
        let pages = reader.pages().len();
        assert!(pages > 1);
        let _ = update(
            &mut reader,
            Message::Reading(Target::Defaults, reading_ui::Action::Size(26)),
        );
        settle_theme(&mut reader);
        assert_eq!(reader.reading.defaults.size, 26);
        assert_eq!(reader.font_size, 22.0, "the book keeps its own size");
        assert_eq!(reader.pages().len(), pages, "and its pages");
    }

    #[test]
    fn escape_from_an_aa_picker_returns_focus_to_its_button() {
        let mut reader = aa_book_with_size_22();
        let font = Control::Reading(
            Target::Book,
            reading_ui::Focus::Picker(reading_ui::Picker::Font),
        );
        let _ = reader.activate(font);
        let choice = Control::Reading(
            Target::Book,
            reading_ui::Focus::Font(reader_document::reading::Font::Spectral),
        );
        assert!(reader.controls().any(|control| control == choice));
        reader.focused = Some(choice);
        let _ = update(&mut reader, escape());
        assert_eq!(reader.reading.picker, None);
        assert_eq!(reader.focused, Some(font));
        assert_eq!(
            reader.menu,
            Some(reading_chrome::Menu::Aa),
            "the panel stays"
        );
    }

    #[test]
    fn a_menu_closes_when_another_document_replaces_this_one() {
        let mut reader = aa_book_with_size_22();
        let _ = update(&mut reader, Message::RevealBar);
        assert_eq!(reader.menu, Some(reading_chrome::Menu::Aa));
        reader.book = Some(super::super::tests::book("dropped"));
        let _ = update(&mut reader, Message::RevealBar);
        assert_eq!(reader.menu, None);
    }

    fn open(reader: &mut Reader, title: &str) {
        let request = reader.request;
        let _ = update(
            reader,
            Message::Loaded {
                request,
                result: Ok(LoadReply {
                    document: LoadedDocument::Reflow(super::super::tests::book(title)),
                    catalog: None,
                }),
            },
        );
    }

    /// The book's own settings arriving from disk.
    fn stored(reader: &mut Reader, options: Option<reader_document::reading::Options>) {
        let message = Message::ReadingLoaded {
            profile: reader.profile.epoch,
            book: reader.book.as_ref().unwrap().fingerprint.clone(),
            result: Ok(options),
        };
        let _ = update(reader, message);
    }

    fn close(reader: &mut Reader) {
        let _ = update(reader, Message::Close(CloseAction::Document));
        let _ = update(
            reader,
            Message::Saved {
                action: CloseAction::Document,
                result: Ok(()),
            },
        );
        assert!(reader.book.is_none());
    }

    fn aa(reader: &mut Reader, control: Control) {
        if reader.menu != Some(reading_chrome::Menu::Aa) {
            let _ = update(reader, Message::ToggleMenu(reading_chrome::Menu::Aa));
        }
        assert!(reader.controls().any(|c| c == control), "{control:?}");
        let _ = reader.activate(control);
        settle_theme(reader);
    }

    #[test]
    fn aa_themes_the_open_book_and_settings_theme_the_rest() {
        let mut reader = Reader::default();
        open(&mut reader, "A");
        settle_pagination(&mut reader);
        stored(&mut reader, None);
        aa(&mut reader, Control::Theme(Target::Book, 1));
        assert_eq!(
            (reader.theme.id, reader.default_theme.id),
            ("soft", "default")
        );
        assert!(
            !reader.preferences_dirty && !reader.preferences_saving,
            "a book's theme is not a preference"
        );
        assert!(reader.section_layout().is_some(), "laid out in Soft");

        close(&mut reader);
        assert_eq!(reader.theme.id, "default", "the library uses the default");
        open(&mut reader, "B");
        settle_pagination(&mut reader);
        stored(&mut reader, None);
        assert_eq!(reader.theme.id, "default");

        reader.settings = Some(settings::Section::Appearance);
        let _ = reader.activate(Control::Theme(Target::Defaults, 2));
        settle_theme(&mut reader);
        reader.settings = None;
        assert_eq!(
            (reader.theme.id, reader.default_theme.id),
            ("clear", "clear")
        );
        assert!(reader.preferences_dirty || reader.preferences_saving);

        close(&mut reader);
        assert_eq!(reader.theme.id, "clear");
        open(&mut reader, "A");
        assert!(!reader.reading.loading, "A's theme is still queued to save");
        assert_eq!(reader.theme.id, "soft", "A keeps its own theme");
        settle_pagination(&mut reader);
        assert!(reader.section_layout().is_some());

        aa(
            &mut reader,
            Control::Reading(Target::Book, reading_ui::Focus::Reset),
        );
        assert_eq!(reader.theme.id, "clear", "back to the default theme");
        assert_eq!(reader.reading.book_theme(), None);
    }

    #[test]
    fn a_late_settings_reply_cannot_undo_a_theme_choice() {
        let mut reader = Reader::default();
        open(&mut reader, "A");
        close(&mut reader);
        open(&mut reader, "B");
        close(&mut reader);
        open(&mut reader, "A");
        settle_pagination(&mut reader);
        stored(&mut reader, None);
        aa(&mut reader, Control::Theme(Target::Book, 1));
        assert_eq!(reader.theme.id, "soft");
        stored(&mut reader, None);
        settle_theme(&mut reader);
        assert_eq!(
            reader.theme.id, "soft",
            "the reply from the first open is stale"
        );
        assert_eq!(
            reader.reading.book_theme(),
            Some(reader_document::reading::ThemeId::Soft)
        );
    }

    #[test]
    fn the_default_themes_card_makes_a_book_follow_the_default_again() {
        let mut reader = aa_book_with_size_22();
        aa(&mut reader, Control::Theme(Target::Book, 3));
        assert_eq!(reader.theme.id, "compact");
        aa(&mut reader, Control::Theme(Target::Book, 0));
        assert_eq!(reader.reading.book_theme(), None);
        assert_eq!(reader.reading.options().size, 22, "the typography stays");
        let _ = update(&mut reader, Message::SetTheme(1));
        settle_theme(&mut reader);
        assert_eq!(reader.theme.id, "soft");
    }

    #[test]
    fn books_open_laid_out_once_in_their_own_theme() {
        let mut reader = Reader::default();
        open(&mut reader, "A");
        settle_pagination(&mut reader);
        stored(&mut reader, None);
        aa(&mut reader, Control::Theme(Target::Book, 1));
        let options = reader.reading.options();
        let _ = reader.reading_saved(Ok(("test-A".into(), Some(options))));
        assert!(reader.reading.settled());

        // Each open counts the cleared view, the layout that starts the page
        // atlas, and the layout once the atlas is ready.
        let start = reader.generation;
        open(&mut reader, "B");
        assert_eq!(reader.theme.id, "default", "B does not start in A's theme");
        settle_pagination(&mut reader);
        stored(&mut reader, None);
        assert_eq!(reader.generation - start, 3, "B is never laid out in Soft");

        let start = reader.generation;
        open(&mut reader, "A");
        assert_eq!(reader.theme.id, "default", "A's settings are still loading");
        settle_pagination(&mut reader);
        assert_eq!(reader.generation - start, 3);
        let start = reader.generation;
        stored(&mut reader, Some(options));
        assert_eq!(reader.theme.id, "soft");
        settle_theme(&mut reader);
        assert_eq!(
            reader.generation - start,
            2,
            "one layout in Soft, then its adapted pages"
        );
        assert!(reader.section_layout().is_some());
    }
}
