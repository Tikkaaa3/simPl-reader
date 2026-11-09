use super::*;
use iced::widget::column;
use reader_document::library_location::{
    self, Location, Outcome, Plan, Progress, Summary, Target, display_path,
};

#[derive(Clone, Debug)]
pub(super) enum Action {
    Change,
    OpenFolder,
    UseDefault,
    Move,
    Cancel,
    Retry,
    Find,
    Forget,
    ConfirmForget,
    Dismiss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Purpose {
    Change,
    Find,
}

#[derive(Clone, Debug)]
pub(super) enum Reply {
    Loaded(Result<Location, String>, Option<Summary>),
    Picked(Purpose, Result<Option<PathBuf>, String>),
    Planned(Result<Plan, String>),
    Progress {
        generation: u64,
        progress: Progress,
    },
    Moved {
        generation: u64,
        result: Result<Outcome, String>,
    },
    Changed(Result<Option<String>, String>),
    Opened(Result<(), String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Focus {
    Change,
    OpenFolder,
    UseDefault,
    Move,
    Cancel,
    Retry,
    Find,
    Forget,
    ConfirmForget,
    BannerRetry,
    BannerCancel,
    Dismiss,
}

#[derive(Debug, Default)]
enum Phase {
    #[default]
    Idle,
    Confirm(Plan),
    Moving(Progress),
    ConfirmForget,
}

#[derive(Debug)]
struct Job {
    generation: u64,
    cancel: Arc<AtomicBool>,
    _task: iced::task::Handle,
}

#[derive(Debug, Default)]
pub(super) struct State {
    pub location: Option<Result<Location, String>>,
    pub summary: Option<Summary>,
    phase: Phase,
    pub notice: Option<String>,
    generation: u64,
    job: Option<Job>,
}

impl State {
    pub(super) fn pending(&self) -> bool {
        matches!(&self.location, Some(Ok(location)) if location.moving)
    }

    fn unavailable(&self) -> Option<&Location> {
        match &self.location {
            Some(Ok(location)) if location.custom && !location.available => Some(location),
            _ => None,
        }
    }

    fn moving(&self) -> Option<&Progress> {
        match &self.phase {
            Phase::Moving(progress) => Some(progress),
            _ => None,
        }
    }

    pub(super) fn cancel_move(&self) {
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }
}

fn size(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let bytes = bytes as f64;
    if bytes >= 1024.0 * MB {
        format!("{:.1} GB", bytes / (1024.0 * MB))
    } else if bytes == 0.0 {
        "0 MB".into()
    } else {
        format!("{:.0} MB", (bytes / MB).max(1.0))
    }
}

fn books(count: usize) -> String {
    if count == 1 {
        "1 book".into()
    } else {
        format!("{count} books")
    }
}

impl Reader {
    pub(super) fn refresh_library(&mut self) -> Task<Message> {
        Task::perform(
            async {
                let location = library_location::location();
                let summary = location
                    .as_ref()
                    .is_ok_and(|location| location.available)
                    .then(library_location::summary)
                    .and_then(Result::ok);
                (location, summary)
            },
            |(location, summary)| Message::LibraryReply(Reply::Loaded(location, summary)),
        )
    }

    fn library_task(
        &mut self,
        work: impl FnOnce() -> Result<Option<String>, String> + Send + 'static,
    ) -> Task<Message> {
        self.profile.busy = true;
        self.library.notice = None;
        Task::perform(async move { work() }, |result| {
            Message::LibraryReply(Reply::Changed(result))
        })
    }

    fn pick_library_folder(&mut self, purpose: Purpose) -> Task<Message> {
        self.profile.busy = true;
        self.library.notice = None;
        let initial = match &self.library.location {
            Some(Ok(location)) if location.custom => location
                .root
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf),
            _ => None,
        };
        let title = match purpose {
            Purpose::Change => "Choose where to keep your library",
            Purpose::Find => "Find your simPl Library folder",
        };
        Task::perform(
            async move { crate::platform::pick_folder(title, initial.as_deref()) },
            move |result| Message::LibraryReply(Reply::Picked(purpose, result)),
        )
    }

    fn plan_library_move(&mut self, target: Target) -> Task<Message> {
        self.profile.busy = true;
        self.library.notice = None;
        Task::perform(async move { library_location::plan(target) }, |result| {
            Message::LibraryReply(Reply::Planned(result))
        })
    }

    pub(super) fn library_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Cancel => {
                if self.library.job.is_some() {
                    self.library.cancel_move();
                } else if matches!(self.library.phase, Phase::Confirm(_) | Phase::ConfirmForget) {
                    self.library.phase = Phase::Idle;
                }
                Task::none()
            }
            Action::Dismiss => {
                self.library.notice = None;
                Task::none()
            }
            _ if self.profile.busy => Task::none(),
            Action::OpenFolder => match &self.library.location {
                Some(Ok(location)) if location.available => {
                    let root = location.root.clone();
                    Task::perform(
                        async move { crate::platform::open_in_explorer(&root) },
                        |result| Message::LibraryReply(Reply::Opened(result)),
                    )
                }
                _ => Task::none(),
            },
            Action::Change if self.profile_ready() => self.pick_library_folder(Purpose::Change),
            Action::UseDefault
                if self.profile_ready()
                    && matches!(&self.library.location, Some(Ok(l)) if l.custom && l.available) =>
            {
                self.plan_library_move(Target::Default)
            }
            Action::Move if self.profile_ready() => match std::mem::take(&mut self.library.phase) {
                Phase::Confirm(plan) => self.start_library_move(plan),
                phase => {
                    self.library.phase = phase;
                    Task::none()
                }
            },
            Action::Find if self.profile_ready() && self.library.unavailable().is_some() => {
                self.pick_library_folder(Purpose::Find)
            }
            Action::Forget if self.profile_settled() && self.library.unavailable().is_some() => {
                self.library.phase = Phase::ConfirmForget;
                Task::none()
            }
            Action::ConfirmForget
                if self.profile_settled() && matches!(self.library.phase, Phase::ConfirmForget) =>
            {
                self.library.phase = Phase::Idle;
                self.library_task(|| {
                    library_location::forget()
                        .map(|()| Some("simPl now uses the default library folder.".into()))
                })
            }
            Action::Retry if self.profile_settled() => {
                self.library_task(|| Ok(library_location::recover()))
            }
            _ => Task::none(),
        }
    }

