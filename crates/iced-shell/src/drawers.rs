//! The contents drawer on the left of the page and the notes drawer on its
//! right. An open drawer docks beside the page, which narrows the page, while
//! the whole page still fits beside it; otherwise it lays over the page, which
//! keeps its width.

use super::*;
use iced::widget::{Space, column, opaque, row};

/// The narrowest reading area a page counts as fitting in, so fit width never
/// shrinks the page to nothing beside a drawer: the room one drawer leaves a
/// 900px window.
const PAGE_FLOOR: f32 = 576.0;

const PADDING: f32 = 14.0;
const TITLE: f32 = 28.0;
const GAP: f32 = 12.0;

/// An open drawer's width in a window `window_width` wide.
pub(super) fn width(window_width: f32) -> f32 {
    (window_width * 0.36).clamp(230.0, 340.0)
}

/// The page's left or right side, where a drawer opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    Left,
    Right,
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
/// they overlay the page because it would not fit beside them.
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
    /// The open document fits, without scrolling sideways, in a reading area
    /// `available` wide.
    pub(super) fn page_fits(&self, available: f32) -> bool {
        if available < PAGE_FLOOR {
            return false;
        }
        match &self.pdf {
            Some(pdf) => pdf.fits_in(available),
            None => self.fit_width || book_desk_width(self, available) <= available,
        }
    }

    pub(super) fn drawers(&self) -> Drawers {
        let open = |shown: bool| {
            if shown {
                width(self.window_size.width)
            } else {
                0.0
            }
        };
        let open = Edges {
            left: open(self.show_contents && !self.contents().is_empty()),
            right: open(self.notes.list.is_some()),
        };
        Drawers {
            open,
            overlay: !self.page_fits(self.window_size.width - open.total()),
        }
    }

    /// The size the open document lays out in: the window less the docked drawers.
    pub(super) fn document_view_size(&self) -> iced::Size {
        iced::Size::new(
            (self.window_size.width - self.drawers().docked().total()).max(1.0),
            self.window_size.height,
        )
    }

    /// Opening one drawer closes the other unless the page fits between both.
    pub(super) fn make_room_for(&mut self, opened: Side) {
        let window = self.window_size.width;
        if self.page_fits(window - 2.0 * width(window)) {
            return;
        }
        match opened {
            Side::Left => self.notes.list = None,
            Side::Right => self.show_contents = false,
        }
    }

    /// Two drawers that no longer leave room for the page, after a resize or a
    /// zoom, keep the notes drawer.
    pub(super) fn fit_drawers(&mut self) {
        let drawers = self.drawers();
        if drawers.open.left > 0.0 && drawers.open.right > 0.0 {
            self.make_room_for(Side::Right);
            if !self.show_contents && matches!(self.focused, Some(Control::ContentsEntry(_))) {
                self.focused = None;
            }
        }
    }

    /// Lays the PDF out again in the width the drawers leave it.
    pub(super) fn resize_pdf_view(&mut self) -> Task<Message> {
        let size = self.document_view_size();
        match &mut self.pdf {
            Some(pdf) => forward_pdf(pdf.document().id, pdf.resize(size, self.scale_factor)),
            None => Task::none(),
        }
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

/// The heading the reader is in: the last one at or before `row`.
pub(super) fn current_heading(
    contents: &[reader_document::epub::TocEntry],
    row: usize,
) -> Option<usize> {
    contents.iter().rposition(|entry| entry.chapter <= row)
}

fn contents(reader: &Reader, active: bool) -> Element<'_, Message> {
    let contents = reader.contents();
    if !reader.show_contents || contents.is_empty() {
        return Space::new().width(0).into();
    }
    let epub_chapter = reader
        .book
        .as_ref()
        .and_then(|book| book.epub.as_ref())
        .map(|chapter| chapter.index);
    let heading = current_heading(contents, reader.anchor().row);
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
                epub_chapter.map_or(heading == Some(start + relative), |chapter| {
                    entry.chapter == chapter
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
        let mut reader = Reader {
            book: Some(Arc::new(source)),
            window_size: Size::new(window, 800.0),
            ..Reader::default()
        };
        reader.width = reader.reading_width(window);
        reader
    }

    fn drawers(window: f32, contents: bool, notes: bool) -> Drawers {
        let mut reader = headed_book(window);
        reader.show_contents = contents;
        reader.notes.list = notes.then(crate::notes::ListTab::default);
        reader.drawers()
    }

    #[test]
    fn a_drawer_docks_only_where_the_whole_page_fits_beside_it() {
        // At zoom 1 the paper and the desk around it need 766px.
        for (window, docked) in [(1105.0, false), (1106.0, true), (1400.0, true)] {
            let drawers = drawers(window, true, false);
            assert_eq!(
                (drawers.docked().left, drawers.overlaid().left),
                if docked { (340.0, 0.0) } else { (0.0, 340.0) },
                "at {window}"
            );
        }
        for window in (1106..=1700).map(|window| window as f32) {
            assert_eq!(
                drawers(window, true, false).docked().left,
                width(window),
                "one drawer stays docked at every width from 1106, so at {window}"
            );
        }
        for window in (1446..=1700).map(|window| window as f32) {
            assert_eq!(
                drawers(window, true, true).docked().total(),
                2.0 * width(window),
                "two drawers stay docked at every width from 1446, so at {window}"
            );
        }
        assert_eq!(
            drawers(1446.0, true, true).docked(),
            Edges {
                left: 340.0,
                right: 340.0
            }
        );
        assert_eq!(
            drawers(600.0, false, true).overlaid(),
            Edges {
                left: 0.0,
                right: 230.0
            }
        );
        let mut fitted = headed_book(1000.0);
        fitted.fit_width = true;
        fitted.show_contents = true;
        assert_eq!(
            fitted.drawers().docked().left,
            340.0,
            "fit width shrinks the page to the room beside the drawer"
        );
        for (window, docked) in [(899.0, 0.0), (900.0, 324.0)] {
            fitted.window_size.width = window;
            assert_eq!(
                fitted.drawers().docked().left,
                docked,
                "fit width keeps the page at least 576px, at {window}"
            );
        }
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
        reader.notes.list = Some(crate::notes::ListTab::default());
        assert_eq!(
            reader.document_view_size(),
            Size::new(860.0, 800.0),
            "an overlaid drawer leaves the page its width"
        );
        reader.window_size.width = 1300.0;
        assert_eq!(
            reader.document_view_size(),
            Size::new(960.0, 800.0),
            "a docked drawer takes its width from it"
        );
    }
}
