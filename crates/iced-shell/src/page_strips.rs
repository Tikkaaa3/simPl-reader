//! Full-height strips along the left and right edges of the reading area that
//! turn the page when pressed. They draw nothing until the pointer is over
//! one; then it fades in a full-height shade and a chevron. [`Hover`] holds the fade and,
//! like the reading bar, takes an explicit [`Instant`] for every transition.
//! The same layer hosts the progress line between the strips' bottom ends.

use super::progress_line::{self, Progress};
use super::*;
use iced::Animation;
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, renderer, widget};
use iced::{Background, Radians, Rectangle, gradient, touch};
use std::time::{Duration, Instant};

const FADE: Duration = Duration::from_millis(150);
const CHEVRON: f32 = 30.0;
/// The shade is the theme's text colour: dark on a light page, light on a
/// dark one. It is strongest at the window edge and keeps a floor at the
/// strip's inner edge, so the hovered strip reads as one band from top to
/// bottom rather than a glow around the chevron. Light text on a dark page
/// stands out more than dark text on a light one, so the dark shade is
/// lighter.
const LIGHT_SHADE: Shade = Shade {
    edge: 0.15,
    inner: 0.055,
};
const DARK_SHADE: Shade = Shade {
    edge: 0.07,
    inner: 0.025,
};

/// The shade's opacity at the window edge and at the strip's inner edge.
struct Shade {
    edge: f32,
    inner: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    Previous,
    Next,
}

const SIDES: [Side; 2] = [Side::Previous, Side::Next];

/// What a press on each strip sends. `None` disables that strip: no shade,
/// no chevron, no hand cursor, and presses fall through to the page.
pub(super) struct Turns {
    previous: Option<Message>,
    next: Option<Message>,
}

impl Turns {
    pub(super) fn of(reader: &Reader, active: bool) -> Self {
        let mut turns = Self {
            previous: None,
            next: None,
        };
        if !active {
            return turns;
        }
        if let Some(pdf) = &reader.pdf {
            let document = pdf.document().id;
            let turn = |next: bool, message| {
                pdf.can_turn(next)
                    .then_some(Message::Pdf { document, message })
            };
            turns.previous = turn(false, pdf_reader::Message::Previous);
            turns.next = turn(true, pdf_reader::Message::Next);
        } else if reader.book.is_some() {
            turns.previous = reader.can_turn(false).then_some(Message::BookPage(false));
            turns.next = reader.can_turn(true).then_some(Message::BookPage(true));
        }
        turns
    }

    fn get(&self, side: Side) -> Option<&Message> {
        match side {
            Side::Previous => self.previous.as_ref(),
            Side::Next => self.next.as_ref(),
        }
    }
}

#[derive(Debug)]
pub(super) struct Hover {
    over: Option<Side>,
    previous: Animation<bool>,
    next: Animation<bool>,
    /// The latest instant seen, which the fades are drawn at.
    now: Instant,
}

impl Default for Hover {
    fn default() -> Self {
        Self {
            over: None,
            previous: Animation::new(false).duration(FADE),
            next: Animation::new(false).duration(FADE),
            now: Instant::now(),
        }
    }
}

impl Hover {
    fn fade(&self, side: Side) -> &Animation<bool> {
        match side {
            Side::Previous => &self.previous,
            Side::Next => &self.next,
        }
    }

    fn fade_mut(&mut self, side: Side) -> &mut Animation<bool> {
        match side {
            Side::Previous => &mut self.previous,
            Side::Next => &mut self.next,
        }
    }

    pub(super) fn hover(&mut self, over: Option<Side>, now: Instant) {
        self.tick(now);
        if self.over == over {
            return;
        }
        if let Some(side) = self.over {
            self.fade_mut(side).go_mut(false, now);
        }
        if let Some(side) = over {
            self.fade_mut(side).go_mut(true, now);
        }
        self.over = over;
    }

    pub(super) fn tick(&mut self, now: Instant) {
        self.now = self.now.max(now);
    }

    /// Frames are needed only while a fade runs, never for a settled hover.
    pub(super) fn needs_frames(&self) -> bool {
        SIDES
            .iter()
            .any(|side| self.fade(*side).is_animating(self.now))
    }

    pub(super) fn alpha(&self, side: Side) -> f32 {
        self.fade(side).interpolate(0.0, 1.0, self.now)
    }

