//! The reading position as one value, shared by the Continue Reading card and
//! the progress line along the bottom of the reading area. The page strips
//! widget hosts the line, so presses, the wheel and the cursor follow the
//! same rules over both.
//!
//! The line is cut into the book's chapters. Under the pointer it shows two
//! places: where the reader is, in the accent colour, and where a press would
//! land. The stretch between them previews the jump, so a press is never
//! blind.

use super::*;
use iced::advanced::renderer;
use iced::{Background, Color, Radians, Rectangle, gradient};

/// The height of the band along the bottom that hovers and jumps.
const BAND: f32 = 24.0;
const REST: f32 = 2.0;
const HOVERED: f32 = 3.0;
/// The space between two chapters on the line.
const GAP: f32 = 2.0;
/// A chapter narrower than this joins the piece before it, so a book of many
/// short chapters still reads as a line.
const MIN_PIECE: f32 = 6.0;
/// The fade behind the hovered line, so its labels read over page text.
const SCRIM: f32 = 104.0;
const SCRIM_SOLID: f32 = 48.0;
/// The light that rises from the line under the pointer.
const BEAM: Size = Size::new(64.0, 52.0);
const BEAM_STEPS: u8 = 8;
/// At its centre, where every step overlaps.
const BEAM_ALPHA: f32 = 0.36;
const HERE_TICK: Size = Size::new(2.0, 9.0);
const NUMBER_SIZE: f32 = 15.0;
const NUMBER_LINE: f32 = 18.0;
const TITLE_SIZE: f32 = 11.0;
const TITLE_LINE: f32 = 14.0;
const TITLE_CHARS: usize = 40;
/// From the line up to the labels' bottom.
const LABEL_LIFT: f32 = 10.0;

/// How far through the open book or PDF the reader is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Progress {
    /// The current page, from 1.
    pub(super) page: u32,
    pub(super) total: u32,
    /// How much of the book lies above the bottom of the view: 1 with the
    /// last line of the last page in view.
    pub(super) fraction: f32,
}

/// A stretch of the book that the line draws as one piece.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Chapter {
    /// Its first page, from 1.
    pub(super) first: u32,
    /// Empty where the book names no chapter.
    pub(super) title: String,
}

/// `starts` in reading order, as chapters that cover the book from page 1.
/// A start on a page that an earlier one already took is dropped.
pub(super) fn chapters(starts: impl IntoIterator<Item = Chapter>) -> Vec<Chapter> {
    let mut chapters: Vec<Chapter> = Vec::new();
    for chapter in starts {
        match chapters.last() {
            None if chapter.first > 1 => {
                chapters.push(Chapter {
                    first: 1,
                    title: String::new(),
                });
                chapters.push(chapter);
            }
            Some(last) if chapter.first <= last.first => {}
            _ => chapters.push(chapter),
        }
    }
    chapters
}

/// The chapter that holds `page`, from 1.
pub(super) fn chapter_of(chapters: &[Chapter], page: u32) -> Option<&Chapter> {
    chapters.iter().rfind(|chapter| chapter.first <= page)
}

/// The line's pieces from left to right, as the x where each starts and
/// ends. Without chapters the line is one piece.
pub(super) fn pieces(band: Rectangle, chapters: &[Chapter], total: u32) -> Vec<[f32; 2]> {
    let end = band.x + band.width;
    let mut starts = vec![band.x];
    for chapter in chapters.iter().skip(1) {
        let x = (band.x + band.width * (chapter.first - 1) as f32 / total.max(1) as f32).round();
        if x - starts[starts.len() - 1] >= MIN_PIECE && end - x >= MIN_PIECE {
            starts.push(x);
        }
    }
    let ends = starts.iter().skip(1).map(|start| start - GAP);
    let ends: Vec<f32> = ends.chain([end]).collect();
    starts.into_iter().zip(ends).map(|(a, b)| [a, b]).collect()
}

/// The page a press at `fraction` of the line's width lands on.
pub(super) fn page_at(fraction: f32, total: u32) -> u32 {
    ((fraction * total as f32).ceil() as u32).clamp(1, total.max(1))
}

