//! Backup/restore and annotation export. Disk work runs in tasks; profile writes
//! are allowed only after the reader's existing save queues have settled.
use super::*;
use iced::widget::{checkbox, column};
use reader_document::backup::{self, ExportFormat, Options, Summary};

#[derive(Clone, Debug)]
pub(super) enum Action {
    Documents(bool),
    Dictionaries(bool),
    Create,
    Restore,
    Confirm,
    Cancel,
    Format(ExportFormat),
    Export,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Focus {
    Documents,
    Dictionaries,
    Create,
    Restore,
    Confirm,
    Cancel,
    Export,
    Format(ExportFormat),
}

#[derive(Debug)]
pub(super) struct State {
    /// Identifies profile-backed read tasks across a restore.
    pub epoch: u64,
    pub busy: bool,
    pub notice: Option<String>,
    pub options: Options,
    pub estimate: Option<Summary>,
    pub pending: Option<(PathBuf, Summary)>,
    pub format: ExportFormat,
    generation: u64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            epoch: 0,
            busy: false,
            notice: None,
            options: Options {
                documents: true,
                dictionaries: false,
            },
            estimate: None,
            pending: None,
            format: ExportFormat::Markdown,
            generation: 0,
        }
    }
}

impl Reader {
    pub(super) fn profile_ready(&self) -> bool {
        self.book.is_none()
            && self.pdf.is_none()
            && self.opening.is_none()
            && self.interactive()
            && self.exit_ready()
            && !self.word_translation.packages_busy()
            && self.shelf.profile_loaded()
    }

    pub(super) fn estimate_backup(&mut self) -> Task<Message> {
        self.profile.generation = self.profile.generation.wrapping_add(1);
        let generation = self.profile.generation;
        let options = self.profile.options;
        Task::perform(async move { backup::estimate(options) }, move |result| {
            Message::BackupEstimate { generation, result }
        })
    }

    pub(super) fn profile_estimated(&mut self, generation: u64, result: Result<Summary, String>) {
        if generation == self.profile.generation {
            match result {
                Ok(summary) => self.profile.estimate = Some(summary),
                Err(error) => {
                    self.profile.estimate = None;
                    self.profile.notice = Some(error);
                }
            }
        }
    }

    pub(super) fn profile_action(&mut self, action: Action) -> Task<Message> {
        if self.profile.busy {
            return Task::none();
        }
        match action {
            Action::Documents(value) => {
                self.profile.options.documents = value;
                self.estimate_backup()
            }
            Action::Dictionaries(value) => {
                self.profile.options.dictionaries = value;
                self.estimate_backup()
            }
            Action::Format(format) => {
                self.profile.format = format;
                Task::none()
            }
            Action::Cancel => {
                self.profile.pending = None;
                Task::none()
            }
            Action::Create | Action::Restore if self.profile_ready() => {
                self.profile.busy = true;
                self.profile.notice = Some("Preparing and verifying the chosen backup…".into());
                let save = matches!(action, Action::Create);
                let options = self.profile.options;
                Task::perform(
                    async move {
                        let path = crate::platform::transfer_dialog(save, None)?;
                        if let Some(path) = path {
                            if save {
                                backup::create(&path, options)?;
                                Ok(Reply::Created(path))
                            } else {
                                let summary = backup::inspect(&path)?;
                                Ok(Reply::Inspected(path, summary))
                            }
                        } else {
                            Ok(Reply::Cancelled)
                        }
                    },
                    Message::ProfileReply,
                )
            }
            Action::Confirm if self.profile_ready() => {
                let Some((path, _)) = self.profile.pending.take() else {
                    return Task::none();
                };
                self.profile.busy = true;
                self.profile.notice = Some("Restoring verified backup…".into());
                Task::perform(
                    async move { backup::restore(&path).map(Reply::Restored) },
                    Message::ProfileReply,
                )
            }
            Action::Export => {
                let Some(notes) = self.notes.data.clone() else {
                    return Task::none();
                };
                let Some(title) = self
                    .book
                    .as_ref()
                    .map(|b| b.title.clone())
                    .or_else(|| self.pdf.as_ref().map(|p| p.document().title.clone()))
                else {
                    return Task::none();
                };
                if self.notes.editor.is_some() {
                    return Task::none();
                }
                let format = self.profile.format;
                self.profile.busy = true;
                self.profile.notice = Some("Exporting reading notes…".into());
                Task::perform(
                    async move {
                        let Some(path) = crate::platform::transfer_dialog(true, Some(format))?
                        else {
                            return Ok(Reply::Cancelled);
                        };
                        backup::export_annotations(&path, &title, &notes, format)?;
                        Ok(Reply::Exported(path))
                    },
                    Message::ProfileReply,
                )
            }
            _ => Task::none(),
        }
    }

