//! The contents drawer on the left of the page and the notes drawer on its
//! right. A wide window docks an open drawer beside the page, which narrows
//! the page; a narrow one lays it over the page, which keeps its width.

use super::*;
use iced::widget::{Space, column, opaque, row};

/// Windows narrower than this lay open drawers over the page.
pub(super) const OVERLAY_BELOW: f32 = 900.0;

const PADDING: f32 = 14.0;
const TITLE: f32 = 28.0;
const GAP: f32 = 12.0;

/// An open drawer's width in a window `window_width` wide.
pub(super) fn width(window_width: f32) -> f32 {
    (window_width * 0.36).clamp(230.0, 340.0)
}

/// Widths along the page's left and right sides.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Edges {
    pub(super) left: f32,
    pub(super) right: f32,
}

impl Edges {
    pub(super) fn total(self) -> f32 {
        self.left + self.right
    }
}

/// The open drawers, contents on the left and notes on the right, and whether
/// the window is narrow enough that they overlay the page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Drawers {
    open: Edges,
    overlay: bool,
}

impl Drawers {
    /// The open widths taken from the page's layout.
    pub(super) fn docked(&self) -> Edges {
        if self.overlay {
            Edges::default()
        } else {
            self.open
        }
    }

    /// The open widths drawn over the page.
    pub(super) fn overlaid(&self) -> Edges {
        if self.overlay {
            self.open
        } else {
            Edges::default()
        }
    }
}

impl Reader {
    pub(super) fn drawers(&self) -> Drawers {
        let open = |shown: bool| {
            if shown {
                width(self.window_size.width)
            } else {
                0.0
            }
        };
        Drawers {
            open: Edges {
                left: open(self.show_contents && !self.contents().is_empty()),
                right: open(self.notes.list.is_some()),
            },
            overlay: self.window_size.width < OVERLAY_BELOW,
        }
    }

    /// The size the open document lays out in: the window less the docked drawers.
    pub(super) fn document_view_size(&self) -> iced::Size {
        iced::Size::new(
            (self.window_size.width - self.drawers().docked().total()).max(1.0),
            self.window_size.height,
        )
    }

    /// Going somewhere in the book closes an overlaid contents drawer, which
    /// would cover the text just asked for. A docked one stays beside it.
    pub(super) fn close_overlaid_contents(&mut self) {
        if self.drawers().overlaid().left > 0.0 {
            self.show_contents = false;
        }
    }
}

/// The height of the contents list in a window `window_height` tall.
pub(super) fn contents_viewport(window_height: f32) -> f32 {
    (window_height - reading_chrome::HEIGHT - 2.0 * PADDING - TITLE - GAP).max(CONTENT_ROW_HEIGHT)
}

/// Both drawers below the bar, as one layer over the page. Each keeps its
/// slot while closed, so crossing the overlay width never moves either one in
/// the widget tree.
pub(super) fn layer(reader: &Reader, active: bool) -> Element<'_, Message> {
    container(row![
        opaque(contents(reader, active)),
        Space::new().width(Length::Fill),
        opaque(notes_ui::sidebar(reader, active)),
    ])
    .padding(iced::Padding {
        top: reading_chrome::HEIGHT,
        ..iced::Padding::ZERO
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn contents(reader: &Reader, active: bool) -> Element<'_, Message> {
    let contents = reader.contents();
    if !reader.show_contents || contents.is_empty() {
        return Space::new().width(0).into();
    }
    let range = reader.contents_range();
    let (start, end) = (range.start, range.end);
    let mut entries = column![Space::new().height(start as f32 * CONTENT_ROW_HEIGHT)];
    for (relative, entry) in contents[start..end].iter().enumerate() {
        let label = text(entry.label.clone())
            .size(13)
            .wrapping(iced::widget::text::Wrapping::None);
        entries = entries.push(
            container(toned_button(
                reader,
                Control::ContentsEntry(start + relative),
                label,
                active.then(|| Message::EpubChapter {
                    index: entry.chapter,
                    fragment: entry.fragment.clone(),
                }),
                ui::ButtonTone::Subtle,
                reader
                    .book
                    .as_ref()
                    .and_then(|b| b.epub.as_ref())
                    .map_or(entry.chapter <= reader.anchor().row, |c| {
                        entry.chapter == c.index
                    }),
            ))
            .height(CONTENT_ROW_HEIGHT)
            .width(Length::Fill)
            .clip(true)
            .padding(iced::Padding {
                left: entry.depth.min(6) as f32 * 12.0,
                ..iced::Padding::default()
            }),
        );
    }
    entries = entries.push(Space::new().height((contents.len() - end) as f32 * CONTENT_ROW_HEIGHT));
    container(
        column![
            container(text("Contents").size(18).font(ui::HEADING)).height(TITLE),
            scrollable(entries)
                .direction(ui::vertical_scrollbar())
                .style(ui::scroll_style)
                .id(iced::advanced::widget::Id::new("epub-contents"))
                .height(contents_viewport(reader.window_size.height))
                .on_scroll(|viewport| Message::ContentsScrolled(viewport.absolute_offset().y)),
        ]
        .spacing(GAP),
    )
    .padding(PADDING)
    .width(width(reader.window_size.width))
    .height(Length::Fill)
    .style(ui::panel)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A book whose first heading gives it a contents list.
    fn headed_book(window: f32) -> Reader {
        let mut source = Arc::try_unwrap(super::super::tests::book("drawers")).unwrap();
        source.items[0] = Item::Heading {
            id: "paragraph-0".into(),
            text: "Chapter One".into(),
            level: 1,
        };
        Reader {
            book: Some(Arc::new(source)),
            window_size: Size::new(window, 800.0),
            ..Reader::default()
        }
    }

    fn drawers(window: f32, contents: bool, notes: bool) -> Drawers {
        let mut reader = headed_book(window);
        reader.show_contents = contents;
        reader.notes.list = notes.then(crate::notes::ListTab::default);
        reader.drawers()
    }

    #[test]
    fn wide_windows_dock_open_drawers_and_narrow_ones_overlay_them() {
        let wide = drawers(1400.0, true, true);
        assert_eq!(
            wide.docked(),
            Edges {
                left: 340.0,
                right: 340.0
            }
        );
        assert_eq!(wide.overlaid(), Edges::default());
        let narrow = drawers(600.0, true, false);
        assert_eq!(narrow.docked(), Edges::default());
        assert_eq!(
            narrow.overlaid(),
            Edges {
                left: 230.0,
                right: 0.0
            }
        );
        assert_eq!(drawers(900.0, true, true).overlaid(), Edges::default());
        assert_eq!(drawers(899.0, true, true).docked(), Edges::default());
    }

    #[test]
    fn a_book_without_headings_has_no_contents_drawer() {
        let mut reader = headed_book(1400.0);
        reader.book = Some(super::super::tests::book("no-headings"));
        reader.show_contents = true;
        assert_eq!(reader.drawers().docked(), Edges::default());
    }

    #[test]
    fn only_docked_drawers_narrow_the_document_view() {
        let mut reader = headed_book(860.0);
        reader.show_contents = true;
        reader.notes.list = Some(crate::notes::ListTab::default());
        assert_eq!(
            reader.document_view_size(),
            Size::new(860.0, 800.0),
            "overlaid drawers leave the page its width"
        );
        reader.window_size.width = 1000.0;
        assert_eq!(
            reader.document_view_size(),
            Size::new(320.0, 800.0),
            "docked drawers take theirs from it"
        );
    }
}
