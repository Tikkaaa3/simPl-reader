pub mod bidi_diagnostic;

pub mod interaction_trace;
pub mod reader;
pub mod selection;
pub mod virtual_reader;

pub mod adapter_diagnostic {
    use log::{Level, LevelFilter, Log, Metadata, Record};
    use std::ffi::OsStr;
    use std::fmt::{self, Write as _};
    use std::io::Write as _;
    use std::sync::atomic::{AtomicU8, Ordering};

    pub const ENVIRONMENT_VARIABLE: &str = "ICED_SHELL_ADAPTER_DIAGNOSTICS";
    pub const MAX_RECORD_BYTES: usize = 1_024;
    const TARGET: &str = "iced_wgpu::window::compositor";
    const PREFIX: &str = "Selected: ";
    const EMPTY: u8 = 0;
    const SELECTED: u8 = 1;
    const INVALID: u8 = 2;

    static COLLECTOR: Collector = Collector::new();
    static LOGGER: AdapterLogger = AdapterLogger;

    pub struct Collector {
        state: AtomicU8,
    }

    impl Default for Collector {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Collector {
        #[must_use]
        pub const fn new() -> Self {
            Self {
                state: AtomicU8::new(EMPTY),
            }
        }

        /// Collects one bounded selected-adapter record and an optional fixed
        /// invalid signal. Formatting never retains more than
        /// [`MAX_RECORD_BYTES`] of any record.
        #[must_use]
        pub fn collect(
            &self,
            target: &str,
            level: Level,
            arguments: fmt::Arguments<'_>,
        ) -> Option<String> {
            if target != TARGET || level != Level::Info {
                return None;
            }

            let mut message = BoundedBuffer::new();
            let formatting_result = message.write_fmt(arguments);
            if formatting_result.is_err() {
                return message
                    .text
                    .starts_with(PREFIX)
                    .then(|| self.invalidate("oversized-selected-record"))
                    .flatten();
            }
            if !message.text.starts_with(PREFIX) {
                return None;
            }

            match self
                .state
                .compare_exchange(EMPTY, SELECTED, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => Some(format!(
                    "ICED_SHELL_ADAPTER_BEGIN target={TARGET}\n{}\nICED_SHELL_ADAPTER_END\n",
                    message.text
                )),
                Err(SELECTED) => self.invalidate("duplicate-selected-record"),
                Err(_) => None,
            }
        }

        fn invalidate(&self, reason: &'static str) -> Option<String> {
            let previous = self.state.swap(INVALID, Ordering::AcqRel);
            (previous != INVALID).then(|| format!("ICED_SHELL_ADAPTER_INVALID {reason}\n"))
        }
    }

    struct BoundedBuffer {
        text: String,
    }

    impl BoundedBuffer {
        fn new() -> Self {
            Self {
                text: String::with_capacity(MAX_RECORD_BYTES),
            }
        }
    }

    impl fmt::Write for BoundedBuffer {
        fn write_str(&mut self, value: &str) -> fmt::Result {
            let remaining = MAX_RECORD_BYTES.saturating_sub(self.text.len());
            if value.len() <= remaining {
                self.text.push_str(value);
                return Ok(());
            }

            let mut end = remaining.min(value.len());
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            self.text.push_str(&value[..end]);
            Err(fmt::Error)
        }
    }

    struct AdapterLogger;

    impl Log for AdapterLogger {
        fn enabled(&self, metadata: &Metadata<'_>) -> bool {
            metadata.level() == Level::Info && metadata.target() == TARGET
        }

        fn log(&self, record: &Record<'_>) {
            if !self.enabled(record.metadata()) {
                return;
            }

            if let Some(output) = COLLECTOR.collect(record.target(), record.level(), *record.args())
            {
                let _ = std::io::stderr().lock().write_all(output.as_bytes());
            }
        }

        fn flush(&self) {
            let _ = std::io::stderr().lock().flush();
        }
    }

    #[must_use]
    pub fn requested(value: Option<&OsStr>) -> bool {
        value == Some(OsStr::new("1"))
    }

