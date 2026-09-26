//! Workload data types and deterministic generation.
//!
//! Only the two contract sizes exist. The complete 1,000-item sequence is
//! an identical prefix of the 10,000-item sequence. Generation depends
//! only on the requested size: no clock, randomness, locale, filesystem,
//! or network input is consulted, and nothing is read or started at
//! crate load time.

use crate::LayoutRecipe;
use crate::curated::{IMAGE_ASSET_PATH, PRELUDE, PreludeItem};
use crate::recipe::LAYOUT_RECIPE;

/// Filler sentences for generated Latin paragraphs.
const FILLER_SENTENCES: &[&str] = &[
    "The quiet reading room keeps its chairs close to the tall windows.",
    "Good typography disappears the moment you stop noticing it.",
    "A heading is a promise that the next paragraphs keep.",
    "Long lines tire the eye before they tire the reader.",
    "The story is always longer than the screen that carries it.",
    "A page with breathing room asks for nothing and gives hours.",
    "Wrapped lines end where the width says, not where the sentence wants.",
    "The selection remembers where it began after the screen has moved.",
    "Fonts are chosen for the text they must carry, not the shelf they sit on.",
    "Local documents open without asking the network for permission.",
    "Scrolling is reading too, and it should feel like it.",
    "An evening reader wants fewer interruptions, not fewer letters.",
];

/// Sentences for the periodic Arabic-dominant paragraphs.
const RTL_SENTENCES: &[&str] = &[
    "القارئ أنهى صفحة كاملة دون أن يشكو من شيء.",
    "الشارع المبلول بالمطر يعكس الأضواء كأنه نص عربي مقروء.",
    "الكلمة العربية تجاور الرقم 42 والمصطلح Latin في السطر نفسه.",
    "النص العربي يبدأ من اليمين ويمتد نحو اليسار.",
    "لا شيء في هذه الجملة يعتمد على محرك الرسم الذي سيستعملها لاحقًا.",
    "المكتبة المحلية تحفظ الكتب دون أن تسأل الشبكة.",
];

/// English inline phrases appended to some generated paragraphs.
const ENGLISH_INLINE: &[&str] = &[
    " The bookshop windows gleamed after the rain.",
    " A quiet reader turned another page by the river.",
];

/// Arabic inline phrases appended to some generated paragraphs.
const ARABIC_INLINE: &[&str] = &[
    " والمصطلح العربي 42 يظهر وسط السطر.",
    " والجملة العربية الثانية تحمل الرقم 7.",
];

/// Japanese inline phrases appended to some generated paragraphs.
const JAPANESE_INLINE: &[&str] = &[
    " そして日本語の文が続く。",
    " さらに日本語の読者が画面をスクロールする。",
];

/// Generated-section rhythm: a heading before every 25th generated
/// paragraph, an image block before every 100th.
const GENERATED_HEADING_EVERY: usize = 25;
/// See [`GENERATED_HEADING_EVERY`].
const GENERATED_IMAGE_EVERY: usize = 100;
/// Period of the dedicated RTL paragraphs in the generated section.
const GENERATED_RTL_EVERY: usize = 97;

/// Body-paragraph count of the small workload.
pub const BODY_PARAGRAPHS_SMALL: usize = 1_000;
/// Body-paragraph count of the large workload.
pub const BODY_PARAGRAPHS_LARGE: usize = 10_000;

/// The two supported benchmark sizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkloadSize {
    /// 1,000 body paragraphs (plus headings/image blocks).
    Small,
    /// 10,000 body paragraphs (plus headings/image blocks).
    Large,
}

impl WorkloadSize {
    /// Body-paragraph target (headings and image blocks are excluded).
    pub fn body_paragraph_count(self) -> usize {
        match self {
            WorkloadSize::Small => BODY_PARAGRAPHS_SMALL,
            WorkloadSize::Large => BODY_PARAGRAPHS_LARGE,
        }
    }
}

/// Explicit base-direction intent for a paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BaseDirection {
    /// Left-to-right base direction.
    Ltr,
    /// Right-to-left base direction.
    Rtl,
}

/// Inline style applied to a logical byte range of a paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InlineStyle {
    /// Bold.
    Bold,
    /// Italic.
    Italic,
}

/// A styled logical byte range. Ranges are UTF-8 byte offsets into the
/// paragraph's logical text, start-inclusive and end-exclusive, always
/// on character boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StyleRun {
    /// First UTF-8 byte offset (inclusive).
    pub start_byte: usize,
    /// One past the last UTF-8 byte offset (exclusive).
    pub end_byte: usize,
    /// Style to apply to the range.
    pub style: InlineStyle,
}

/// One ordered workload item. Headings and image blocks are additional
/// ordered items and are excluded from the body-paragraph count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    /// A heading.
    Heading {
        /// Stable unique ID.
        id: String,
        /// Heading text.
        text: String,
        /// Heading level (1 = top level).
        level: u8,
    },
    /// A body paragraph.
    Paragraph {
        /// Stable unique ID.
        id: String,
        /// Paragraph text in logical Unicode order.
        text: String,
        /// Explicit base-direction intent.
        base_direction: BaseDirection,
        /// Inline styles over logical byte ranges.
        style_runs: Vec<StyleRun>,
    },
    /// An image block referencing a manifest-listed asset.
    Image {
        /// Stable unique ID.
        id: String,
        /// Fixture-root-relative asset path.
        asset_path: String,
    },
}

impl Item {
    /// Item ID.
    pub fn id(&self) -> &str {
        match self {
            Item::Heading { id, .. } | Item::Paragraph { id, .. } | Item::Image { id, .. } => id,
        }
    }

