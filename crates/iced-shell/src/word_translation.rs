//! Selection-driven local dictionary cards and persistent reading settings.
#[cfg(test)]
#[path = "word_translation_tests.rs"]
mod tests;
use super::*;
use iced::advanced::mouse;
use iced::widget::column;
use iced::widget::{mouse_area, opaque};
use reader_document::dictionary::{self, Language, Settings, Translation};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Picker {
    Source,
    Target,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Focus {
    Automatic,
    Source,
    Target,
    Language(Picker, Language),
    Close,
    Copy,
    Highlight,
    Listen,
}

#[derive(Clone, Debug)]
pub(super) enum Action {
    ToggleAutomatic,
    Picker(Picker),
    Language(Picker, Language),
    Close,
    Copy,
    Highlight,
    Listen,
}

#[derive(Debug, Default)]
pub(super) struct WordTranslation {
    pub settings: Settings,
    pub picker: Option<Picker>,
    pub popup: Option<Popup>,
    generation: u64,
    task: Option<iced::task::Handle>,
    pub click: Option<mouse::Click>,
    pub word: Option<(Endpoint, Endpoint)>,
}

#[derive(Debug)]
pub(super) struct Popup {
    at: iced::Point,
    pub query: String,
    result: Option<Result<Option<Translation>, String>>,
    selection: bool,
    automatic: bool,
}

impl WordTranslation {
    pub fn dismiss(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.task = None;
        self.popup = None;
    }

    pub fn reset(&mut self) {
        self.dismiss();
        self.click = None;
        self.word = None;
        self.picker = None;
    }

    pub fn bounds(&self, window: Size) -> Option<iced::Rectangle> {
        let popup = self.popup.as_ref()?;
        let size = Size::new(
            (window.width - 24.0).clamp(180.0, 360.0),
            (window.height - 80.0).clamp(160.0, 300.0),
        );
        let x = popup.at.x.min(window.width - size.width - 12.0).max(12.0);
        let below = popup.at.y + 12.0;
        let y = if below + size.height + 12.0 <= window.height {
            below
        } else {
            (popup.at.y - size.height - 12.0).max(40.0)
        };
        Some(iced::Rectangle::new(iced::Point::new(x, y), size))
    }

    pub fn contains(&self, point: iced::Point, window: Size) -> bool {
        self.bounds(window)
            .is_some_and(|bounds| bounds.contains(point))
    }
}

pub(crate) fn word_range(text: &str, byte: usize) -> Option<std::ops::Range<usize>> {
    let mut ending = None;
    for (start, word) in text.unicode_word_indices() {
        let end = start + word.len();
        if start <= byte && byte < end {
            return Some(start..end);
        }
        // Native paragraph hit tests may return the caret just after a final glyph.
        if byte == end {
            ending = Some(start..end);
        }
    }
    ending
}

impl Reader {
    /// A second click selects the whole logical word, in either native reader.
    pub(super) fn word_click(&mut self) -> bool {
        let click = mouse::Click::new(
            self.pointer,
            mouse::Button::Left,
            self.word_translation.click,
        );
        let double = matches!(
            click.kind(),
            mouse::click::Kind::Double | mouse::click::Kind::Triple
        );
        self.word_translation.click = Some(click);
        self.word_translation.word = None;
        self.word_translation.dismiss();
        self.notes.popup = None;
        double
    }

    pub(super) fn select_word(&mut self, endpoint: &Endpoint) {
        let Some(text) = self
            .book
            .as_ref()
            .and_then(|book| book.items.iter().find(|item| item.id() == endpoint.item_id))
            .and_then(Item::text)
        else {
            return;
        };
        let Some(range) = word_range(text, endpoint.byte_offset) else {
            return;
        };
        let from = Endpoint {
            item_id: endpoint.item_id.clone(),
            byte_offset: range.start,
        };
        let to = Endpoint {
            item_id: endpoint.item_id.clone(),
            byte_offset: range.end,
        };
        self.selection.begin(from.clone());
        self.selection.extend(to.clone());
        self.word_translation.word = Some((from, to));
    }

    fn short_selection(&self) -> Option<String> {
        let book = self.book.as_ref()?;
        let bounds = self.selection.bounds(&book.items)?;
        let mut text = String::new();
        for index in bounds.start_item..=bounds.end_item {
            let logical = book.items.get(index)?.text()?;
            if let Some(range) = bounds.range_for_item(index, logical) {
                if text.len() + range.len() + 1 > dictionary::MAX_QUERY_BYTES {
                    return None;
                }
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(&logical[range]);
            }
        }
        dictionary::query(&text, self.word_translation.settings.source)?;
        Some(text.trim().to_owned())
    }

    pub(super) fn auto_translate_selection(&mut self) -> Option<Task<Message>> {
        if !self.word_translation.settings.automatic
            || !self.interactive()
            || self.show_search
            || self.show_settings
            || self.notes.editor.is_some()
            || self.confirm_remove.is_some()
        {
            return None;
        }
        if let Some(pdf) = &self.pdf {
            let selection = pdf.selection()?;
            if !pdf.document().can_copy
                || selection.anchor.page != selection.focus.page
                || selection.anchor.index.abs_diff(selection.focus.index) > 255
            {
                return None;
            }
        } else {
            self.short_selection()?;
        }
        let task = self.translate_selection();
        if let Some(popup) = &mut self.word_translation.popup {
            popup.automatic = true;
        }
        Some(task)
    }

    pub(super) fn translate_selection(&mut self) -> Task<Message> {
        if self.pdf.is_none() {
            return match self.short_selection() {
                Some(text) => {
                    let task = self.translate_text(text);
                    self.word_translation.popup.as_mut().unwrap().selection = true;
                    task
                }
                None => self.translation_error("Select a word or short phrase (up to 4 words)."),
            };
        }
        let pdf = self.pdf.as_ref().unwrap();
        if !pdf.document().can_copy {
            return self
                .translation_error("This PDF does not permit copying text for translation.");
        }
        let Some(selection) = pdf.selection() else {
            return self.translation_error("Select a word to translate.");
        };
        if selection.anchor.page != selection.focus.page
            || selection.anchor.index.abs_diff(selection.focus.index) > 255
        {
            return self.translation_error("Select a word or short phrase on one PDF page.");
        }
        let document = pdf.document().id;
        let session = pdf.document().session.clone();
        self.begin_translation("Selected word".into());
        self.word_translation.popup.as_mut().unwrap().selection = true;
        let generation = self.word_translation.generation;
        let (task, handle) = Task::perform(
            async move { session.copy(selection).await },
            move |result| Message::DictionaryText {
                document,
                generation,
                result,
            },
        )
        .abortable();
        self.word_translation.task = Some(handle.abort_on_drop());
        task
    }

    fn begin_translation(&mut self, query: String) {
        self.word_translation.dismiss();
        self.notes.popup = None;
        self.notes.pressed = None;
        self.focused = None;
        self.word_translation.popup = Some(Popup {
            at: self.pointer,
            query,
            result: None,
            selection: false,
            automatic: false,
        });
    }

    fn translation_error(&mut self, error: &str) -> Task<Message> {
        self.begin_translation("Word translation".into());
        self.word_translation.popup.as_mut().unwrap().result = Some(Err(error.into()));
        Task::none()
    }

    pub(super) fn translate_text(&mut self, text: String) -> Task<Message> {
        let settings = self.word_translation.settings;
        if dictionary::query(&text, settings.source).is_none() {
            return self.translation_error("Select a word or short phrase (up to 4 words).");
        }
        self.begin_translation(text.clone());
        let generation = self.word_translation.generation;
        let (task, handle) = Task::perform(
            async move { dictionary::lookup(&text, settings.source, settings.target) },
            move |result| Message::DictionaryReady { generation, result },
        )
        .abortable();
        self.word_translation.task = Some(handle.abort_on_drop());
        task
    }

    pub(super) fn dictionary_text(
        &mut self,
        document: u64,
        generation: u64,
        result: Result<String, String>,
    ) -> Task<Message> {
        if self.word_translation.generation != generation
            || self.word_translation.popup.is_none()
            || self.pdf.as_ref().map(|pdf| pdf.document().id) != Some(document)
        {
            return Task::none();
        }
        self.word_translation.task = None;
        let automatic = self.word_translation.popup.as_ref().unwrap().automatic;
        match result {
            Ok(text)
                if automatic
                    && dictionary::query(&text, self.word_translation.settings.source)
                        .is_none() =>
            {
                self.word_translation.dismiss();
                self.open_popup(notes::Target::Selection, false);
                Task::none()
            }
            Ok(text) => {
                let task = self.translate_text(text);
                if let Some(popup) = &mut self.word_translation.popup {
                    popup.selection = !matches!(popup.result, Some(Err(_)));
                    popup.automatic = automatic;
                }
                task
            }
            Err(error) => self.translation_error(&error),
        }
    }

    pub(super) fn dictionary_ready(
        &mut self,
        generation: u64,
        result: Result<Option<Translation>, String>,
    ) {
        if self.word_translation.generation != generation {
            return;
        }
        if let Some(popup) = &mut self.word_translation.popup {
            popup.result = Some(result);
            self.word_translation.task = None;
        }
    }

    pub(super) fn dictionary_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Close => {
                self.word_translation.dismiss();
                self.focused = None;
                Task::none()
            }
            Action::Copy => self
                .word_translation
                .popup
                .as_ref()
                .and_then(|popup| popup.result.as_ref())
                .and_then(|result| result.as_ref().ok())
                .and_then(Option::as_ref)
                .map(|translation| translation.meanings.join("; "))
                .map_or_else(Task::none, iced::clipboard::write),
            Action::Highlight => {
                if !self
                    .word_translation
                    .popup
                    .as_ref()
                    .is_some_and(|popup| popup.selection)
                {
                    return Task::none();
                }
                self.word_translation.dismiss();
                self.open_popup(notes::Target::Selection, false);
                self.notes_action(notes::Action::Popup(notes::PopupItem::Color(
                    self.notes.color,
                )))
            }
            Action::Listen => {
                let Some(query) = self
                    .word_translation
                    .popup
                    .as_ref()
                    .filter(|popup| matches!(popup.result, Some(Ok(_))))
                    .map(|popup| popup.query.clone())
                else {
                    return Task::none();
                };
                self.word_translation.dismiss();
                self.read_passage_aloud(query)
            }
            Action::Picker(picker) => {
                self.word_translation.picker =
                    (self.word_translation.picker != Some(picker)).then_some(picker);
                Task::none()
            }
            Action::ToggleAutomatic | Action::Language(_, _) => {
                match action {
                    Action::ToggleAutomatic => self.word_translation.settings.automatic ^= true,
                    Action::Language(Picker::Source, language) => {
                        self.word_translation.settings.source = language;
                        self.focused = Some(Control::Dictionary(Focus::Source));
                    }
                    Action::Language(Picker::Target, language) => {
                        self.word_translation.settings.target = language;
                        self.focused = Some(Control::Dictionary(Focus::Target));
                    }
                    _ => unreachable!(),
                }
                self.word_translation.settings = self.word_translation.settings.validated();
                self.word_translation.picker = None;
                self.word_translation.dismiss();
                self.preferences_dirty = true;
                self.persist_preferences()
            }
        }
    }

    pub(super) fn dictionary_controls(&self) -> Vec<Control> {
        let mut controls = Vec::new();
        if self.show_settings {
            controls.extend([Focus::Automatic, Focus::Source, Focus::Target]);
            if let Some(picker) = self.word_translation.picker {
                let languages = if picker == Picker::Source {
                    Language::ALL.to_vec()
                } else {
                    self.word_translation.settings.source.targets()
                };
                controls.extend(
                    languages
                        .into_iter()
                        .map(|language| Focus::Language(picker, language)),
                );
            }
        } else if self.word_translation.popup.is_some() {
            controls.push(Focus::Close);
            if self
                .word_translation
                .popup
                .as_ref()
                .is_some_and(|popup| matches!(popup.result, Some(Ok(Some(_)))))
            {
                controls.push(Focus::Copy);
            }
            if self
                .word_translation
                .popup
                .as_ref()
                .is_some_and(|popup| popup.selection)
            {
                controls.push(Focus::Highlight);
            }
            if self
                .word_translation
                .popup
                .as_ref()
                .is_some_and(|popup| matches!(popup.result, Some(Ok(_))))
            {
                controls.push(Focus::Listen);
            }
        }
        controls.into_iter().map(Control::Dictionary).collect()
    }
}