    /// Installs the bounded diagnostic sink only for an exact process opt-in.
    ///
    /// Returns whether the sink was installed. Installation can fail only if
    /// some other process-local logger was installed first.
    pub fn install_if_requested() -> Result<bool, &'static str> {
        if !requested(std::env::var_os(ENVIRONMENT_VARIABLE).as_deref()) {
            return Ok(false);
        }

        log::set_logger(&LOGGER).map_err(|_| "another logger was already installed")?;
        log::set_max_level(LevelFilter::Info);
        Ok(true)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Control {
    Info,
    Exit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusDirection {
    Next,
    Previous,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HoverEvent {
    Enter(Control),
    Exit(Control),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivationKey {
    Enter,
    Space,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    ToggleInfo,
    HideInfo,
    Activate(Control),
    Focus(Control),
    PointerPress(Control),
    PointerRelease(Option<Control>),
}

#[derive(Debug, Eq, PartialEq)]
pub struct ShellState {
    panel_visible: bool,
    exit_requested: bool,
    focused: Option<Control>,
    pending_pointer: Option<Control>,
}

impl Default for ShellState {
    fn default() -> Self {
        Self {
            panel_visible: false,
            exit_requested: false,
            focused: Some(Control::Info),
            pending_pointer: None,
        }
    }
}

impl ShellState {
    #[must_use]
    pub const fn panel_visible(&self) -> bool {
        self.panel_visible
    }

    #[must_use]
    pub const fn exit_requested(&self) -> bool {
        self.exit_requested
    }

    #[must_use]
    pub const fn focused(&self) -> Option<Control> {
        self.focused
    }

    #[must_use]
    pub const fn pending_pointer(&self) -> Option<Control> {
        self.pending_pointer
    }

    /// Applies one shell command and reports whether observable state changed.
    pub fn dispatch(&mut self, command: Command) -> bool {
        match command {
            Command::ToggleInfo => {
                self.panel_visible = !self.panel_visible;
                true
            }
            Command::HideInfo => {
                let changed = self.panel_visible;
                self.panel_visible = false;
                changed
            }
            Command::Activate(control) => {
                self.focused = Some(control);
                match control {
                    Control::Info => {
                        self.panel_visible = !self.panel_visible;
                        true
                    }
                    Control::Exit => {
                        let changed = !self.exit_requested;
                        self.exit_requested = true;
                        changed
                    }
                }
            }
            Command::Focus(control) => {
                let changed = self.focused != Some(control);
                self.focused = Some(control);
                changed
            }
            Command::PointerPress(control) => {
                let changed =
                    self.focused != Some(control) || self.pending_pointer != Some(control);
                self.focused = Some(control);
                self.pending_pointer = Some(control);
                changed
            }
            Command::PointerRelease(control) => {
                let pressed = self.pending_pointer.take();
                match pressed {
                    Some(pressed_control) if Some(pressed_control) == control => {
                        self.dispatch(Command::Activate(pressed_control))
                    }
                    _ => false,
                }
            }
        }
    }

    /// Applies a single key activation. Repeated key events are ignored so a
    /// held key cannot dispatch a second command through a shell-owned path.
    pub fn activate_key(&mut self, control: Control, key: ActivationKey, repeat: bool) -> bool {
        let _ = key;
        if repeat {
            return false;
        }
        self.dispatch(Command::Activate(control))
    }

    /// Returns the toolbar control selected by one forward or reverse traversal.
    #[must_use]
    pub const fn traverse(&self, direction: FocusDirection) -> Control {
        traverse_focus(self.focused, direction)
    }
}

pub const TOOLBAR_HEIGHT: f32 = 62.0;
pub const TOOLBAR_SEPARATOR_HEIGHT: f32 = 1.0;
pub const TOOLBAR_CONTENT_HEIGHT: f32 = TOOLBAR_HEIGHT - TOOLBAR_SEPARATOR_HEIGHT;
pub const CONTROL_WIDTH: f32 = 72.0;
pub const CONTROL_HEIGHT: f32 = 38.0;

/// Updates the current pointer target without letting a late sibling exit
/// erase a newer enter notification.
#[must_use]
pub fn hover_transition(current: Option<Control>, event: HoverEvent) -> Option<Control> {
    match event {
        HoverEvent::Enter(control) => Some(control),
        HoverEvent::Exit(control) if current == Some(control) => None,
        HoverEvent::Exit(_) => current,
    }
}

/// Chooses the next toolbar focus from the actual focused control.
///
/// `None` represents focus on the body/root or outside the two controls.
#[must_use]
pub const fn traverse_focus(focused: Option<Control>, direction: FocusDirection) -> Control {
    match (focused, direction) {
        (None, FocusDirection::Next) | (Some(Control::Exit), FocusDirection::Next) => Control::Info,
        (Some(Control::Info), FocusDirection::Next) => Control::Exit,
        (None, FocusDirection::Previous) | (Some(Control::Info), FocusDirection::Previous) => {
            Control::Exit
        }
        (Some(Control::Exit), FocusDirection::Previous) => Control::Info,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActivationKey, Command, Control, FocusDirection, HoverEvent, ShellState,
        adapter_diagnostic, hover_transition, traverse_focus,
    };

    #[test]
    fn adapter_diagnostic_is_strictly_opt_in() {
        assert!(!adapter_diagnostic::requested(None));
        assert!(!adapter_diagnostic::requested(Some(std::ffi::OsStr::new(
            "0"
        ))));
        assert!(adapter_diagnostic::requested(Some(std::ffi::OsStr::new(
            "1"
        ))));
    }

    #[test]
    fn adapter_collector_ignores_irrelevant_records_and_frames_one_selection() {
        let collector = adapter_diagnostic::Collector::new();
        assert_eq!(
            collector.collect(
                "iced_wgpu::window::compositor",
                log::Level::Info,
                format_args!("Available adapters: {}", "x".repeat(2_000))
            ),
            None
        );
        assert_eq!(
            collector.collect("other", log::Level::Info, format_args!("Selected: ignored")),
            None
        );

        let selected =
            "Selected: AdapterInfo { name: \"GPU\", device_type: DiscreteGpu, backend: Dx12 }";
        assert_eq!(
            collector.collect(
                "iced_wgpu::window::compositor",
                log::Level::Info,
                format_args!("{selected}")
            ),
            Some("ICED_SHELL_ADAPTER_BEGIN target=iced_wgpu::window::compositor\nSelected: AdapterInfo { name: \"GPU\", device_type: DiscreteGpu, backend: Dx12 }\nICED_SHELL_ADAPTER_END\n".into())
        );
    }

    #[test]
    fn adapter_collector_emits_a_bounded_duplicate_signal() {
        let collector = adapter_diagnostic::Collector::new();
        let selected = format_args!("Selected: AdapterInfo {{ name: \"first\" }}");
        assert!(
            collector
                .collect("iced_wgpu::window::compositor", log::Level::Info, selected)
                .is_some()
        );
        assert_eq!(
            collector.collect(
                "iced_wgpu::window::compositor",
                log::Level::Info,
                format_args!("Selected: AdapterInfo {{ name: \"second\" }}")
            ),
            Some("ICED_SHELL_ADAPTER_INVALID duplicate-selected-record\n".into())
        );
        assert_eq!(
            collector.collect(
                "iced_wgpu::window::compositor",
                log::Level::Info,
                format_args!("Selected: AdapterInfo {{ name: \"third\" }}")
            ),
            None
        );
    }

    #[test]
    fn adapter_collector_bounds_formatting_at_byte_edges() {
        let exact = "x".repeat(adapter_diagnostic::MAX_RECORD_BYTES - "Selected: ".len());
        let collector = adapter_diagnostic::Collector::new();
        let output = collector
            .collect(
                "iced_wgpu::window::compositor",
                log::Level::Info,
                format_args!("Selected: {exact}"),
            )
            .expect("exact byte limit is accepted");
        assert!(output.contains(&exact));

        let oversized = "x".repeat(adapter_diagnostic::MAX_RECORD_BYTES);
        let collector = adapter_diagnostic::Collector::new();
        assert_eq!(
            collector.collect(
                "iced_wgpu::window::compositor",
                log::Level::Info,
                format_args!("Selected: {oversized}")
            ),
            Some("ICED_SHELL_ADAPTER_INVALID oversized-selected-record\n".into())
        );
    }

    #[test]
    fn sibling_hover_exit_cannot_erase_the_new_control() {
        let hover = hover_transition(None, HoverEvent::Enter(Control::Exit));
        let hover = hover_transition(hover, HoverEvent::Enter(Control::Info));
        let hover = hover_transition(hover, HoverEvent::Exit(Control::Exit));
        assert_eq!(hover, Some(Control::Info));

        let hover = hover_transition(None, HoverEvent::Enter(Control::Info));
        let hover = hover_transition(hover, HoverEvent::Enter(Control::Exit));
        let hover = hover_transition(hover, HoverEvent::Exit(Control::Info));
        assert_eq!(hover, Some(Control::Exit));
    }

    #[test]
    fn hover_exit_only_clears_its_own_control() {
        assert_eq!(
            hover_transition(Some(Control::Info), HoverEvent::Exit(Control::Info)),
            None
        );
        assert_eq!(
            hover_transition(Some(Control::Exit), HoverEvent::Exit(Control::Info)),
            Some(Control::Exit)
        );
    }

    #[test]
    fn default_focus_is_info() {
        assert_eq!(ShellState::default().focused(), Some(Control::Info));
    }

    #[test]
    fn panel_commands_are_repeatable_and_hidden_escape_is_a_noop() {
        let mut state = ShellState::default();
        assert!(!state.panel_visible());

        assert!(!state.dispatch(Command::HideInfo));
        assert!(!state.panel_visible());
        assert!(state.dispatch(Command::ToggleInfo));
        assert!(state.panel_visible());
        assert!(state.dispatch(Command::ToggleInfo));
        assert!(!state.panel_visible());
        assert!(!state.dispatch(Command::HideInfo));
    }

    #[test]
    fn focus_traversal_wraps_and_enters_from_non_toolbar_focus() {
        assert_eq!(
            traverse_focus(Some(Control::Info), FocusDirection::Next),
            Control::Exit
        );
        assert_eq!(
            traverse_focus(Some(Control::Exit), FocusDirection::Next),
            Control::Info
        );
        assert_eq!(
            traverse_focus(Some(Control::Info), FocusDirection::Previous),
            Control::Exit
        );
        assert_eq!(
            traverse_focus(Some(Control::Exit), FocusDirection::Previous),
            Control::Info
        );
        assert_eq!(traverse_focus(None, FocusDirection::Next), Control::Info);
        assert_eq!(
            traverse_focus(None, FocusDirection::Previous),
            Control::Exit
        );
    }

    #[test]
    fn activation_keys_dispatch_once_and_ignore_repeat() {
        let mut state = ShellState::default();

        assert!(state.activate_key(Control::Info, ActivationKey::Enter, false));
        assert!(state.panel_visible());
        assert!(!state.activate_key(Control::Info, ActivationKey::Enter, true));
        assert!(state.panel_visible());
        assert!(state.activate_key(Control::Info, ActivationKey::Space, false));
        assert!(!state.panel_visible());
    }

    #[test]
    fn activating_exit_requests_exit_without_changing_panel() {
        let mut state = ShellState::default();
        state.dispatch(Command::ToggleInfo);

        assert!(state.dispatch(Command::Activate(Control::Exit)));
        assert!(state.exit_requested());
        assert!(state.panel_visible());
        assert!(!state.dispatch(Command::Activate(Control::Exit)));
    }

    #[test]
    fn canceled_pointer_release_clears_activation_without_dispatching() {
        let mut state = ShellState::default();

        assert!(state.dispatch(Command::PointerPress(Control::Info)));
        assert_eq!(state.focused(), Some(Control::Info));
        assert_eq!(state.pending_pointer(), Some(Control::Info));
        assert!(!state.dispatch(Command::PointerRelease(Some(Control::Exit))));
        assert_eq!(state.pending_pointer(), None);
        assert!(!state.panel_visible());

        assert!(state.dispatch(Command::PointerPress(Control::Exit)));
        assert!(state.dispatch(Command::PointerRelease(Some(Control::Exit))));
        assert!(state.exit_requested());
    }
}
