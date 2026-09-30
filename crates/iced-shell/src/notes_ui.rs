//! The reader's side of bookmarks, highlights and notes: creating and changing
//! them, and the pop-up menu, note editor and list that show them. The rules
//! live in `crate::notes`; storage is `reader_document::annotations`.
//!
//! Nothing here adds a toolbar control. Highlights start from a selection or a
//! right click, bookmarks from Ctrl+D or a right click, and the right sidebar
//! opens with Ctrl+B or its edge toggle.

use super::*;
use crate::notes::{self, Action, Entry, Focus, ListTab, Popup, PopupItem, Target};
use iced::widget::{Space, column, mouse_area, opaque, row, text_editor, tooltip};
use reader_document::annotations::{
    self, Annotations, BookmarkPlace, HighlightColor, PdfPoint, Place, ReflowPoint,
};

/// The most rows the list shows; a book rarely has more, and each row is a widget.
const MAX_LIST_ROWS: usize = 150;

pub(super) fn note_editor_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("note-editor")
}

const RIBBON: iced::Color = iced::Color::from_rgb(0.78, 0.20, 0.17);

pub(super) fn sidebar_width(reader: &Reader) -> f32 {
    26.0 + if reader.notes.list.is_some() {
        (reader.window_size.width * 0.36).clamp(230.0, 340.0)
    } else {
        0.0
    }
}

fn unavailable(blocked: Option<&String>) -> String {
    match blocked {
        Some(error) => format!("Bookmarks and notes are unavailable for this book: {error}"),
        None => "Bookmarks and notes are still loading.".into(),
    }
}

impl Reader {
    pub(super) fn document_view_size(&self) -> iced::Size {
        iced::Size::new(
            (self.window_size.width - sidebar_width(self)).max(1.0),
            self.window_size.height,
        )
    }
    /// The fingerprint of whatever is open, for either kind of reader.
    fn notes_book(&self) -> Option<String> {
        self.book
            .as_ref()
            .map(|book| book.fingerprint.clone())
            .or_else(|| {
                self.pdf
                    .as_ref()
                    .map(|pdf| pdf.document().fingerprint.clone())
            })
    }

    /// The EPUB chapter on screen, as its canonical href.
    fn chapter_href(&self) -> Option<String> {
        let chapter = self.book.as_ref()?.epub.as_ref()?;
        chapter
            .document
            .section(chapter.index)
            .map(|section| section.href.clone())
    }

    fn page_label(&self) -> String {
        self.active_page()
            .map(|page| page.label)
            .unwrap_or_default()
    }

    /// Resolves the saved highlights against the text that is on screen now.
    pub(super) fn refresh_marks(&mut self) {
        self.notes.marks = match (&self.book, &self.notes.data) {
            (Some(book), Some(data))
                if data.fingerprint.eq_ignore_ascii_case(&book.fingerprint) =>
            {
                notes::marks(&book.items, self.chapter_href().as_deref(), data)
            }
            _ => Vec::new(),
        };
        self.push_pdf_marks();
    }

    /// Gives the PDF page view what it draws: highlights and bookmarked pages.
    fn push_pdf_marks(&mut self) {
        let Some(pdf) = &mut self.pdf else { return };
        let (marks, pages) = match &self.notes.data {
            Some(data)
                if data
                    .fingerprint
                    .eq_ignore_ascii_case(&pdf.document().fingerprint) =>
            {
                (
                    data.highlights
                        .iter()
                        .filter_map(|highlight| match &highlight.place {
                            Place::Pdf { from, to } => Some(pdf_reader::Mark {
                                from: *from,
                                to: *to,
                                id: highlight.id,
                                tint: notes::tint(highlight.color),
                                note: highlight.note.is_some(),
                            }),
                            Place::Reflow { .. } => None,
                        })
                        .collect(),
                    data.bookmarks
                        .iter()
                        .filter_map(|bookmark| match &bookmark.place {
                            BookmarkPlace::Pdf { page, .. } => Some(*page),
                            BookmarkPlace::Reflow { .. } => None,
                        })
                        .collect(),
                )
            }
            _ => (Vec::new(), Vec::new()),
        };
        pdf.set_marks(marks, pages);
    }

    /// Called whenever a document finished opening: loads its records when it is
    /// a different book, and re-resolves them when only the chapter changed.
    pub(super) fn sync_notes(&mut self) -> Task<Message> {
        self.notes.close_transients();
        let Some(fingerprint) = self.notes_book() else {
            self.notes.reset(None);
            return Task::none();
        };
        if self.notes.book.as_deref() == Some(fingerprint.as_str()) {
            self.refresh_marks();
            return Task::none();
        }
        self.notes.reset(Some(&fingerprint));
        if let Some(data) = self.notes.pending_for(&fingerprint) {
            self.notes.data = Some(data);
            self.refresh_marks();
            return Task::none();
        }
        let reply = fingerprint.clone();
        let profile = self.profile.epoch;
        Task::perform(
            async move { annotations::load(&fingerprint) },
            move |result| Message::NotesLoaded {
                profile,
                fingerprint: reply.clone(),
                result,
            },
        )
    }

    pub(super) fn notes_loaded(
        &mut self,
        fingerprint: String,
        result: Result<Annotations, String>,
    ) {
        if self.notes.book.as_deref() != Some(fingerprint.as_str()) {
            return;
        }
        match result {
            Ok(data) => {
                self.notes.data = Some(data);
                self.notes.blocked = None;
                self.refresh_marks();
            }
            Err(error) => {
                // The unreadable file stays as it is; nothing will overwrite it.
                self.error = Some(unavailable(Some(&error)));
                self.notes.blocked = Some(error);
            }
        }
    }

    /// Starts writing whatever is queued, one write at a time.
    pub(super) fn persist_notes(&mut self) -> Task<Message> {
        let Some(data) = self.notes.take_pending_save() else {
            return Task::none();
        };
        Task::perform(async move { annotations::save(&data) }, Message::NotesSaved)
    }

    pub(super) fn notes_saved(&mut self, result: Result<(), String>) -> Task<Message> {
        let success = result.is_ok();
        self.notes.finish_save(success);
        if let Err(error) = result {
            self.error = Some(format!("Could not save bookmarks and notes: {error}"));
            if let Some(action) = self.pending_notes_close.take() {
                self.failed_close = Some(action);
                self.pending_exit = false;
            }
            return Task::none();
        }
        let task = self.persist_notes();
        if self.notes.settled()
            && let Some(action) = self.pending_notes_close.take()
        {
            return Task::batch([task, self.finish_close(action)]);
        }
        task
    }