pub(super) fn focus_action(focus: Focus) -> Action {
    match focus {
        Focus::Automatic => Action::ToggleAutomatic,
        Focus::Source => Action::Picker(Picker::Source),
        Focus::Target => Action::Picker(Picker::Target),
        Focus::Language(picker, language) => Action::Language(picker, language),
        Focus::Close => Action::Close,
        Focus::Copy => Action::Copy,
        Focus::Highlight => Action::Highlight,
        Focus::Listen => Action::Listen,
    }
}

fn button<'a>(reader: &Reader, focus: Focus, label: String) -> Element<'a, Message> {
    control_button(
        reader,
        Control::Dictionary(focus),
        text(label).size(12).shaping(text::Shaping::Advanced).width(
            if matches!(focus, Focus::Language(_, _)) {
                Length::Fill
            } else {
                Length::Shrink
            },
        ),
        Some(Message::Dictionary(focus_action(focus))),
    )
}

pub(super) fn settings(reader: &Reader) -> Element<'_, Message> {
    let settings = reader.word_translation.settings;
    let mut content = column![
        row![text("Translate selected words").font(ui::MEDIUM).size(13).width(Length::Fill),
            button(reader, Focus::Automatic, if settings.automatic { "Automatic: On" } else { "Automatic: Off" }.into())]
            .spacing(8).align_y(iced::Alignment::Center),
        text("Double-click a word or finish selecting a short phrase to show its meaning. With automatic off, select text and right-click → Translate.")
            .size(12).width(Length::Fill).style(ui::muted_text),
        row![
            column![text("Input language").size(12).style(ui::muted_text),
                button(reader, Focus::Source, format!("{} ▾", settings.source))].spacing(6).width(Length::Fill),
            column![text("Output language").size(12).style(ui::muted_text),
                button(reader, Focus::Target, format!("{} ▾", settings.target))].spacing(6).width(Length::Fill),
        ].spacing(16),
    ].spacing(10);
    if let Some(picker) = reader.word_translation.picker {
        let languages = if picker == Picker::Source {
            Language::ALL.to_vec()
        } else {
            settings.source.targets()
        };
        let selected = if picker == Picker::Source {
            settings.source
        } else {
            settings.target
        };
        let mut choices = column![].spacing(6);
        for pair in languages.chunks(2) {
            let mut options = row![].spacing(6).width(Length::Fill);
            for &language in pair {
                let label = if language == selected {
                    format!("{language} ✓")
                } else {
                    language.to_string()
                };
                options = options.push(button(reader, Focus::Language(picker, language), label));
            }
            choices = choices.push(options);
        }
        content = content.push(
            container(choices)
                .padding(8)
                .width(Length::Fill)
                .style(ui::panel),
        );
    }
    content.push(text("Built-in offline dictionaries. Output choices show the available pairs. Korean currently supports Korean → English.")
        .size(12).width(Length::Fill).style(ui::muted_text)).into()
}

