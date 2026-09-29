//! Read aloud: the selection, or page after page, through the Windows voices.
//!
//! Text is queued to the engine a few paragraphs (or PDF pages) ahead. A
//! timer asks the engine which one it is speaking and turns the page to
//! follow it; moving to another page yourself restarts reading there.

use std::collections::VecDeque;

use super::*;
use crate::speech::{self, Speaker, Voice};
use iced::widget::{column, row};

/// Paragraphs or PDF pages queued ahead of the one being spoken.
const LOOKAHEAD: usize = 3;
/// How often the reader checks what the engine is speaking.
const TICK_MS: u64 = 200;

#[derive(Clone, Debug)]
pub(super) enum Action {
    /// Read the selection if there is one, otherwise from this page on; or stop reading.
    Toggle,
    /// Read only the selection (from the context menu).
    Selection,
    Pause,
    Resume,
    Slower,
    Faster,
    Voice(VoiceChoice),
}

/// An entry of the voice picker; an empty id chooses the voice automatically.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct VoiceChoice {
    pub id: String,
    label: String,
}

impl std::fmt::Display for VoiceChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

const AUTOMATIC: &str = "Automatic (matches the text)";

#[derive(Debug, Default)]
pub(super) struct ReadAloud {
    speaker: Option<Speaker>,
    /// Why Windows speech cannot be used, once that is known.
    pub(super) unavailable: Option<String>,
    pub(super) session: Option<Session>,
    /// The chosen voice id; empty means automatic.
    pub(super) voice: String,
    pub(super) rate: i8,
    /// Identifies extraction requests across restarts in the same PDF.
    generation: u64,
}

#[derive(Debug)]
pub(super) struct Session {
    /// The open document this session reads; another document ends it.
    document: String,
    pub(super) selection: bool,
    /// A PDF selection whose text is still being extracted.
    pending_text: bool,
    pub(super) paused: bool,
    queued: VecDeque<Queued>,
    next: Next,
    /// The page shown for the text being read; if the reader shows another, the
    /// reader moved there.
    expected: Option<Place>,
    pub(super) notice: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Place {
    Book { chapter: usize, top: f32 },
    Pdf(usize),
}

#[derive(Debug)]
struct Queued {
    stream: u32,
    unit: Unit,
    /// UTF-16 offset of the spoken text within its paragraph, and the paragraph length.
    start: u32,
    total: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Unit {
    Row(usize),
    PdfPage(usize),
    Text,
}

#[derive(Clone, Debug, PartialEq)]
enum Next {
    /// Paragraphs of this chapter from `row` on; the first may start at `skip` bytes.
    Rows {
        chapter: usize,
        row: usize,
        skip: usize,
    },
    /// This chapter is finished; open the next once the queue is spoken.
    Chapter(usize),
    /// Waiting for the next chapter to be laid out.
    Opening(usize),
    Pdf {
        page: usize,
        text: Option<String>,
        fetching: bool,
    },
    Nothing,
}

impl ReadAloud {
    pub(super) fn active(&self) -> bool {
        self.session.is_some()
    }

    pub(super) fn ticking(&self) -> bool {
        self.session.as_ref().is_some_and(|session| !session.paused)
    }

    fn speaker(&mut self) -> Option<&mut Speaker> {
        if self.speaker.is_none() && self.unavailable.is_none() {
            match Speaker::new() {
                Ok(speaker) => self.speaker = Some(speaker),
                Err(error) => self.unavailable = Some(error),
            }
        }
        self.speaker.as_mut()
    }

    /// The picker entries: automatic first, then every installed voice.
    pub(super) fn choices(&mut self) -> Vec<VoiceChoice> {
        let mut choices = vec![VoiceChoice {
            id: String::new(),
            label: AUTOMATIC.into(),
        }];
        if let Some(speaker) = self.speaker() {
            choices.extend(speaker.voices().map(|voice| VoiceChoice {
                id: voice.id.clone(),
                label: voice.to_string(),
            }));
        }
        choices
    }

    pub(super) fn chosen(&self, choices: &[VoiceChoice]) -> Option<VoiceChoice> {
        choices
            .iter()
            .find(|choice| choice.id == self.voice)
            .or_else(|| choices.first())
            .cloned()
    }

    fn stop(&mut self) {
        if let Some(speaker) = self.speaker.as_mut() {
            speaker.stop();
        }
        self.session = None;
    }

    /// Picks the voice for `sample`: the chosen one, or in automatic mode an
    /// installed voice for the language the text seems to be in.
    fn prepare(&mut self, sample: &str) -> Result<(), String> {
        let chosen = self.voice.clone();
        let rate = self.rate;
        let speaker = self
            .speaker()
            .ok_or_else(|| "Windows speech is not available.".to_owned())?;
        let voice = if chosen.is_empty() {
            speech::guess_language(sample).and_then(|language| {
                speaker
                    .voices()
                    .find(|voice: &&Voice| voice.speaks(language))
                    .map(|voice| voice.id.clone())
            })
        } else {
            Some(chosen)
        };
        if speaker.use_voice(voice.as_deref()).is_err() {
            // An uninstalled voice falls back to the Windows default.
            speaker.use_voice(None)?;
        }
        speaker.set_rate(rate);
        Ok(())
    }
}

impl Reader {
    /// Identifies the open document so a session ends when it changes.
    fn reading_document(&self) -> Option<String> {
        if let Some(pdf) = &self.pdf {
            return Some(format!("pdf:{}", pdf.document().id));
        }
        self.book
            .as_ref()
            .map(|book| format!("book:{}", book.fingerprint))
    }