    /// Applies one change to the loaded records and queues it to be saved.
    fn change_notes<T>(
        &mut self,
        edit: impl FnOnce(&mut Annotations) -> Result<T, String>,
    ) -> Option<T> {
        let Some(data) = self.notes.data.as_mut() else {
            self.error = Some(unavailable(self.notes.blocked.as_ref()));
            return None;
        };
        match edit(data) {
            Ok(value) => {
                self.notes.changed();
                self.refresh_marks();
                Some(value)
            }
            Err(error) => {
                self.error = Some(error);
                None
            }
        }
    }

    // ---------------------------------------------------------- bookmarks

    /// Where a bookmark on the page in view would return to.
    fn bookmark_here(&self) -> Option<(BookmarkPlace, String, String)> {
        let book = self.book.as_ref()?;
        let page = self.active_page()?;
        let row = page.rows.start.min(book.items.len().checked_sub(1)?);
        let within = ((page.content.start - self.heights.start(row))
            / self.heights.height(row).max(1.0))
        .clamp(0.0, 1.0);
        // The excerpt is the first words of text on the page, from the page top.
        let excerpt = book.items[row..page.rows.end.max(row + 1).min(book.items.len())]
            .iter()
            .enumerate()
            .find_map(|(offset, item)| {
                let text = item.text().filter(|text| !text.trim().is_empty())?;
                let from = if offset == 0 {
                    text.floor_char_boundary((within * text.len() as f32) as usize)
                } else {
                    0
                };
                Some(notes::preview(&text[from..], 120))
            })
            .unwrap_or_default();
        Some((
            BookmarkPlace::Reflow {
                chapter: self.chapter_href(),
                item_id: book.items[row].id().to_owned(),
                within,
                page_number: page.number,
            },
            page.label,
            excerpt,
        ))
    }

    /// Ctrl+D and the menu entry: bookmark the page in view, or remove its bookmark.
    pub(super) fn toggle_bookmark(&mut self) -> Task<Message> {
        if !self.interactive() {
            return Task::none();
        }
        if let Some(pdf) = &self.pdf {
            let page = pdf.page_index() as u32;
            let existing = self
                .notes
                .is_bookmarked_pdf(page)
                .map(|bookmark| bookmark.id);
            self.change_notes(|data| match existing {
                Some(id) => {
                    data.remove_bookmark(id);
                    Ok(())
                }
                None => data
                    .add_bookmark(
                        BookmarkPlace::Pdf { page, within: 0.0 },
                        (page + 1).to_string(),
                        String::new(),
                    )
                    .map(|_| ()),
            });
        } else if let Some(page) = self.active_page() {
            let existing = self
                .notes
                .is_bookmarked_reflow(page.number)
                .map(|bookmark| bookmark.id);
            match existing {
                Some(id) => {
                    self.change_notes(|data| {
                        data.remove_bookmark(id);
                        Ok(())
                    });
                }
                None => {
                    let Some((place, label, excerpt)) = self.bookmark_here() else {
                        return Task::none();
                    };
                    self.change_notes(|data| data.add_bookmark(place, label, excerpt));
                }
            }
        }
        self.persist_notes()
    }

    pub(super) fn page_is_bookmarked(&self) -> bool {
        self.active_page()
            .is_some_and(|page| self.notes.is_bookmarked_reflow(page.number).is_some())
    }

    // --------------------------------------------------------- highlights

    /// The selected passage of a reflowed book, as a saved place and its text.
    fn selected_passage(&self) -> Option<(Place, String)> {
        let book = self.book.as_ref()?;
        let bounds = self.selection.bounds(&book.items)?;
        if (bounds.start_item, bounds.start_byte) == (bounds.end_item, bounds.end_byte) {
            return None;
        }
        let quote = notes::text_of(&book.items, bounds);
        if quote.trim().is_empty() {
            return None;
        }
        let point = |item: usize, byte: usize| ReflowPoint {
            item_id: book.items[item].id().to_owned(),
            byte,
        };
        Some((
            Place::Reflow {
                chapter: self.chapter_href(),
                from: point(bounds.start_item, bounds.start_byte),
                to: point(bounds.end_item, bounds.end_byte),
            },
            quote,
        ))
    }

    /// Whether something is selected that could be highlighted.
    fn has_passage(&self) -> bool {
        if let Some(pdf) = &self.pdf {
            return pdf.selection().is_some();
        }
        self.selected_passage().is_some()
    }

    fn highlight_selection(&mut self, color: HighlightColor, then_note: bool) -> Option<u64> {
        self.notes.color = color;
        let bounds = self.book.as_ref().and_then(|book| {
            let bounds = self.selection.bounds(&book.items)?;
            Some(notes::snap_to_words(&book.items, bounds))
        })?;
        let plan = notes::merge_reflow(&self.notes.marks, color, bounds);
        if plan.noted > 1 && !then_note && plan.bounds == bounds {
            self.selection.clear();
            return plan.ids.first().copied();
        }
        let (place, quote) = if plan.noted > 1 {
            self.selected_passage()?
        } else {
            let book = self.book.as_ref()?;
            let point = |item: usize, byte: usize| ReflowPoint {
                item_id: book.items[item].id().to_owned(),
                byte,
            };
            (
                Place::Reflow {
                    chapter: self.chapter_href(),
                    from: point(plan.bounds.start_item, plan.bounds.start_byte),
                    to: point(plan.bounds.end_item, plan.bounds.end_byte),
                },
                notes::text_of(&book.items, plan.bounds),
            )
        };
        let page = self.page_label();
        let id = self.change_notes(|data| {
            if plan.noted > 1 {
                data.add_highlight(place, color, page, quote)
            } else {
                data.merge_highlights(&plan.ids, place, color, page, quote)
            }
        })?;
        self.selection.clear();
        Some(id)
    }

    /// Ctrl+H: highlight the selection with the color used last.
    pub(super) fn highlight_shortcut(&mut self) -> Task<Message> {
        if !self.interactive() || !self.has_passage() {
            return Task::none();
        }
        self.notes.popup = None;
        self.make_highlight(self.notes.color, false)
    }

    /// Highlights the selection, then optionally opens the note editor for it.
    fn make_highlight(&mut self, color: HighlightColor, then_note: bool) -> Task<Message> {
        if self.pdf.is_some() {
            return self.highlight_pdf_selection(color, then_note);
        }
        let Some(id) = self.highlight_selection(color, then_note) else {
            return Task::none();
        };
        let editor = then_note.then(|| self.open_editor(id));
        Task::batch([self.persist_notes(), editor.unwrap_or_else(Task::none)])
    }

