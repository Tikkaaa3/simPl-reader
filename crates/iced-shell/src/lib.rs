pub mod bidi_diagnostic;

pub mod interaction_trace;
pub mod reader;
pub mod selection;
pub mod virtual_reader;

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
        ActivationKey, Command, Control, FocusDirection, HoverEvent, ShellState, hover_transition,
        traverse_focus,
    };

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