    pub(super) fn profile_reply(&mut self, result: Result<Reply, String>) -> Task<Message> {
        self.profile.busy = false;
        let mut tasks = Vec::new();
        match result {
            Ok(Reply::Cancelled) => self.profile.notice = None,
            Ok(Reply::Created(path) | Reply::Exported(path)) => {
                self.profile.notice = Some(format!("Saved to {}", path.display()))
            }
            Ok(Reply::Inspected(path, summary)) => {
                self.profile.notice = None;
                self.profile.pending = Some((path, summary));
            }
            Ok(Reply::Restored(previous)) => {
                self.profile.epoch = self.profile.epoch.wrapping_add(1);
                self.profile.notice = Some(if previous.as_os_str().is_empty() {
                    "Backup restored.".into()
                } else {
                    format!(
                        "Backup restored. Previous profile kept at {}",
                        previous.display()
                    )
                });
                self.preferences_dirty = false;
                self.preferences_writable = true;
                self.preferences_loading = true;
                self.history_corrupt = false;
                self.recent_dirty = false;
                self.recent_loading = true;
                self.recent.clear();
                self.shelf = shelf::Shelf::default();
                self.notes = notes::Notes::default();
                self.word_translation.reload_profile();
                tasks.push(Task::perform(
                    async { preferences::load() },
                    Message::PreferencesLoaded,
                ));
                tasks.push(Task::perform(
                    async { recent::load() },
                    Message::RecentLoaded,
                ));
                tasks.push(shelf::Shelf::load().map(Message::Shelf));
                tasks.push(self.refresh_dictionaries());
                tasks.push(self.estimate_backup());
            }
            Err(error) => self.profile.notice = Some(error),
        }
        if self.pending_exit && self.exit_ready() {
            iced::exit()
        } else {
            Task::batch(tasks)
        }
    }

    pub(super) fn profile_focus(&mut self, focus: Focus) -> Task<Message> {
        self.profile_action(match focus {
            Focus::Documents => Action::Documents(!self.profile.options.documents),
            Focus::Dictionaries => Action::Dictionaries(!self.profile.options.dictionaries),
            Focus::Create => Action::Create,
            Focus::Restore => Action::Restore,
            Focus::Confirm => Action::Confirm,
            Focus::Cancel => Action::Cancel,
            Focus::Export => Action::Export,
            Focus::Format(format) => Action::Format(format),
        })
    }
}

#[derive(Clone, Debug)]
pub(super) enum Reply {
    Cancelled,
    Created(PathBuf),
    Inspected(PathBuf, Summary),
    Restored(PathBuf),
    Exported(PathBuf),
}

pub(super) fn controls(reader: &Reader) -> impl Iterator<Item = Control> + Clone + '_ {
    [
        (!reader.profile.busy).then_some(Focus::Documents),
        (!reader.profile.busy).then_some(Focus::Dictionaries),
        reader.profile_ready().then_some(Focus::Create),
        reader.profile_ready().then_some(Focus::Restore),
        (reader.profile.pending.is_some() && reader.profile_ready()).then_some(Focus::Confirm),
        (reader.profile.pending.is_some() && !reader.profile.busy).then_some(Focus::Cancel),
        (!reader.profile.busy
            && reader.notes.data.is_some()
            && reader.reading_title().is_some()
            && reader.notes.editor.is_none())
        .then_some(Focus::Export),
    ]
    .into_iter()
    .flatten()
    .chain(
        [
            ExportFormat::Markdown,
            ExportFormat::Text,
            ExportFormat::Json,
        ]
        .into_iter()
        .filter(move |_| !reader.profile.busy)
        .map(Focus::Format),
    )
    .map(Control::Profile)
}