    fn highlight_pdf_selection(&mut self, color: HighlightColor, then_note: bool) -> Task<Message> {
        self.notes.color = color;
        let Some(pdf) = &self.pdf else {
            return Task::none();
        };
        let Some(selection) = pdf.selection() else {
            return Task::none();
        };
        let document = pdf.document().clone();
        if !document.can_copy {
            self.error = Some(
                "This PDF does not permit copying text, so its text cannot be highlighted.".into(),
            );
            return Task::none();
        }
        let (from, to) = if selection.anchor <= selection.focus {
            (selection.anchor, selection.focus)
        } else {
            (selection.focus, selection.anchor)
        };
        let selected = (
            PdfPoint {
                page: from.page,
                index: from.index,
            },
            PdfPoint {
                page: to.page,
                index: to.index,
            },
        );
        let plan = self.notes.data.as_ref().map_or(
            notes::PdfMerge {
                from: selected.0,
                to: selected.1,
                ids: Vec::new(),
                noted: 0,
            },
            |data| notes::merge_pdf(data, color, selected.0, selected.1),
        );
        let (from, to, merge_ids) = if plan.noted > 1 {
            (selected.0, selected.1, Vec::new())
        } else {
            (plan.from, plan.to, plan.ids)
        };
        let id = document.id;
        let session = document.session.clone();
        Task::perform(
            async move {
                session
                    .copy(reader_pdf::Selection {
                        anchor: reader_pdf::TextPoint {
                            page: from.page,
                            index: from.index,
                        },
                        focus: reader_pdf::TextPoint {
                            page: to.page,
                            index: to.index,
                        },
                    })
                    .await
            },
            move |result| Message::PdfHighlightText {
                document: id,
                from,
                to,
                merge_ids: merge_ids.clone(),
                color,
                then_note,
                result,
            },
        )
    }

    /// The text of a PDF selection arrived: save the highlight.
    pub(super) fn pdf_highlight_text(
        &mut self,
        document: u64,
        (from, to): (PdfPoint, PdfPoint),
        merge_ids: Vec<u64>,
        color: HighlightColor,
        then_note: bool,
        result: Result<String, String>,
    ) -> Task<Message> {
        if self.pdf.as_ref().map(|pdf| pdf.document().id) != Some(document) {
            return Task::none();
        }
        let quote = match result {
            Ok(text) if !text.trim().is_empty() => text,
            Ok(_) => {
                self.error = Some("There is no selectable text here to highlight.".into());
                return Task::none();
            }
            Err(error) => {
                self.error = Some(format!("Could not highlight this text: {error}"));
                return Task::none();
            }
        };
        let page = (from.page + 1).to_string();
        let Some(id) = self.change_notes(|data| {
            data.merge_highlights(&merge_ids, Place::Pdf { from, to }, color, page, quote)
        }) else {
            return Task::none();
        };
        if let Some(pdf) = &mut self.pdf {
            pdf.clear_selection();
        }
        let editor = then_note.then(|| self.open_editor(id));
        Task::batch([self.persist_notes(), editor.unwrap_or_else(Task::none)])
    }

    // ------------------------------------------------------------ pop-ups

    /// Shows the menu at the pointer. Nothing opens over a modal or while loading.
    pub(super) fn open_popup(&mut self, target: Target, menu: bool) {
        if !self.interactive()
            || self.notes.editor.is_some()
            || self.show_search
            || self.show_settings
            || self.confirm_remove.is_some()
        {
            return;
        }
        self.notes.popup = Some(Popup {
            at: self.pointer,
            target,
            menu,
        });
        self.notes.pressed = None;
        self.focused = None;
    }

    pub(super) fn popup_contains(&self, point: iced::Point) -> bool {
        self.notes
            .popup
            .as_ref()
            .is_some_and(|popup| popup.bounds(self.window_size).contains(point))
    }

    /// The left button went up. A finished selection, or a plain click on a
    /// highlight, opens the pop-up next to the pointer.
    pub(super) fn selection_released(&mut self, was_dragging: bool) -> Task<Message> {
        if !was_dragging || self.notes.popup.is_some() || self.notes.editor.is_some() {
            return Task::none();
        }
        if let Some(task) = self.auto_translate_selection() {
            return task;
        }
        if self.has_passage() {
            self.open_popup(Target::Selection, false);
        } else if let Some(id) = self.notes.pressed.take() {
            self.open_popup(Target::Highlight(id), false);
        }
        Task::none()
    }

    /// A right click on text of a reflowed book.
    pub(super) fn context_at(&mut self, endpoint: Endpoint) {
        self.word_translation.dismiss();
        let Some(book) = &self.book else { return };
        let Some(row) = book
            .items
            .iter()
            .position(|item| item.id() == endpoint.item_id)
        else {
            return;
        };
        let point = (row, endpoint.byte_offset);
        let inside = self.selection.bounds(&book.items).is_some_and(|bounds| {
            (bounds.start_item, bounds.start_byte) != (bounds.end_item, bounds.end_byte)
                && (bounds.start_item, bounds.start_byte) <= point
                && point <= (bounds.end_item, bounds.end_byte)
        });
        let target = if inside {
            Target::Selection
        } else if let Some(id) = notes::mark_at(&self.notes.marks, &book.items, &endpoint) {
            Target::Highlight(id)
        } else {
            Target::Page
        };
        self.open_popup(target, true);
    }

    /// A right click on a PDF page: on a highlight or the selection, or anywhere else.
    pub(super) fn context_pdf(&mut self, point: Option<reader_pdf::TextPoint>) {
        self.word_translation.dismiss();
        let Some(pdf) = &self.pdf else { return };
        let selected = pdf.selection().is_some_and(|selection| {
            let (from, to) = if selection.anchor <= selection.focus {
                (selection.anchor, selection.focus)
            } else {
                (selection.focus, selection.anchor)
            };
            point.is_some_and(|point| from <= point && point <= to)
        });
        let target = if selected {
            Target::Selection
        } else if let Some(id) = point.and_then(|point| pdf.mark_at(point)) {
            Target::Highlight(id)
        } else {
            Target::Page
        };
        self.open_popup(target, true);
    }

    fn open_editor(&mut self, id: u64) -> Task<Message> {
        let Some(highlight) = self.notes.data.as_ref().and_then(|data| data.highlight(id)) else {
            return Task::none();
        };
        self.notes.editor = Some(notes::Editor {
            id,
            content: text_editor::Content::with_text(highlight.note.as_deref().unwrap_or("")),
            existing: highlight.note.is_some(),
        });
        self.notes.popup = None;
        self.focused = None;
        iced::widget::operation::focus(note_editor_id())
    }

