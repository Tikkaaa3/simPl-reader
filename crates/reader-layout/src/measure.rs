//! The desktop's item layout without a window, event loop or selection state.
use crate::{
    book::Book,
    fonts, paragraph,
    style::{BookStyle, MINIMAL},
    text::{FontRole, map_document_paragraph},
    themes,
};
use iced_core::layout::{Limits, Node};
use iced_core::text::{Alignment, Paragraph, Renderer as TextRenderer};
use iced_core::widget::Tree;
use iced_core::{Element, Layout, Length, Pixels, Rectangle, Size, Widget, mouse, renderer};
use iced_renderer::Renderer;
use iced_widget::{container, image, row, text};
use reader_document::{BaseDirection, InlineStyle, Item, StyleRun, reading::Options};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

type NativeParagraph = <Renderer as TextRenderer>::Paragraph;
type ItemElement = Element<'static, (), iced_widget::Theme, Renderer>;

struct ReadingParagraph {
    mapped: crate::text::MappedParagraph,
    family: Option<&'static str>,
    size: f32,
    line: f32,
    alignment: Alignment,
}
impl Widget<(), iced_widget::Theme, Renderer> for ReadingParagraph {
    fn size(&self) -> Size<Length> {
        Size::new(paragraph::width(self.alignment), Length::Shrink)
    }
    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &Limits) -> Node {
        let runs = paragraph::runs(&self.mapped, self.family);
        let spans = paragraph::spans(
            runs.iter().map(|(s, f)| (s.as_str(), *f)),
            self.size,
            self.line,
        );
        paragraph::node(limits, self.alignment, |bounds| {
            NativeParagraph::with_spans(paragraph::text(
                spans.as_slice(),
                bounds,
                self.size,
                self.line,
            ))
            .min_bounds()
        })
    }
    fn draw(
        &self,
        _: &Tree,
        _: &mut Renderer,
        _: &iced_widget::Theme,
        _: &renderer::Style,
        _: Layout<'_>,
        _: mouse::Cursor,
        _: &Rectangle,
    ) {
    }
}

pub fn styled_item_size(
    style: &BookStyle,
    book: &Book,
    index: usize,
    item: &Item,
    body: f32,
) -> f32 {
    if let Some(source) = &book.pdf_source
        && let Some(block) = source.conversion.blocks.get(index)
    {
        return body * block.size_ratio.clamp(0.8, 2.0);
    }
    style.block_size(item, book.structure.get(item.id()), body)
}
pub fn book_item_size(book: &Book, index: usize, item: &Item, body: f32) -> f32 {
    styled_item_size(&MINIMAL, book, index, item, body)
}