    fn book_chapter(&self) -> usize {
        self.book
            .as_ref()
            .and_then(|book| book.epub.as_ref())
            .map_or(0, |chapter| chapter.index)
    }

    fn place(&self) -> Option<Place> {
        if let Some(pdf) = &self.pdf {
            return Some(Place::Pdf(pdf.page_index()));
        }
        self.active_page().map(|page| Place::Book {
            chapter: self.book_chapter(),
            top: page.top,
        })
    }

    pub(super) fn read_aloud(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Toggle if self.read_aloud.active() => {
                self.read_aloud.stop();
                Task::none()
            }
            Action::Toggle => self.start_reading(true),
            Action::Selection => self.start_reading(true),
            Action::Pause => {
                if let Some(speaker) = self.read_aloud.speaker.as_mut() {
                    speaker.pause();
                }
                if let Some(session) = &mut self.read_aloud.session {
                    session.paused = true;
                }
                Task::none()
            }
            Action::Resume => {
                if let Some(speaker) = self.read_aloud.speaker.as_mut() {
                    speaker.resume();
                }
                if let Some(session) = &mut self.read_aloud.session {
                    session.paused = false;
                }
                Task::none()
            }
            Action::Slower | Action::Faster => {
                let step = if matches!(action, Action::Faster) {
                    1
                } else {
                    -1
                };
                self.read_aloud.rate =
                    (self.read_aloud.rate + step).clamp(speech::MIN_RATE, speech::MAX_RATE);
                let rate = self.read_aloud.rate;
                if let Some(speaker) = self.read_aloud.speaker.as_mut() {
                    speaker.set_rate(rate);
                }
                self.preferences_dirty = true;
                self.persist_preferences()
            }
            Action::Voice(choice) => {
                self.read_aloud.voice = choice.id;
                self.preferences_dirty = true;
                // A new voice takes over at once: read again from this page.
                let restart = match &self.read_aloud.session {
                    Some(session) if !session.selection => {
                        self.read_aloud.stop();
                        self.start_reading(false)
                    }
                    _ => Task::none(),
                };
                Task::batch([self.persist_preferences(), restart])
            }
        }
    }

    fn start_reading(&mut self, prefer_selection: bool) -> Task<Message> {
        self.read_aloud.stop();
        let Some(document) = self.reading_document() else {
            return Task::none();
        };
        if let Some(pdf) = &self.pdf {
            let source = pdf.document().clone();
            let selection = pdf.selection().filter(|_| prefer_selection);
            let page = pdf.page_index();
            if !source.can_copy {
                self.error = Some(
                    "This PDF does not permit copying its text, so it cannot be read aloud.".into(),
                );
                return Task::none();
            }
            if let Some(selection) = selection {
                let id = source.id;
                let session = source.session.clone();
                self.begin(document, true, Next::Nothing, None);
                let generation = self.read_aloud.generation;
                if let Some(session) = &mut self.read_aloud.session {
                    session.pending_text = true;
                }
                return Task::perform(
                    async move { session.copy(selection).await },
                    move |result| Message::ReadAloudText {
                        document: id,
                        generation,
                        result,
                    },
                );
            }
            self.begin(
                document,
                false,
                Next::Pdf {
                    page,
                    text: None,
                    fetching: false,
                },
                Some(Place::Pdf(page)),
            );
            return self.read_aloud_tick();
        }
        let Some(book) = &self.book else {
            return Task::none();
        };
        let selected = self
            .selection
            .copy_text(&book.items)
            .filter(|text| prefer_selection && !text.trim().is_empty());
        if let Some(text) = selected {
            self.begin(document, true, Next::Nothing, None);
            self.speak_text(&text);
            return Task::none();
        }
        let Some(page) = self.active_page() else {
            return Task::none();
        };
        let skip = self.page_start_byte(&page);
        let place = self.place();
        let chapter = self.book_chapter();
        self.begin(
            document,
            false,
            Next::Rows {
                chapter,
                row: page.rows.start,
                skip,
            },
            place,
        );
        self.read_aloud_tick()
    }

    /// Reads one passage, such as a highlight's quote.
    pub(super) fn read_passage_aloud(&mut self, text: String) -> Task<Message> {
        self.read_aloud.stop();
        let Some(document) = self.reading_document() else {
            return Task::none();
        };
        if !text.trim().is_empty() {
            self.begin(document, true, Next::Nothing, None);
            self.speak_text(&text);
        }
        Task::none()
    }