    fn remove_entry(&mut self, entry: Entry) -> Task<Message> {
        match entry {
            Entry::Bookmark(id) => {
                self.change_notes(|data| {
                    data.remove_bookmark(id);
                    Ok(())
                });
            }
            Entry::Highlight(id) => {
                if self.notes.expanded_note == Some(id) {
                    self.notes.expanded_note = None;
                }
                if self
                    .notes
                    .editor
                    .as_ref()
                    .is_some_and(|editor| editor.id == id)
                {
                    self.notes.editor = None;
                }
                self.change_notes(|data| {
                    data.remove_highlight(id);
                    Ok(())
                });
            }
        }
        if matches!(self.focused, Some(Control::Notes(_))) {
            self.focused = None;
        }
        self.persist_notes()
    }

    fn popup_item(&mut self, target: Target, item: PopupItem) -> Task<Message> {
        match (target, item) {
            (Target::Selection, PopupItem::Translate) => self.translate_selection(),
            (Target::Highlight(id), PopupItem::Translate) => {
                let quote = self
                    .notes
                    .data
                    .as_ref()
                    .and_then(|data| data.highlight(id))
                    .map(|highlight| highlight.quote.clone());
                quote.map_or_else(Task::none, |quote| self.translate_text(quote))
            }
            (Target::Selection, PopupItem::Color(color)) => self.make_highlight(color, false),
            (Target::Highlight(id), PopupItem::Color(color)) => {
                self.notes.color = color;
                self.change_notes(|data| {
                    data.set_color(id, color)
                        .then_some(())
                        .ok_or_else(|| "This highlight no longer exists.".to_owned())
                });
                self.persist_notes()
            }
            (Target::Selection, PopupItem::Note) => self.make_highlight(self.notes.color, true),
            (Target::Highlight(id), PopupItem::Note) => self.open_editor(id),
            (Target::Selection, PopupItem::Copy) => {
                if let Some(pdf) = &self.pdf {
                    let document = pdf.document().id;
                    update_inner(
                        self,
                        Message::Pdf {
                            document,
                            message: pdf_reader::Message::Copy,
                        },
                    )
                } else {
                    update_inner(self, Message::Copy)
                }
            }
            (Target::Highlight(id), PopupItem::Copy) => self
                .notes
                .data
                .as_ref()
                .and_then(|data| data.highlight(id))
                .map(|highlight| highlight.quote.clone())
                .map_or_else(Task::none, iced::clipboard::write),
            (Target::Selection, PopupItem::ReadAloud) => {
                self.read_aloud(read_aloud::Action::Selection)
            }
            (Target::Highlight(id), PopupItem::ReadAloud) => {
                match self
                    .notes
                    .data
                    .as_ref()
                    .and_then(|data| data.highlight(id))
                    .map(|highlight| highlight.quote.clone())
                {
                    Some(quote) => self.read_passage_aloud(quote),
                    None => Task::none(),
                }
            }
            (Target::Highlight(id), PopupItem::RemoveHighlight) => {
                self.remove_entry(Entry::Highlight(id))
            }
            (_, PopupItem::ToggleBookmark) => self.toggle_bookmark(),
            _ => Task::none(),
        }
    }

