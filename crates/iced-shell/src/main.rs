#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_os = "windows"))]
compile_error!("simPl currently supports Windows only");

mod app;
mod book_pages;
mod book_style;
mod chrome;
mod dictionary_download;
mod document_scroll;
mod find;
mod notes;
mod pdf_reader;
mod platform;
mod shelf;
mod speech;
mod themes;
mod ui;

use iced::event;
use iced::keyboard::{self, Key, key};
use iced::widget::{column, container, mouse_area, row, scrollable, text};
use iced::{
    Background, Border, Color, Element, Font, Length, Padding, Size, Subscription, Task, Theme,
    border, mouse, window,
};
use iced_shell::{
    ActivationKey, CONTROL_HEIGHT, CONTROL_WIDTH, Command, Control, FocusDirection, HoverEvent,
    ShellState, TOOLBAR_CONTENT_HEIGHT, TOOLBAR_SEPARATOR_HEIGHT, bidi_diagnostic,
    hover_transition, interaction_trace, reader, selection, virtual_reader,
};
use shell_startup_markers::{Emitter, Event};

/// Process-scoped startup-marker gate: when this variable is absent (the
/// normal case), shell behavior, titles, and timing-relevant work are
/// unchanged; when it is set, the shell appends a bounded, QPC-stamped
/// marker stream for optional startup diagnostics.
const STARTUP_MARKERS_VARIABLE: &str = "ICED_SHELL_STARTUP_MARKERS";
const BIDI_DIAGNOSTICS_VARIABLE: &str = "ICED_SHELL_BIDI_DIAGNOSTICS";
const BIDI_TRACE_PATH_VARIABLE: &str = "ICED_SHELL_BIDI_TRACE_PATH";
const BIDI_DIAGNOSTIC_CASES: [&str; 3] = ["p-00004", "p-00003", "p-00001"];

const ROOT: Color = Color::from_rgb8(0x12, 0x16, 0x1d);
const TOOLBAR: Color = Color::from_rgb8(0x1a, 0x20, 0x29);
const BUTTON: Color = Color::from_rgb8(0x25, 0x2b, 0x35);
const BUTTON_HOVER: Color = Color::from_rgb8(0x34, 0x3c, 0x49);
const BODY_TEXT: Color = Color::from_rgb8(0xd7, 0xdc, 0xe2);
const BRIGHT_TEXT: Color = Color::from_rgb8(0xf3, 0xf4, 0xf6);
const FOCUS: Color = Color::from_rgb8(0x6e, 0xa8, 0xfe);
const NEUTRAL_BORDER: Color = Color::from_rgb8(0x4b, 0x55, 0x63);
const PANEL_BORDER: Color = Color::from_rgb8(0x46, 0x52, 0x67);
const PANEL: Color = Color::from_rgb8(0x20, 0x27, 0x33);
const PANEL_HEADING: Color = Color::from_rgb8(0x9e, 0xc5, 0xfe);

#[derive(Debug)]
struct Shell {
    state: ShellState,
    native_test_status: bool,
    reader_test_status: bool,
    reader_selection_test_status: bool,
    reader_last_hit: Option<(reader_document::Endpoint, iced::Point)>,
    reader_last_pointer: Option<iced::Point>,
    reader_mode: bool,
    reader_large: bool,
    reader_selection: Option<selection::SelectionState>,
    reader_heights: Option<virtual_reader::HeightIndex>,
    reader_measurements: Option<virtual_reader::Measurements>,
    reader_viewport_height: f32,
    reader_generation: u64,
    virtual_test_status: bool,
    reader_built: std::sync::atomic::AtomicUsize,
    reader_laid_out: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
    reader_peak_built: std::sync::atomic::AtomicUsize,
    reader_peak_layout: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
    reader_first: std::sync::atomic::AtomicUsize,
    reader_end: std::sync::atomic::AtomicUsize,
    bidi_diagnostic_mode: bool,
    bidi_case_index: usize,
    bidi_variant_index: usize,
    bidi_trace: Option<std::sync::Arc<std::sync::Mutex<bidi_diagnostic::TraceWriter>>>,
    bidi_summary: Option<bidi_diagnostic::TraceSummary>,
    reader_phase: ReaderPhase,
    reader_width: reader::WidthScenario,
    reader_pending: Option<ReaderContent>,
    reader_content: Option<ReaderContent>,
    reader_font_count: usize,
    reader_scroll_offset: f32,
    reader_scroll_maximum: f32,
    window_active: bool,
    hovered_control: Option<Control>,
    marker_emitter: Option<Emitter>,
    interaction_trace: Option<std::sync::Mutex<interaction_trace::Trace>>,
}

#[derive(Debug)]
enum ReaderPhase {
    Disabled,
    WaitingForWindowOpened,
    Queued,
    Loading,
    LoadingFonts,
    Ready,
    Closed,
    Failed(String),
}

#[derive(Debug)]
struct ReaderContent {
    fixture_revision: String,
    workload: reader_workload::Workload,
    image: iced::widget::image::Handle,
}

impl Shell {
    fn new(
        reader_mode: bool,
        reader_large: bool,
        bidi_diagnostic_mode: bool,
        bidi_trace: Option<std::sync::Arc<std::sync::Mutex<bidi_diagnostic::TraceWriter>>>,
    ) -> Self {
        let reader_test_status = reader_mode
            && std::env::var_os("ICED_SHELL_READER_TEST_STATUS")
                .is_some_and(|value| value == std::ffi::OsStr::new("1"));
        let reader_selection_test_status = reader_mode
            && std::env::var_os("ICED_SHELL_SELECTION_TEST_STATUS").as_deref()
                == Some(std::ffi::OsStr::new("1"));
        let virtual_test_status = reader_mode
            && std::env::var_os("ICED_SHELL_VIRTUAL_TEST_STATUS").as_deref()
                == Some(std::ffi::OsStr::new("1"));
        Self {
            state: ShellState::default(),
            native_test_status: std::env::var_os("ICED_SHELL_NATIVE_TEST_STATUS").is_some(),
            reader_test_status,
            reader_selection_test_status,
            reader_last_hit: None,
            reader_last_pointer: None,
            reader_mode,
            reader_large,
            reader_selection: reader_mode.then(Default::default),
            reader_heights: None,
            reader_measurements: reader_mode.then(Default::default),
            reader_viewport_height: 600.0,
            reader_generation: 0,
            virtual_test_status,
            reader_built: Default::default(),
            reader_laid_out: virtual_test_status.then(Default::default),
            reader_peak_built: Default::default(),
            reader_peak_layout: virtual_test_status.then(Default::default),
            reader_first: Default::default(),
            reader_end: Default::default(),
            bidi_diagnostic_mode,
            bidi_case_index: 0,
            bidi_variant_index: 0,
            bidi_trace,
            bidi_summary: None,
            reader_phase: if reader_mode || bidi_diagnostic_mode {
                ReaderPhase::WaitingForWindowOpened
            } else {
                ReaderPhase::Disabled
            },
            reader_width: reader::WidthScenario::Wide,
            reader_pending: None,
            reader_content: None,
            reader_font_count: 0,
            reader_scroll_offset: 0.0,
            reader_scroll_maximum: 0.0,
            window_active: true,
            hovered_control: None,
            marker_emitter: shell_startup_markers::emitter_from_env(STARTUP_MARKERS_VARIABLE),
            interaction_trace: if reader_mode {
                std::env::var_os(interaction_trace::VARIABLE).map(|path| {
                    std::sync::Mutex::new(
                        interaction_trace::Trace::create(std::path::Path::new(&path))
                            .expect("W14 trace creation failed"),
                    )
                })
            } else {
                None
            },
        }
    }
}