    fn begin(&mut self, document: String, selection: bool, next: Next, expected: Option<Place>) {
        self.read_aloud.generation = self.read_aloud.generation.wrapping_add(1);
        self.read_aloud.session = Some(Session {
            document,
            selection,
            pending_text: false,
            paused: false,
            queued: VecDeque::new(),
            next,
            expected,
            notice: None,
        });
    }

    /// Where on its first paragraph a page starts: a paragraph that began on
    /// the previous page is read from the sentence the page starts in.
    fn page_start_byte(&self, page: &book_pages::Page) -> usize {
        let Some(text) = self
            .book
            .as_ref()
            .and_then(|book| book.items.get(page.rows.start))
            .and_then(Item::text)
        else {
            return 0;
        };
        let height = self.heights.height(page.rows.start).max(1.0);
        let fraction =
            ((page.content.start - self.heights.start(page.rows.start)) / height).clamp(0.0, 1.0);
        sentence_start(text, (text.len() as f32 * fraction) as usize)
    }

    fn speak_text(&mut self, text: &str) {
        if let Err(error) = self.read_aloud.prepare(text) {
            self.read_aloud.session = None;
            self.error = Some(error);
            return;
        }
        let Some(speaker) = self.read_aloud.speaker.as_mut() else {
            return;
        };
        // The user may have paused while PDF selection extraction was pending.
        if self
            .read_aloud
            .session
            .as_ref()
            .is_some_and(|session| session.paused)
        {
            speaker.pause();
        }
        match speaker.speak(text) {
            Ok(stream) => {
                if let Some(session) = &mut self.read_aloud.session {
                    session.queued.push_back(Queued {
                        stream,
                        unit: Unit::Text,
                        start: 0,
                        total: text.encode_utf16().count() as u32,
                    });
                }
            }
            Err(error) => {
                self.read_aloud.session = None;
                self.error = Some(error);
            }
        }
    }

    /// Ends a session whose document is no longer open.
    pub(super) fn sync_read_aloud(&mut self) {
        let Some(session) = &self.read_aloud.session else {
            return;
        };
        if self.reading_document().as_deref() != Some(session.document.as_str()) {
            self.read_aloud.stop();
        }
    }

    pub(super) fn read_aloud_text(
        &mut self,
        document: u64,
        generation: u64,
        result: Result<String, String>,
    ) {
        if generation != self.read_aloud.generation
            || self.pdf.as_ref().map(|pdf| pdf.document().id) != Some(document)
            || !self
                .read_aloud
                .session
                .as_ref()
                .is_some_and(|session| session.pending_text)
        {
            return;
        }
        if let Some(session) = &mut self.read_aloud.session {
            session.pending_text = false;
        }
        match result {
            Ok(text) if !text.trim().is_empty() => self.speak_text(&text),
            Ok(_) => self.read_aloud.stop(),
            Err(error) => {
                self.read_aloud.stop();
                self.error = Some(format!("Could not read the selection: {error}"));
            }
        }
    }

    pub(super) fn read_aloud_pdf_page(
        &mut self,
        document: u64,
        generation: u64,
        page: usize,
        result: Result<String, String>,
    ) -> Task<Message> {
        if generation != self.read_aloud.generation
            || self.pdf.as_ref().map(|pdf| pdf.document().id) != Some(document)
        {
            return Task::none();
        }
        let Some(session) = &mut self.read_aloud.session else {
            return Task::none();
        };
        if let Next::Pdf {
            page: wanted,
            text,
            fetching,
        } = &mut session.next
            && *wanted == page
            && *fetching
        {
            *fetching = false;
            match result {
                // A page without a text layer is skipped, not spoken as silence.
                Ok(value) => *text = Some(value),
                Err(error) => {
                    session.notice = Some(format!("Page {} could not be read: {error}", page + 1));
                    *text = Some(String::new());
                }
            }
        }
        self.read_aloud_tick()
    }

    /// Follows the engine: drops finished text, queues more, turns pages.
    pub(super) fn read_aloud_tick(&mut self) -> Task<Message> {
        self.sync_read_aloud();
        // Even chapters with no text must advance before the engine is created.
        let status = match self
            .read_aloud
            .speaker
            .as_ref()
            .map(Speaker::status)
            .transpose()
        {
            Ok(status) => status.unwrap_or(speech::Status {
                done: true,
                ..Default::default()
            }),
            Err(error) => {
                self.read_aloud.stop();
                self.error = Some(error);
                return Task::none();
            }
        };
        let Some(session) = self.read_aloud.session.as_mut() else {
            return Task::none();
        };
        if session.paused {
            return Task::none();
        }
        while session
            .queued
            .front()
            .is_some_and(|queued| queued.stream < status.stream || status.done)
        {
            session.queued.pop_front();
        }
        if session.selection {
            if !session.pending_text && session.queued.is_empty() {
                self.read_aloud.session = None;
            }
            return Task::none();
        }
        // Moving to another page yourself restarts reading there.
        let opening = match session.next {
            Next::Opening(chapter) => Some(chapter),
            _ => None,
        };
        let expected = session.expected;
        if self.opening.is_some() || self.pagination.is_some() {
            return Task::none();
        }
        if opening.is_none() && expected.is_some() && self.place() != expected {
            return self.start_reading(false);
        }
        if let Some(chapter) = opening {
            let ready = self.book_chapter() == chapter;
            if !ready {
                // Loading failed, or navigation landed in a different chapter.
                self.read_aloud.stop();
                return Task::none();
            }
            let place = self.place();
            if let Some(session) = &mut self.read_aloud.session {
                session.next = Next::Rows {
                    chapter,
                    row: 0,
                    skip: 0,
                };
                session.expected = place;
            }
        }
        let mut tasks = Vec::new();
        self.queue_ahead(&mut tasks);
        tasks.push(self.follow(status));
        let interactive = self.interactive();
        let Some(session) = &mut self.read_aloud.session else {
            return Task::batch(tasks);
        };
        if session.queued.is_empty() {
            match session.next {
                Next::Chapter(chapter) if interactive => {
                    session.next = Next::Opening(chapter);
                    session.expected = None;
                    tasks.push(self.navigate(Some(chapter), None, None, Navigation::Preserve));
                }
                Next::Nothing => self.read_aloud.session = None,
                _ => {}
            }
        }
        Task::batch(tasks)
    }