    /// A strip that can no longer turn the page goes blank at once, even
    /// with the pointer still on it; once it can again, it fades back in
    /// under the resting pointer.
    pub(super) fn reconcile(&mut self, turns: &Turns, now: Instant) {
        self.tick(now);
        for side in SIDES {
            if turns.get(side).is_none() {
                *self.fade_mut(side) = Animation::new(false).duration(FADE);
            } else if self.over == Some(side) {
                self.fade_mut(side).go_mut(true, now);
            }
        }
    }
}

#[cfg(test)]
impl Hover {
    /// Runs any fade to its end, as the next frames would.
    pub(super) fn settle(&mut self) {
        let at = self.now + FADE;
        self.tick(at);
    }
}

impl Reader {
    /// Applies what one message changed to the strips: a page that can no
    /// longer turn, a closed document, or a dialog that blocks the page.
    pub(super) fn sync_strips(&mut self, now: Instant) {
        let turns = Turns::of(self, self.interactive() && self.confirm_remove.is_none());
        self.strips.reconcile(&turns, now);
    }
}

/// Each strip's width follows the window: 7% of it, from 48 to 128 pixels.
pub(super) fn width(window_width: f32) -> f32 {
    (window_width * 0.07).clamp(48.0, 128.0).round()
}

/// The strips over the reading area `bounds`. The right one stops short of
/// the scrollbar rail, and both stop `bottom_inset` above the bottom.
pub(super) fn regions(bounds: Rectangle, width: f32, bottom_inset: f32) -> [Rectangle; 2] {
    let inner = (bounds.width - crate::document_scroll::RAIL_WIDTH).max(0.0);
    let width = width.min(inner / 2.0);
    let height = (bounds.height - bottom_inset).max(0.0);
    [
        Rectangle {
            x: bounds.x,
            y: bounds.y,
            width,
            height,
        },
        Rectangle {
            x: bounds.x + inner - width,
            y: bounds.y,
            width,
            height,
        },
    ]
}

/// The strips as one layer to stack over the reading area.
pub(super) fn layer(reader: &Reader, active: bool) -> Element<'static, Message> {
    Element::new(Strips {
        turns: Turns::of(reader, active),
        alpha: SIDES.map(|side| reader.strips.alpha(side)),
        width: width(reader.window_size.width),
        bottom_inset: bottom_inset(reader),
        progress: reader.progress(),
        jumps: active,
        labelled: reader.bar.visible(),
    })
}

/// How far above the reading area's bottom the strips stop: above the PDF
/// status row, and above the horizontal scrollbar while the page is zoomed
/// wider than the window. Otherwise they run to the bottom edge.
pub(super) fn bottom_inset(reader: &Reader) -> f32 {
    let status = if reader.pdf.is_some() {
        pdf_reader::STATUS_HEIGHT
    } else {
        0.0
    };
    let scrollbar = if super::scrolls_sideways(reader) {
        crate::document_scroll::RAIL_WIDTH
    } else {
        0.0
    };
    status + scrollbar
}

struct Strips {
    turns: Turns,
    alpha: [f32; 2],
    width: f32,
    bottom_inset: f32,
    progress: Option<Progress>,
    /// A press on the line jumps; otherwise it only shows the position.
    jumps: bool,
    /// The reading bar shows, so the line shows its label too.
    labelled: bool,
}

/// A part of the layer that acts on the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Zone {
    Strip(Side),
    Line,
}

impl Zone {
    fn side(self) -> Option<Side> {
        match self {
            Zone::Strip(side) => Some(side),
            Zone::Line => None,
        }
    }
}

#[derive(Default)]
struct State {
    gesture: Gesture,
    last_over: Option<Zone>,
    /// The right or middle button is down. The strips step aside until it is
    /// released so the page under them keeps its context menu and autoscroll.
    other_button: bool,
}

/// The left button press the strips are following.
#[derive(Default)]
enum Gesture {
    #[default]
    None,
    /// Pressed on the page: the strips stay out of its way until the release,
    /// so a selection dragged into a strip keeps the pointer.
    Page,
    /// Pressed on a strip or the line, which already acted.
    Zone,
}

impl Strips {
    /// The enabled zone under `cursor`.
    fn zone_at(&self, bounds: Rectangle, cursor: mouse::Cursor) -> Option<Zone> {
        self.zone_at_point(bounds, cursor.position()?)
    }

    fn zone_at_point(&self, bounds: Rectangle, point: iced::Point) -> Option<Zone> {
        let regions = regions(bounds, self.width, self.bottom_inset);
        let strip = SIDES.into_iter().zip(regions).find_map(|(side, region)| {
            (region.contains(point) && self.turns.get(side).is_some()).then_some(Zone::Strip(side))
        });
        strip.or_else(|| {
            (self.jumps && self.progress.is_some() && self.band(bounds).contains(point))
                .then_some(Zone::Line)
        })
    }