    /// Logical text for selectable items (`Paragraph`, `Heading`);
    /// `None` for image blocks.
    pub fn text(&self) -> Option<&str> {
        match self {
            Item::Heading { text, .. } | Item::Paragraph { text, .. } => Some(text),
            Item::Image { .. } => None,
        }
    }
}

/// One generated benchmark workload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workload {
    size: WorkloadSize,
    items: Vec<Item>,
}

impl Workload {
    /// Requested size.
    pub fn size(&self) -> WorkloadSize {
        self.size
    }

    /// Fixture revision shared with the manifest.
    pub fn fixture_revision(&self) -> &'static str {
        crate::FIXTURE_REVISION
    }

    /// Ordered items (headings, paragraphs, image blocks).
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Number of body paragraphs (headings/image blocks excluded).
    pub fn body_paragraph_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| matches!(item, Item::Paragraph { .. }))
            .count()
    }

    /// Total ordered item count including headings and image blocks.
    pub fn total_item_count(&self) -> usize {
        self.items.len()
    }

    /// Look up an item by its stable ID.
    pub fn item_by_id(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|item| item.id() == id)
    }

    /// The shared layout recipe (PoC test input, not product defaults).
    pub fn layout_recipe(&self) -> &'static LayoutRecipe {
        &LAYOUT_RECIPE
    }
}

/// Generate the workload for `size`, deterministically.
///
/// The output depends only on `size`: the same size always yields the
/// same ordered items, and the first `Small` items of `Large` are an
/// identical prefix. Paragraph IDs continue the curated prelude's
/// numbering.
pub fn workload(size: WorkloadSize) -> Workload {
    let target = size.body_paragraph_count();
    let mut items: Vec<Item> = Vec::new();
    let mut body_paragraphs = 0usize;

    for prelude_item in PRELUDE {
        match *prelude_item {
            PreludeItem::Heading { id, text, level } => {
                items.push(Item::Heading {
                    id: id.to_string(),
                    text: text.to_string(),
                    level,
                });
            }
            PreludeItem::Paragraph {
                id,
                text,
                base_direction,
                style_runs,
            } => {
                body_paragraphs += 1;
                items.push(Item::Paragraph {
                    id: id.to_string(),
                    text: text.to_string(),
                    base_direction,
                    style_runs: style_runs.to_vec(),
                });
            }
            PreludeItem::Image { id, asset_path } => {
                items.push(Item::Image {
                    id: id.to_string(),
                    asset_path: asset_path.to_string(),
                });
            }
        }
    }

    let mut heading_number = 3usize; // prelude used h-0001, h-0002
    let mut image_number = 2usize; // prelude used img-0001
    let mut generated_index = 0usize;
    while body_paragraphs < target {
        generated_index += 1;
        if generated_index.is_multiple_of(GENERATED_HEADING_EVERY) {
            items.push(Item::Heading {
                id: format!("h-{heading_number:04}"),
                text: format!("Section {}", heading_number - 2),
                level: 2,
            });
            heading_number += 1;
        }
        if generated_index.is_multiple_of(GENERATED_IMAGE_EVERY) {
            items.push(Item::Image {
                id: format!("img-{image_number:04}"),
                asset_path: IMAGE_ASSET_PATH.to_string(),
            });
            image_number += 1;
        }
        body_paragraphs += 1;
        items.push(generated_paragraph(body_paragraphs, generated_index));
    }

    Workload { size, items }
}

/// Build one generated paragraph from its workload index.
fn generated_paragraph(body_number: usize, generated_index: usize) -> Item {
    let id = format!("p-{body_number:05}");
    if generated_index.is_multiple_of(GENERATED_RTL_EVERY) {
        return Item::Paragraph {
            id,
            text: RTL_SENTENCES[(generated_index / GENERATED_RTL_EVERY) % RTL_SENTENCES.len()]
                .to_string(),
            base_direction: BaseDirection::Rtl,
            style_runs: Vec::new(),
        };
    }

    let sentence_count = match generated_index % 5 {
        0 | 1 => 1,
        2 | 3 => 3,
        _ => 7,
    };
    let first = FILLER_SENTENCES[(generated_index * 7) % FILLER_SENTENCES.len()];
    let words: Vec<&str> = first.split_whitespace().collect();
    let word = words[(generated_index / 2) % words.len()];

    let mut text = String::new();
    let mut runs = Vec::new();
    let before = &first[..first.find(word).expect("word comes from its sentence")];
    let after = &first[first.find(word).unwrap() + word.len()..];
    text.push_str(before);
    // The word is always part of the text; only the style run is
    // conditional. Dropping the word would corrupt ordinary paragraphs.
    let start = text.len();
    text.push_str(word);
    let end = text.len();
    if generated_index % 3 != 2 && end > start {
        let style = if generated_index.is_multiple_of(2) {
            InlineStyle::Bold
        } else {
            InlineStyle::Italic
        };
        runs.push(StyleRun {
            start_byte: start,
            end_byte: end,
            style,
        });
    }
    text.push_str(after);
    for j in 1..sentence_count {
        let sentence = FILLER_SENTENCES[(generated_index * 7 + j * 5 + 3) % FILLER_SENTENCES.len()];
        text.push(' ');
        text.push_str(sentence);
    }
    if generated_index.is_multiple_of(7) {
        text.push_str(ENGLISH_INLINE[(generated_index / 7) % ENGLISH_INLINE.len()]);
    }
    if generated_index.is_multiple_of(11) {
        text.push_str(ARABIC_INLINE[(generated_index / 11) % ARABIC_INLINE.len()]);
    }
    if generated_index.is_multiple_of(13) {
        text.push_str(JAPANESE_INLINE[(generated_index / 13) % JAPANESE_INLINE.len()]);
    }

    Item::Paragraph {
        id,
        text,
        base_direction: BaseDirection::Ltr,
        style_runs: runs,
    }
}
