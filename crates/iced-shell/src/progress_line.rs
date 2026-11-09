//! The reading position as one value, shared by the Continue Reading card and
//! the progress line along the bottom of the reading area. The page strips
//! widget hosts the line, so presses, the wheel and the cursor follow the
//! same rules over both.

use super::*;
use iced::Rectangle;
use iced::advanced::renderer;

/// The height of the band along the bottom that hovers and jumps.
const BAND: f32 = 24.0;
const REST: f32 = 2.0;
const HOVERED: f32 = 6.0;
const LABEL_SIZE: f32 = 11.0;
const LABEL_LINE: f32 = 14.0;
const LABEL_PADDING: [f32; 2] = [2.0, 6.0];

/// How far through the open book or PDF the reader is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Progress {
    /// The current page, from 1.
    pub(super) page: u32,
    pub(super) total: u32,
    /// From 0 at the start of the first page to 1 at the end of the last.
    pub(super) fraction: f32,
}

impl Progress {
    pub(super) fn label(&self) -> String {
        format!(
            "Page {} of {} · {}%",
            self.page,
            self.total,
            shelf::percent(self.fraction)
        )
    }
}

/// The page a press at `fraction` of the line's width lands on.
pub(super) fn page_at(fraction: f32, total: u32) -> u32 {
    ((fraction * total as f32).ceil() as u32).clamp(1, total.max(1))
}

/// The bottom band between the page strips, over the reading area `bounds`
/// as the strips see it.
pub(super) fn band(bounds: Rectangle, strip_width: f32, bottom_inset: f32) -> Rectangle {
    let [previous, next] = page_strips::regions(bounds, strip_width, bottom_inset);
    let left = previous.x + previous.width;
    let height = BAND.min(previous.height);
    Rectangle {
        x: left,
        y: previous.y + previous.height - height,
        width: (next.x - left).max(0.0),
        height,
    }
}

/// The drawn line, along the bottom of the band.
pub(super) fn line(band: Rectangle, hovered: bool) -> Rectangle {
    let height = if hovered { HOVERED } else { REST }.min(band.height);
    Rectangle {
        y: band.y + band.height - height,
        height,
        ..band
    }
}

/// The page a press at `point` in `band` jumps to.
pub(super) fn page_under(band: Rectangle, point: iced::Point, total: u32) -> u32 {
    let fraction = if band.width > 0.0 {
        ((point.x - band.x) / band.width).clamp(0.0, 1.0)
    } else {
        0.0
    };
    page_at(fraction, total)
}

pub(super) fn draw(
    renderer: &mut iced::Renderer,
    theme: &iced::Theme,
    band: Rectangle,
    progress: Progress,
    hovered: bool,
    labelled: bool,
) {
    use iced::advanced::Renderer as _;
    use iced::advanced::text::Renderer as _;
    let palette = ui::palette(theme);
    let track = line(band, hovered);
    let quad = |bounds| renderer::Quad {
        bounds,
        ..renderer::Quad::default()
    };
    renderer.fill_quad(quad(track), palette.border);
    renderer.fill_quad(
        quad(Rectangle {
            width: track.width * progress.fraction,
            ..track
        }),
        palette.accent,
    );
    if !labelled {
        return;
    }
    let label = progress.label();
    let text_width = selection::text_width(&label, ui::SANS, LABEL_SIZE).ceil();
    let [padding_y, padding_x] = LABEL_PADDING;
    let size = Size::new(text_width + 2.0 * padding_x, LABEL_LINE + 2.0 * padding_y);
    let backing = Rectangle {
        x: (band.center_x() - size.width / 2.0).round(),
        y: track.y - size.height,
        width: size.width,
        height: size.height,
    };
    renderer.fill_quad(
        renderer::Quad {
            bounds: backing,
            border: iced::Border {
                color: palette.border,
                width: 1.0,
                // The label stands on the line, so no page text shows
                // between them.
                radius: iced::border::Radius::default().top(4.0),
            },
            ..renderer::Quad::default()
        },
        palette.background,
    );
    renderer.fill_text(
        iced::advanced::Text {
            content: label,
            bounds: backing.size(),
            size: iced::Pixels(LABEL_SIZE),
            line_height: iced::advanced::text::LineHeight::Absolute(iced::Pixels(LABEL_LINE)),
            font: ui::SANS,
            align_x: iced::advanced::text::Alignment::Center,
            align_y: iced::alignment::Vertical::Center,
            shaping: iced::advanced::text::Shaping::Advanced,
            wrapping: iced::advanced::text::Wrapping::None,
        },
        backing.center(),
        palette.muted,
        backing,
    );
}