    fn start_library_move(&mut self, plan: Plan) -> Task<Message> {
        use iced_futures::futures::{SinkExt, StreamExt};
        self.library.generation = self.library.generation.wrapping_add(1);
        let generation = self.library.generation;
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let summary = plan.summary();
        self.profile.busy = true;
        self.library.notice = None;
        self.library.phase = Phase::Moving(Progress {
            books: 0,
            books_total: summary.books,
            bytes: 0,
            bytes_total: summary.bytes,
        });
        let stream = iced::stream::channel(
            8,
            move |mut output: iced_futures::futures::channel::mpsc::Sender<Message>| async move {
                let (mut sender, mut receiver) = iced_futures::futures::channel::mpsc::channel(8);
                let spawned = std::thread::Builder::new()
                    .name("library-move".into())
                    .spawn(move || {
                        let mut shown = Progress::default();
                        let result = library_location::run_move(
                            &plan,
                            &mut |progress| {
                                if progress.books != shown.books
                                    || progress.bytes >= shown.bytes + 1024 * 1024
                                {
                                    shown = progress;
                                    let _ =
                                        sender.try_send(Message::LibraryReply(Reply::Progress {
                                            generation,
                                            progress,
                                        }));
                                }
                            },
                            &worker_cancel,
                        );
                        let _ = iced_futures::futures::executor::block_on(
                            sender.send(Message::LibraryReply(Reply::Moved { generation, result })),
                        );
                    });
                if let Err(error) = spawned {
                    let _ = output
                        .send(Message::LibraryReply(Reply::Moved {
                            generation,
                            result: Err(format!("Could not start the library move: {error}")),
                        }))
                        .await;
                    return;
                }
                while let Some(message) = receiver.next().await {
                    if output.send(message).await.is_err() {
                        break;
                    }
                }
            },
        );
        let (task, handle) = Task::run(stream, |message| message).abortable();
        self.library.job = Some(Job {
            generation,
            cancel,
            _task: handle.abort_on_drop(),
        });
        task
    }