    /// Queues the next paragraph or PDF page; returns false when nothing more can be queued now.
    fn queue_one(&mut self, tasks: &mut Vec<Task<Message>>) -> bool {
        let Some(session) = &self.read_aloud.session else {
            return false;
        };
        if session.queued.len() >= LOOKAHEAD {
            return false;
        }
        let first = session.queued.is_empty();
        let (text, unit, start, total, next) = match session.next.clone() {
            Next::Rows { chapter, row, skip } => {
                let Some(book) = &self.book else {
                    return false;
                };
                let found = book
                    .items
                    .iter()
                    .enumerate()
                    .skip(row)
                    .find_map(|(index, item)| {
                        let whole = item.text()?;
                        let from = if index == row {
                            skip.min(whole.len())
                        } else {
                            0
                        };
                        let from = if whole.is_char_boundary(from) {
                            from
                        } else {
                            0
                        };
                        (!whole[from..].trim().is_empty()).then_some((index, from, whole))
                    });
                let Some((index, from, whole)) = found else {
                    let chapters = book
                        .epub
                        .as_ref()
                        .map_or(0, |chapter| chapter.document.chapters.len());
                    let next = if chapter + 1 < chapters {
                        Next::Chapter(chapter + 1)
                    } else {
                        Next::Nothing
                    };
                    if let Some(session) = &mut self.read_aloud.session {
                        session.next = next;
                    }
                    return false;
                };
                (
                    whole[from..].to_owned(),
                    Unit::Row(index),
                    whole[..from].encode_utf16().count() as u32,
                    whole.encode_utf16().count() as u32,
                    Next::Rows {
                        chapter,
                        row: index + 1,
                        skip: 0,
                    },
                )
            }
            Next::Pdf {
                page,
                text,
                fetching,
            } => {
                let Some(pdf) = &self.pdf else {
                    return false;
                };
                if page >= pdf.document().pages.len() {
                    if let Some(session) = &mut self.read_aloud.session {
                        session.next = Next::Nothing;
                    }
                    return false;
                }
                let Some(text) = text else {
                    if !fetching {
                        let document = pdf.document().id;
                        let generation = self.read_aloud.generation;
                        let source = pdf.document().session.clone();
                        if let Some(session) = &mut self.read_aloud.session {
                            session.next = Next::Pdf {
                                page,
                                text: None,
                                fetching: true,
                            };
                        }
                        tasks.push(Task::perform(
                            async move { source.page_text(page as u32).await },
                            move |result| Message::ReadAloudPdfPage {
                                document,
                                generation,
                                page,
                                result,
                            },
                        ));
                    }
                    return false;
                };
                let next = Next::Pdf {
                    page: page + 1,
                    text: None,
                    fetching: false,
                };
                if text.trim().is_empty() {
                    // No text layer on this page: go on with the next.
                    if let Some(session) = &mut self.read_aloud.session {
                        session.next = next;
                    }
                    return true;
                }
                let total = text.encode_utf16().count() as u32;
                (text, Unit::PdfPage(page), 0, total, next)
            }
            Next::Chapter(_) | Next::Opening(_) | Next::Nothing => return false,
        };
        if first {
            // Include the prose after a short heading when choosing a language.
            let sample = match unit {
                Unit::Row(row) => self
                    .book
                    .as_ref()
                    .map(|book| {
                        book.items
                            .iter()
                            .skip(row)
                            .filter_map(Item::text)
                            .flat_map(|text| text.chars().chain([' ']))
                            .take(4000)
                            .collect::<String>()
                    })
                    .unwrap_or_else(|| text.clone()),
                _ => text.clone(),
            };
            if let Err(error) = self.read_aloud.prepare(&sample) {
                self.read_aloud.stop();
                self.error = Some(error);
                return false;
            }
        }
        let Some(speaker) = self.read_aloud.speaker.as_ref() else {
            return false;
        };
        match speaker.speak(&text) {
            Ok(stream) => {
                if let Some(session) = &mut self.read_aloud.session {
                    session.queued.push_back(Queued {
                        stream,
                        unit,
                        start,
                        total,
                    });
                    session.next = next;
                }
                true
            }
            Err(error) => {
                self.read_aloud.stop();
                self.error = Some(error);
                false
            }
        }
    }