    fn band(&self, bounds: Rectangle) -> Rectangle {
        progress_line::band(bounds, self.width, self.bottom_inset)
    }

    /// What a press at `point` sends.
    fn action(&self, bounds: Rectangle, point: iced::Point) -> Option<Message> {
        match self.zone_at_point(bounds, point)? {
            Zone::Strip(side) => self.turns.get(side).cloned(),
            Zone::Line => {
                let total = self.progress?.total;
                Some(Message::ProgressJump(progress_line::page_under(
                    self.band(bounds),
                    point,
                    total,
                )))
            }
        }
    }

    fn draw_strip(
        &self,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        side: Side,
        region: Rectangle,
        alpha: f32,
    ) {
        use iced::advanced::Renderer as _;
        use iced::advanced::text::Renderer as _;
        let palette = ui::palette(theme);
        let shade = if theme.extended_palette().is_dark {
            DARK_SHADE
        } else {
            LIGHT_SHADE
        };
        let angle = match side {
            Side::Previous => Radians(-std::f32::consts::FRAC_PI_2),
            Side::Next => Radians(std::f32::consts::FRAC_PI_2),
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds: region,
                ..renderer::Quad::default()
            },
            Background::Gradient(
                gradient::Linear::new(angle)
                    .add_stop(0.0, palette.text.scale_alpha(shade.inner * alpha))
                    .add_stop(1.0, palette.text.scale_alpha(shade.edge * alpha))
                    .into(),
            ),
        );
        renderer.fill_text(
            iced::advanced::Text {
                content: match side {
                    Side::Previous => "\u{e5cb}",
                    Side::Next => "\u{e5cc}",
                }
                .into(),
                bounds: iced::Size::new(CHEVRON, CHEVRON),
                size: iced::Pixels(CHEVRON),
                line_height: iced::advanced::text::LineHeight::Absolute(iced::Pixels(CHEVRON)),
                font: ui::ICONS,
                align_x: iced::advanced::text::Alignment::Center,
                align_y: iced::alignment::Vertical::Center,
                shaping: iced::advanced::text::Shaping::Basic,
                wrapping: iced::advanced::text::Wrapping::None,
            },
            region.center(),
            palette.text.scale_alpha(0.75 * alpha),
            region,
        );
    }
}