/// The bottom band between the page strips, over the reading area `bounds`
/// as the strips see it.
pub(super) fn band(
    bounds: Rectangle,
    strip_width: f32,
    bottom_inset: f32,
    covered: drawers::Edges,
) -> Rectangle {
    let [previous, next] = page_strips::regions(bounds, strip_width, bottom_inset, covered).strips;
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

/// What the line shows of the book.
pub(super) struct Shown<'a> {
    pub(super) progress: Progress,
    pub(super) chapters: &'a [Chapter],
    /// The x of a pointer in the band, which a press would jump to.
    pub(super) pointer: Option<f32>,
}

/// Draws the line in `band`. Under a pointer its fade spans `fade`, the x
/// where the page strips begin and end, so it has no edge beside the line.
pub(super) fn draw(
    renderer: &mut iced::Renderer,
    theme: &iced::Theme,
    band: Rectangle,
    fade: [f32; 2],
    shown: Shown<'_>,
) {
    use iced::advanced::Renderer as _;
    let palette = ui::palette(theme);
    let Shown {
        progress,
        chapters,
        pointer,
    } = shown;
    let track = line(band, pointer.is_some());
    let here = track.x + track.width * progress.fraction;
    let quad = |x: [f32; 2], y: f32, height: f32| renderer::Quad {
        bounds: Rectangle {
            x: x[0],
            y,
            width: (x[1] - x[0]).max(0.0),
            height,
        },
        ..renderer::Quad::default()
    };
    let rising = |from: Color| {
        Background::Gradient(
            gradient::Linear::new(Radians(0.0))
                .add_stop(0.0, from)
                .add_stop(1.0, from.scale_alpha(0.0))
                .into(),
        )
    };
    let bottom = track.y + track.height;
    if let Some(pointer) = pointer {
        // Solid behind the labels, where a flat fill is cheap, and fading
        // out above them.
        renderer.fill_quad(
            quad(fade, bottom - SCRIM_SOLID, SCRIM_SOLID),
            palette.background,
        );
        renderer.fill_quad(
            quad(fade, bottom - SCRIM, SCRIM - SCRIM_SOLID),
            rising(palette.background),
        );
        // A quad takes one gradient, so the light's soft sides are steps
        // too faint to see apart.
        for step in 1..=BEAM_STEPS {
            let half = BEAM.width / 2.0 * step as f32 / BEAM_STEPS as f32;
            let beam = [
                (pointer - half).max(band.x),
                (pointer + half).min(band.x + band.width),
            ];
            renderer.fill_quad(
                quad(beam, bottom - BEAM.height, BEAM.height),
                rising(palette.text.scale_alpha(BEAM_ALPHA / BEAM_STEPS as f32)),
            );
        }
    }
    // The stretch a press would cross: ahead it fills as a ghost, behind it
    // dims what the reader would give up.
    let ghost = pointer.map(|pointer| {
        if pointer > here {
            ([here, pointer], palette.accent.scale_alpha(0.5))
        } else {
            ([pointer, here], palette.background.scale_alpha(0.7))
        }
    });
    for piece in pieces(band, chapters, progress.total) {
        let within = |x: [f32; 2]| [x[0].max(piece[0]), x[1].min(piece[1])];
        renderer.fill_quad(
            quad(piece, track.y, track.height),
            palette.text.scale_alpha(0.22),
        );
        renderer.fill_quad(
            quad(within([track.x, here]), track.y, track.height),
            palette.accent,
        );
        if let Some((stretch, colour)) = ghost {
            renderer.fill_quad(quad(within(stretch), track.y, track.height), colour);
        }
    }
    // At rest the line is all there is: the page field holds the numbers.
    let Some(pointer) = pointer else {
        return;
    };
    renderer.fill_quad(
        quad(
            [here - HERE_TICK.width / 2.0, here + HERE_TICK.width / 2.0],
            bottom - HERE_TICK.height,
            HERE_TICK.height,
        ),
        palette.accent,
    );
    let target = page_under(band, iced::Point::new(pointer, band.y), progress.total);
    let title = chapter_of(chapters, target).map_or("", |chapter| chapter.title.as_str());
    let there = Label::new(band, pointer, title, target);
    let here = Label::new(band, here, "", progress.page);
    // Beside the pointer's own number, the reader's would only crowd it.
    if !here.meets(&there) {
        here.draw(renderer, bottom, palette.accent, palette.muted);
    }
    there.draw(renderer, bottom, palette.text, palette.muted);
}