    fn queue_ahead(&mut self, tasks: &mut Vec<Task<Message>>) {
        while self.queue_one(tasks) {}
    }

    /// Shows the page with the word being spoken.
    fn follow(&mut self, status: speech::Status) -> Task<Message> {
        let Some(current) = self.read_aloud.session.as_ref().and_then(|session| {
            session
                .queued
                .iter()
                .find(|queued| queued.stream == status.stream)
                .map(|queued| (queued.unit, queued.start, queued.total))
        }) else {
            return Task::none();
        };
        match current {
            (Unit::PdfPage(page), ..) => {
                if let Some(session) = &mut self.read_aloud.session {
                    session.expected = Some(Place::Pdf(page));
                }
                let Some(pdf) = self.pdf.as_mut() else {
                    return Task::none();
                };
                if pdf.page_index() == page {
                    return Task::none();
                }
                let document = pdf.document().id;
                pdf.go_to_page(page)
                    .map(move |message| Message::Pdf { document, message })
            }
            (Unit::Row(row), start, total) => {
                let fraction = ((start + status.word) as f32 / total.max(1) as f32).clamp(0.0, 1.0);
                let y = self.heights.start(row) + self.heights.height(row) * fraction;
                let pages = self.pages();
                let Some(target) = pages
                    .iter()
                    .position(|page| page.content.start <= y && y < page.content.end)
                    .or_else(|| pages.iter().rposition(|page| page.rows.contains(&row)))
                else {
                    return Task::none();
                };
                let chapter = self.book_chapter();
                let top = pages[target].top;
                if let Some(session) = &mut self.read_aloud.session {
                    session.expected = Some(Place::Book { chapter, top });
                }
                if self.active_page().is_some_and(|page| page.top == top) {
                    return Task::none();
                }
                self.go_to_local_page(target)
            }
            (Unit::Text, ..) => Task::none(),
        }
    }

    /// What the player bar says it is doing.
    pub(super) fn read_aloud_caption(&self) -> String {
        let Some(session) = &self.read_aloud.session else {
            return String::new();
        };
        if session.selection {
            return "Reading the selection".into();
        }
        match session.next {
            Next::Opening(_) => return "Opening the next chapter…".into(),
            Next::Pdf { fetching: true, .. } if session.queued.is_empty() => {
                return "Reading the page…".into();
            }
            _ => {}
        }
        match self.place() {
            Some(Place::Pdf(page)) => format!("Reading page {}", page + 1),
            Some(Place::Book { .. }) => self
                .active_page()
                .map(|page| format!("Reading page {}", page.label))
                .unwrap_or_else(|| "Reading".into()),
            None => "Reading".into(),
        }
    }
}

/// The start of the sentence holding byte `at`, or the next word start if the
/// sentence began far back.
fn sentence_start(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    if at == 0 {
        return 0;
    }
    let before = &text[..at];
    let sentence = before
        .char_indices()
        .rev()
        .find(|(index, c)| {
            matches!(c, '.' | '!' | '?' | '…' | '"' | '”')
                && before[index + c.len_utf8()..].starts_with(char::is_whitespace)
        })
        .map(|(index, c)| {
            let after = index + c.len_utf8();
            after + (before[after..].len() - before[after..].trim_start().len())
        });
    match sentence {
        Some(start) if at - start < 400 => start,
        _ => before
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map_or(0, |(space, c)| space + c.len_utf8()),
    }
}

/// Ticks while something is being read.
pub(super) fn ticks() -> impl iced_futures::futures::Stream<Item = ()> {
    use iced_futures::futures::{SinkExt, StreamExt};

    iced::stream::channel(
        1,
        |mut output: iced_futures::futures::channel::mpsc::Sender<()>| async move {
            let (mut sender, mut receiver) = iced_futures::futures::channel::mpsc::channel(1);
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(TICK_MS));
                    if sender
                        .try_send(())
                        .is_err_and(|error| error.is_disconnected())
                    {
                        break;
                    }
                }
            });
            while let Some(()) = receiver.next().await {
                if output.send(()).await.is_err() {
                    break;
                }
            }
        },
    )
}

