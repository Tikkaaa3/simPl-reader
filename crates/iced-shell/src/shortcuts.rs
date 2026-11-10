//! Which keys do what, as the Shortcuts section of Settings lists them.
use super::*;
use iced::widget::Column;

pub(super) struct Group {
    pub title: &'static str,
    /// `(keys, action)` rows in the order they are listed.
    pub rows: &'static [(&'static str, &'static str)],
}

pub(super) const GROUPS: &[Group] = &[
    Group {
        title: "General",
        rows: &[
            ("Tab / Shift+Tab", "Move between controls"),
            ("Enter / Space", "Activate the focused control"),
            ("Escape", "Close a menu, panel or dialog"),
            ("F1", "Open or close this list"),
            ("F8", "Keep the reading bar visible"),
            ("F11", "Enter or leave fullscreen"),
        ],
    },
    Group {
        title: "Library",
        rows: &[
            ("Ctrl+O", "Open a file where it is"),
            ("Ctrl+I", "Add a document to the library"),
            ("Ctrl+K", "Search and switch books"),
            ("Ctrl+R", "Show recent documents"),
            ("Ctrl+W", "Save and return to the Library"),
            (
                "Space in the Library",
                "Resume the latest unfinished book, when no control has focus",
            ),
        ],
    },
    Group {
        title: "Pages",
        rows: &[
            ("Left / Right", "Previous or next page"),
            ("Up / Down", "Scroll within the page"),
            ("Page Up / Down / Space", "Scroll a screen at a time"),
            (
                "Ctrl+Home / Ctrl+End",
                "Top or bottom of the page; first or last page of a PDF document",
            ),
            ("Ctrl+L", "Go to a page"),
            ("Ctrl+T", "Show or hide the contents of an EPUB"),
            ("Ctrl+Page Up / Down", "Previous or next EPUB chapter"),
            ("Alt+Left", "Return from an internal link"),
        ],
    },
    Group {
        title: "Find and zoom",
        rows: &[
            ("Ctrl+F", "Find in the book; press again to close"),
            (
                "Enter / Shift+Enter",
                "Next or previous match while finding",
            ),
            ("Ctrl+plus / minus / 0", "Zoom in, out or back to 100%"),
            ("Ctrl+wheel / pinch", "Zoom in or out"),
            ("Ctrl+Shift+F", "Fit width, or back to the previous zoom"),
            (
                "Hold the middle button",
                "Scroll; move away to change speed, release or Escape to stop",
            ),
        ],
    },
    Group {
        title: "Notes",
        rows: &[
            ("Ctrl+C", "Copy the selected text"),
            ("Ctrl+A", "Select all"),
            ("Ctrl+H", "Highlight the selection"),
            ("Ctrl+D", "Add or remove a bookmark on the page"),
            ("Ctrl+B", "Show or hide bookmarks, highlights and notes"),
            ("Select text", "Highlight it or add a note"),
            ("Ctrl+Enter in a note", "Save the note"),
            ("Right-click a highlight", "Edit or remove it"),
            (
                "Right-click a page",
                "Bookmark it, or start reading aloud from that place",
            ),
        ],
    },
    Group {
        title: "Listen",
        rows: &[("Ctrl+Shift+U", "Read aloud from the current page, or stop")],
    },
];

/// The Shortcuts section: one row for each shortcut, under its group.
pub(super) fn table<'a>() -> Element<'a, Message> {
    Column::with_children(GROUPS.iter().map(|group| {
        Column::with_children(
            std::iter::once(
                text(group.title.to_uppercase())
                    .font(ui::MEDIUM)
                    .size(11)
                    .style(ui::muted_text)
                    .into(),
            )
            .chain(group.rows.iter().map(|&(keys, action)| {
                row![
                    text(keys)
                        .font(ui::MEDIUM)
                        .size(12)
                        .width(Length::FillPortion(2)),
                    text(action).size(12).width(Length::FillPortion(3)),
                ]
                .spacing(12)
                .into()
            })),
        )
        .spacing(6)
        .into()
    }))
    .spacing(18)
    .width(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(keys: &str) -> Option<&'static str> {
        GROUPS
            .iter()
            .flat_map(|group| group.rows)
            .find(|(row, _)| *row == keys)
            .map(|(_, action)| *action)
    }

    #[test]
    fn the_shortcut_list_names_the_keys_the_reader_handles() {
        assert_eq!(action("F8"), Some("Keep the reading bar visible"));
        assert_eq!(
            action("Space in the Library"),
            Some("Resume the latest unfinished book, when no control has focus")
        );
        assert!(
            GROUPS
                .iter()
                .flat_map(|group| group.rows)
                .all(|(keys, action)| !format!("{keys} {action}").to_lowercase().contains("edge"))
        );
    }
}