/// A page number over the line, with its chapter's title above it.
struct Label {
    title: String,
    number: String,
    /// The x of its centre, kept inside the band.
    centre: f32,
    width: f32,
    title_width: f32,
    number_width: f32,
}

impl Label {
    fn new(band: Rectangle, at: f32, title: &str, page: u32) -> Self {
        let measure = |title: &str| selection::text_width(title, ui::SANS, TITLE_SIZE);
        let mut kept = title.chars().count().min(TITLE_CHARS);
        let cut = |kept: usize| {
            if kept == title.chars().count() {
                title.to_owned()
            } else {
                let cut: String = title.chars().take(kept.saturating_sub(1)).collect();
                format!("{}…", cut.trim_end())
            }
        };
        // A narrow band, as beside an open drawer, holds less of a title.
        let mut shown = cut(kept);
        while kept > 1 && measure(&shown) > band.width {
            kept = ((kept as f32 * band.width / measure(&shown)) as usize).clamp(1, kept - 1);
            shown = cut(kept);
        }
        let title = shown;
        let number = page.to_string();
        let title_width = measure(&title).ceil();
        let number_width = selection::text_width(&number, ui::MEDIUM, NUMBER_SIZE).ceil();
        let width = number_width.max(title_width);
        let half = (width / 2.0).min(band.width / 2.0);
        Self {
            title,
            number,
            centre: at.clamp(band.x + half, band.x + band.width - half),
            width,
            title_width,
            number_width,
        }
    }

    fn meets(&self, other: &Self) -> bool {
        (self.centre - other.centre).abs() < (self.width + other.width) / 2.0 + 12.0
    }