    pub(super) fn library_reply(&mut self, reply: Reply) -> Task<Message> {
        match reply {
            Reply::Loaded(location, summary) => {
                self.library.location = Some(location);
                self.library.summary = summary;
                Task::none()
            }
            Reply::Picked(_, Ok(None)) => {
                self.profile.busy = false;
                self.finish_library_job()
            }
            Reply::Picked(_, Err(error)) => {
                self.profile.busy = false;
                self.library.notice = Some(error);
                self.finish_library_job()
            }
            Reply::Picked(Purpose::Change, Ok(Some(folder))) => Task::perform(
                async move { library_location::plan(Target::Folder(folder)) },
                |result| Message::LibraryReply(Reply::Planned(result)),
            ),
            Reply::Picked(Purpose::Find, Ok(Some(folder))) => Task::perform(
                async move {
                    library_location::relink(&folder).map(|count| {
                        Some(format!(
                            "Found {} in {}.",
                            books(count),
                            library_location::location()
                                .map(|location| display_path(&location.root))
                                .unwrap_or_default()
                        ))
                    })
                },
                |result| Message::LibraryReply(Reply::Changed(result)),
            ),
            Reply::Planned(result) => {
                self.profile.busy = false;
                match result {
                    Ok(plan) => self.library.phase = Phase::Confirm(plan),
                    Err(error) => self.library.notice = Some(error),
                }
                self.finish_library_job()
            }
            Reply::Progress {
                generation,
                progress,
            } => {
                if self.library.job.as_ref().map(|job| job.generation) == Some(generation) {
                    self.library.phase = Phase::Moving(progress);
                }
                Task::none()
            }
            Reply::Moved { generation, result } => {
                if self.library.job.as_ref().map(|job| job.generation) != Some(generation) {
                    return Task::none();
                }
                self.library.job = None;
                self.library.phase = Phase::Idle;
                self.profile.busy = false;
                let kept = match &self.library.location {
                    Some(Ok(location)) => display_path(&location.root),
                    _ => String::new(),
                };
                let changed = !matches!(result, Ok(Outcome::Cancelled { .. }) | Err(_));
                self.library.notice = Some(match result {
                    Ok(Outcome::Moved { to, books: count }) => {
                        format!("Moved {} to {}.", books(count), display_path(&to))
                    }
                    Ok(Outcome::Cancelled { kept }) => {
                        format!("Your books are still in {}.", display_path(&kept))
                    }
                    Ok(Outcome::Committed { to, error }) => format!(
                        "Your library is now {}, but cleanup didn't finish: {error} Retry.",
                        display_path(&to)
                    ),
                    Err(error) => format!("Your books are still in {kept}. {error}"),
                });
                if changed {
                    self.reload_library_paths()
                } else {
                    self.finish_library_job()
                }
            }
            Reply::Opened(result) => {
                if let Err(error) = result {
                    self.library.notice = Some(error);
                }
                Task::none()
            }
            Reply::Changed(result) => {
                self.profile.busy = false;
                match result {
                    Ok(notice) => {
                        if notice.is_some() {
                            self.library.notice = notice;
                        }
                        self.reload_library_paths()
                    }
                    Err(error) => {
                        self.library.notice = Some(error);
                        self.finish_library_job()
                    }
                }
            }
        }
    }

    fn reload_library_paths(&mut self) -> Task<Message> {
        if self.pending_exit && self.exit_ready() {
            return iced::exit();
        }
        let mut tasks = self.reload_profile();
        tasks.push(self.refresh_library());
        Task::batch(tasks)
    }

    fn finish_library_job(&mut self) -> Task<Message> {
        if self.pending_exit && self.exit_ready() {
            iced::exit()
        } else {
            self.refresh_library()
        }
    }

    pub(super) fn library_focus(&mut self, focus: Focus) -> Task<Message> {
        self.library_action(match focus {
            Focus::Change => Action::Change,
            Focus::OpenFolder => Action::OpenFolder,
            Focus::UseDefault => Action::UseDefault,
            Focus::Move => Action::Move,
            Focus::Cancel | Focus::BannerCancel => Action::Cancel,
            Focus::Retry | Focus::BannerRetry => Action::Retry,
            Focus::Find => Action::Find,
            Focus::Forget => Action::Forget,
            Focus::ConfirmForget => Action::ConfirmForget,
            Focus::Dismiss => Action::Dismiss,
        })
    }
}

