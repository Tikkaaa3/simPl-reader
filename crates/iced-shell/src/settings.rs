//! What Settings is made of: its sections, the pane each one shows and the
//! controls that take keyboard focus there.
use super::*;
use iced::widget::{Column, Space, column};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Section {
    Reading,
    Appearance,
    Listen,
    Translation,
    Library,
    Shortcuts,
}

impl Section {
    /// The sections in the order of the list.
    pub(super) const ALL: [Self; 6] = [
        Self::Reading,
        Self::Appearance,
        Self::Listen,
        Self::Translation,
        Self::Library,
        Self::Shortcuts,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Reading => "Reading",
            Self::Appearance => "Appearance",
            Self::Listen => "Listen",
            Self::Translation => "Translation",
            Self::Library => "Library & data",
            Self::Shortcuts => "Shortcuts",
        }
    }

    /// The next section down the list, or up it. The list does not wrap.
    pub(super) fn step(self, down: bool) -> Option<Self> {
        let index = Self::ALL.iter().position(|&section| section == self)?;
        if down {
            Self::ALL.get(index + 1).copied()
        } else {
            index.checked_sub(1).map(|index| Self::ALL[index])
        }
    }
}

const MAX_WIDTH: f32 = 840.0;
const MAX_HEIGHT: f32 = 640.0;
const LIST_WIDTH: f32 = 160.0;

/// The open section's entry in the list, then the controls of its pane in
/// visual order. The list is one Tab stop, so the other entries are left out.
pub(super) fn controls(reader: &Reader, section: Section) -> Vec<Control> {
    let mut controls = vec![Control::SettingsSection(section)];
    match section {
        Section::Reading => {
            controls.extend(reading_ui::controls(reader, reading_ui::Target::Defaults))
        }
        Section::Appearance => {
            controls.extend(
                (0..themes::THEMES.len())
                    .map(|index| Control::Theme(reading_ui::Target::Defaults, index)),
            );
            controls.extend([
                Control::Appearance(Appearance::Light),
                Control::Appearance(Appearance::Dark),
                Control::Fullscreen,
            ]);
        }
        Section::Listen => {
            let rate = reader.read_aloud.rate;
            controls.extend((rate > crate::speech::MIN_RATE).then_some(Control::ReadAloudSlower));
            controls.extend((rate < crate::speech::MAX_RATE).then_some(Control::ReadAloudFaster));
        }
        Section::Translation => controls.extend(reader.translation_controls()),
        Section::Library => {
            controls.extend(library_ui::settings_controls(reader));
            controls.extend(profile_ui::controls(reader));
        }
        Section::Shortcuts => {}
    }
    controls
}

/// The Settings modal. It keeps one size whichever section is open, so the
/// heading and the section list stay put and only the pane scrolls.
pub(super) fn modal(reader: &Reader, section: Section) -> Element<'_, Message> {
    let entries = Section::ALL.map(|entry| {
        toned_button(
            reader,
            Control::SettingsSection(entry),
            text(entry.label()).size(13).width(Length::Fill),
            Some(Message::SettingsSection(entry)),
            ui::ButtonTone::Subtle,
            entry == section,
        )
    });
    let list = Column::with_children(entries).spacing(2).width(LIST_WIDTH);
    let body = row![
        list,
        modal_scroll(pane(reader, section)).height(Length::Fill)
    ]
    .spacing(20);
    // A whole-pixel heading keeps the pane's top edge on a whole pixel, where
    // the damage of its clipped rows lines up with the pane's own.
    let heading = container(modal_heading(reader, "Settings", true)).height(38);
    container(column![heading, body].spacing(16))
        .padding(24)
        .width((reader.window_size.width - 32.0).min(MAX_WIDTH))
        .height((reader.window_size.height - 64.0).clamp(160.0, MAX_HEIGHT))
        .style(ui::panel)
        .into()
}

/// One half of the Light and Dark choice. The current one keeps the selected
/// look and stays pressable, so it never reads as disabled.
fn appearance_choice<'a>(
    reader: &Reader,
    appearance: Appearance,
    label: &'static str,
) -> Element<'a, Message> {
    toned_button(
        reader,
        Control::Appearance(appearance),
        text(label).size(12),
        Some(Message::SetAppearance(appearance)),
        ui::ButtonTone::Surface,
        reader.appearance == appearance,
    )
}