    pub(super) fn notes_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::ToggleBookmark => self.toggle_bookmark(),
            Action::ToggleList => {
                self.notes.list = if self.notes.list.is_some() {
                    None
                } else {
                    Some(ListTab::default())
                };
                self.focused = None;
                let size = self.document_view_size();
                match &mut self.pdf {
                    Some(pdf) => {
                        forward_pdf(pdf.document().id, pdf.resize(size, self.scale_factor))
                    }
                    None => Task::none(),
                }
            }
            Action::ToggleNote(id) => {
                self.notes.expanded_note = if self.notes.expanded_note == Some(id) {
                    None
                } else if self
                    .notes
                    .data
                    .as_ref()
                    .and_then(|data| data.highlight(id))
                    .and_then(|highlight| highlight.note.as_ref())
                    .is_some()
                {
                    Some(id)
                } else {
                    None
                };
                Task::none()
            }
            Action::Popup(item) => {
                let Some(popup) = self.notes.popup.take() else {
                    return Task::none();
                };
                self.focused = None;
                self.popup_item(popup.target, item)
            }
            Action::EditNote(id) => self.open_editor(id),
            Action::Editing(edit) => {
                if let Some(editor) = &mut self.notes.editor {
                    editor.content.perform(edit);
                }
                Task::none()
            }
            Action::SaveNote | Action::DeleteNote => {
                let Some(editor) = self.notes.editor.as_ref() else {
                    return Task::none();
                };
                let text = if matches!(action, Action::SaveNote) {
                    editor.content.text()
                } else {
                    String::new()
                };
                if text.trim().len() > annotations::MAX_NOTE_BYTES {
                    self.error = Some(format!(
                        "A note can hold up to {} bytes",
                        annotations::MAX_NOTE_BYTES
                    ));
                    return Task::none();
                }
                let id = editor.id;
                if self.change_notes(|data| data.set_note(id, &text)).is_none() {
                    return Task::none();
                }
                self.notes.editor = None;
                self.persist_notes()
            }
            Action::CancelNote => {
                self.notes.editor = None;
                Task::none()
            }
            Action::Tab(tab) => {
                self.notes.list = Some(tab);
                Task::none()
            }
            Action::Go(entry) => self.go_to(entry),
            Action::Remove(entry) => self.remove_entry(entry),
        }
    }

    // --------------------------------------------------------- navigation

    /// Turns to a global page of the page map, loading its chapter if needed.
    pub(super) fn go_to_atlas_page(&mut self, section: usize, page: usize) -> Task<Message> {
        let Some(book) = &self.book else {
            return Task::none();
        };
        if book.epub.as_ref().is_some_and(|c| c.index != section) {
            let fingerprint = book.fingerprint.clone();
            let task = self.navigate(Some(section), None, None, Navigation::Preserve);
            if self.opening.is_some() {
                self.pending_page = Some((fingerprint, section, page));
            }
            return task;
        }
        self.go_to_local_page(page)
    }

    fn go_to_page_number(&mut self, number: u32) -> Option<Task<Message>> {
        let atlas = self.atlas.as_ref()?;
        let (section, page) = atlas
            .sections
            .iter()
            .enumerate()
            .find_map(|(section, layout)| {
                layout
                    .pages
                    .iter()
                    .position(|page| page.number == number)
                    .map(|page| (section, page))
            })?;
        Some(self.go_to_atlas_page(section, page))
    }

    /// The chapter index of a saved chapter href in the open EPUB.
    fn chapter_index(&self, href: Option<&str>) -> Option<usize> {
        let chapter = self.book.as_ref()?.epub.as_ref()?;
        chapter.document.section_index(href?)
    }

    fn go_to(&mut self, entry: Entry) -> Task<Message> {
        if !self.interactive() {
            return Task::none();
        }
        let Some(data) = &self.notes.data else {
            return Task::none();
        };
        match entry {
            Entry::Bookmark(id) => match data.bookmark(id).map(|bookmark| bookmark.place.clone()) {
                Some(BookmarkPlace::Pdf { page, .. }) => match &mut self.pdf {
                    Some(pdf) => {
                        let document = pdf.document().id;
                        forward_pdf(document, pdf.go_to_page(page as usize))
                    }
                    None => Task::none(),
                },
                Some(BookmarkPlace::Reflow {
                    chapter,
                    item_id,
                    within,
                    page_number,
                }) => {
                    // The page map is the truth for page numbers; the saved item is the fallback.
                    if let Some(task) = self.go_to_page_number(page_number) {
                        return task;
                    }
                    let location = ReturnLocation {
                        chapter: self.chapter_index(chapter.as_deref()),
                        item_id,
                        within,
                    };
                    self.navigate(location.chapter, None, Some(location), Navigation::Preserve)
                }
                None => Task::none(),
            },
            Entry::Highlight(id) => {
                let Some(highlight) = data.highlight(id).cloned() else {
                    return Task::none();
                };
                match highlight.place {
                    Place::Pdf { from, to } => match &mut self.pdf {
                        Some(pdf) => {
                            let document = pdf.document().id;
                            forward_pdf(
                                document,
                                pdf.reveal(
                                    from.page,
                                    from.index,
                                    if from.page == to.page {
                                        to.index
                                    } else {
                                        from.index
                                    },
                                ),
                            )
                        }
                        None => Task::none(),
                    },
                    Place::Reflow { chapter, .. } => {
                        if let Some(mark) = self.notes.marks.iter().find(|mark| mark.id == id) {
                            let row = mark.bounds.start_item;
                            let length = self
                                .book
                                .as_ref()
                                .and_then(|book| book.items.get(row))
                                .and_then(Item::text)
                                .map_or(1, |text| text.len().max(1));
                            let anchor = Anchor {
                                row,
                                fraction: (mark.bounds.start_byte as f32 / length as f32)
                                    .clamp(0.0, 1.0),
                            };
                            if self.pagination.is_some() || self.pages().is_empty() {
                                return self.rebuild_geometry(anchor);
                            }
                            return self.reveal_anchor(anchor);
                        }
                        if chapter == self.chapter_href() {
                            self.error =
                                Some("This passage was not found in the book any more.".into());
                            return Task::none();
                        }
                        let Some(index) = self.chapter_index(chapter.as_deref()) else {
                            self.error =
                                Some("This passage's chapter is no longer in the book.".into());
                            return Task::none();
                        };
                        self.notes.pending_reveal = Some(id);
                        self.navigate(Some(index), None, None, Navigation::Preserve)
                    }
                }
            }
        }
    }

    // --------------------------------------------------------------- list

    fn chapter_rank(&self, chapter: Option<&str>) -> usize {
        self.chapter_index(chapter).unwrap_or(0)
    }

    /// The rows of the open list tab, in reading order.
    pub(super) fn list_entries(&self) -> Vec<Entry> {
        let (Some(tab), Some(data)) = (self.notes.list, &self.notes.data) else {
            return Vec::new();
        };
        let pdf = self.pdf.is_some();
        match tab {
            ListTab::Bookmarks => {
                let mut rows: Vec<_> = data
                    .bookmarks
                    .iter()
                    .filter(|bookmark| matches!(bookmark.place, BookmarkPlace::Pdf { .. }) == pdf)
                    .collect();
                rows.sort_by_key(|bookmark| match &bookmark.place {
                    BookmarkPlace::Reflow { page_number, .. } => *page_number,
                    BookmarkPlace::Pdf { page, .. } => *page,
                });
                rows.into_iter()
                    .take(MAX_LIST_ROWS)
                    .map(|bookmark| Entry::Bookmark(bookmark.id))
                    .collect()
            }
            ListTab::Highlights => {
                let mut rows: Vec<_> = data
                    .highlights
                    .iter()
                    .filter(|highlight| matches!(highlight.place, Place::Pdf { .. }) == pdf)
                    .collect();
                rows.sort_by_cached_key(|highlight| match &highlight.place {
                    Place::Reflow { chapter, from, .. } => (
                        self.chapter_rank(chapter.as_deref()),
                        from.item_id.clone(),
                        from.byte,
                    ),
                    Place::Pdf { from, .. } => (from.page as usize, String::new(), from.index),
                });
                rows.into_iter()
                    .take(MAX_LIST_ROWS)
                    .map(|highlight| Entry::Highlight(highlight.id))
                    .collect()
            }
        }
    }

    /// The keyboard-focusable note controls that are on screen.
    pub(super) fn notes_controls(&self) -> Vec<Control> {
        let mut controls: Vec<Control> = vec![Control::Notes(Focus::ToggleList)];
        controls.extend(
            self.notes
                .popup
                .iter()
                .flat_map(Popup::entries)
                .map(|item| Control::Notes(Focus::Popup(item))),
        );
        if self.notes.list.is_some() {
            controls.push(Control::Notes(Focus::Tab(ListTab::Bookmarks)));
            controls.push(Control::Notes(Focus::Tab(ListTab::Highlights)));
            for entry in self.list_entries() {
                controls.push(Control::Notes(Focus::Go(entry)));
                if let Entry::Highlight(id) = entry {
                    if self
                        .notes
                        .data
                        .as_ref()
                        .and_then(|data| data.highlight(id))
                        .and_then(|highlight| highlight.note.as_ref())
                        .is_some()
                    {
                        controls.push(Control::Notes(Focus::ToggleNote(id)));
                    }
                    controls.push(Control::Notes(Focus::Edit(id)));
                }
                controls.push(Control::Notes(Focus::Remove(entry)));
            }
        }
        controls
    }

    /// Up and Down move through the open menu.
    pub(super) fn step_popup_focus(&mut self, forward: bool) {
        let Some(popup) = &self.notes.popup else {
            return;
        };
        let entries = popup.entries();
        if entries.is_empty() {
            return;
        }
        let current = self.focused.and_then(|control| match control {
            Control::Notes(Focus::Popup(item)) => entries.iter().position(|entry| *entry == item),
            _ => None,
        });
        let next = match (current, forward) {
            (None, true) => 0,
            (None, false) => entries.len() - 1,
            (Some(index), true) => (index + 1) % entries.len(),
            (Some(index), false) => (index + entries.len() - 1) % entries.len(),
        };
        self.focused = Some(Control::Notes(Focus::Popup(entries[next])));
    }
}

// ------------------------------------------------------------------- views