impl Shell {
    fn trace(&self, event: &'static str, value: f32, start: Option<u64>) {
        if let Some(trace) = &self.interaction_trace {
            trace
                .lock()
                .expect("W14 trace lock failed")
                .record(event, value, start)
                .expect("W14 trace write failed");
        }
    }

    fn trace_start(&self) -> Option<u64> {
        self.interaction_trace
            .as_ref()
            .map(|_| shell_startup_markers::qpc().expect("W14 QPC failed"))
    }

    /// Emits one bounded marker; the emitter itself emits each event kind at
    /// most once, and a missing gate or a failed marker stream never changes
    /// normal shell behavior.
    fn emit_marker(&self, event: Event) {
        // The once-per-kind guard precedes the QPC read so marker-mode work
        // stays bounded (no per-frame QPC reads after the first emission).
        if let Some(emitter) = &self.marker_emitter {
            if emitter.has_emitted(event) {
                return;
            }
            if let Some(qpc) = shell_startup_markers::qpc() {
                emitter.emit(event, qpc);
            }
        }
    }
}

#[derive(Clone, Debug)]
enum Message {
    Press(Control),
    Hover(HoverEvent),
    ReaderSelectionStart(reader_document::Endpoint),
    ReaderSelectionMove {
        endpoint: reader_document::Endpoint,
        point: iced::Point,
    },
    ToggleReaderWidth,
    BidiDiagnosticNextVariant,
    BidiDiagnosticNextCase,
    ReaderScroll {
        generation: u64,
        offset: f32,
        maximum: f32,
        viewport: f32,
    },
    LoadReaderPackage,
    ToggleReaderContent,
    ReaderPackageLoaded(Result<std::sync::Arc<reader::ReaderPackage>, String>),
    ReaderFontLoaded(Result<(), iced::font::Error>),
    Event(iced::Event),
}