impl Widget<Message, iced::Theme, iced::Renderer> for Strips {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut widget::Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        let mut press = |state: &mut State, at: Option<iced::Point>| {
            if !matches!(state.gesture, Gesture::None) {
                return;
            }
            match at.and_then(|point| self.action(bounds, point)) {
                Some(message) => {
                    shell.publish(message);
                    shell.capture_event();
                    state.gesture = Gesture::Zone;
                }
                None => state.gesture = Gesture::Page,
            }
        };
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                press(state, cursor.position());
            }
            iced::Event::Touch(touch::Event::FingerPressed { position, .. }) => {
                press(state, Some(*position));
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | iced::Event::Touch(
                touch::Event::FingerLifted { .. } | touch::Event::FingerLost { .. },
            ) => {
                // A release never acts; a zone's own release stays with it.
                if matches!(std::mem::take(&mut state.gesture), Gesture::Zone) {
                    shell.capture_event();
                }
            }
            iced::Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Right | mouse::Button::Middle,
            )) => state.other_button = true,
            iced::Event::Mouse(mouse::Event::ButtonReleased(
                mouse::Button::Right | mouse::Button::Middle,
            )) => state.other_button = false,
            // A redraw also rechecks the hover, so strips that moved under a
            // resting pointer (the notes drawer opened, the window resized)
            // follow it without waiting for the next mouse move.
            iced::Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft)
            | iced::Event::Window(window::Event::RedrawRequested(_)) => {
                let over = match state.gesture {
                    Gesture::Page => None,
                    Gesture::None | Gesture::Zone => self.zone_at(bounds, cursor),
                };
                if over != state.last_over {
                    let was = std::mem::replace(&mut state.last_over, over);
                    let side = over.and_then(Zone::side);
                    if side != was.and_then(Zone::side) {
                        shell.publish(Message::StripHover(side));
                    }
                    // The line's hover look is drawn from the cursor, with
                    // no fade, so one redraw shows it.
                    if (over == Some(Zone::Line)) != (was == Some(Zone::Line)) {
                        shell.request_redraw();
                    }
                }
            }
            iced::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if !matches!(state.gesture, Gesture::Page) && self.zone_at(bounds, cursor).is_some()
                {
                    shell.publish(Message::ChromeWheel(*delta));
                    shell.capture_event();
                }
            }
            iced::Event::Window(window::Event::Unfocused) => {
                state.gesture = Gesture::None;
                state.other_button = false;
            }
            _ => {}
        }
    }

    /// The hand cursor over an enabled zone also hides the pointer from the
    /// page under it, which is why the zone forwards the wheel itself.
    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if !matches!(state.gesture, Gesture::Page)
            && !state.other_button
            && self.zone_at(layout.bounds(), cursor).is_some()
        {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let regions = regions(bounds, self.width, self.bottom_inset);
        for ((side, region), alpha) in SIDES.into_iter().zip(regions).zip(self.alpha) {
            if alpha > 0.0 && self.turns.get(side).is_some() {
                self.draw_strip(renderer, theme, side, region, alpha);
            }
        }
        if let Some(progress) = self.progress {
            let state = tree.state.downcast_ref::<State>();
            let hovered = !matches!(state.gesture, Gesture::Page)
                && self.zone_at(bounds, cursor) == Some(Zone::Line);
            progress_line::draw(
                renderer,
                theme,
                self.band(bounds),
                progress,
                hovered,
                hovered || self.labelled,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Point;

    #[test]
    fn strip_width_follows_the_window() {
        for (window, strip) in [
            (600.0, 48.0),
            (844.0, 59.0),
            (1384.0, 97.0),
            (2560.0, 128.0),
        ] {
            assert_eq!(width(window), strip, "at {window}");
        }
    }

    #[test]
    fn strip_regions_stop_inside_the_rail_and_above_the_bottom_inset() {
        let bounds = Rectangle::new(Point::new(0.0, 0.0), iced::Size::new(660.0, 800.0));
        assert_eq!(
            regions(bounds, 70.0, 0.0),
            [
                Rectangle::new(Point::new(0.0, 0.0), iced::Size::new(70.0, 800.0)),
                Rectangle::new(Point::new(576.0, 0.0), iced::Size::new(70.0, 800.0)),
            ]
        );
        let [_, next] = regions(bounds, 70.0, pdf_reader::STATUS_HEIGHT);
        assert_eq!(next.height, 800.0 - pdf_reader::STATUS_HEIGHT);
        let narrow = Rectangle::new(Point::new(0.0, 0.0), iced::Size::new(100.0, 800.0));
        let [previous, next] = regions(narrow, 70.0, 0.0);
        assert_eq!(
            (previous.width, next.x),
            (43.0, 43.0),
            "strips never overlap"
        );
    }

    #[test]
    fn strip_hover_fade_settles_without_frames() {
        let t0 = Instant::now();
        let mut hover = Hover {
            now: t0,
            ..Hover::default()
        };
        assert!(!hover.needs_frames());
        hover.hover(Some(Side::Next), t0);
        assert!(hover.needs_frames());
        hover.tick(t0 + FADE / 2);
        let midway = hover.alpha(Side::Next);
        assert!(0.0 < midway && midway < 1.0, "half way in: {midway}");
        hover.tick(t0 + FADE);
        assert_eq!(hover.alpha(Side::Next), 1.0);
        assert_eq!(hover.alpha(Side::Previous), 0.0);
        assert!(!hover.needs_frames(), "a settled hover needs no frames");

        let t1 = t0 + Duration::from_secs(10);
        hover.hover(Some(Side::Previous), t1);
        assert!(hover.needs_frames());
        hover.tick(t1 + FADE);
        assert_eq!(hover.alpha(Side::Next), 0.0);
        assert_eq!(hover.alpha(Side::Previous), 1.0);
        assert!(!hover.needs_frames());

        let t2 = t1 + Duration::from_secs(1);
        let next_only = Turns {
            previous: None,
            next: Some(Message::BookPage(true)),
        };
        hover.reconcile(&next_only, t2);
        assert_eq!(
            hover.alpha(Side::Previous),
            0.0,
            "a strip that can't turn goes blank at once"
        );
        assert!(!hover.needs_frames());
        let both = Turns {
            previous: Some(Message::BookPage(false)),
            next: Some(Message::BookPage(true)),
        };
        hover.reconcile(&both, t2);
        assert!(hover.needs_frames(), "it fades back in under the pointer");
        hover.tick(t2 + FADE);
        assert_eq!(hover.alpha(Side::Previous), 1.0);
        hover.hover(None, t2 + FADE);
        hover.tick(t2 + FADE * 2);
        assert_eq!(hover.alpha(Side::Previous), 0.0);
        assert!(!hover.needs_frames());
    }
}