/// The Read aloud player shown under the toolbar while something is read.
pub(super) fn player<'a>(reader: &'a Reader, active: bool) -> Element<'a, Message> {
    let Some(session) = &reader.read_aloud.session else {
        return iced::widget::Space::new().height(0).into();
    };
    let action = |action: Action| active.then_some(Message::ReadAloud(action));
    let pause = if session.paused {
        control_button(
            reader,
            Control::ReadAloudPause,
            text("Resume").font(ui::MEDIUM).size(13),
            action(Action::Resume),
        )
    } else {
        control_button(
            reader,
            Control::ReadAloudPause,
            text("Pause").font(ui::MEDIUM).size(13),
            action(Action::Pause),
        )
    };
    let rate = reader.read_aloud.rate;
    let mut bar = row![
        text("●").size(10).style(if session.paused {
            ui::muted_text
        } else {
            ui::accent_text
        }),
        text(reader.read_aloud_caption())
            .size(13)
            .style(ui::secondary_text)
            .width(Length::Fill),
        hinted_control(
            reader,
            Control::ReadAloudSlower,
            text("−").size(16),
            "Slower",
            (rate > speech::MIN_RATE)
                .then_some(())
                .and(action(Action::Slower)),
        ),
        text(format!("{:.1}×", speech::speed(rate)))
            .size(12)
            .style(ui::muted_text),
        hinted_control(
            reader,
            Control::ReadAloudFaster,
            text("+").size(16),
            "Faster",
            (rate < speech::MAX_RATE)
                .then_some(())
                .and(action(Action::Faster)),
        ),
        pause,
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);
    if let Some(notice) = &session.notice {
        bar = bar.push(text(notice).size(11).style(ui::muted_text));
    }
    container(bar).padding([8, 12]).style(ui::panel).into()
}