fn update(shell: &mut Shell, message: Message) -> Task<Message> {
    let mut task = Task::none();
    match message {
        Message::Press(control) => {
            shell.state.dispatch(Command::PointerPress(control));
        }
        Message::Hover(event) => {
            shell.hovered_control = hover_transition(shell.hovered_control, event);
        }
        Message::ReaderSelectionStart(endpoint) => {
            if shell.reader_mode
                && matches!(shell.reader_phase, ReaderPhase::Ready)
                && let Some(selection) = shell.reader_selection.as_mut()
            {
                selection.begin(endpoint);
            }
        }
        Message::ReaderSelectionMove { endpoint, point } => {
            if shell.reader_selection_test_status {
                shell.reader_last_hit = Some((endpoint.clone(), point));
            }
            if shell.reader_mode
                && matches!(shell.reader_phase, ReaderPhase::Ready)
                && let Some(selection) = shell.reader_selection.as_mut()
                && selection.is_dragging()
            {
                selection.extend(endpoint);
            }
        }
        Message::ToggleReaderWidth => {
            if shell.reader_mode && matches!(shell.reader_phase, ReaderPhase::Ready) {
                shell.reader_generation = shell.reader_generation.wrapping_add(1);
            }
            let old_offset = shell.reader_scroll_offset;
            let anchor = shell
                .reader_heights
                .as_ref()
                .map(|index| index.anchor_at(old_offset));
            shell.reader_width = shell.reader_width.toggled();
            shell.trace("width", shell.reader_width.width_dip() as f32, None);
            if let Some(content) = &shell.reader_content {
                let index = virtual_reader::HeightIndex::for_workload(
                    &content.workload,
                    shell.reader_width.width_dip(),
                );
                shell.reader_scroll_offset =
                    anchor.map_or(0.0, |anchor| index.resolve_anchor(anchor));
                shell.reader_heights = Some(index);
                task = reader_scroll_to(shell.reader_scroll_offset);
            }
            refresh_bidi_trace(shell);
        }
        Message::BidiDiagnosticNextVariant => {
            if shell.bidi_diagnostic_mode {
                shell.bidi_variant_index =
                    (shell.bidi_variant_index + 1) % reader::bidi_diagnostic_variants().len();
                shell.reader_width = reader::WidthScenario::Wide;
                refresh_bidi_trace(shell);
            }
        }
        Message::BidiDiagnosticNextCase => {
            if shell.bidi_diagnostic_mode {
                shell.bidi_case_index = (shell.bidi_case_index + 1) % BIDI_DIAGNOSTIC_CASES.len();
                shell.bidi_variant_index = 0;
                shell.reader_width = reader::WidthScenario::Wide;
                refresh_bidi_trace(shell);
            }
        }
        Message::ReaderScroll {
            generation,
            offset,
            maximum,
            viewport,
        } => {
            if !shell.reader_mode
                || !matches!(shell.reader_phase, ReaderPhase::Ready)
                || generation != shell.reader_generation
            {
                return Task::none();
            }
            let started = shell.trace_start();
            shell.reader_scroll_offset = offset;
            shell.reader_scroll_maximum = maximum;
            shell.reader_viewport_height = viewport;
            if let Some(new_offset) = refine_reader_heights(shell) {
                task = reader_scroll_to(new_offset);
            }
            shell.trace("scroll", shell.reader_scroll_offset, started);
            shell.trace("maximum", shell.reader_scroll_maximum, None);
            shell.trace("viewport", shell.reader_viewport_height, None);
        }
        Message::ToggleReaderContent => {
            if !shell.reader_mode || shell.bidi_diagnostic_mode {
                return Task::none();
            }
            if matches!(shell.reader_phase, ReaderPhase::Ready) {
                if let Some(selection) = shell.reader_selection.as_mut() {
                    selection.clear();
                }
                shell.reader_last_hit = None;
                shell.reader_phase = ReaderPhase::Closed;
                shell.reader_pending = None;
                shell.reader_content = None;
                shell.reader_heights = None;
                shell.reader_generation = shell.reader_generation.wrapping_add(1);
                shell.reader_scroll_offset = 0.0;
                shell.reader_scroll_maximum = 0.0;
                shell.reader_viewport_height = 600.0;
                shell
                    .reader_built
                    .store(0, std::sync::atomic::Ordering::Relaxed);
                if let Some(counter) = &shell.reader_laid_out {
                    counter.store(0, std::sync::atomic::Ordering::Relaxed);
                }
                shell
                    .reader_peak_built
                    .store(0, std::sync::atomic::Ordering::Relaxed);
                if let Some(counter) = &shell.reader_peak_layout {
                    counter.store(0, std::sync::atomic::Ordering::Relaxed);
                }
                shell
                    .reader_first
                    .store(0, std::sync::atomic::Ordering::Relaxed);
                shell
                    .reader_end
                    .store(0, std::sync::atomic::Ordering::Relaxed);
                if let Some(reports) = &shell.reader_measurements {
                    reports.lock().clear();
                }
            } else if matches!(shell.reader_phase, ReaderPhase::Closed) {
                shell.reader_generation = shell.reader_generation.wrapping_add(1);
                shell.reader_scroll_offset = 0.0;
                shell.reader_scroll_maximum = 0.0;
                shell.reader_viewport_height = 600.0;
                shell.reader_phase = ReaderPhase::Queued;
                return Task::perform(async {}, |_| Message::LoadReaderPackage);
            }
        }
        Message::LoadReaderPackage => {
            if !matches!(shell.reader_phase, ReaderPhase::Queued) {
                return Task::none();
            }
            shell.reader_phase = ReaderPhase::Loading;
            let start = match std::env::current_dir() {
                Ok(start) => start,
                Err(error) => {
                    shell.reader_phase = ReaderPhase::Failed(format!(
                        "cannot determine fixture working directory: {error}"
                    ));
                    return Task::none();
                }
            };
            let size = if shell.reader_large {
                reader_workload::WorkloadSize::Large
            } else {
                reader_workload::WorkloadSize::Small
            };
            return Task::perform(
                async move {
                    reader::load_reader_package_from_size(&start, size).map(std::sync::Arc::new)
                },
                Message::ReaderPackageLoaded,
            );
        }
        Message::ReaderPackageLoaded(Err(error)) => {
            shell.reader_phase = ReaderPhase::Failed(error);
            shell.reader_pending = None;
            return Task::none();
        }
        Message::ReaderPackageLoaded(Ok(package)) => {
            if !matches!(shell.reader_phase, ReaderPhase::Loading) {
                return Task::none();
            }
            let package = match std::sync::Arc::try_unwrap(package) {
                Ok(package) => package,
                Err(_) => {
                    shell.reader_phase = ReaderPhase::Failed(
                        "reader package unexpectedly remained shared during setup".to_string(),
                    );
                    return Task::none();
                }
            };
            let reader::ReaderPackage {
                fixture_root: _,
                fixture_revision,
                workload,
                fonts,
                image,
            } = package;
            shell.reader_pending = Some(ReaderContent {
                fixture_revision,
                workload,
                image: iced::widget::image::Handle::from_rgba(
                    image.width,
                    image.height,
                    image.rgba,
                ),
            });
            shell.reader_phase = ReaderPhase::LoadingFonts;
            shell.reader_font_count = 0;
            let font_tasks = fonts
                .into_iter()
                .map(|font| {
                    iced::font::load(std::borrow::Cow::Owned(font.bytes))
                        .map(Message::ReaderFontLoaded)
                })
                .collect::<Vec<_>>();
            task = Task::batch(font_tasks);
        }
        Message::ReaderFontLoaded(Err(error)) => {
            if matches!(shell.reader_phase, ReaderPhase::LoadingFonts) {
                shell.reader_pending = None;
                shell.reader_phase = ReaderPhase::Failed(format!(
                    "Iced could not load a validated fixture font: {error:?}"
                ));
            }
        }
        Message::ReaderFontLoaded(Ok(())) => {
            if matches!(shell.reader_phase, ReaderPhase::LoadingFonts) {
                shell.reader_font_count += 1;
                if shell.reader_font_count == 6 {
                    shell.reader_content = shell.reader_pending.take();
                    if shell.reader_mode {
                        shell.reader_heights = shell.reader_content.as_ref().map(|content| {
                            virtual_reader::HeightIndex::for_workload(
                                &content.workload,
                                shell.reader_width.width_dip(),
                            )
                        });
                    }
                    shell.reader_phase = ReaderPhase::Ready;
                    shell.trace("width", shell.reader_width.width_dip() as f32, None);
                    refresh_bidi_trace(shell);
                }
            }
        }
        Message::Event(event) => match event {
            iced::Event::Window(window::Event::Opened { .. }) => {
                shell.emit_marker(Event::WindowCreated);
                if matches!(shell.reader_phase, ReaderPhase::WaitingForWindowOpened) {
                    shell.reader_phase = ReaderPhase::Queued;
                    task = Task::perform(async {}, |_| Message::LoadReaderPackage);
                }
            }
            iced::Event::Window(window::Event::RedrawRequested(_)) => {
                let started = shell.trace_start();
                shell.emit_marker(Event::RedrawRequested);
                if let Some(new_offset) = refine_reader_heights(shell) {
                    task = reader_scroll_to(new_offset);
                }
                shell.trace("redraw", 0.0, started);
            }
            iced::Event::Mouse(mouse::Event::WheelScrolled { .. }) => {
                shell.trace("wheel", 0.0, None)
            }
            iced::Event::Window(window::Event::Resized(size)) => {
                shell.trace("resize", size.height, None)
            }
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                repeat,
                ..
            }) => match key.as_ref() {
                Key::Named(key::Named::Tab) if !repeat => {
                    let direction = if modifiers.shift() {
                        FocusDirection::Previous
                    } else {
                        FocusDirection::Next
                    };
                    let control = shell.state.traverse(direction);
                    shell.state.dispatch(Command::Focus(control));
                }
                Key::Named(key::Named::Enter) => {
                    if let Some(control) = shell.state.focused() {
                        shell
                            .state
                            .activate_key(control, ActivationKey::Enter, repeat);
                    }
                }
                Key::Named(key::Named::Space) => {
                    if let Some(control) = shell.state.focused() {
                        shell
                            .state
                            .activate_key(control, ActivationKey::Space, repeat);
                    }
                }
                Key::Named(key::Named::F1) if !repeat => {
                    shell.state.dispatch(Command::ToggleInfo);
                }
                Key::Named(key::Named::Escape) if !repeat => {
                    shell.state.dispatch(Command::HideInfo);
                }
                Key::Named(key::Named::F2) if !repeat && shell.bidi_diagnostic_mode => {
                    task = update(shell, Message::BidiDiagnosticNextVariant);
                }
                Key::Named(key::Named::F3) if !repeat && shell.bidi_diagnostic_mode => {
                    task = update(shell, Message::BidiDiagnosticNextCase);
                }
                Key::Named(key::Named::F4) if !repeat && shell.bidi_diagnostic_mode => {
                    task = update(shell, Message::ToggleReaderWidth);
                }
                Key::Character(character)
                    if !repeat
                        && modifiers.control()
                        && character.eq_ignore_ascii_case("c")
                        && shell.reader_mode
                        && matches!(shell.reader_phase, ReaderPhase::Ready) =>
                {
                    // No selection, a collapsed range, or an empty partial
                    // endpoint leaves the existing OS clipboard untouched.
                    if let Some(text) = selected_reader_copy(shell) {
                        task = iced::clipboard::write(text);
                    }
                }
                Key::Named(key::Named::F5)
                    if !repeat && shell.reader_mode && !shell.bidi_diagnostic_mode =>
                {
                    task = update(shell, Message::ToggleReaderContent);
                }
                _ => {}
            },
            iced::Event::Mouse(mouse::Event::CursorMoved { position })
                if shell.reader_selection_test_status =>
            {
                shell.reader_last_pointer = Some(position);
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                shell
                    .state
                    .dispatch(Command::PointerRelease(shell.hovered_control));
                if let Some(selection) = shell.reader_selection.as_mut() {
                    selection.end_drag();
                }
            }
            iced::Event::Window(window::Event::Focused) => shell.window_active = true,
            iced::Event::Window(window::Event::Unfocused) => {
                shell.window_active = false;
                if let Some(selection) = shell.reader_selection.as_mut() {
                    selection.end_drag();
                }
            }
            _ => {}
        },
    }

    if shell.state.exit_requested() {
        iced::exit()
    } else {
        task
    }
}

fn refresh_bidi_trace(shell: &mut Shell) {
    if !shell.bidi_diagnostic_mode || !matches!(shell.reader_phase, ReaderPhase::Ready) {
        return;
    }
    let case_id = BIDI_DIAGNOSTIC_CASES[shell.bidi_case_index];
    let variant = reader::bidi_diagnostic_variants()[shell.bidi_variant_index];
    let width = shell.reader_width.width_dip();
    let result = match (shell.reader_content.as_ref(), shell.bidi_trace.as_ref()) {
        (Some(content), Some(trace)) => trace
            .lock()
            .map_err(|_| "bounded diagnostic trace writer lock was poisoned".to_string())
            .and_then(|mut trace| trace.record(&content.workload, case_id, variant, width)),
        (None, _) => Err("reader fixture is unavailable for the diagnostic".to_string()),
        (_, None) => Err("bounded diagnostic trace writer is unavailable".to_string()),
    };
    match result {
        Ok(summary) => shell.bidi_summary = Some(summary),
        Err(error) => shell.reader_phase = ReaderPhase::Failed(error),
    }
}

