use iced::widget::{button, column, container, mouse_area, row, scrollable, stack, text, Space};
use iced::{
    gradient, keyboard, mouse, window, Background, Border, Color, Element, Length, Radians, Size,
    Subscription, Task, Theme,
};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone)]
enum Msg {
    TopEnter,
    TopExit,
    BarEnter,
    BarExit,
    StripEnter(Side),
    StripExit(Side),
    StripPress(Side),
    Scrolled(f32),
    StripWheel(mouse::ScrollDelta),
    Frame(Instant),
    Resized(Size),
    Key(keyboard::Key),
    Noop,
}

struct Proto {
    page: u32,
    size: Size,
    bar_hovered: bool,
    bar_target: bool,
    bar_alpha: f32,
    hide_at: Option<Instant>,
    hover: Option<Side>,
    variant: u8,
    last: Instant,
}

const PAGES: u32 = 14;

fn strip_width(window_width: f32) -> f32 {
    (window_width * 0.07).clamp(48.0, 128.0)
}

fn boot() -> (Proto, Task<Msg>) {
    let now = Instant::now();
    (
        Proto {
            page: 4,
            size: Size::new(1400.0, 900.0),
            bar_hovered: false,
            bar_target: true,
            bar_alpha: 1.0,
            hide_at: Some(now + Duration::from_millis(2000)),
            hover: None,
            variant: 0,
            last: now,
        },
        Task::none(),
    )
}

fn update(s: &mut Proto, m: Msg) -> Task<Msg> {
    let now = Instant::now();
    match m {
        Msg::TopEnter | Msg::BarEnter => {
            println!("bar reveal ({m:?})");
            s.bar_hovered = true;
            s.bar_target = true;
            s.hide_at = None;
        }
        Msg::TopExit | Msg::BarExit => {
            s.bar_hovered = false;
            s.hide_at = Some(now + Duration::from_millis(1500));
        }
        Msg::StripEnter(side) => s.hover = Some(side),
        Msg::StripExit(side) if s.hover == Some(side) => s.hover = None,
        Msg::StripExit(_) => {}
        Msg::StripPress(side) => {
            let before = s.page;
            match side {
                Side::Left if s.page > 1 => s.page -= 1,
                Side::Right if s.page < PAGES => s.page += 1,
                _ => {}
            }
            println!("strip press {side:?}: page {before} -> {}", s.page);
        }
        Msg::Scrolled(y) => println!("page scrolled y={y:.0}"),
        Msg::StripWheel(delta) => {
            let y = match delta {
                mouse::ScrollDelta::Lines { y, .. } => -y * 60.0,
                mouse::ScrollDelta::Pixels { y, .. } => -y,
            };
            println!("strip wheel forwarded dy={y:.0}");
            s.last = now;
            return iced::widget::operation::scroll_by(
                "page",
                iced::widget::scrollable::AbsoluteOffset { x: 0.0, y },
            );
        }
        Msg::Frame(at) => {
            if let Some(deadline) = s.hide_at {
                if at >= deadline && !s.bar_hovered {
                    s.bar_target = false;
                    s.hide_at = None;
                    println!("bar auto-hide");
                }
            }
            let dt = at.saturating_duration_since(s.last).as_secs_f32();
            let step = dt / 0.15;
            s.bar_alpha = if s.bar_target {
                (s.bar_alpha + step).min(1.0)
            } else {
                (s.bar_alpha - step).max(0.0)
            };
        }
        Msg::Resized(size) => {
            s.size = size;
            println!("resized {:.0}x{:.0} strip={:.0}", size.width, size.height, strip_width(size.width));
        }
        Msg::Key(keyboard::Key::Character(c)) if c.as_str() == "v" => {
            s.variant = (s.variant + 1) % 3;
            println!("variant {}", s.variant);
        }
        Msg::Key(_) | Msg::Noop => {}
    }
    s.last = now;
    Task::none()
}

const PARAGRAPH: &str = "The book lay open on the kitchen table, exactly where she had left it the evening before. Beyond the window, the garden was slowly becoming visible: first the stone path, then the pear tree, then the small blue gate at the end of the wall.";

fn page_view(s: &Proto) -> Element<'_, Msg> {
    let mut body = column![text(format!("Chapter 1 · page {} of {PAGES}", s.page)).size(30)].spacing(22);
    for _ in 0..10 {
        body = body.push(text(PARAGRAPH).size(20).line_height(1.6));
    }
    let paper = container(body.padding(48))
        .max_width(720)
        .style(|_: &Theme| container::Style {
            background: Some(Color::WHITE.into()),
            border: Border { color: Color::from_rgb8(0xe4, 0xe4, 0xe4), width: 1.0, radius: 2.0.into() },
            ..Default::default()
        });
    scrollable(container(paper).center_x(Length::Fill).padding([24, 0]))
        .id("page")
        .on_scroll(|v| Msg::Scrolled(v.absolute_offset().y))
        .height(Length::Fill)
        .into()
}