fn note(value: &'static str) -> iced::widget::Text<'static> {
    text(value)
        .size(12)
        .width(Length::Fill)
        .style(ui::muted_text)
}

fn rule<'a>() -> Element<'a, Message> {
    container(Space::new().height(1))
        .width(Length::Fill)
        .style(ui::rule)
        .into()
}

fn pane(reader: &Reader, section: Section) -> Element<'_, Message> {
    match section {
        Section::Reading => {
            let mut content = column![].spacing(12);
            if reader.book.is_some() || reader.pdf.is_some() {
                content = content.push(
                    text("These defaults are for every book without its own settings. The open book's own settings are in Aa on the reading bar.")
                        .size(12)
                        .width(Length::Fill)
                        .style(ui::secondary_text),
                );
            }
            content.push(reading_ui::settings(reader)).into()
        }
        Section::Appearance => column![
            text("Theme").font(ui::MEDIUM).size(13),
            theme_picker(reader, reading_ui::Target::Defaults),
            note("A theme sets the reading font, spacing and colors. Page numbers stay the same in every theme. Every book uses this theme until you choose another for it from Aa in its reading bar."),
            rule(),
            row![
                text("Light or dark").font(ui::MEDIUM).size(13).width(Length::Fill),
                appearance_choice(reader, Appearance::Light, "Light"),
                appearance_choice(reader, Appearance::Dark, "Dark"),
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center),
            row![
                text("Fullscreen").font(ui::MEDIUM).size(13).width(Length::Fill),
                control_button(
                    reader,
                    Control::Fullscreen,
                    text(if reader.fullscreen { "Exit fullscreen (F11)" } else { "Fullscreen (F11)" }).size(12),
                    Some(Message::ToggleFullscreen),
                ),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        ]
        .spacing(12)
        .into(),
        Section::Listen => read_aloud::settings(reader, reader.voice_choices.clone()),
        Section::Translation => word_translation::settings(reader),
        Section::Library => column![
            library_ui::settings(reader),
            rule(),
            profile_ui::settings(reader)
        ]
        .spacing(16)
        .into(),
        Section::Shortcuts => column![
            note("Press F1 anywhere to show this list at the top of the window."),
            shortcuts::table(),
        ]
        .spacing(16)
        .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(reader: &mut Reader, named: key::Named, code: key::Code) {
        let _ = update(
            reader,
            Message::Event(
                iced::Event::Keyboard(keyboard::Event::KeyPressed {
                    key: Key::Named(named),
                    modified_key: Key::Named(named),
                    physical_key: key::Physical::Code(code),
                    location: keyboard::Location::Standard,
                    modifiers: keyboard::Modifiers::default(),
                    text: None,
                    repeat: false,
                }),
                window::Id::unique(),
            ),
        );
    }

    fn toggle_settings(reader: &mut Reader) {
        let _ = update(reader, Message::Chrome(chrome::Action::Settings));
    }

    #[test]
    fn settings_focus_stays_in_the_open_section() {
        let mut reader = Reader::default();
        toggle_settings(&mut reader);
        let _ = update(&mut reader, Message::SettingsSection(Section::Listen));
        assert_eq!(
            reader.controls().collect::<Vec<_>>(),
            vec![
                Control::DismissOverlay,
                Control::SettingsSection(Section::Listen),
                Control::ReadAloudSlower,
                Control::ReadAloudFaster,
            ]
        );
    }

    #[test]
    fn the_section_list_is_one_tab_stop_that_the_arrows_move_along() {
        let mut reader = Reader::default();
        toggle_settings(&mut reader);
        assert_eq!(reader.focused, Some(Control::DismissOverlay));
        press(&mut reader, key::Named::Tab, key::Code::Tab);
        assert_eq!(
            reader.focused,
            Some(Control::SettingsSection(Section::Reading))
        );
        press(&mut reader, key::Named::ArrowDown, key::Code::ArrowDown);
        press(&mut reader, key::Named::ArrowDown, key::Code::ArrowDown);
        assert_eq!(reader.settings, Some(Section::Listen));
        assert_eq!(
            reader.focused,
            Some(Control::SettingsSection(Section::Listen))
        );

        for _ in 0..3 {
            press(&mut reader, key::Named::ArrowUp, key::Code::ArrowUp);
        }
        assert_eq!(
            reader.settings,
            Some(Section::Reading),
            "Up stops at the top"
        );
        for _ in 0..6 {
            press(&mut reader, key::Named::ArrowDown, key::Code::ArrowDown);
        }
        assert_eq!(
            reader.settings,
            Some(Section::Shortcuts),
            "Down stops at the bottom"
        );

        press(&mut reader, key::Named::Escape, key::Code::Escape);
        assert_eq!(reader.settings, None);
    }

    #[test]
    fn appearance_switches_only_from_the_other_choice() {
        let mut reader = Reader::default();
        toggle_settings(&mut reader);
        let _ = update(&mut reader, Message::SettingsSection(Section::Appearance));
        let controls: Vec<_> = reader.controls().collect();
        assert_eq!(
            controls[controls.len() - 3..],
            [
                Control::Appearance(Appearance::Light),
                Control::Appearance(Appearance::Dark),
                Control::Fullscreen,
            ]
        );
        let _ = reader.activate(Control::Appearance(Appearance::Light));
        assert_eq!(reader.appearance, Appearance::Light);
        let _ = reader.activate(Control::Appearance(Appearance::Dark));
        assert_eq!(reader.appearance, Appearance::Dark);
        let _ = reader.activate(Control::Appearance(Appearance::Dark));
        assert_eq!(reader.appearance, Appearance::Dark, "the current one stays");
    }

    #[test]
    fn settings_opens_on_reading_every_time() {
        let mut reader = Reader::default();
        toggle_settings(&mut reader);
        assert_eq!(reader.settings, Some(Section::Reading));
        let _ = update(&mut reader, Message::SettingsSection(Section::Translation));
        toggle_settings(&mut reader);
        assert_eq!(reader.settings, None);
        toggle_settings(&mut reader);
        assert_eq!(reader.settings, Some(Section::Reading));
    }
}