fn button<'a>(shell: &Shell, control: Control, label: &'a str) -> Element<'a, Message> {
    let focused = shell.state.focused() == Some(control);
    let hovered = shell.hovered_control == Some(control);
    let content = container(text(label).size(15).color(BRIGHT_TEXT))
        .width(CONTROL_WIDTH)
        .height(CONTROL_HEIGHT)
        .padding(Padding::from([0, 16]))
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .style(move |_| container::Style {
            background: Some(Background::Color(if hovered {
                BUTTON_HOVER
            } else {
                BUTTON
            })),
            border: Border {
                color: if focused { FOCUS } else { NEUTRAL_BORDER },
                width: 2.0,
                radius: 6.0.into(),
            },
            ..container::Style::default()
        });

    mouse_area(content)
        .on_press(Message::Press(control))
        .on_enter(Message::Hover(HoverEvent::Enter(control)))
        .on_exit(Message::Hover(HoverEvent::Exit(control)))
        .interaction(mouse::Interaction::Pointer)
        .into()
}

fn view(shell: &Shell) -> Element<'_, Message> {
    let started = shell.trace_start();
    // App-declared first render submission; supporting evidence only, never
    // the externally observed displayed-frame endpoint.
    shell.emit_marker(Event::RenderSubmitted);

    let toolbar = container(
        row![
            button(shell, Control::Info, "Info"),
            button(shell, Control::Exit, "Exit")
        ]
        .spacing(10)
        .align_y(iced::alignment::Vertical::Center),
    )
    .height(TOOLBAR_CONTENT_HEIGHT)
    .width(Length::Fill)
    .padding(Padding::from([0, 20]))
    .align_y(iced::alignment::Vertical::Center)
    .style(|_| container::Style {
        background: Some(Background::Color(TOOLBAR)),
        border: Border {
            color: Color::from_rgb8(0x30, 0x38, 0x46),
            width: 0.0,
            radius: border::Radius::default(),
        },
        ..container::Style::default()
    });

    let body = if (shell.reader_mode || shell.bidi_diagnostic_mode)
        && !matches!(
            shell.reader_phase,
            ReaderPhase::Disabled | ReaderPhase::WaitingForWindowOpened | ReaderPhase::Queued
        ) {
        reader_body(shell)
    } else {
        empty_shell_body(shell)
    };

    let toolbar_border = container("")
        .height(TOOLBAR_SEPARATOR_HEIGHT)
        .width(Length::Fill)
        .style(|_| container::Style::default().background(Color::from_rgb8(0x30, 0x38, 0x46)));

    let result = container(column![toolbar, toolbar_border, {
        let body = container(body).padding(24).width(Length::Fill);
        if shell.reader_mode || shell.bidi_diagnostic_mode {
            body.height(Length::Fill)
        } else {
            body
        }
    }])
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(Background::Color(ROOT)),
        text_color: Some(BODY_TEXT),
        ..container::Style::default()
    })
    .into();
    shell.trace("view", 0.0, started);
    result
}

fn empty_shell_body(shell: &Shell) -> Element<'_, Message> {
    let mut body = column![
        text("Empty local shell").size(20).color(BRIGHT_TEXT),
        text("This prototype does not open documents or load content yet.")
            .size(15)
            .color(BODY_TEXT)
    ]
    .spacing(18);
    if shell.state.panel_visible() {
        body = body.push(info_panel());
    }
    body.into()
}

fn info_panel() -> Element<'static, Message> {
    container(
        column![
            text("Iced shell prototype").size(17).color(PANEL_HEADING),
            text("This is a native shell prototype, not a document reader.")
                .size(15)
                .color(BODY_TEXT)
        ]
        .spacing(8),
    )
    .max_width(540)
    .padding(18)
    .style(|_| container::Style {
        background: Some(Background::Color(PANEL)),
        border: Border {
            color: PANEL_BORDER,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..container::Style::default()
    })
    .into()
}

fn reader_body(shell: &Shell) -> Element<'_, Message> {
    let reader_view: Element<'_, Message> = match &shell.reader_phase {
        ReaderPhase::Disabled | ReaderPhase::WaitingForWindowOpened | ReaderPhase::Queued => {
            empty_shell_body(shell)
        }
        ReaderPhase::Loading => column![
            text("Loading the local reader workload…")
                .size(20)
                .color(BRIGHT_TEXT),
            text("Validating the pinned fixture manifest and local assets.")
                .size(15)
                .color(BODY_TEXT)
        ]
        .spacing(18)
        .into(),
        ReaderPhase::LoadingFonts => column![
            text("Loading the local reader workload…")
                .size(20)
                .color(BRIGHT_TEXT),
            text("Verified fixture assets are ready; loading six controlled fonts.")
                .size(15)
                .color(BODY_TEXT)
        ]
        .spacing(18)
        .into(),
        ReaderPhase::Ready if shell.bidi_diagnostic_mode => bidi_diagnostic_body(shell),
        ReaderPhase::Closed => text("Reader content released. Press F5 to reopen the local fixture.").size(15).color(BODY_TEXT).into(),
        ReaderPhase::Ready => ready_reader_body(shell),
        ReaderPhase::Failed(error) => column![
            text("Reader PoC could not load its local fixture.")
                .size(20)
                .color(BRIGHT_TEXT),
            text("Run from the repository root or a descendant and restore the manifest-listed fonts/image. No substitute assets are used.")
                .size(15)
                .color(BODY_TEXT),
            text(error.as_str()).size(13).color(BODY_TEXT)
        ]
        .spacing(12)
        .into(),
    };

    if shell.state.panel_visible() {
        column![reader_view, info_panel()].spacing(18).into()
    } else {
        reader_view
    }
}