pub(super) fn settings(reader: &Reader) -> Element<'_, Message> {
    let action = |action| Message::Profile(action);
    let options = reader.profile.options;
    let mut content = column![
        text("Library backup").font(ui::MEDIUM).size(13),
        backup_checkbox(
            reader,
            Focus::Documents,
            "Include book files",
            options.documents
        ),
        backup_checkbox(
            reader,
            Focus::Dictionaries,
            "Include downloaded dictionaries",
            options.dictionaries
        ),
    ]
    .spacing(8);
    if let Some(summary) = &reader.profile.estimate {
        content = content.push(
            text(format!(
                "{} files · {:.1} MB before compression",
                summary.files,
                summary.bytes as f64 / 1_048_576.0
            ))
            .size(12)
            .style(ui::muted_text),
        );
    }
    content = content.push(
        row![
            control_button(
                reader,
                Control::Profile(Focus::Create),
                text("Create backup…").size(12),
                reader.profile_ready().then_some(action(Action::Create))
            ),
            control_button(
                reader,
                Control::Profile(Focus::Restore),
                text("Restore backup…").size(12),
                reader.profile_ready().then_some(action(Action::Restore))
            ),
        ]
        .spacing(8),
    );
    content = content.push(text("Close the book before backup or restore. Settings, shelves, reading positions and annotations are included; temporary caches are rebuilt. Only library-managed book files are bundled. Linked originals and books omitted from a backup may need Locate on another computer.").size(12).style(ui::muted_text));
    if let Some((path, summary)) = &reader.profile.pending {
        content = content.push(text(format!("Restore {}?\n{} files · {:.1} MB · books: {} · dictionaries: {}\nThis replaces the current library and settings. The previous profile is retained in a recovery folder.",
            path.file_name().unwrap_or_default().to_string_lossy(), summary.files, summary.bytes as f64 / 1_048_576.0,
            if summary.documents { "included" } else { "keep local files" }, if summary.dictionaries { "included" } else { "keep local packs" })).size(12))
            .push(row![
                control_button(reader, Control::Profile(Focus::Confirm), text("Restore and replace").size(12), reader.profile_ready().then_some(action(Action::Confirm))),
                control_button(reader, Control::Profile(Focus::Cancel), text("Cancel").size(12), (!reader.profile.busy).then_some(action(Action::Cancel))),
            ].spacing(8));
    }
    let mut formats = row![].spacing(6);
    for format in [
        ExportFormat::Markdown,
        ExportFormat::Text,
        ExportFormat::Json,
    ] {
        formats = formats.push(toned_button(
            reader,
            Control::Profile(Focus::Format(format)),
            text(format.to_string()).size(12),
            (!reader.profile.busy).then_some(Message::Profile(Action::Format(format))),
            ui::ButtonTone::Surface,
            reader.profile.format == format,
        ));
    }
    content=content.push(text("Export reading notes").font(ui::MEDIUM).size(13)).push(formats)
        .push(control_button(reader, Control::Profile(Focus::Export),text("Export…").size(12),(!reader.profile.busy && reader.notes.data.is_some() && reader.reading_title().is_some() && reader.notes.editor.is_none()).then_some(action(Action::Export))))
        .push(text("Open a book to export its saved bookmarks, highlighted quotes and notes, including page/chapter information.").size(12).style(ui::muted_text));
    if let Some(notice) = &reader.profile.notice {
        content = content.push(text(notice).size(12).style(ui::secondary_text));
    }
    content.into()
}