fn menu_button<'a>(
    reader: &Reader,
    item: PopupItem,
    label: &'static str,
    hint: &'static str,
    tone: ui::ButtonTone,
) -> Element<'a, Message> {
    let focused = reader.focused == Some(Control::Notes(Focus::Popup(item)));
    let button = iced::widget::button(
        row![
            text(label).size(13),
            Space::new().width(Length::Fill),
            text(hint).size(11).style(ui::muted_text),
        ]
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .height(notes::POPUP_ROW)
    .padding([0, 10])
    .on_press(Message::Notes(Action::Popup(item)))
    .style(move |theme, status| ui::button_style(theme, status, tone, focused, false));
    if focused {
        container(button)
            .id(iced::advanced::widget::Id::new(ui::FOCUSED_CONTROL))
            .into()
    } else {
        button.into()
    }
}

fn swatch<'a>(reader: &Reader, color: HighlightColor) -> Element<'a, Message> {
    let item = PopupItem::Color(color);
    let focused = reader.focused == Some(Control::Notes(Focus::Popup(item)));
    let chosen = reader.notes.color == color;
    let button = iced::widget::button(Space::new().width(18).height(18))
        .padding(4)
        .on_press(Message::Notes(Action::Popup(item)))
        .style(move |theme, status| {
            let hovered = matches!(status, iced::widget::button::Status::Hovered);
            iced::widget::button::Style {
                background: Some(notes::solid(color).into()),
                border: iced::Border {
                    color: if focused || chosen || hovered {
                        ui::palette(theme).accent
                    } else {
                        iced::Color::TRANSPARENT
                    },
                    width: if focused { 2.0 } else { 1.5 },
                    radius: 12.0.into(),
                },
                ..Default::default()
            }
        });
    let button: Element<'a, Message> = tooltip(
        button,
        text(format!("Highlight {}", color.label().to_lowercase())).size(12),
        tooltip::Position::Bottom,
    )
    .gap(4)
    .padding(6)
    .style(ui::panel)
    .into();
    if focused {
        container(button)
            .id(iced::advanced::widget::Id::new(ui::FOCUSED_CONTROL))
            .into()
    } else {
        button
    }
}

/// The floating menu next to the pointer, above everything but modals.
pub(super) fn popup_layer(reader: &Reader) -> Option<Element<'_, Message>> {
    let popup = reader.notes.popup.as_ref()?;
    let entries = popup.entries();
    let origin = popup.origin(reader.window_size);
    let mut menu = column![].spacing(notes::POPUP_SPACING);
    let (after_colors, before_removal) = notes::Popup::separated(&entries);
    let separator = || {
        container(
            container(Space::new().height(1))
                .width(Length::Fill)
                .style(ui::rule),
        )
        .height(notes::POPUP_SEPARATOR)
        .padding([0, 6])
        .center_y(notes::POPUP_SEPARATOR)
    };
    let swatches: Vec<_> = entries
        .iter()
        .filter_map(|entry| match entry {
            PopupItem::Color(color) => Some(swatch(reader, *color)),
            _ => None,
        })
        .collect();
    if !swatches.is_empty() {
        menu = menu.push(
            container(row(swatches).spacing(10).align_y(iced::Alignment::Center))
                .height(notes::POPUP_ROW)
                .center_y(notes::POPUP_ROW)
                .padding([0, 8]),
        );
        if after_colors {
            menu = menu.push(separator());
        }
    }
    let has_note = match popup.target {
        Target::Highlight(id) => reader
            .notes
            .data
            .as_ref()
            .and_then(|data| data.highlight(id))
            .is_some_and(|highlight| highlight.note.is_some()),
        _ => false,
    };
    let bookmarked = if reader.pdf.is_some() {
        reader.pdf.as_ref().is_some_and(|pdf| {
            reader
                .notes
                .is_bookmarked_pdf(pdf.page_index() as u32)
                .is_some()
        })
    } else {
        reader.page_is_bookmarked()
    };
    for entry in entries {
        let button = match entry {
            PopupItem::Color(_) => None,
            PopupItem::Note => Some(menu_button(
                reader,
                entry,
                if has_note { "Edit note" } else { "Add note" },
                "",
                ui::ButtonTone::Subtle,
            )),
            PopupItem::Copy => Some(menu_button(
                reader,
                entry,
                "Copy",
                "Ctrl+C",
                ui::ButtonTone::Subtle,
            )),
            PopupItem::ReadAloud => Some(menu_button(
                reader,
                entry,
                "Read aloud",
                "Ctrl+Shift+U",
                ui::ButtonTone::Subtle,
            )),
            PopupItem::Translate => Some(menu_button(
                reader,
                entry,
                "Translate",
                "",
                ui::ButtonTone::Subtle,
            )),
            PopupItem::RemoveHighlight => Some(menu_button(
                reader,
                entry,
                "Remove highlight",
                "",
                ui::ButtonTone::Destructive,
            )),
            PopupItem::ToggleBookmark => Some(menu_button(
                reader,
                entry,
                if bookmarked {
                    "Remove bookmark"
                } else {
                    "Add bookmark"
                },
                "Ctrl+D",
                ui::ButtonTone::Subtle,
            )),
        };
        if let Some(button) = button {
            if before_removal && entry == PopupItem::RemoveHighlight {
                menu = menu.push(separator());
            }
            menu = menu.push(button);
        }
    }
    let panel = container(menu)
        .padding(notes::POPUP_PADDING)
        .width(notes::POPUP_WIDTH)
        .style(ui::panel);
    Some(
        container(opaque(panel))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(iced::Padding {
                left: origin.x,
                top: origin.y,
                ..iced::Padding::ZERO
            })
            .align_x(iced::alignment::Horizontal::Left)
            .align_y(iced::alignment::Vertical::Top)
            .into(),
    )
}

fn note_editor_style(theme: &iced::Theme, status: text_editor::Status) -> text_editor::Style {
    let palette = ui::palette(theme);
    let focused = matches!(status, text_editor::Status::Focused { .. });
    text_editor::Style {
        background: palette.raised.into(),
        border: iced::Border {
            color: if focused {
                palette.accent
            } else {
                palette.control_border
            },
            width: if focused { 2.0 } else { 1.0 },
            radius: 4.0.into(),
        },
        placeholder: palette.muted,
        value: palette.text,
        selection: palette.accent.scale_alpha(0.25),
    }
}

fn flat_button<'a>(
    label: &'static str,
    message: Message,
    tone: ui::ButtonTone,
) -> Element<'a, Message> {
    iced::widget::button(text(label).font(ui::MEDIUM).size(13))
        .padding([7, 12])
        .on_press(message)
        .style(move |theme, status| ui::button_style(theme, status, tone, false, false))
        .into()
}