fn render_item(
    book: &Book,
    index: usize,
    item: &Item,
    width: f32,
    body: f32,
    style: &BookStyle,
    family: Option<&'static str>,
) -> ItemElement {
    if let Some(source) = &book.pdf_source
        && let Some(rect) = source.conversion.illustrations.get(item.id())
    {
        let info = source.document.pages[source.conversion.blocks[index].sources[0].page as usize];
        let placement = source
            .conversion
            .placements
            .get(item.id())
            .copied()
            .unwrap_or(reader_pdf::book::Placement {
                offset: 0.0,
                width: 1.0,
            });
        let w = width * placement.width;
        let h =
            w * info.height * (rect.bottom - rect.top) / (info.width * (rect.right - rect.left));
        return container(
            container(text("Loading illustration…").size(14))
                .width(w)
                .height(h)
                .center_x(w),
        )
        .width(width)
        .height(h)
        .padding(iced_core::Padding {
            left: width * placement.offset,
            ..iced_core::Padding::ZERO
        })
        .into();
    }
    if let Item::Image { asset_path, .. } = item {
        return match book.images.get(asset_path) {
            Some(asset) => {
                let scale = (width / asset.width.max(1) as f32).min(1.0);
                container(
                    image(asset.handle.clone())
                        .width(asset.width as f32 * scale)
                        .height(asset.height as f32 * scale),
                )
                .center_x(Length::Fill)
                .into()
            }
            None => text("Image unavailable").size(body).into(),
        };
    }
    let logical = item.text().unwrap_or_default();
    let heading = [StyleRun {
        start_byte: 0,
        end_byte: logical.len(),
        style: InlineStyle::Bold,
    }];
    let (direction, styles) = match item {
        Item::Paragraph {
            base_direction,
            style_runs,
            ..
        } => (*base_direction, style_runs.as_slice()),
        _ => (BaseDirection::Ltr, heading.as_slice()),
    };
    let pdf = book
        .pdf_source
        .as_ref()
        .and_then(|s| s.conversion.blocks.get(index))
        .map(|b| &b.layout);
    let size = styled_item_size(style, book, index, item, body);
    if let Some(reader_pdf::book::BlockLayout::Toc {
        number_start,
        indent,
    }) = pdf
        && *number_start <= logical.len()
        && logical.is_char_boundary(*number_start)
    {
        let title_end = number_start.saturating_sub(1);
        let part = |start: usize, end: usize, alignment| -> ItemElement {
            let styles = styles
                .iter()
                .filter_map(|run| {
                    let a = run.start_byte.max(start);
                    let b = run.end_byte.min(end);
                    (a < b).then_some(StyleRun {
                        start_byte: a - start,
                        end_byte: b - start,
                        style: run.style,
                    })
                })
                .collect::<Vec<_>>();
            match map_document_paragraph(&logical[start..end], direction, &styles) {
                Ok(mapped) => Element::new(ReadingParagraph {
                    mapped,
                    family,
                    size,
                    line: size * style.line_height,
                    alignment,
                }),
                Err(error) => text(format!("Cannot display this entry: {error}")).into(),
            }
        };
        let number_width = ((logical[*number_start..].chars().count() as f32 * size * 0.85) + size)
            .max(size * 3.0);
        return container(
            row![
                container(part(0, title_end, Alignment::Default)).width(Length::Fill),
                container(part(*number_start, logical.len(), Alignment::Right))
                    .width(number_width)
                    .align_x(iced_core::alignment::Horizontal::Right)
            ]
            .spacing(size * 0.5),
        )
        .width(width)
        .padding(iced_core::Padding {
            left: width * indent.clamp(0.0, 0.2),
            ..iced_core::Padding::ZERO
        })
        .into();
    }
    let mapped = match mapped_item(book, item) {
        Ok(mapped) => mapped,
        Err(error) => return text(format!("Cannot display this paragraph: {error}")).into(),
    };
    let alignment = match pdf {
        Some(reader_pdf::book::BlockLayout::Centered) => Alignment::Center,
        Some(reader_pdf::book::BlockLayout::Right) => Alignment::Right,
        _ => Alignment::Default,
    };
    let paragraph = Element::new(ReadingParagraph {
        mapped,
        family,
        size,
        line: size * style.line_height,
        alignment,
    });
    let mut padding = style.block_padding(item, book.structure.get(item.id()), body, width);
    if let Some(source) = &book.pdf_source
        && let Some(block) = source.conversion.blocks.get(index)
        && let Some(info) = source.document.pages.get(block.sources[0].page as usize)
    {
        padding.top += width * (info.height / info.width.max(1.0)) * block.top_gap.clamp(0.0, 0.55);
    }
    if let Some(
        reader_pdf::book::BlockLayout::List { indent }
        | reader_pdf::book::BlockLayout::Inset { indent },
    ) = pdf
    {
        padding.left += width * indent.clamp(0.0, 0.2);
    }
    let mut block = container(paragraph).padding(padding);
    match alignment {
        Alignment::Center => block = block.center_x(Length::Fill),
        Alignment::Right => {
            block = block
                .width(Length::Fill)
                .align_x(iced_core::alignment::Horizontal::Right)
        }
        _ => {}
    }
    block.into()
}

pub fn measure_book(
    book: Arc<Book>,
    width: f32,
    size: f32,
    cancel: &AtomicBool,
) -> Option<Vec<f32>> {
    measure_book_for(book, width, size, cancel, themes::default_theme())
}
pub fn measure_book_for(
    book: Arc<Book>,
    width: f32,
    size: f32,
    cancel: &AtomicBool,
    theme: &'static themes::ReadingTheme,
) -> Option<Vec<f32>> {
    measure_book_with(book, width, size, cancel, theme, Options::default())
}
pub fn measure_book_with(
    book: Arc<Book>,
    width: f32,
    size: f32,
    cancel: &AtomicBool,
    theme: &'static themes::ReadingTheme,
    options: Options,
) -> Option<Vec<f32>> {
    fonts::load();
    let style = themes::effective_style(theme, book.pdf_source.is_some(), options);
    let family = themes::effective_family(theme, book.pdf_source.is_some(), options);
    let renderer = Renderer::new(fonts::SANS, Pixels(13.0));
    let limits = Limits::new(Size::ZERO, Size::new(width, f32::INFINITY));
    let mut heights = Vec::with_capacity(book.items.len());
    for (index, item) in book.items.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let mut element = render_item(&book, index, item, width, size, &style, family);
        let mut tree = Tree::new(&element);
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        heights.push(
            node.size().height
                + if index + 1 == book.items.len() {
                    0.0
                } else {
                    style.gap(size)
                },
        );
    }
    Some(heights)
}