fn enabled(reader: &Reader) -> Vec<Focus> {
    let state = &reader.library;
    let ready = reader.profile_ready();
    let settled = reader.profile_settled();
    let location = match &state.location {
        Some(Ok(location)) => Some(location),
        _ => None,
    };
    let mut focus = Vec::new();
    if state.moving().is_some() {
        focus.extend([Focus::Cancel, Focus::BannerCancel]);
        return focus;
    }
    match &state.phase {
        Phase::Confirm(_) => {
            focus.extend(ready.then_some(Focus::Move));
            focus.push(Focus::Cancel);
        }
        Phase::ConfirmForget => {
            focus.extend(settled.then_some(Focus::ConfirmForget));
            focus.push(Focus::Cancel);
        }
        _ => {}
    }
    if let Some(location) = location {
        focus.extend((ready && location.available).then_some(Focus::Change));
        focus.extend((location.available && !reader.profile.busy).then_some(Focus::OpenFolder));
        focus.extend((ready && location.custom && location.available).then_some(Focus::UseDefault));
    }
    let unavailable = state.unavailable().is_some();
    let retry =
        settled && (state.pending() || unavailable || matches!(state.location, Some(Err(_))));
    focus.extend(retry.then_some(Focus::Retry));
    focus.extend(retry.then_some(Focus::BannerRetry));
    focus.extend((ready && unavailable).then_some(Focus::Find));
    focus.extend(
        (settled && unavailable && !matches!(state.phase, Phase::ConfirmForget))
            .then_some(Focus::Forget),
    );
    focus.extend(state.notice.is_some().then_some(Focus::Dismiss));
    focus
}

pub(super) fn controls(reader: &Reader) -> impl Iterator<Item = Control> + Clone + '_ {
    let banner = banner_visible(reader);
    enabled(reader)
        .into_iter()
        .filter(move |focus| match focus {
            Focus::BannerRetry | Focus::BannerCancel | Focus::Dismiss => banner,
            _ => reader.show_settings,
        })
        .map(Control::Library)
}

fn button<'a>(reader: &Reader, focus: Focus, label: &'static str) -> Element<'a, Message> {
    let message = enabled(reader).contains(&focus).then(|| {
        Message::Library(match focus {
            Focus::Change => Action::Change,
            Focus::OpenFolder => Action::OpenFolder,
            Focus::UseDefault => Action::UseDefault,
            Focus::Move => Action::Move,
            Focus::Cancel | Focus::BannerCancel => Action::Cancel,
            Focus::Retry | Focus::BannerRetry => Action::Retry,
            Focus::Find => Action::Find,
            Focus::Forget => Action::Forget,
            Focus::ConfirmForget => Action::ConfirmForget,
            Focus::Dismiss => Action::Dismiss,
        })
    });
    control_button(
        reader,
        Control::Library(focus),
        text(label).size(12),
        message,
    )
}

fn progress_line(progress: &Progress) -> String {
    format!(
        "Moving books… {} of {} · {} of {}",
        progress.books,
        progress.books_total,
        size(progress.bytes),
        size(progress.bytes_total)
    )
}

fn unavailable_line(location: &Location) -> String {
    format!(
        "Library folder not found: {}. Connect the drive, then Retry. Books you opened in place still work.",
        display_path(&location.root)
    )
}