/// The modal note editor, drawn like the other modal panels.
pub(super) fn editor_overlay<'a>(
    reader: &'a Reader,
    base: Element<'a, Message>,
) -> Element<'a, Message> {
    let Some(editor) = &reader.notes.editor else {
        return iced::widget::stack![base].into();
    };
    let quote = reader
        .notes
        .data
        .as_ref()
        .and_then(|data| data.highlight(editor.id))
        .map(|highlight| notes::preview(&highlight.quote, 220))
        .unwrap_or_default();
    let mut buttons = row![].spacing(8).align_y(iced::Alignment::Center);
    if editor.existing {
        buttons = buttons.push(flat_button(
            "Delete note",
            Message::Notes(Action::DeleteNote),
            ui::ButtonTone::Destructive,
        ));
    }
    buttons = buttons
        .push(Space::new().width(Length::Fill))
        .push(flat_button(
            "Cancel",
            Message::Notes(Action::CancelNote),
            ui::ButtonTone::Surface,
        ))
        .push(flat_button(
            "Save",
            Message::Notes(Action::SaveNote),
            ui::ButtonTone::Quiet,
        ));
    let length = editor.content.text().trim().len();
    let footer = if length * 5 >= annotations::MAX_NOTE_BYTES * 4 {
        format!(
            "{length} / {} bytes · Ctrl+Enter Save · Esc Cancel",
            annotations::MAX_NOTE_BYTES
        )
    } else {
        "Ctrl+Enter Save · Esc Cancel".to_owned()
    };
    let contents = column![
        text("Note").font(ui::HEADING).size(22),
        quotation(quote, None),
        text_editor(&editor.content)
            .id(note_editor_id())
            .placeholder("Write a note…")
            .on_action(|action| Message::Notes(Action::Editing(action)))
            .font(ui::SANS)
            .size(14)
            .wrapping(text::Wrapping::WordOrGlyph)
            .padding(10)
            .height(150)
            .style(note_editor_style),
        buttons,
        text(footer)
            .size(11)
            .style(if length > annotations::MAX_NOTE_BYTES {
                ui::danger_text
            } else {
                ui::muted_text
            }),
    ]
    .spacing(14);
    let panel = container(contents)
        .padding(24)
        .width((reader.window_size.width - 32.0).clamp(240.0, 560.0))
        .style(ui::panel);
    let backdrop = mouse_area(
        container(Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(iced::Color::BLACK.scale_alpha(0.55).into()),
                ..container::Style::default()
            }),
    );
    iced::widget::stack![
        base,
        backdrop,
        container(opaque(panel))
            .center_x(Length::Fill)
            .center_y(Length::Fill)
    ]
    .into()
}

/// A quoted passage: italic serif on a faint wash of the highlight (or accent) color.
fn quotation<'a>(quote: String, tint: Option<iced::Color>) -> Element<'a, Message> {
    container(
        text(quote)
            .font(ui::SERIF_ITALIC)
            .size(14)
            .line_height(1.45)
            .style(ui::primary_text)
            .shaping(text::Shaping::Advanced)
            .wrapping(text::Wrapping::WordOrGlyph)
            .width(Length::Fill),
    )
    .padding([6, 10])
    .width(Length::Fill)
    .style(move |theme| {
        let palette = ui::palette(theme);
        let color = tint.unwrap_or(palette.accent);
        container::Style {
            background: Some(ui::mix(palette.background, color, 0.14).into()),
            border: iced::Border {
                color: ui::mix(palette.background, color, 0.45),
                width: 1.0,
                radius: 4.0.into(),
            },
            ..container::Style::default()
        }
    })
    .into()
}

/// One bookmark or highlight in the drawer, set apart as its own quiet card.
fn entry_card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(8)
        .width(Length::Fill)
        .style(|theme| {
            let palette = ui::palette(theme);
            container::Style {
                background: Some(palette.background.into()),
                border: iced::Border {
                    color: palette.border.scale_alpha(0.6),
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..container::Style::default()
            }
        })
        .into()
}

/// A red ribbon at the top corner of a bookmarked page; clicking it removes the bookmark.
pub(super) fn ribbon<'a>(reader: &Reader, paper_width: f32) -> Element<'a, Message> {
    if !reader.page_is_bookmarked() {
        return Space::new().width(0).height(0).into();
    }
    let shape = container(Space::new().width(20).height(36)).style(|_| container::Style {
        background: Some(RIBBON.into()),
        border: iced::Border {
            radius: iced::border::Radius {
                top_left: 0.0,
                top_right: 0.0,
                bottom_right: 6.0,
                bottom_left: 6.0,
            },
            ..iced::Border::default()
        },
        ..container::Style::default()
    });
    container(
        tooltip(
            mouse_area(shape).on_press(Message::Notes(Action::ToggleBookmark)),
            text("Bookmarked. Click to remove.").size(12),
            tooltip::Position::Bottom,
        )
        .gap(4)
        .padding(6)
        .style(ui::panel),
    )
    .width(paper_width)
    .align_x(iced::alignment::Horizontal::Right)
    .padding(iced::Padding {
        right: 40.0,
        ..iced::Padding::ZERO
    })
    .into()
}

/// The reader-side drawer. Its edge toggle remains visible when the drawer is closed.
pub(super) fn sidebar(reader: &Reader, active: bool) -> Element<'_, Message> {
    let open = reader.notes.list.is_some();
    let focus = reader.focused == Some(Control::Notes(Focus::ToggleList));
    let toggle = iced::widget::button(
        container(text(if open { "›" } else { "‹" }).size(20).line_height(1.0)).center(20),
    )
    .padding(0)
    .width(20)
    .height(56)
    .on_press_maybe(active.then_some(Message::Notes(Action::ToggleList)))
    .style(move |theme, status| {
        let palette = ui::palette(theme);
        let hovered = matches!(
            status,
            iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
        );
        iced::widget::button::Style {
            text_color: if focus || hovered || open {
                palette.accent
            } else {
                palette.secondary
            },
            background: Some(
                if hovered {
                    palette.raised
                } else {
                    palette.surface
                }
                .into(),
            ),
            border: iced::Border {
                color: if focus {
                    palette.accent
                } else {
                    palette.border
                },
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        }
    });
    let toggle: Element<'_, Message> = tooltip(
        toggle,
        text(if open {
            "Hide bookmarks and notes"
        } else {
            "Show bookmarks and notes"
        })
        .size(12),
        tooltip::Position::Left,
    )
    .into();
    let edge = container(toggle)
        .width(26)
        .height(Length::Fill)
        .center_x(26)
        .center_y(Length::Fill);
    match reader.notes.list {
        Some(tab) => row![edge, list_panel(reader, active, tab)]
            .height(Length::Fill)
            .into(),
        None => edge.into(),
    }
}