pub(super) fn popup_layer(reader: &Reader) -> Option<Element<'_, Message>> {
    let popup = reader.word_translation.popup.as_ref()?;
    let bounds = reader.word_translation.bounds(reader.window_size)?;
    let settings = reader.word_translation.settings;
    let mut body = column![].spacing(7).width(Length::Fill);
    let provider;
    match &popup.result {
        None => {
            body = body.push(text("Looking up locally…").size(13).style(ui::muted_text));
            provider = "Offline dictionary";
        }
        Some(Err(error)) => {
            body = body.push(
                text(error)
                    .size(13)
                    .width(Length::Fill)
                    .style(ui::muted_text),
            );
            provider = "Offline dictionary";
        }
        Some(Ok(None)) => {
            body = body.push(text("No entry found for this selection.").size(13).width(Length::Fill))
                .push(text("Try a base form, a smaller selection, or check the input language in Settings.")
                    .size(12).width(Length::Fill).style(ui::muted_text));
            provider = match settings.source {
                Language::Korean => "Kaikki / Wiktionary",
                Language::Chinese => "WikDict / CC-CEDICT",
                _ => "WikDict / Wiktionary",
            };
        }
        Some(Ok(Some(translation))) => {
            if translation.base_form {
                body = body.push(
                    text(format!("Base form: {}", translation.headword))
                        .size(12)
                        .style(ui::muted_text),
                );
            }
            for meaning in &translation.meanings {
                body = body.push(
                    text(format!("• {meaning}"))
                        .size(14)
                        .width(Length::Fill)
                        .shaping(text::Shaping::Advanced),
                );
            }
            provider = translation.provider.as_str();
        }
    }
    let mut actions = row![].spacing(6);
    if matches!(popup.result, Some(Ok(Some(_)))) {
        actions = actions.push(button(reader, Focus::Copy, "Copy meaning".into()));
    }
    if popup.selection {
        actions = actions.push(button(reader, Focus::Highlight, "Highlight".into()));
    }
    if matches!(popup.result, Some(Ok(_))) {
        actions = actions.push(button(reader, Focus::Listen, "Listen".into()));
    }
    let title = popup.query.chars().take(64).collect::<String>();
    let panel = container(
        column![
            row![
                text(title)
                    .font(ui::MEDIUM)
                    .size(17)
                    .width(Length::Fill)
                    .shaping(text::Shaping::Advanced),
                button(reader, Focus::Close, "×".into())
            ]
            .spacing(8),
            text(format!("{} → {}", settings.source, settings.target))
                .size(11)
                .style(ui::muted_text),
            scrollable(container(body).padding(iced::Padding {
                right: 12.0,
                ..Default::default()
            }))
            .direction(ui::vertical_scrollbar())
            .style(ui::scroll_style)
            .height(Length::Fill),
            actions,
            text(format!("{provider} · CC BY-SA 4.0"))
                .size(10)
                .width(Length::Fill)
                .style(ui::muted_text),
        ]
        .spacing(10),
    )
    .padding(14)
    .width(bounds.width)
    .height(bounds.height)
    .style(ui::panel);
    Some(
        container(mouse_area(opaque(panel)).interaction(mouse::Interaction::default()))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(iced::Padding {
                left: bounds.x,
                top: bounds.y,
                ..iced::Padding::ZERO
            })
            .align_x(iced::alignment::Horizontal::Left)
            .align_y(iced::alignment::Vertical::Top)
            .into(),
    )
}