    fn draw(&self, renderer: &mut iced::Renderer, line_bottom: f32, number: Color, title: Color) {
        use iced::advanced::text::Renderer as _;
        let mut bottom = line_bottom - LABEL_LIFT;
        let number = (
            &self.number,
            self.number_width,
            ui::MEDIUM,
            NUMBER_SIZE,
            NUMBER_LINE,
            number,
        );
        let title = (
            &self.title,
            self.title_width,
            ui::SANS,
            TITLE_SIZE,
            TITLE_LINE,
            title,
        );
        for (content, width, font, size, height, colour) in [number, title] {
            if content.is_empty() {
                continue;
            }
            // The renderer repaints a text from its position by its bounds,
            // whatever its alignment. A label that moves is therefore placed
            // by its own left top corner, or its old pixels would stay.
            let bounds = Rectangle {
                x: (self.centre - width / 2.0).floor(),
                y: bottom - height,
                width: width + 1.0,
                height,
            };
            renderer.fill_text(
                iced::advanced::Text {
                    content: content.clone(),
                    bounds: bounds.size(),
                    size: iced::Pixels(size),
                    line_height: iced::advanced::text::LineHeight::Absolute(iced::Pixels(height)),
                    font,
                    align_x: iced::advanced::text::Alignment::Left,
                    align_y: iced::alignment::Vertical::Top,
                    shaping: iced::advanced::text::Shaping::Advanced,
                    wrapping: iced::advanced::text::Wrapping::None,
                },
                bounds.position(),
                colour,
                bounds,
            );
            bottom -= height;
        }
    }
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
        // The line follows the bottom of the view, so the end of the book
        // fills it. Measured from the top of the view it could not: the top
        // never reaches the end of the last page.
        let (page, seen, total) = if let Some(pdf) = &self.pdf {
            (
                pdf.position().page,
                pdf.pages_seen(),
                pdf.document().pages.len() as u32,
            )
        } else {
            let page = book_pages::current(self.pages(), self.offset)?;
            // A page is a whole sheet, so a short one ends in blank paper.
            // It is read once its last line is in view.
            let text = book_pages::TOP + page.content.end - page.content.start;
            let within = (self.offset + self.viewport - page.top) / text.max(1.0);
            (
                page.number,
                page.number as f32 + within.clamp(0.0, 1.0),
                self.page_total() as u32,
            )
        };
        (total > 0).then(|| Progress {
            page: page + 1,
            total,
            fraction: (seen / total as f32).clamp(0.0, 1.0),
        })
    }

    /// The open book's chapters by their first page. An EPUB's start at the
    /// files its contents name, as the page map numbers whole files and a
    /// file with no entry continues the chapter before it. Another book's
    /// start at its headings. A PDF has none, so its line is one piece.
    pub(super) fn progress_chapters(&self) -> Vec<Chapter> {
        let (Some(book), Some(atlas)) = (&self.book, &self.atlas) else {
            return Vec::new();
        };
        if self.pdf.is_some() || atlas.fingerprint != book.fingerprint {
            return Vec::new();
        }
        if let Some(epub) = &book.epub {
            let first = |file: usize| Some(atlas.sections.get(file)?.pages.first()?.number + 1);
            let contents = &epub.document.contents;
            if contents.is_empty() {
                return chapters((0..atlas.sections.len()).filter_map(|file| {
                    Some(Chapter {
                        first: first(file)?,
                        title: String::new(),
                    })
                }));
            }
            let mut starts: Vec<Chapter> = contents
                .iter()
                .filter_map(|entry| {
                    Some(Chapter {
                        first: first(entry.chapter)?,
                        title: entry.label.clone(),
                    })
                })
                .collect();
            // A publisher may list entries out of reading order. The sort
            // is stable, so a file's first listed entry names it.
            starts.sort_by_key(|chapter| chapter.first);
            return chapters(starts);
        }
        let pages = self.pages();
        chapters(self.contents().iter().filter_map(|entry| {
            let page = pages.get(pages.partition_point(|page| page.rows.end <= entry.chapter))?;
            Some(Chapter {
                first: page.number + 1,
                title: entry.label.clone(),
            })
        }))
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
        let none = drawers::Edges::default();
        let left_drawer = drawers::Edges {
            left: 300.0,
            right: 0.0,
        };
        let right_drawer = drawers::Edges {
            left: 0.0,
            right: 300.0,
        };
        // Window width, reading area width (the window less any docked
        // drawer), the overlaid drawers, bottom inset, then the strips' inner
        // edges and the band's top.
        for (window, area, covered, inset, left, right, top) in [
            (860.0, 860.0, none, 0.0, 60.0, 800.0, 676.0),
            (860.0, 860.0, right_drawer, 0.0, 60.0, 500.0, 676.0),
            (860.0, 860.0, left_drawer, 0.0, 360.0, 800.0, 676.0),
            (1400.0, 1400.0, none, 0.0, 98.0, 1302.0, 676.0),
            (1400.0, 1060.0, none, 0.0, 98.0, 962.0, 676.0),
            (
                860.0,
                860.0,
                none,
                pdf_reader::STATUS_HEIGHT,
                60.0,
                800.0,
                650.0,
            ),
        ] {
            let bounds = Rectangle::new(Point::ORIGIN, Size::new(area, 700.0));
            let strip = page_strips::width(window);
            let [previous, next] = page_strips::regions(bounds, strip, inset, covered).strips;
            let band = band(bounds, strip, inset, covered);
            assert_eq!(band.x, previous.x + previous.width, "meets the left strip");
            assert_eq!(band.x + band.width, next.x, "meets the right strip");
            let width = right - left;
            assert_eq!(
                rounded(band),
                [left, top, width, 24.0],
                "band at {window} in {area} above {inset}"
            );
            assert_eq!(rounded(line(band, false)), [left, top + 22.0, width, 2.0]);
            assert_eq!(rounded(line(band, true)), [left, top + 21.0, width, 3.0]);
        }
    }

    fn chapter(first: u32, title: &str) -> Chapter {
        Chapter {
            first,
            title: title.into(),
        }
    }

    #[test]
    fn chapters_cover_the_book_from_its_first_page() {
        assert_eq!(chapters([]), []);
        assert_eq!(
            chapters([chapter(1, "One"), chapter(9, "Two")]),
            [chapter(1, "One"), chapter(9, "Two")]
        );
        assert_eq!(
            chapters([chapter(4, "One"), chapter(9, "Two")]),
            [chapter(1, ""), chapter(4, "One"), chapter(9, "Two")],
            "the pages before the first heading are a piece without a name"
        );
        assert_eq!(
            chapters([chapter(1, "One"), chapter(1, "Part"), chapter(9, "Two")]),
            [chapter(1, "One"), chapter(9, "Two")],
            "two starts on one page are one chapter"
        );
    }

    #[test]
    fn a_page_belongs_to_the_last_chapter_that_starts_at_or_before_it() {
        let list = [chapter(1, "One"), chapter(9, "Two"), chapter(31, "Three")];
        for (page, title) in [
            (1, "One"),
            (8, "One"),
            (9, "Two"),
            (30, "Two"),
            (99, "Three"),
        ] {
            assert_eq!(chapter_of(&list, page).unwrap().title, title, "page {page}");
        }
        assert_eq!(chapter_of(&[], 5), None);
    }

    #[test]
    fn the_line_is_cut_where_chapters_start() {
        let band = Rectangle::new(Point::new(100.0, 676.0), Size::new(1000.0, 24.0));
        assert_eq!(pieces(band, &[], 100), [[100.0, 1100.0]]);
        // Chapters at pages 1, 11 and 51 of 100.
        let list = [chapter(1, "One"), chapter(11, "Two"), chapter(51, "Three")];
        assert_eq!(
            pieces(band, &list, 100),
            [[100.0, 198.0], [200.0, 598.0], [600.0, 1100.0]]
        );
        // A chapter of one page in a thousand is half a pixel wide: it joins
        // the piece before it, and so does one at the very end.
        let crowded = [
            chapter(1, "One"),
            chapter(500, "Two"),
            chapter(501, "Short"),
            chapter(999, "Last"),
        ];
        assert_eq!(
            pieces(band, &crowded, 1000),
            [[100.0, 597.0], [599.0, 1100.0]]
        );
    }

    #[test]
    fn a_label_stays_inside_a_narrow_band() {
        ui::load_test_fonts();
        // The band at a 540 px window with the contents drawer open.
        let band = Rectangle::new(Point::new(300.0, 676.0), Size::new(214.0, 24.0));
        let long = "Wide Words Make Many Whole Wide Windows Wider";
        for at in [300.0, 407.0, 514.0] {
            let label = Label::new(band, at, long, 12);
            assert!(label.title.ends_with('…'), "cut: {:?}", label.title);
            assert!(label.title.chars().count() > 8, "kept: {:?}", label.title);
            assert!(
                label.centre - label.width / 2.0 >= band.x
                    && label.centre + label.width / 2.0 <= band.x + band.width,
                "at {at}: {} wide around {}",
                label.width,
                label.centre
            );
        }
        let wide = Rectangle::new(Point::new(100.0, 676.0), Size::new(1000.0, 24.0));
        assert_eq!(Label::new(wide, 600.0, "The Tide", 12).title, "The Tide");
        assert_eq!(Label::new(wide, 600.0, long, 12).title.chars().count(), 40);
    }

    #[test]
    fn click_fraction_maps_to_page() {
        assert_eq!(page_at(0.5, 14), 7);
        assert_eq!(page_at(1.0, 14), 14);
        assert_eq!(page_at(0.0, 14), 1);
        assert_eq!(page_at(0.51, 14), 8);
    }
}