pub(super) fn settings(reader: &Reader) -> Element<'_, Message> {
    let state = &reader.library;
    let mut content = column![text("Library folder").font(ui::MEDIUM).size(13)].spacing(8);
    match &state.location {
        None => {
            content = content.push(
                text("Checking the library folder…")
                    .size(12)
                    .style(ui::muted_text),
            )
        }
        Some(Err(error)) => {
            content = content.push(text(error.clone()).size(12).style(ui::danger_text));
        }
        Some(Ok(location)) => {
            let mut line = row![
                text(display_path(&location.root))
                    .size(12)
                    .width(Length::Fill)
                    .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
            ]
            .spacing(8);
            if let Some(summary) = &state.summary {
                line = line.push(
                    text(if summary.books == 0 {
                        "No imported books".to_string()
                    } else {
                        format!("{} · {}", books(summary.books), size(summary.bytes))
                    })
                    .size(12)
                    .style(ui::muted_text),
                );
            }
            content = content.push(line);
            let mut buttons = row![
                button(reader, Focus::Change, "Change…"),
                button(reader, Focus::OpenFolder, "Open folder"),
            ]
            .spacing(8);
            if location.custom {
                buttons = buttons.push(button(reader, Focus::UseDefault, "Use default folder"));
            }
            content = content.push(buttons);
            if !location.available {
                content = content
                    .push(
                        text(unavailable_line(location))
                            .size(12)
                            .style(ui::danger_text),
                    )
                    .push(
                        row![
                            button(reader, Focus::Retry, "Retry"),
                            button(reader, Focus::Find, "Find it…"),
                            button(reader, Focus::Forget, "Stop using this folder…"),
                        ]
                        .spacing(8),
                    );
            } else if location.moving {
                content = content
                    .push(
                        text("A library move didn't finish.")
                            .size(12)
                            .style(ui::danger_text),
                    )
                    .push(button(reader, Focus::Retry, "Retry"));
            }
        }
    }
    match &state.phase {
        Phase::Confirm(plan) => {
            let summary = plan.summary();
            content = content.push(
                text(format!(
                    "Move {} ({}) to {}?",
                    books(summary.books),
                    size(summary.bytes),
                    display_path(plan.to())
                ))
                .size(12),
            );
            content = content.push(
                row![
                    button(reader, Focus::Move, "Move"),
                    button(reader, Focus::Cancel, "Cancel"),
                ]
                .spacing(8),
            );
        }
        Phase::Moving(progress) => {
            content = content
                .push(text(progress_line(progress)).size(12))
                .push(button(reader, Focus::Cancel, "Cancel"));
        }
        Phase::ConfirmForget => {
            let root = state
                .unavailable()
                .map(|l| display_path(&l.root))
                .unwrap_or_default();
            let mut line =
                format!("Books in {root} stay there. Their library cards show as missing.");
            if state.pending() {
                line.push_str(" Some books may still be in the previous library folder.");
            }
            content = content.push(text(line).size(12)).push(
                row![
                    button(reader, Focus::ConfirmForget, "Stop using this folder"),
                    button(reader, Focus::Cancel, "Cancel"),
                ]
                .spacing(8),
            );
        }
        Phase::Idle => {}
    }
    let hint = if state.pending() {
        Some("Finish the library move first.")
    } else if reader.book.is_some() || reader.pdf.is_some() {
        Some("Close the book to change the library folder.")
    } else {
        None
    };
    if let Some(hint) = hint {
        content = content.push(text(hint).size(12).style(ui::muted_text));
    }
    content = content.push(
        text(
            "Imported books are kept here. Settings, reading positions and notes stay on this PC.",
        )
        .size(12)
        .style(ui::muted_text),
    );
    if let Some(notice) = &state.notice {
        content = content.push(text(notice.clone()).size(12).style(ui::secondary_text));
    }
    content.into()
}

pub(super) fn banner_visible(reader: &Reader) -> bool {
    let state = &reader.library;
    state.moving().is_some()
        || state.notice.is_some()
        || state.pending()
        || state.unavailable().is_some()
        || matches!(state.location, Some(Err(_)))
}

pub(super) fn banner(reader: &Reader) -> Option<Element<'_, Message>> {
    let state = &reader.library;
    if !banner_visible(reader) {
        return None;
    }
    let mut lines = column![].spacing(6);
    let mut actions = row![].spacing(8);
    if let Some(progress) = state.moving() {
        lines = lines.push(text(progress_line(progress)).size(13));
        actions = actions.push(button(reader, Focus::BannerCancel, "Cancel"));
    } else {
        let problem = match &state.location {
            Some(Err(error)) => Some(error.clone()),
            Some(Ok(location)) if location.custom && !location.available => {
                Some(unavailable_line(location))
            }
            Some(Ok(location)) if location.moving => Some(format!(
                "A library move to {} didn't finish.",
                display_path(&location.root)
            )),
            _ => None,
        };
        if let Some(problem) = problem {
            lines = lines.push(
                text(problem)
                    .size(13)
                    .style(ui::danger_text)
                    .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
            );
            actions = actions.push(button(reader, Focus::BannerRetry, "Retry"));
        }
    }
    if let Some(notice) = &state.notice
        && state.moving().is_none()
    {
        lines = lines.push(
            text(notice.clone())
                .size(13)
                .wrapping(iced::advanced::text::Wrapping::WordOrGlyph),
        );
        actions = actions.push(button(reader, Focus::Dismiss, "Dismiss"));
    }
    Some(
        container(
            row![lines.width(Length::Fill), actions]
                .spacing(8)
                .align_y(iced::Alignment::Center),
        )
        .padding(12)
        .style(ui::panel)
        .into(),
    )
}