fn ready_reader_body(shell: &Shell) -> Element<'_, Message> {
    let Some(content) = shell.reader_content.as_ref() else {
        return text("Reader state is inconsistent; close and restart the PoC.")
            .size(15)
            .color(BODY_TEXT)
            .into();
    };
    let recipe = &reader_workload::LAYOUT_RECIPE;
    let width = shell.reader_width.width_dip();
    let width_label = format!("Content width: {width} DIP (click to switch)");
    let width_control = mouse_area(
        container(text(width_label).size(14).color(BRIGHT_TEXT))
            .padding(Padding::from([8, 12]))
            .style(|_| container::Style {
                background: Some(Background::Color(BUTTON)),
                border: Border {
                    color: NEUTRAL_BORDER,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..container::Style::default()
            }),
    )
    .on_press(Message::ToggleReaderWidth)
    .interaction(mouse::Interaction::Pointer);
    let header = row![
        text(format!(
            "{} · {} body paragraphs · {} heading/image items",
            content.fixture_revision,
            content.workload.body_paragraph_count(),
            content
                .workload
                .total_item_count()
                .saturating_sub(content.workload.body_paragraph_count())
        ))
        .size(13)
        .color(BODY_TEXT)
        .width(Length::Fill),
        width_control
    ]
    .spacing(10)
    .align_y(iced::alignment::Vertical::Center);

    let Some(index) = shell.reader_heights.as_ref() else {
        return text("Reader height index unavailable.").into();
    };
    const OVERSCAN_DIP: f32 = 600.0;
    let range = index.window(
        shell.reader_scroll_offset,
        shell.reader_viewport_height,
        OVERSCAN_DIP,
    );
    if shell.virtual_test_status {
        shell
            .reader_built
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }
    let selection = shell
        .reader_selection
        .as_ref()
        .and_then(|selection| selection.bounds(content.workload.items()));
    let dragging = shell
        .reader_selection
        .as_ref()
        .is_some_and(selection::SelectionState::is_dragging);
    let track_hit_test = shell.reader_selection_test_status;
    let first_row = range.start;
    let row_items = content.workload.items()[range.clone()]
        .iter()
        .enumerate()
        .map(move |(local, item)| (first_row + local, item));
    let rows = virtual_reader::construct_rows(
        row_items,
        |(item_index, item)| {
            render_reader_item(
                item,
                item_index,
                content,
                recipe,
                selection,
                dragging,
                track_hit_test,
            )
        },
        shell.virtual_test_status.then_some(&shell.reader_built),
    );
    if shell.virtual_test_status {
        use std::sync::atomic::Ordering::Relaxed;
        shell
            .reader_peak_built
            .fetch_max(shell.reader_built.load(Relaxed), Relaxed);
        shell.reader_first.store(range.start, Relaxed);
        shell.reader_end.store(range.end, Relaxed);
    }
    let blocks = virtual_reader::VisibleRows::new(
        range,
        index,
        width as f32,
        recipe.paragraph_gap_dip,
        rows,
        virtual_reader::LayoutReports {
            measurements: shell
                .reader_measurements
                .as_ref()
                .expect("reader measurements initialized")
                .clone(),
            generation: shell.reader_generation,
            counters: shell
                .reader_laid_out
                .as_ref()
                .zip(shell.reader_peak_layout.as_ref())
                .map(|(last, peak)| (last.clone(), peak.clone())),
        },
    );
    let fixed_width = container(blocks).width(Length::Fixed(width as f32));
    let centered = container(fixed_width)
        .width(Length::Fill)
        .center_x(Length::Fill);
    let generation = shell.reader_generation;
    let scroll = container(
        scrollable(centered)
            .id(reader_scroll_id())
            .width(Length::Fill)
            .height(Length::Fill)
            .on_scroll(move |viewport| Message::ReaderScroll {
                generation,
                offset: viewport.absolute_offset().y,
                maximum: (viewport.content_bounds().height - viewport.bounds().height).max(0.0),
                viewport: viewport.bounds().height,
            }),
    )
    .width(Length::Fill)
    // Keep this wrapper Shrink so the Column's fill minimum cannot override
    // max_height; the nested Scrollable still fills this bounded area.
    .height(Length::Shrink)
    .max_height(iced::Pixels(recipe.viewport_height_dip as f32));

    column![header, scroll]
        .spacing(12)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn reader_scroll_id() -> iced::advanced::widget::Id {
    iced::advanced::widget::Id::new("reader-workload")
}

fn reader_scroll_to(offset: f32) -> Task<Message> {
    use iced::advanced::widget::{
        operate,
        operation::scrollable::{self, AbsoluteOffset},
    };
    operate(scrollable::scroll_to(
        reader_scroll_id(),
        AbsoluteOffset {
            x: None,
            y: Some(offset),
        },
    ))
}

/// Drain only native layout reports from the active width. The source text
/// and compact index are separate from the bounded active widget tree.
fn refine_reader_heights(shell: &mut Shell) -> Option<f32> {
    if !matches!(shell.reader_phase, ReaderPhase::Ready) || !shell.reader_mode {
        return None;
    }
    let mut reports = shell.reader_measurements.as_ref()?.lock();
    let pending = std::mem::take(&mut *reports);
    drop(reports);
    let index = shell.reader_heights.as_mut()?;
    let mut new_offset = shell.reader_scroll_offset;
    let mut changed = false;
    for (row, height, width, generation) in pending {
        if generation != shell.reader_generation
            || width != shell.reader_width.width_dip() as f32
            || row >= index.len()
        {
            continue;
        }
        if (index.height(row) - height).abs() > 0.01 {
            new_offset = index.refine(row, height, new_offset);
            changed = true;
        }
    }
    if changed {
        shell.reader_scroll_offset = new_offset;
        shell.reader_scroll_maximum = (index.total() - shell.reader_viewport_height).max(0.0);
        Some(new_offset)
    } else {
        None
    }
}

fn bidi_diagnostic_body(shell: &Shell) -> Element<'static, Message> {
    let Some(content) = shell.reader_content.as_ref() else {
        return text("BiDi diagnostic fixture is unavailable.")
            .size(15)
            .color(BRIGHT_TEXT)
            .into();
    };
    let case_id = BIDI_DIAGNOSTIC_CASES[shell.bidi_case_index];
    let variant = reader::bidi_diagnostic_variants()[shell.bidi_variant_index];
    let Some(reader_document::Item::Paragraph {
        text: logical_text,
        base_direction,
        style_runs,
        ..
    }) = content.workload.item_by_id(case_id)
    else {
        return text(format!("Diagnostic case {case_id} is unavailable."))
            .size(15)
            .color(BRIGHT_TEXT)
            .into();
    };
    let mapped = match reader::map_diagnostic_paragraph(
        logical_text,
        *base_direction,
        style_runs,
        variant,
    ) {
        Ok(mapped) => mapped,
        Err(error) => return text(error).size(15).color(BRIGHT_TEXT).into(),
    };
    let width = shell.reader_width.width_dip();
    let marker = if *base_direction == reader_document::BaseDirection::Rtl {
        if variant.leading_rlm {
            "present (U+200F)"
        } else {
            "absent"
        }
    } else {
        "not applicable (LTR adapter adds no RLM)"
    };
    let roles = mapped
        .runs
        .iter()
        .map(|run| format!("{:?}@{}..{}", run.role, run.bytes.start, run.bytes.end))
        .collect::<Vec<_>>()
        .join(", ");
    let summary = shell.bidi_summary;
    let summary_text = summary.map_or_else(
        || "layout trace unavailable".to_string(),
        |summary| {
            format!(
                "Cosmic-layout source order={} levels={} p-00004 bracket positions={}",
                summary.source_order.label(),
                summary
                    .levels
                    .map_or("NOT_APPLICABLE", reader::DiagnosticDisposition::label),
                summary
                    .p00004_bracket_positions
                    .map_or("NOT_APPLICABLE", reader::DiagnosticDisposition::label),
            )
        },
    );
    let paragraph = iced::widget::rich_text(
        mapped
            .runs
            .iter()
            .map(|run| {
                iced::widget::text::Span::new(mapped.text[run.bytes.clone()].to_string())
                    .font(run.role.iced_font())
                    .size(reader_workload::LAYOUT_RECIPE.body_font_size_dip)
                    .line_height(iced::widget::text::LineHeight::Absolute(iced::Pixels(
                        reader_workload::LAYOUT_RECIPE.body_line_height_dip,
                    )))
                    .color(BODY_TEXT)
            })
            .collect::<Vec<iced::widget::text::Span<'static>>>(),
    )
    .size(reader_workload::LAYOUT_RECIPE.body_font_size_dip)
    .line_height(iced::widget::text::LineHeight::Absolute(iced::Pixels(
        reader_workload::LAYOUT_RECIPE.body_line_height_dip,
    )))
    .font(Font::with_name("Noto Sans"))
    .color(BODY_TEXT)
    .width(Length::Fill);
    let fixed_paragraph = container(paragraph).width(Length::Fixed(width as f32));
    let centered_paragraph = container(fixed_paragraph)
        .width(Length::Fill)
        .center_x(Length::Fill);

    column![
        text(format!("Iced native BiDi diagnostic · {case_id}")).size(19).color(BRIGHT_TEXT),
        text(format!("Variant={} · width={width} DIP · F2 variant / F3 case / F4 width", variant.label()))
            .size(13).color(BODY_TEXT),
        text(format!("Logical source={logical_text:?}")).size(13).color(BODY_TEXT),
        text(format!("Mapped input={:?}", mapped.text)).size(13).color(BODY_TEXT),
        text(format!("Fixture direction={base_direction:?}; Iced paragraph direction API=unsupported; RLM={marker}"))
            .size(13).color(BODY_TEXT),
        text(format!("Requested span/font roles: {roles}")).size(13).color(BODY_TEXT),
        text(summary_text).size(13).color(BRIGHT_TEXT),
        centered_paragraph
    ]
    .spacing(10)
    .width(Length::Fill)
    .into()
}