/// The Read aloud section of the settings panel.
pub(super) fn settings(reader: &Reader, choices: Vec<VoiceChoice>) -> Element<'_, Message> {
    let chosen = reader.read_aloud.chosen(&choices);
    let rate = reader.read_aloud.rate;
    let voice: Element<'_, Message> = if let Some(error) = &reader.read_aloud.unavailable {
        text(error.clone()).size(12).style(ui::danger_text).into()
    } else {
        iced::widget::pick_list(choices, chosen, |choice| {
            Message::ReadAloud(Action::Voice(choice))
        })
        .text_size(12)
        .padding([6, 10])
        .width(280)
        .into()
    };
    column![
        row![
            text("Voice").font(ui::MEDIUM).size(13),
            iced::widget::Space::new().width(Length::Fill),
            voice,
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center),
        row![
            text("Speed").font(ui::MEDIUM).size(13),
            iced::widget::Space::new().width(Length::Fill),
            control_button(
                reader,
                Control::ReadAloudSlower,
                text("−").size(14),
                (rate > speech::MIN_RATE).then_some(Message::ReadAloud(Action::Slower)),
            ),
            text(format!("{:.1}×", speech::speed(rate)))
                .size(12)
                .style(ui::muted_text),
            control_button(
                reader,
                Control::ReadAloudFaster,
                text("+").size(14),
                (rate < speech::MAX_RATE).then_some(Message::ReadAloud(Action::Faster)),
            ),
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center),
        text(
            "Reads the selection, or from the current page on, with the voices installed in \
             Windows; no network is used. Automatic picks a voice for the language of the text. \
             Add voices in Windows Settings › Time & language › Speech. Ctrl+Shift+U starts or stops."
        )
        .size(12)
        .style(ui::muted_text),
    ]
    .spacing(10)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book_reader() -> Reader {
        let mut reader = Reader {
            book: Some(Arc::new(Book {
                path: "tts-test.html".into(),
                title: "Speech test".into(),
                author: None,
                cover: false,
                fingerprint: "tts-test".into(),
                items: (0..4)
                    .map(|index| Item::Paragraph {
                        id: format!("paragraph-{index}"),
                        text: "Before. Selected words. After. ".repeat(10),
                        base_direction: BaseDirection::Ltr,
                        style_runs: Vec::new(),
                    })
                    .collect(),
                images: HashMap::new(),
                structure: HashMap::new(),
                anchors: HashMap::new(),
                page_breaks: Vec::new(),
                warnings: Vec::new(),
                restored: None,
                epub: None,
                pdf_source: None,
                contents: std::sync::OnceLock::new(),
            })),
            ..Default::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 0,
            fraction: 0.0,
        });
        settle_pagination(&mut reader);
        reader
    }

    fn pdf_reader(case: &str) -> Option<Reader> {
        use crate::pdf_reader::tests::{complete, tall_pdf};
        if !std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("pdfium.dll")
            .exists()
        {
            eprintln!("skipped: pdfium.dll is not beside the test executable");
            return None;
        }
        let path =
            std::env::temp_dir().join(format!("simpl-tts-{case}-{}.pdf", std::process::id()));
        std::fs::write(&path, tall_pdf()).unwrap();
        let document = complete(reader_pdf::open(path.clone())).unwrap();
        let _ = std::fs::remove_file(path);
        let (pdf, _) = pdf_reader::Reader::new(document, None, Size::new(1280.0, 800.0), 1.0);
        Some(Reader {
            pdf: Some(pdf),
            ..Default::default()
        })
    }

    #[test]
    fn stale_pdf_extractions_cannot_replace_a_restarted_session() {
        let Some(mut reader) = pdf_reader("generation") else {
            return;
        };
        let document = reader.reading_document().unwrap();
        let id = reader.pdf.as_ref().unwrap().document().id;
        reader.begin(document.clone(), true, Next::Nothing, None);
        let stale = reader.read_aloud.generation;
        reader.begin(document.clone(), true, Next::Nothing, None);
        reader.read_aloud.session.as_mut().unwrap().pending_text = true;
        reader.read_aloud_text(id, stale, Err("old selection failure".into()));
        assert!(reader.read_aloud.session.as_ref().unwrap().pending_text);
        assert!(reader.error.is_none());
        let generation = reader.read_aloud.generation;
        reader.read_aloud_text(id, generation, Ok(String::new()));
        assert!(!reader.read_aloud.active());

        reader.begin(
            document,
            false,
            Next::Pdf {
                page: 0,
                text: None,
                fetching: true,
            },
            None,
        );
        reader.read_aloud.session.as_mut().unwrap().paused = true;
        let _ = reader.read_aloud_pdf_page(id, stale, 0, Err("old page failure".into()));
        assert!(matches!(
            reader.read_aloud.session.as_ref().unwrap().next,
            Next::Pdf {
                fetching: true,
                text: None,
                ..
            }
        ));
        let generation = reader.read_aloud.generation;
        let _ = reader.read_aloud_pdf_page(id, generation, 0, Ok("Current page".into()));
        assert!(
            matches!(&reader.read_aloud.session.as_ref().unwrap().next, Next::Pdf { fetching: false, text: Some(text), .. } if text == "Current page")
        );
        assert!(reader.read_aloud.session.as_ref().unwrap().notice.is_none());
    }

    #[test]
    fn textless_books_and_failed_chapter_loads_end_without_a_speaker() {
        let mut reader = book_reader();
        Arc::get_mut(reader.book.as_mut().unwrap())
            .unwrap()
            .items
            .clear();
        let document = reader.reading_document().unwrap();
        reader.begin(
            document.clone(),
            false,
            Next::Rows {
                chapter: 0,
                row: 0,
                skip: 0,
            },
            None,
        );
        let _ = reader.read_aloud_tick();
        assert!(!reader.read_aloud.active());
        assert!(reader.read_aloud.speaker.is_none());

        reader.begin(document, false, Next::Opening(1), None);
        reader.error = Some("Chapter could not be loaded".into());
        let _ = reader.read_aloud_tick();
        assert!(!reader.read_aloud.active());
        assert!(reader.error.is_some());
    }

    #[test]
    #[ignore = "Native speech QA: uses Windows voices at zero volume"]
    fn native_read_aloud_selection_page_pause_and_document_close() {
        let mut reader = book_reader();
        let speaker = Speaker::new().unwrap();
        speaker.mute();
        reader.read_aloud.speaker = Some(speaker);
        reader.selection.begin(Endpoint {
            item_id: "paragraph-0".into(),
            byte_offset: 8,
        });
        reader.selection.extend(Endpoint {
            item_id: "paragraph-0".into(),
            byte_offset: 23,
        });
        reader.selection.end_drag();
        let _ = reader.read_aloud(Action::Selection);
        let session = reader.read_aloud.session.as_ref().unwrap();
        assert!(session.selection);
        assert_eq!(session.queued.len(), 1);
        assert_eq!(
            session.queued[0].total,
            "Selected words.".encode_utf16().count() as u32
        );
        let _ = reader.read_aloud(Action::Pause);
        assert!(reader.read_aloud.session.as_ref().unwrap().paused);
        assert!(!reader.read_aloud.ticking());
        let _ = reader.read_aloud(Action::Resume);
        assert!(reader.read_aloud.ticking());
        let _ = reader.read_aloud(Action::Toggle);
        assert!(!reader.read_aloud.active());

        reader.selection.clear();
        let _ = reader.read_aloud(Action::Toggle);
        let session = reader.read_aloud.session.as_ref().unwrap();
        assert!(!session.selection);
        assert_eq!(session.queued.len(), LOOKAHEAD);
        assert_eq!(session.queued[0].unit, Unit::Row(0));
        reader.book = None;
        reader.sync_read_aloud();
        assert!(!reader.read_aloud.active());
        assert!(reader.error.is_none());
    }

    #[test]
    #[ignore = "Native speech QA: uses Windows voices at zero volume and PDFium"]
    fn native_read_aloud_pending_pdf_selection_stays_paused() {
        let Some(mut reader) = pdf_reader("paused") else {
            return;
        };
        let document = reader.reading_document().unwrap();
        let id = reader.pdf.as_ref().unwrap().document().id;
        reader.begin(document, true, Next::Nothing, None);
        reader.read_aloud.session.as_mut().unwrap().pending_text = true;
        let _ = reader.read_aloud(Action::Pause);
        // Install a muted engine after Pause, as when the first extraction creates it.
        let speaker = Speaker::new().unwrap();
        speaker.mute();
        reader.read_aloud.speaker = Some(speaker);
        let generation = reader.read_aloud.generation;
        reader.read_aloud_text(
            id,
            generation,
            Ok("This selection must wait until playback is resumed.".into()),
        );
        let session = reader.read_aloud.session.as_ref().unwrap();
        assert!(session.paused && !session.pending_text);
        assert_eq!(session.queued.len(), 1);
        let speaker = reader.read_aloud.speaker.as_ref().unwrap();
        let before = speaker.status().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let after = speaker.status().unwrap();
        // SAPI may activate the queued stream while paused, without speaking it.
        assert_eq!(before.word, after.word);
        assert!(!after.done);
        let _ = reader.read_aloud(Action::Resume);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while reader.read_aloud.active() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(TICK_MS));
            let _ = reader.read_aloud_tick();
        }
        assert!(!reader.read_aloud.active());
        assert!(reader.error.is_none());
    }

    #[test]
    #[ignore = "Native EPUB speech QA: SIMPL_TTS_EPUB points to empty/prose/empty/prose chapters"]
    fn native_read_aloud_epub_chapter_transitions() {
        let path = PathBuf::from(
            std::env::var_os("SIMPL_TTS_EPUB")
                .expect("Set SIMPL_TTS_EPUB to the authored TTS fixture"),
        );
        let epub = Arc::new(reader_document::epub::open(&path).unwrap());
        assert_eq!(epub.chapters.len(), 4);
        let mut reader = Reader {
            book: Some(Arc::new(load_epub_chapter(epub.clone(), 0, None).unwrap())),
            ..Default::default()
        };
        let _ = reader.rebuild_geometry(Anchor {
            row: 0,
            fraction: 0.0,
        });
        settle_pagination(&mut reader);
        let document = reader.reading_document().unwrap();
        reader.begin(
            document.clone(),
            false,
            Next::Rows {
                chapter: 0,
                row: 0,
                skip: 0,
            },
            reader.place(),
        );
        let _ = reader.read_aloud_tick();
        assert!(matches!(
            reader.read_aloud.session.as_ref().unwrap().next,
            Next::Opening(1)
        ));
        assert!(reader.read_aloud.speaker.is_none());
        let speaker = Speaker::new().unwrap();
        speaker.mute();
        reader.read_aloud.speaker = Some(speaker);

        for chapter in 1..4 {
            let loaded = load_epub_chapter(epub.clone(), chapter, None).unwrap();
            let request = reader.request;
            let _ = update(
                &mut reader,
                Message::Loaded {
                    request,
                    result: Ok(LoadReply {
                        document: LoadedDocument::Reflow(Arc::new(loaded)),
                        catalog: None,
                    }),
                },
            );
            settle_pagination(&mut reader);
            assert_eq!(reader.reading_document(), Some(document.clone()));
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                let _ = reader.read_aloud_tick();
                assert!(reader.error.is_none(), "{:?}", reader.error);
                if chapter == 3 && !reader.read_aloud.active() {
                    break;
                }
                if reader.read_aloud.session.as_ref().is_some_and(
                    |session| matches!(session.next, Next::Opening(next) if next == chapter + 1),
                ) {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "chapter {chapter} did not finish"
                );
                std::thread::sleep(std::time::Duration::from_millis(TICK_MS));
            }
        }
    }

    #[test]
    #[ignore = "Visual speech QA: renders the production widgets to target/tts-previews"]
    fn render_read_aloud_previews() {
        for bytes in ui::font_data() {
            iced::advanced::graphics::text::font_system()
                .write()
                .unwrap()
                .load_font(std::borrow::Cow::Borrowed(bytes));
        }
        let output = PathBuf::from("../../target/tts-previews");
        std::fs::create_dir_all(&output).unwrap();
        let mut reader = book_reader();
        let document = reader.reading_document().unwrap();
        reader.begin(document, false, Next::Nothing, reader.place());
        reader.voice_choices = vec![VoiceChoice {
            id: String::new(),
            label: AUTOMATIC.into(),
        }];
        for (name, width) in [("wide", 1280.0), ("narrow", 540.0)] {
            reader.window_size = Size::new(width, 800.0);
            reader.show_settings = false;
            super::super::book_preview::render(
                &mut reader,
                &output.join(format!("player-{name}.png")),
            );
            reader.show_settings = true;
            super::super::book_preview::render(
                &mut reader,
                &output.join(format!("settings-{name}.png")),
            );
        }
    }

    #[test]
    fn reading_resumes_at_the_sentence_a_page_starts_in() {
        let text = "First sentence here. Second one starts here and runs on for a while.";
        let second = text.find("Second").unwrap();
        assert_eq!(sentence_start(text, second + 10), second);
        assert_eq!(sentence_start(text, 3), 0);
        assert_eq!(sentence_start(text, 0), 0);
        assert_eq!(sentence_start(text, text.len() + 5), second);
        // Without a sentence end nearby, reading starts at a word.
        let long = format!("{} tail", "word ".repeat(200));
        let start = sentence_start(&long, long.len() - 2);
        assert!(long[start..].starts_with("tail") || long[..start].ends_with(' '));
        // Multi-byte text never splits a character.
        let turkish =
            "Çin fikri ise mandalanın narin beden simgesi olduğunu söyler. Doğu teorisine göre";
        let start = sentence_start(turkish, turkish.len() - 3);
        assert!(turkish[start..].starts_with("Doğu"));
        let unicode_space = "First\u{2003}second word";
        let start = sentence_start(unicode_space, unicode_space.find("second").unwrap() + 3);
        assert_eq!(&unicode_space[start..], "second word");
    }
}