fn strip(s: &Proto, side: Side) -> Element<'_, Msg> {
    let hovered = s.hover == Some(side);
    let enabled = match side {
        Side::Left => s.page > 1,
        Side::Right => s.page < PAGES,
    };
    let show = hovered && enabled;
    let glyph = if side == Side::Left { "‹" } else { "›" };
    let variant = s.variant;
    let icon_color = Color::from_rgba(0.1, 0.1, 0.1, if show { 0.75 } else { 0.0 });
    let chevron = text(glyph).size(40).line_height(1.0).color(icon_color);
    let chevron: Element<'_, Msg> = if variant == 1 {
        container(chevron)
            .center(48)
            .style(move |_: &Theme| container::Style {
                background: Some(Color::from_rgba(1.0, 1.0, 1.0, if show { 0.7 } else { 0.0 }).into()),
                border: Border { radius: 24.0.into(), color: Color::from_rgba(0.0, 0.0, 0.0, if show { 0.08 } else { 0.0 }), width: 1.0 },
                ..Default::default()
            })
            .into()
    } else {
        chevron.into()
    };
    let zone = container(chevron)
        .width(strip_width(s.size.width))
        .height(Length::Fill)
        .center_y(Length::Fill)
        .center_x(strip_width(s.size.width))
        .style(move |_: &Theme| {
            if !show {
                return container::Style::default();
            }
            let background = if variant == 2 {
                Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.035))
            } else {
                let edge = Color::from_rgba(0.0, 0.0, 0.0, 0.07);
                let clear = Color::from_rgba(0.0, 0.0, 0.0, 0.0);
                let angle = if side == Side::Left { Radians(std::f32::consts::FRAC_PI_2) } else { Radians(-std::f32::consts::FRAC_PI_2) };
                Background::Gradient(gradient::Linear::new(angle).add_stop(0.0, clear).add_stop(1.0, edge).into())
            };
            container::Style { background: Some(background), ..Default::default() }
        });
    let mut area = mouse_area(zone)
        .on_enter(Msg::StripEnter(side))
        .on_exit(Msg::StripExit(side))
        .on_scroll(Msg::StripWheel);
    if enabled {
        area = area.on_press(Msg::StripPress(side)).interaction(mouse::Interaction::Pointer);
    }
    area.into()
}

fn bar(s: &Proto) -> Element<'_, Msg> {
    if s.bar_alpha <= 0.0 {
        return mouse_area(Space::new().width(Length::Fill).height(48))
            .on_enter(Msg::TopEnter)
            .on_exit(Msg::TopExit)
            .into();
    }
    let a = s.bar_alpha;
    let label = move |t: &'static str| text(t).size(13).color(Color::from_rgba(0.15, 0.15, 0.15, a));
    let content = row![

        button(label("← Library")).style(button::text),
        Space::new().width(Length::Fill),
        label("4 / 14"),
        Space::new().width(Length::Fill),
        button(label("Aa")).style(button::text),
        button(label("Find")).style(button::text),
        button(label("Listen")).style(button::text),
        button(label("Notes")).style(button::text),
    ]
    .spacing(8)
    .padding([0, 16])
    .align_y(iced::Alignment::Center);
    let shell = container(content)
        .width(Length::Fill)
        .height(44)
        .center_y(44)
        .style(move |_: &Theme| container::Style {
            background: Some(Color::from_rgba(0.98, 0.98, 0.98, 0.94 * a).into()),
            border: Border { color: Color::from_rgba(0.0, 0.0, 0.0, 0.08 * a), width: 1.0, radius: 0.0.into() },
            shadow: iced::Shadow { color: Color::from_rgba(0.0, 0.0, 0.0, 0.08 * a), offset: iced::Vector::new(0.0, 2.0), blur_radius: 12.0 },
            ..Default::default()
        });
    mouse_area(shell).on_enter(Msg::BarEnter).on_exit(Msg::BarExit).into()
}

fn view(s: &Proto) -> Element<'_, Msg> {
    let strips = row![strip(s, Side::Left), Space::new().width(Length::Fill), strip(s, Side::Right)].height(Length::Fill);
    let top = column![bar(s), Space::new().height(Length::Fill)];
    let hud = container(
        text(format!("variant {} · window {:.0} · strip {:.0}px", s.variant, s.size.width, strip_width(s.size.width)))
            .size(11)
            .color(Color::from_rgba(0.3, 0.3, 0.3, 0.8)),
    )
    .align_bottom(Length::Fill)
    .align_right(Length::Fill)
    .padding(8);
    container(stack![page_view(s), strips, top, hud])
        .style(|_: &Theme| container::Style {
            background: Some(Color::from_rgb8(0xf2, 0xf1, 0xee).into()),
            ..Default::default()
        })
        .into()
}

fn subscription(s: &Proto) -> Subscription<Msg> {
    let mut subs = vec![
        window::resize_events().map(|(_, size)| Msg::Resized(size)),
        keyboard::listen().map(|event| match event {
            keyboard::Event::KeyPressed { key, .. } => Msg::Key(key),
            _ => Msg::Noop,
        }),
    ];
    let animating = (s.bar_target && s.bar_alpha < 1.0) || (!s.bar_target && s.bar_alpha > 0.0);
    if s.hide_at.is_some() || animating {
        subs.push(window::frames().map(Msg::Frame));
    }
    Subscription::batch(subs)
}

fn main() -> iced::Result {
    iced::application(boot, update, view)
        .subscription(subscription)
        .window(window::Settings { size: Size::new(1400.0, 900.0), ..Default::default() })
        .title(|_: &Proto| "chrome-proto".to_string())
        .theme(|_: &Proto| Theme::Light)
        .run()
}