fn backup_checkbox<'a>(
    reader: &Reader,
    focus: Focus,
    label: &'static str,
    value: bool,
) -> Element<'a, Message> {
    let focused = reader.focused == Some(Control::Profile(focus));
    let input = checkbox(value)
        .label(label)
        .text_size(12)
        .on_toggle_maybe((!reader.profile.busy).then_some(move |value| {
            Message::Profile(match focus {
                Focus::Documents => Action::Documents(value),
                _ => Action::Dictionaries(value),
            })
        }))
        .style(move |theme, status| {
            let mut style = iced::widget::checkbox::primary(theme, status);
            if focused {
                style.border.width = 2.0;
                style.border.color = theme.palette().primary;
            }
            style
        });
    let wrapped = container(input);
    if focused {
        wrapped
            .id(iced::advanced::widget::Id::new(ui::FOCUSED_CONTROL))
            .into()
    } else {
        wrapped.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ready() -> Reader {
        let mut reader = Reader {
            recent_loading: false,
            ..Default::default()
        };
        let _ = reader.shelf.update(shelf::Message::Loaded(Ok(Vec::new())));
        let _ = reader.shelf.update(shelf::Message::Saved(Ok(())));
        let _ = reader.shelf.update(shelf::Message::ShelvesLoaded(Ok(
            reader_document::shelves::Shelves::default(),
        )));
        reader
    }
    #[test]
    fn destructive_restore_waits_for_loads_writes_and_document_close() {
        let mut reader = ready();
        assert!(reader.profile_ready());
        reader.preferences_saving = true;
        let _ = reader.profile_action(Action::Restore);
        assert!(!reader.profile.busy);
        reader.preferences_saving = false;
        reader.shelf = shelf::Shelf::default();
        reader.shelf.loading = false;
        assert!(!reader.profile_ready());
        let _ = reader.shelf.update(shelf::Message::ShelvesLoaded(Ok(
            reader_document::shelves::Shelves::default(),
        )));
        assert!(reader.profile_ready());
        reader.book = Some(super::super::tests::book("open"));
        let _ = reader.profile_action(Action::Create);
        assert!(!reader.profile.busy);
        assert!(!controls(&reader).any(|control| control == Control::Profile(Focus::Restore)));
    }
    #[test]
    fn verification_only_prepares_a_review_and_cancel_leaves_profile_idle() {
        let mut reader = ready();
        let summary = Summary {
            files: 10,
            bytes: 1234,
            documents: true,
            dictionaries: false,
        };
        let _ = reader.profile_reply(Ok(Reply::Inspected(PathBuf::from("review.zip"), summary)));
        assert!(reader.profile.pending.is_some() && !reader.profile.busy);
        assert!(controls(&reader).any(|control| control == Control::Profile(Focus::Confirm)));
        let _ = reader.profile_action(Action::Cancel);
        assert!(reader.profile.pending.is_none());
        let _ = reader.profile_action(Action::Confirm);
        assert!(!reader.profile.busy);
    }
    #[test]
    fn busy_profile_prevents_mutation_and_stale_size_results_are_ignored() {
        let mut reader = ready();
        reader.profile.busy = true;
        let _ = reader.profile_action(Action::Documents(false));
        assert!(reader.profile.options.documents);
        assert!(!reader.interactive() && !reader.exit_ready());
        let _ = reader.profile_reply(Err("Checksum mismatch".into()));
        assert!(reader.profile_ready());
        assert_eq!(reader.profile.notice.as_deref(), Some("Checksum mismatch"));
        reader.profile.generation = 2;
        reader.profile_estimated(
            1,
            Ok(Summary {
                files: 999,
                bytes: 999,
                documents: false,
                dictionaries: false,
            }),
        );
        assert!(reader.profile.estimate.is_none());
        reader.show_settings = true;
        for format in [
            ExportFormat::Markdown,
            ExportFormat::Text,
            ExportFormat::Json,
        ] {
            let control = Control::Profile(Focus::Format(format));
            assert!(reader.controls().any(|c| c == control));
            let _ = reader.activate(control);
            assert_eq!(reader.profile.format, format);
        }
    }

    #[test]
    fn reopening_same_book_after_restore_rejects_old_notes_and_reading_options() {
        use reader_document::annotations::{Annotations, BookmarkPlace};
        let mut reader = ready();
        let old_profile = reader.profile.epoch;
        let _ = reader.profile_reply(Ok(Reply::Restored(PathBuf::new())));
        let mut book = super::super::tests::book("restored");
        Arc::get_mut(&mut book).unwrap().fingerprint = "a".repeat(64);
        reader.book = Some(book);
        let fingerprint = reader.book.as_ref().unwrap().fingerprint.clone();
        let _ = reader.sync_notes();
        let _ = reader.sync_reading_settings();
        let mut old_notes = Annotations::new(&fingerprint).unwrap();
        old_notes
            .add_bookmark(
                BookmarkPlace::Pdf {
                    page: 0,
                    within: 0.0,
                },
                "1".into(),
                "Old profile".into(),
            )
            .unwrap();
        let _ = update_inner(
            &mut reader,
            Message::NotesLoaded {
                profile: old_profile,
                fingerprint: fingerprint.clone(),
                result: Ok(old_notes.clone()),
            },
        );
        assert!(reader.notes.data.is_none());
        let old_options = reader_document::reading::Options {
            size: 36,
            ..Default::default()
        };
        let _ = update_inner(
            &mut reader,
            Message::ReadingLoaded {
                profile: old_profile,
                book: fingerprint.clone(),
                result: Ok(Some(old_options)),
            },
        );
        assert!(reader.reading.loading);
        assert_ne!(reader.reading_options().size, 36);
        let current_profile = reader.profile.epoch;
        let current_notes = Annotations::new(&fingerprint).unwrap();
        let _ = update_inner(
            &mut reader,
            Message::NotesLoaded {
                profile: current_profile,
                fingerprint: fingerprint.clone(),
                result: Ok(current_notes.clone()),
            },
        );
        let _ = update_inner(
            &mut reader,
            Message::ReadingLoaded {
                profile: current_profile,
                book: fingerprint.clone(),
                result: Ok(Some(reader_document::reading::Options {
                    size: 26,
                    ..Default::default()
                })),
            },
        );
        assert!(!reader.reading.loading);
        assert_eq!(reader.reading_options().size, 26);
        let _ = update_inner(
            &mut reader,
            Message::NotesLoaded {
                profile: old_profile,
                fingerprint: fingerprint.clone(),
                result: Ok(old_notes),
            },
        );
        let _ = update_inner(
            &mut reader,
            Message::ReadingLoaded {
                profile: old_profile,
                book: fingerprint,
                result: Ok(Some(old_options)),
            },
        );
        assert_eq!(reader.notes.data.as_ref(), Some(&current_notes));
        assert_eq!(reader.reading_options().size, 26);
    }

    #[test]
    #[ignore = "Production backup UI visual QA: writes target/reader-comfort-previews"]
    fn render_backup_previews() {
        ui::load_test_fonts();
        let output = PathBuf::from("../../target/reader-comfort-previews");
        std::fs::create_dir_all(&output).unwrap();
        let mut reader = ready();
        reader.show_settings = true;
        reader.profile.estimate = Some(Summary {
            files: 42,
            bytes: 4 * 1024 * 1024,
            documents: true,
            dictionaries: false,
        });
        reader.focused = Some(Control::Profile(Focus::Create));
        super::super::book_preview::render(&mut reader, &output.join("backup-library.png"));
        reader.profile.pending = Some((
            PathBuf::from("simPl-backup.zip"),
            Summary {
                files: 42,
                bytes: 4 * 1024 * 1024,
                documents: true,
                dictionaries: false,
            },
        ));
        reader.appearance = Appearance::Dark;
        reader.window_size = Size::new(540.0, 800.0);
        reader.focused = Some(Control::Profile(Focus::Confirm));
        super::super::book_preview::render(
            &mut reader,
            &output.join("restore-review-narrow-dark.png"),
        );
    }
}