fn render_reader_item(
    item: &reader_document::Item,
    item_index: usize,
    content: &ReaderContent,
    recipe: &reader_workload::LayoutRecipe,
    selection: Option<selection::SelectionBounds>,
    dragging: bool,
    track_hit_test: bool,
) -> Element<'static, Message> {
    match item {
        reader_document::Item::Heading {
            id, text: heading, ..
        } => render_selectable_rich_text(SelectableParagraphView {
            item_id: id,
            logical_text: heading,
            direction: reader_document::BaseDirection::Ltr,
            style_runs: &[reader_document::StyleRun {
                start_byte: 0,
                end_byte: heading.len(),
                style: reader_document::InlineStyle::Bold,
            }],
            font_size: recipe.heading_font_size_dip,
            line_height: recipe.heading_line_height_dip,
            selection: selection.and_then(|bounds| bounds.range_for_item(item_index, heading)),
            dragging,
            track_hit_test,
        }),
        reader_document::Item::Paragraph {
            id,
            text: paragraph,
            base_direction,
            style_runs,
            ..
        } => render_selectable_rich_text(SelectableParagraphView {
            item_id: id,
            logical_text: paragraph,
            direction: *base_direction,
            style_runs,
            font_size: recipe.body_font_size_dip,
            line_height: recipe.body_line_height_dip,
            selection: selection.and_then(|bounds| bounds.range_for_item(item_index, paragraph)),
            dragging,
            track_hit_test,
        }),
        reader_document::Item::Image { .. } => container(
            iced::widget::image(content.image.clone())
                .width(recipe.image_display_size_dip.0 as f32)
                .height(recipe.image_display_size_dip.1 as f32)
                .content_fit(iced::ContentFit::Fill),
        )
        .width(Length::Fill)
        .center_x(Length::Fill)
        .into(),
    }
}

struct SelectableParagraphView<'a> {
    item_id: &'a str,
    logical_text: &'a str,
    direction: reader_document::BaseDirection,
    style_runs: &'a [reader_document::StyleRun],
    font_size: f32,
    line_height: f32,
    selection: Option<std::ops::Range<usize>>,
    dragging: bool,
    track_hit_test: bool,
}