fn list_panel(reader: &Reader, active: bool, tab: ListTab) -> Element<'_, Message> {
    let data = reader.notes.data.as_ref();
    let counts = |bookmarks: bool| {
        let pdf = reader.pdf.is_some();
        data.map_or(0, |data| {
            if bookmarks {
                data.bookmarks
                    .iter()
                    .filter(|bookmark| matches!(bookmark.place, BookmarkPlace::Pdf { .. }) == pdf)
                    .count()
            } else {
                data.highlights
                    .iter()
                    .filter(|highlight| matches!(highlight.place, Place::Pdf { .. }) == pdf)
                    .count()
            }
        })
    };
    let tab_button = |which: ListTab, label: String| {
        toned_button(
            reader,
            Control::Notes(Focus::Tab(which)),
            text(label).font(ui::MEDIUM).size(13),
            active.then_some(Message::Notes(Action::Tab(which))),
            ui::ButtonTone::Surface,
            tab == which,
        )
    };
    let header = column![
        text("Bookmarks and notes").size(18).font(ui::HEADING),
        row![
            tab_button(ListTab::Bookmarks, format!("Bookmarks ({})", counts(true))),
            tab_button(
                ListTab::Highlights,
                format!("Highlights ({})", counts(false))
            ),
        ]
        .spacing(6),
    ]
    .spacing(12);
    let entries = reader.list_entries();
    let mut rows = column![].spacing(6);
    for entry in &entries {
        let Some(row) = list_row(reader, *entry, active) else {
            continue;
        };
        rows = rows.push(row);
    }
    if entries.is_empty() {
        rows = rows.push(
            text(match (data, tab) {
                (None, _) => "Loading…",
                (Some(_), ListTab::Bookmarks) => {
                    "No bookmarks yet. Press Ctrl+D, or right-click a page and choose Add bookmark."
                }
                (Some(_), ListTab::Highlights) => {
                    "No highlights yet. Select some text to highlight it or to add a note."
                }
            })
            .size(13)
            .style(ui::muted_text),
        );
    }
    container(
        column![
            header,
            scrollable(container(rows).padding(iced::Padding {
                right: 8.0,
                ..iced::Padding::ZERO
            }))
            .direction(ui::vertical_scrollbar())
            .style(ui::scroll_style)
            .height(Length::Fill),
        ]
        .spacing(12),
    )
    .padding(14)
    .width((reader.window_size.width * 0.36).clamp(230.0, 340.0))
    .height(Length::Fill)
    .style(ui::panel)
    .into()
}

fn list_row(reader: &Reader, entry: Entry, active: bool) -> Option<Element<'_, Message>> {
    let data = reader.notes.data.as_ref()?;
    let remove = control_button(
        reader,
        Control::Notes(Focus::Remove(entry)),
        text("Remove").font(ui::MEDIUM).size(12),
        active.then_some(Message::Notes(Action::Remove(entry))),
    );
    fn go<'a>(
        reader: &Reader,
        entry: Entry,
        active: bool,
        label: Element<'a, Message>,
    ) -> Element<'a, Message> {
        container(toned_button(
            reader,
            Control::Notes(Focus::Go(entry)),
            label,
            active.then_some(Message::Notes(Action::Go(entry))),
            ui::ButtonTone::Subtle,
            false,
        ))
        .width(Length::Fill)
        .into()
    }
    match entry {
        Entry::Bookmark(id) => {
            let bookmark = data.bookmark(id)?;
            let label = column![
                text(format!("Page {}", bookmark.page))
                    .size(11)
                    .font(ui::MEDIUM)
                    .style(ui::muted_text),
                text(bookmark.excerpt.clone())
                    .size(13)
                    .style(ui::secondary_text)
                    .shaping(text::Shaping::Advanced)
                    .wrapping(text::Wrapping::WordOrGlyph),
            ]
            .spacing(3);
            Some(entry_card(
                column![
                    go(reader, entry, active, label.into()),
                    row![Space::new().width(Length::Fill), remove]
                ]
                .spacing(6),
            ))
        }
        Entry::Highlight(id) => {
            let highlight = data.highlight(id)?;
            let label = column![
                text(format!("Page {}", highlight.page))
                    .size(11)
                    .font(ui::MEDIUM)
                    .style(ui::muted_text),
                quotation(
                    notes::preview(&highlight.quote, 160),
                    Some(notes::solid(highlight.color))
                ),
            ]
            .spacing(6);
            let edit = control_button(
                reader,
                Control::Notes(Focus::Edit(id)),
                text(if highlight.note.is_some() {
                    "Edit note"
                } else {
                    "Add note"
                })
                .font(ui::MEDIUM)
                .size(12),
                active.then_some(Message::Notes(Action::EditNote(id))),
            );
            let mut details = column![go(reader, entry, active, label.into())].spacing(6);
            if let Some(note) = &highlight.note {
                let expanded = reader.notes.expanded_note == Some(id);
                let note_label = column![
                    text(notes::preview(note, 160))
                        .size(12)
                        .style(ui::secondary_text)
                        .shaping(text::Shaping::Advanced)
                        .wrapping(text::Wrapping::WordOrGlyph),
                    text(if expanded {
                        "Hide full note ↑"
                    } else {
                        "Read full note ↓"
                    })
                    .size(11)
                    .style(ui::muted_text),
                ]
                .spacing(4)
                .width(Length::Fill);
                details = details.push(
                    container(toned_button(
                        reader,
                        Control::Notes(Focus::ToggleNote(id)),
                        note_label,
                        active.then_some(Message::Notes(Action::ToggleNote(id))),
                        ui::ButtonTone::Subtle,
                        expanded,
                    ))
                    .width(Length::Fill),
                );
                if expanded {
                    let full: Element<'_, Message> = container(
                        text(note.clone())
                            .size(13)
                            .shaping(text::Shaping::Advanced)
                            .wrapping(text::Wrapping::WordOrGlyph)
                            .width(Length::Fill),
                    )
                    .padding(10)
                    .width(Length::Fill)
                    .style(ui::inset)
                    .into();
                    let full: Element<'_, Message> =
                        if note.chars().count() > 240 || note.lines().count() > 6 {
                            iced::widget::scrollable(full)
                                .direction(ui::vertical_scrollbar())
                                .style(ui::scroll_style)
                                .height(220)
                                .into()
                        } else {
                            full
                        };
                    details = details.push(full);
                }
            }
            Some(entry_card(
                column![
                    details,
                    row![Space::new().width(Length::Fill), edit, remove].spacing(6),
                ]
                .spacing(6),
            ))
        }
    }
}