impl Reader {
    /// Turns to `page`, from 1, as the page field would. A book is addressed
    /// by its global page number, never by a publisher's page label.
    pub(super) fn progress_jump(&mut self, page: u32) -> Task<Message> {
        let index = page.saturating_sub(1);
        if let Some(pdf) = &self.pdf {
            let document = pdf.document().id;
            return update_inner(
                self,
                Message::Pdf {
                    document,
                    message: pdf_reader::Message::GoTo(index as usize),
                },
            );
        }
        self.go_to_page_number(index).unwrap_or_else(Task::none)
    }

    /// `None` without a document, and while a book is still being paginated.
    pub(super) fn progress(&self) -> Option<Progress> {
        let (page, within, total) = if let Some(pdf) = &self.pdf {
            let position = pdf.position();
            (
                position.page,
                position.within,
                pdf.document().pages.len() as u32,
            )
        } else {
            let page = book_pages::current(self.pages(), self.offset)?;
            let within = ((self.offset - page.top) / page.height).clamp(0.0, 1.0);
            (page.number, within, self.page_total() as u32)
        };
        (total > 0).then(|| Progress {
            page: page + 1,
            total,
            fraction: ((page as f32 + within) / total as f32).clamp(0.0, 1.0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Point;

    /// Rounds to hundredths, as the drawer's width is not a whole pixel.
    fn rounded(rect: Rectangle) -> [f32; 4] {
        [rect.x, rect.y, rect.width, rect.height].map(|value| (value * 100.0).round() / 100.0)
    }

    #[test]
    fn progress_line_runs_between_the_strips() {
        // Window width, reading area width (the window less the notes drawer),
        // bottom inset, then the strips' inner edges and the band's top.
        for (window, area, inset, left, right, top) in [
            (860.0, 860.0, 0.0, 60.0, 786.0, 676.0),
            (860.0, 550.4, 0.0, 60.0, 476.4, 676.0),
            (1400.0, 1400.0, 0.0, 98.0, 1288.0, 676.0),
            (1400.0, 1060.0, 0.0, 98.0, 948.0, 676.0),
            (860.0, 860.0, pdf_reader::STATUS_HEIGHT, 60.0, 786.0, 650.0),
        ] {
            let bounds = Rectangle::new(Point::ORIGIN, Size::new(area, 700.0));
            let strip = page_strips::width(window);
            let [previous, next] = page_strips::regions(bounds, strip, inset);
            let band = band(bounds, strip, inset);
            assert_eq!(band.x, previous.x + previous.width, "meets the left strip");
            assert_eq!(band.x + band.width, next.x, "meets the right strip");
            let width = right - left;
            assert_eq!(
                rounded(band),
                [left, top, width, 24.0],
                "band at {window} in {area} above {inset}"
            );
            assert_eq!(rounded(line(band, false)), [left, top + 22.0, width, 2.0]);
            assert_eq!(rounded(line(band, true)), [left, top + 18.0, width, 6.0]);
        }
    }

    #[test]
    fn click_fraction_maps_to_page() {
        assert_eq!(page_at(0.5, 14), 7);
        assert_eq!(page_at(1.0, 14), 14);
        assert_eq!(page_at(0.0, 14), 1);
        assert_eq!(page_at(0.51, 14), 8);
    }
}