fn render_selectable_rich_text(view: SelectableParagraphView<'_>) -> Element<'static, Message> {
    let SelectableParagraphView {
        item_id,
        logical_text,
        direction,
        style_runs,
        font_size,
        line_height,
        selection,
        dragging,
        track_hit_test,
    } = view;
    let mapped = match reader::map_paragraph(logical_text, direction, style_runs) {
        Ok(mapped) => mapped,
        Err(error) => {
            return text(format!("Invalid fixture style ranges: {error}"))
                .size(15)
                .color(BRIGHT_TEXT)
                .into();
        }
    };
    selection::selectable_text(
        selection::SelectableParagraphConfig {
            links: Vec::new(),
            focused_link: None,
            font_family: None,
            marks: Vec::new(),
            spoken: None,
            item_id: item_id.to_owned(),
            logical_text: logical_text.to_owned(),
            mapped,
            item_offset: 0,
            alignment: iced::advanced::text::Alignment::Default,
            font_size,
            line_height,
            selection,
            dragging,
            track_hit_test,
        },
        Message::ReaderSelectionStart,
        |endpoint, point| Message::ReaderSelectionMove { endpoint, point },
        None,
        None,
    )
}

fn subscription(_: &Shell) -> Subscription<Message> {
    event::listen_with(|event, _status, _window| Some(Message::Event(event)))
}

fn title(shell: &Shell) -> String {
    if shell.bidi_diagnostic_mode {
        return bidi_diagnostic_title(shell);
    }
    if !shell.native_test_status && !shell.reader_test_status && !shell.reader_selection_test_status
    {
        return "Iced Shell PoC".into();
    }

    let mut details = Vec::new();
    if shell.reader_test_status || shell.reader_selection_test_status {
        details.push(reader_test_status(shell));
    }
    if shell.native_test_status {
        let panel = if shell.state.panel_visible() {
            "visible"
        } else {
            "hidden"
        };
        let focus = match shell.state.focused() {
            Some(Control::Info) => "info",
            Some(Control::Exit) => "exit",
            None => "none",
        };
        let active = if shell.window_active { "yes" } else { "no" };
        let hover = match shell.hovered_control {
            Some(Control::Info) => "info",
            Some(Control::Exit) => "exit",
            None => "none",
        };
        details.push(format!(
            "panel={panel};focus={focus};hover={hover};active={active}"
        ));
    }
    format!("Iced Shell PoC [{}]", details.join(";"))
}

fn bidi_diagnostic_title(shell: &Shell) -> String {
    let phase = match &shell.reader_phase {
        ReaderPhase::Disabled => "disabled",
        ReaderPhase::WaitingForWindowOpened => "waiting",
        ReaderPhase::Queued => "shell",
        ReaderPhase::Loading | ReaderPhase::LoadingFonts => "loading",
        ReaderPhase::Ready => "ready",
        ReaderPhase::Closed => "closed",
        ReaderPhase::Failed(_) => "failed",
    };
    let case_id = BIDI_DIAGNOSTIC_CASES[shell.bidi_case_index];
    let variant = reader::bidi_diagnostic_variants()[shell.bidi_variant_index];
    let width = shell.reader_width.width_dip();
    let (order, levels, brackets) = shell.bidi_summary.map_or(
        ("INCONCLUSIVE", "NOT_APPLICABLE", "NOT_APPLICABLE"),
        |summary| {
            (
                summary.source_order.label(),
                summary
                    .levels
                    .map_or("NOT_APPLICABLE", reader::DiagnosticDisposition::label),
                summary
                    .p00004_bracket_positions
                    .map_or("NOT_APPLICABLE", reader::DiagnosticDisposition::label),
            )
        },
    );
    format!(
        "Iced Shell PoC [bidi={phase};case={case_id};variant={};width={width};order={order};levels={levels};brackets={brackets}]",
        variant.label()
    )
}

fn selected_reader_copy(shell: &Shell) -> Option<String> {
    if !shell.reader_mode
        || shell.bidi_diagnostic_mode
        || !matches!(shell.reader_phase, ReaderPhase::Ready)
    {
        return None;
    }
    shell
        .reader_selection
        .as_ref()?
        .copy_text(shell.reader_content.as_ref()?.workload.items())
        .filter(|text| !text.is_empty())
}

fn reader_test_status(shell: &Shell) -> String {
    let width = shell.reader_width.width_dip();
    let (phase, body, items, error) = match &shell.reader_phase {
        ReaderPhase::Disabled => ("empty", 0, 0, "none"),
        ReaderPhase::WaitingForWindowOpened => ("waiting", 0, 0, "none"),
        ReaderPhase::Queued | ReaderPhase::Loading | ReaderPhase::LoadingFonts => {
            ("loading", 0, 0, "none")
        }
        ReaderPhase::Closed => ("closed", 0, 0, "none"),
        ReaderPhase::Ready => {
            shell
                .reader_content
                .as_ref()
                .map_or(("failed", 0, 0, "state"), |content| {
                    (
                        "ready",
                        content.workload.body_paragraph_count(),
                        content.workload.total_item_count(),
                        "none",
                    )
                })
        }
        ReaderPhase::Failed(message) => {
            let kind = if message.contains("no ancestor") {
                "root"
            } else if message.contains("missing manifest asset") {
                "missing-asset"
            } else if message.contains("checksum mismatch") {
                "checksum"
            } else if message.contains("image") {
                "image"
            } else {
                "fixture"
            };
            ("failed", 0, 0, kind)
        }
    };
    let mut result = format!(
        "reader={phase};width={width};body={body};items={items};error={error};scroll={:.0}/{:.0}",
        shell.reader_scroll_offset, shell.reader_scroll_maximum
    );
    if shell.virtual_test_status {
        let ordering = std::sync::atomic::Ordering::Relaxed;
        let first = shell.reader_first.load(ordering);
        let end = shell.reader_end.load(ordering);
        let ids = shell
            .reader_content
            .as_ref()
            .map(|content| content.workload.items());
        let first_id = ids
            .and_then(|items| items.get(first))
            .map_or("none", reader_document::Item::id);
        let last_id = ids
            .and_then(|items| items.get(end.saturating_sub(1)))
            .map_or("none", reader_document::Item::id);
        let anchor = shell
            .reader_heights
            .as_ref()
            .map(|index| {
                let row = index.window(shell.reader_scroll_offset, 0.0, 0.0).start;
                (
                    ids.and_then(|items| items.get(row))
                        .map_or("none", reader_document::Item::id),
                    shell.reader_scroll_offset - index.start(row),
                )
            })
            .unwrap_or(("none", 0.0));
        result.push_str(&format!(";built={};layout={};peak={}/{};index={};range={first}..{end};ids={first_id}..{last_id};anchor={};within={:.1}",
            shell.reader_built.load(ordering), shell.reader_laid_out.as_ref().map_or(0, |counter| counter.load(ordering)),
            shell.reader_peak_built.load(ordering), shell.reader_peak_layout.as_ref().map_or(0, |counter| counter.load(ordering)),
            shell.reader_heights.as_ref().map_or(0, virtual_reader::HeightIndex::len), anchor.0, anchor.1));
    }
    if shell.reader_selection_test_status {
        let dragging = shell
            .reader_selection
            .as_ref()
            .is_some_and(selection::SelectionState::is_dragging);
        let selection = shell
            .reader_selection
            .as_ref()
            .and_then(selection::SelectionState::endpoints)
            .map(|(anchor, focus)| {
                format!(
                    "{}@{}->{}@{}",
                    anchor.item_id, anchor.byte_offset, focus.item_id, focus.byte_offset
                )
            })
            .unwrap_or_else(|| "none".to_string());
        let hit = shell.reader_last_hit.as_ref().map_or_else(
            || "none".to_string(),
            |(endpoint, point)| {
                format!(
                    "{}@{};point={:.1},{:.1}",
                    endpoint.item_id, endpoint.byte_offset, point.x, point.y
                )
            },
        );
        let pointer = shell.reader_last_pointer.map_or_else(
            || "none".to_string(),
            |point| format!("{:.1},{:.1}", point.x, point.y),
        );
        let marks = shell
            .reader_content
            .as_ref()
            .and_then(|content| {
                let bounds = shell
                    .reader_selection
                    .as_ref()?
                    .bounds(content.workload.items())?;
                let first = shell
                    .reader_first
                    .load(std::sync::atomic::Ordering::Relaxed);
                let end = shell.reader_end.load(std::sync::atomic::Ordering::Relaxed);
                Some(
                    content.workload.items()[first.min(end)..end]
                        .iter()
                        .enumerate()
                        .filter_map(|(local, item)| {
                            let text = item.text()?;
                            let item_index = first + local;
                            let range = bounds.range_for_item(item_index, text)?;
                            Some(format!("{}@{}-{}", item.id(), range.start, range.end))
                        })
                        .collect::<Vec<_>>()
                        .join(","),
                )
            })
            .unwrap_or_default();
        result.push_str(&format!(
            ";selection={selection};drag={dragging};hit={hit};cursor={pointer};marks={marks}"
        ));
    }
    result
}

fn theme(_: &Shell) -> Theme {
    Theme::Dark
}

fn application_style(_: &Shell, _: &Theme) -> iced::theme::Style {
    iced::theme::Style {
        background_color: ROOT,
        text_color: BODY_TEXT,
    }
}

fn main() {
    platform::mark_reader_running();
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let reader_mode = reader::reader_poc_requested(&arguments);
    let reader_large = reader::reader_large_requested(&arguments);
    let bidi_diagnostic_mode = reader::bidi_diagnostic_requested(&arguments);
    let shell_mode = arguments.iter().any(|argument| argument == "--shell-poc");
    if !reader_mode && !reader_large && !bidi_diagnostic_mode && !shell_mode {
        let (path, error) = match arguments.as_slice() {
            [] => (None, None),
            [path] if !path.to_string_lossy().starts_with("--") => {
                (Some(std::path::PathBuf::from(path)), None)
            }
            [separator, path] if separator == "--" => (Some(std::path::PathBuf::from(path)), None),
            _ => (
                None,
                Some(
                    "Pass one local HTML, PDF, EPUB, text or Markdown path, or use Open to choose a file."
                        .to_owned(),
                ),
            ),
        };
        if let Err(error) = app::run(path, error) {
            eprintln!("failed to run simPl: {error}");
            std::process::exit(1);
        }
        return;
    }
    if arguments.len() != 1 {
        eprintln!("select one explicit diagnostic mode without extra arguments");
        std::process::exit(2);
    }
    if reader_mode && reader_large {
        eprintln!("select one reader workload size");
        std::process::exit(2);
    }
    if (reader_mode || reader_large) && bidi_diagnostic_mode {
        eprintln!("--reader-poc and --bidi-diagnostic are separate opt-in modes");
        std::process::exit(2);
    }

    let bidi_trace = if bidi_diagnostic_mode {
        if !reader::bidi_diagnostic_enabled(std::env::var_os(BIDI_DIAGNOSTICS_VARIABLE).as_deref())
        {
            eprintln!("--bidi-diagnostic requires {BIDI_DIAGNOSTICS_VARIABLE}=1");
            std::process::exit(2);
        }
        for variable in [
            STARTUP_MARKERS_VARIABLE,
            "ICED_SHELL_NATIVE_TEST_STATUS",
            "ICED_SHELL_READER_TEST_STATUS",
        ] {
            if std::env::var_os(variable).is_some() {
                eprintln!("--bidi-diagnostic refuses conflicting evidence gate {variable}");
                std::process::exit(2);
            }
        }
        let Some(path) = std::env::var_os(BIDI_TRACE_PATH_VARIABLE) else {
            eprintln!("--bidi-diagnostic requires {BIDI_TRACE_PATH_VARIABLE}");
            std::process::exit(2);
        };
        match bidi_diagnostic::TraceWriter::create(path) {
            Ok(trace) => Some(std::sync::Arc::new(std::sync::Mutex::new(trace))),
            Err(error) => {
                eprintln!("cannot create bounded BiDi trace: {error}");
                std::process::exit(2);
            }
        }
    } else {
        None
    };

    // Opt-in, process-scoped startup markers are deliberately not activated
    // for the separate BiDi diagnostic mode.
    let entry_emitter = if bidi_diagnostic_mode {
        None
    } else {
        shell_startup_markers::emitter_from_env(STARTUP_MARKERS_VARIABLE)
    };
    if let (Some(emitter), Some(qpc)) = (&entry_emitter, shell_startup_markers::qpc()) {
        emitter.emit(Event::ProcessEntry, qpc);
    }

    let result = iced::application(
        move || {
            Shell::new(
                reader_mode || reader_large,
                reader_large,
                bidi_diagnostic_mode,
                bidi_trace.clone(),
            )
        },
        update,
        view,
    )
    .title(title)
    .subscription(subscription)
    .theme(theme)
    .style(application_style)
    .default_font(Font::with_name("Segoe UI"))
    .window(window::Settings {
        size: Size::new(1000.0, 720.0),
        min_size: Some(Size::new(640.0, 480.0)),
        position: window::Position::Centered,
        ..window::Settings::default()
    })
    .run();

    if let Err(error) = result {
        eprintln!("failed to run Iced Shell PoC: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod reader_allocation_tests {
    use super::*;

    #[test]
    fn queued_scroll_after_close_cannot_poison_reopen_offset() {
        let mut shell = Shell::new(true, false, false, None);
        shell.reader_phase = ReaderPhase::Ready;
        shell.reader_scroll_offset = 400.0;
        let old_generation = shell.reader_generation;
        let _ = update(&mut shell, Message::ToggleReaderContent);
        let _ = update(
            &mut shell,
            Message::ReaderScroll {
                generation: old_generation,
                offset: 9_000.0,
                maximum: 20_000.0,
                viewport: 200.0,
            },
        );
        assert!(matches!(shell.reader_phase, ReaderPhase::Closed));
        assert_eq!(shell.reader_scroll_offset, 0.0);
        let _ = update(&mut shell, Message::ToggleReaderContent);
        let _ = update(
            &mut shell,
            Message::ReaderScroll {
                generation: old_generation,
                offset: 12_000.0,
                maximum: 20_000.0,
                viewport: 200.0,
            },
        );
        assert_eq!(shell.reader_scroll_offset, 0.0);
        shell.reader_phase = ReaderPhase::Ready;
        shell.reader_heights = Some(virtual_reader::HeightIndex::new(vec![40.0, 50.0]));
        shell
            .reader_measurements
            .as_ref()
            .unwrap()
            .lock()
            .push((0, 200.0, 800.0, old_generation));
        let _ = update(
            &mut shell,
            Message::ReaderScroll {
                generation: old_generation,
                offset: 20.0,
                maximum: 20_000.0,
                viewport: 200.0,
            },
        );
        assert_eq!(shell.reader_scroll_offset, 0.0);
        assert!(refine_reader_heights(&mut shell).is_none());
        assert_eq!(shell.reader_heights.as_ref().unwrap().height(0), 40.0);
    }

    #[test]
    fn ctrl_c_payload_is_nonempty_only_for_a_ready_reader_selection() {
        let workload = reader_workload::workload(reader_workload::WorkloadSize::Small);
        let mut shell = Shell::new(false, false, false, None);
        assert!(selected_reader_copy(&shell).is_none());

        shell.reader_mode = true;
        shell.reader_phase = ReaderPhase::Ready;
        shell.reader_content = Some(ReaderContent {
            fixture_revision: reader_workload::FIXTURE_REVISION.to_string(),
            workload,
            image: iced::widget::image::Handle::from_rgba(1, 1, vec![0, 0, 0, 255]),
        });
        shell.reader_selection = Some(selection::SelectionState::default());
        assert!(selected_reader_copy(&shell).is_none());

        let reader_workload::SelectionCase {
            name,
            anchor,
            focus,
            ..
        } = reader_workload::selection_cases()
            .into_iter()
            .find(|case| case.name == "style-boundary")
            .expect("golden selection case");
        let selection = shell.reader_selection.as_mut().expect("reader selection");
        selection.begin(anchor);
        selection.extend(focus);
        selection.end_drag();
        assert_eq!(
            selected_reader_copy(&shell).as_deref().map(str::as_bytes),
            Some(
                std::fs::read(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../fixtures/reader-workload/references/expected-copy")
                        .join(format!("{name}.txt"))
                )
                .expect("selection golden")
                .as_slice()
            )
        );

        shell.reader_phase = ReaderPhase::Closed;
        assert!(selected_reader_copy(&shell).is_none());
    }

    #[test]
    fn pointer_release_and_focus_loss_stop_drag_without_clearing_selection() {
        let workload = reader_workload::workload(reader_workload::WorkloadSize::Small);
        let mut shell = Shell::new(true, false, false, None);
        shell.reader_phase = ReaderPhase::Ready;
        shell.reader_content = Some(ReaderContent {
            fixture_revision: reader_workload::FIXTURE_REVISION.to_string(),
            workload: workload.clone(),
            image: iced::widget::image::Handle::from_rgba(1, 1, vec![0, 0, 0, 255]),
        });
        let start = reader_document::Endpoint {
            item_id: "p-00009".into(),
            byte_offset: 22,
        };
        let end = reader_document::Endpoint {
            item_id: "p-00009".into(),
            byte_offset: 56,
        };
        let selection = shell.reader_selection.as_mut().expect("reader selection");
        selection.begin(start.clone());
        selection.extend(end.clone());
        assert!(selection.is_dragging());

        let _ = update(
            &mut shell,
            Message::Event(iced::Event::Window(window::Event::Unfocused)),
        );
        let selection = shell.reader_selection.as_ref().expect("reader selection");
        assert!(!selection.is_dragging());
        assert_eq!(selection.endpoints(), Some((&start, &end)));
        assert!(
            selection
                .copy_text(workload.items())
                .is_some_and(|text| !text.is_empty())
        );

        let selection = shell.reader_selection.as_mut().expect("reader selection");
        selection.begin(start.clone());
        selection.extend(end.clone());
        let _ = update(
            &mut shell,
            Message::Event(iced::Event::Mouse(mouse::Event::ButtonReleased(
                mouse::Button::Left,
            ))),
        );
        let selection = shell.reader_selection.as_ref().expect("reader selection");
        assert!(!selection.is_dragging());
        assert_eq!(selection.endpoints(), Some((&start, &end)));
    }

    #[test]
    fn f5_close_and_reopen_clear_reader_selection_and_drag_state() {
        let workload = reader_workload::workload(reader_workload::WorkloadSize::Small);
        let mut shell = Shell::new(true, false, false, None);
        shell.reader_phase = ReaderPhase::Ready;
        let selection = shell.reader_selection.as_mut().expect("reader selection");
        selection.begin(reader_document::Endpoint {
            item_id: "p-00009".into(),
            byte_offset: 22,
        });
        selection.extend(reader_document::Endpoint {
            item_id: "p-00009".into(),
            byte_offset: 56,
        });
        assert!(selection.is_dragging());

        let _ = update(&mut shell, Message::ToggleReaderContent);
        assert!(matches!(shell.reader_phase, ReaderPhase::Closed));
        let selection = shell.reader_selection.as_ref().expect("reader selection");
        assert!(!selection.is_dragging());
        assert_eq!(selection.copy_text(workload.items()), None);

        let _ = update(&mut shell, Message::ToggleReaderContent);
        let selection = shell.reader_selection.as_ref().expect("reader selection");
        assert!(!selection.is_dragging());
        assert_eq!(selection.copy_text(workload.items()), None);
    }

    #[test]
    fn default_shell_allocates_no_reader_runtime_or_diagnostic_state() {
        let shell = Shell::new(false, false, false, None);
        assert!(shell.reader_measurements.is_none());
        assert!(shell.reader_laid_out.is_none());
        assert!(shell.reader_peak_layout.is_none());
        assert!(shell.reader_heights.is_none());
        assert!(shell.reader_content.is_none());
        assert!(shell.reader_selection.is_none());
        assert!(shell.interaction_trace.is_none());
        assert!(!shell.reader_selection_test_status);
        assert!(shell.reader_last_hit.is_none());
        assert!(shell.reader_last_pointer.is_none());
    }
}