fn mapped_item(book: &Book, item: &Item) -> Result<crate::text::MappedParagraph, String> {
    let logical = item.text().unwrap_or_default();
    let heading = [StyleRun {
        start_byte: 0,
        end_byte: logical.len(),
        style: InlineStyle::Bold,
    }];
    let (direction, styles) = match item {
        Item::Paragraph {
            base_direction,
            style_runs,
            ..
        } => (*base_direction, style_runs.as_slice()),
        _ => (BaseDirection::Ltr, heading.as_slice()),
    };
    let mut mapped = map_document_paragraph(logical, direction, styles)?;
    if matches!(item, Item::Heading { .. }) {
        for run in &mut mapped.runs {
            if run.role == FontRole::EditorialBold {
                run.role = FontRole::EditorialMedium;
            }
        }
    }
    let semantics = book.structure.get(item.id());
    if semantics.is_some_and(|s| {
        matches!(
            s.kind,
            reader_document::BlockKind::Preformatted | reader_document::BlockKind::Formula
        )
    }) {
        for run in &mut mapped.runs {
            run.role = match run.role {
                FontRole::EditorialBold | FontRole::SystemBold => FontRole::CodeBold,
                FontRole::EditorialItalic | FontRole::SystemItalic => FontRole::CodeItalic,
                FontRole::EditorialBoldItalic | FontRole::SystemBoldItalic => {
                    FontRole::CodeBoldItalic
                }
                _ => FontRole::Code,
            };
        }
    }
    Ok(mapped)
}

/// Canonical vertical source position, using the same shaping as the page atlas.
/// The midpoint of the source line avoids choosing the preceding page at a cut.
pub fn source_y(book: &Book, row: usize, byte: usize) -> Result<f32, String> {
    Ok(source_ys(book, row, &[byte])?[0])
}

/// Shape a paragraph once when locating many search matches in the same row.
pub fn source_ys(book: &Book, row: usize, bytes: &[usize]) -> Result<Vec<f32>, String> {
    fonts::load();
    let item = book.items.get(row).ok_or("Source row is unavailable")?;
    let logical = item.text().ok_or("Source row has no text")?;
    if bytes
        .iter()
        .any(|&byte| byte > logical.len() || !logical.is_char_boundary(byte))
    {
        return Err("Source byte is invalid".into());
    }
    let theme = themes::default_theme();
    let options = Options::default();
    let style = themes::effective_style(theme, book.pdf_source.is_some(), options);
    let family = themes::effective_family(theme, book.pdf_source.is_some(), options);
    let mapped = mapped_item(book, item)?;
    let prefix = mapped.text.len() - logical.len();
    let size = styled_item_size(&style, book, row, item, MINIMAL.default_size);
    let padding = style.block_padding(
        item,
        book.structure.get(item.id()),
        MINIMAL.default_size,
        crate::atlas::TEXT,
    );
    let runs = paragraph::runs(&mapped, family);
    let spans = paragraph::spans(
        runs.iter().map(|(s, f)| (s.as_str(), *f)),
        size,
        size * style.line_height,
    );
    let shaped = NativeParagraph::with_spans(paragraph::text(
        spans.as_slice(),
        Size::new(
            (crate::atlas::TEXT - padding.left - padding.right).max(1.0),
            f32::INFINITY,
        ),
        size,
        size * style.line_height,
    ));
    let mut offsets = Vec::new();
    let mut offset = 0;
    for line in mapped.text.split_inclusive('\n') {
        offsets.push(offset);
        offset += line.len();
    }
    let mut lines = Vec::new();
    for run in shaped.buffer().layout_runs() {
        let y = run.line_top + run.line_height * 0.5;
        let start = offsets.get(run.line_i).copied().unwrap_or(offset);
        let end = run
            .glyphs
            .iter()
            .map(|g| start + g.end)
            .max()
            .unwrap_or(start);
        lines.push((end, padding.top + y));
    }
    Ok(bytes
        .iter()
        .map(|&byte| {
            let index = lines.partition_point(|(end, _)| *end <= byte + prefix);
            lines
                .get(index)
                .or_else(|| lines.last())
                .map_or(padding.top, |(_, y)| *y)
        })
        .collect())
}
