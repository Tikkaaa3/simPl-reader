//! Behavioral tests for the generated benchmark workloads, exercised
//! through the library's public interface.

use reader_workload::{
    BODY_PARAGRAPHS_LARGE, BODY_PARAGRAPHS_SMALL, BaseDirection, InlineStyle, Item, WorkloadSize,
    workload,
};

#[test]
fn body_counts_are_exactly_the_two_supported_sizes() {
    let small = workload(WorkloadSize::Small);
    let large = workload(WorkloadSize::Large);
    assert_eq!(small.body_paragraph_count(), BODY_PARAGRAPHS_SMALL);
    assert_eq!(large.body_paragraph_count(), BODY_PARAGRAPHS_LARGE);
    // Headings and image blocks are additional ordered items; the exact
    // totals follow from the prelude (3 non-body items) plus the
    // every-25th-heading / every-100th-image generated rhythm.
    assert!(small.total_item_count() > BODY_PARAGRAPHS_SMALL);
    assert!(large.total_item_count() > BODY_PARAGRAPHS_LARGE);
    assert_eq!(small.total_item_count(), 1_051);
    assert_eq!(large.total_item_count(), 10_501);
    assert_eq!(
        small.total_item_count(),
        small.body_paragraph_count()
            + small
                .items()
                .iter()
                .filter(|i| !matches!(i, Item::Paragraph { .. }))
                .count()
    );
    assert_eq!(
        large.total_item_count(),
        large.body_paragraph_count()
            + large
                .items()
                .iter()
                .filter(|i| !matches!(i, Item::Paragraph { .. }))
                .count()
    );
}

#[test]
fn generation_is_repeatable_across_calls() {
    assert_eq!(workload(WorkloadSize::Small), workload(WorkloadSize::Small));
    assert_eq!(workload(WorkloadSize::Large), workload(WorkloadSize::Large));
}

#[test]
fn small_is_an_identical_prefix_of_large() {
    let small = workload(WorkloadSize::Small);
    let large = workload(WorkloadSize::Large);
    let large_prefix = &large.items()[..small.items().len()];
    assert_eq!(small.items(), large_prefix);
    assert_eq!(small.fixture_revision(), large.fixture_revision());
}

#[test]
fn ids_are_unique_and_stably_formatted() {
    let large = workload(WorkloadSize::Large);
    let mut seen = std::collections::HashSet::new();
    for item in large.items() {
        assert!(
            seen.insert(item.id().to_string()),
            "duplicate item id {}",
            item.id()
        );
    }
    // ID numbering continues across prelude and generated sections.
    assert!(large.item_by_id("p-00001").is_some());
    assert_eq!(large.item_by_id("p-10000").map(Item::id), Some("p-10000"));
    assert!(large.item_by_id("h-0001").is_some());
    assert!(large.item_by_id("img-0001").is_some());
}

#[test]
fn paragraphs_vary_clearly_in_length() {
    let large = workload(WorkloadSize::Large);
    let mut lengths: Vec<usize> = large
        .items()
        .iter()
        .filter_map(|item| match item {
            Item::Paragraph { text, .. } => Some(text.len()),
            _ => None,
        })
        .collect();
    lengths.sort_unstable();
    let short_max = *lengths.first().unwrap();
    let long_min = *lengths.last().unwrap();
    assert!(
        short_max < 200,
        "expected clearly short paragraphs, min length {short_max}"
    );
    assert!(
        long_min > 600,
        "expected clearly long paragraphs, max length {long_min}"
    );
    assert!(
        lengths.windows(2).any(|w| w[1] - w[0] > 50),
        "expected varied lengths, got sorted extremes {short_max}..{long_min}"
    );
}

#[test]
fn mixed_styles_appear_throughout_the_corpus() {
    let large = workload(WorkloadSize::Large);
    let styled_tail = large.items()[large.items().len() - 100..]
        .iter()
        .filter(|item| matches!(item, Item::Paragraph { style_runs, .. } if !style_runs.is_empty()))
        .count();
    assert!(
        styled_tail > 0,
        "styled paragraphs must not be a prelude-only feature"
    );

    for item in large.items() {
        if let Item::Paragraph {
            text, style_runs, ..
        } = item
        {
            for run in style_runs {
                assert!(run.start_byte <= run.end_byte);
                assert!(run.end_byte <= text.len());
                assert!(text.is_char_boundary(run.start_byte));
                assert!(text.is_char_boundary(run.end_byte));
            }
        }
    }
    // Both styles occur overall.
    let styles: std::collections::HashSet<_> = large
        .items()
        .iter()
        .flat_map(|item| match item {
            Item::Paragraph { style_runs, .. } => {
                style_runs.iter().map(|r| r.style).collect::<Vec<_>>()
            }
            _ => Vec::new(),
        })
        .collect();
    assert!(styles.contains(&InlineStyle::Bold));
    assert!(styles.contains(&InlineStyle::Italic));
}

#[test]
fn curated_english_and_unicode_coverage_is_present_in_the_shared_prefix() {
    let workload = workload(WorkloadSize::Small);
    let all: String = workload
        .items()
        .iter()
        .filter_map(Item::text)
        .collect::<Vec<_>>()
        .join("\n");

    // The first prose paragraph uses English Latin text; the dedicated
    // multilingual passages below remain available for Unicode coverage.
    assert!(
        workload
            .item_by_id("p-00001")
            .unwrap()
            .text()
            .unwrap()
            .is_ascii()
    );
    // Arabic with digits and Latin embedded.
    assert!(all.contains('ع') && all.contains("42") && all.contains("Latin"));
    // Hebrew mixed with Latin.
    assert!(all.contains('א') && all.contains("English"));
    // Japanese/CJK.
    assert!(all.contains('日') && all.contains('語'));
    // Decomposed base-plus-combining-mark text (e + U+0301, i + U+0308).
    assert!(all.contains("e\u{0301}") && all.contains("i\u{0308}"));
    // Latin ligature candidate.
    assert!(all.contains("ffi"));
    // Supplementary-plane scalar (U+1D11E) for indexing checks.
    assert!(all.contains('\u{1D11E}'));
    // Emoji probe (documented as non-gating for rendering).
    assert!(all.contains('\u{1F642}'));

    // Explicit base-direction intent: RTL paragraphs exist and are not
    // pre-reversed (logical order preserved).
    let rtl_paragraphs: Vec<&Item> = workload
        .items()
        .iter()
        .filter(|item| {
            matches!(
                item,
                Item::Paragraph {
                    base_direction: BaseDirection::Rtl,
                    ..
                }
            )
        })
        .collect();
    assert!(rtl_paragraphs.len() >= 4);
    assert!(
        rtl_paragraphs
            .iter()
            .any(|item| item.text().unwrap().contains('ع')),
        "an RTL paragraph must contain Arabic text"
    );
}

#[test]
fn headings_and_image_blocks_exist_in_both_sizes() {
    for size in [WorkloadSize::Small, WorkloadSize::Large] {
        let w = workload(size);
        assert!(w.items().iter().any(|i| matches!(i, Item::Heading { .. })));
        assert!(w.items().iter().any(|i| matches!(i, Item::Image { .. })));
        // Image blocks reference the manifest-listed asset.
        assert!(
            w.items()
                .iter()
                .any(|i| matches!(i, Item::Image { asset_path, .. }
                if asset_path == "assets/images/reader-sample.png"))
        );
    }
}

#[test]
fn unstyled_generated_paragraphs_preserve_all_words() {
    // Generated index 2 (p-00014) takes the unstyled branch; the selected
    // word must not disappear from the text.
    let w = workload(WorkloadSize::Large);
    let item = w.item_by_id("p-00014").expect("generated paragraph exists");
    let Item::Paragraph {
        text, style_runs, ..
    } = item
    else {
        panic!("p-00014 must be a paragraph");
    };
    assert!(
        text.starts_with("A heading is a promise that the next paragraphs keep."),
        "unstyled paragraph must keep every word, got: {text:?}"
    );
    assert!(
        !text.contains("  "),
        "dropped words leave double spaces: {text:?}"
    );
    assert!(style_runs.is_empty());
}

#[test]
fn styled_generated_paragraphs_preserve_all_words() {
    // Generated index 1 (p-00013) is styled; the styled word stays in the
    // text and the run covers it on character boundaries.
    let w = workload(WorkloadSize::Large);
    let item = w.item_by_id("p-00013").expect("generated paragraph exists");
    let Item::Paragraph {
        text, style_runs, ..
    } = item
    else {
        panic!("p-00013 must be a paragraph");
    };
    assert!(
        text.starts_with("The selection remembers where it began after the screen has moved."),
        "styled paragraph must keep every word, got: {text:?}"
    );
    assert_eq!(style_runs.len(), 1);
    let run = &style_runs[0];
    assert_eq!(run.style, reader_workload::InlineStyle::Italic);
    assert_eq!(&text[run.start_byte..run.end_byte], "The");
    assert!(text.is_char_boundary(run.start_byte));
    assert!(text.is_char_boundary(run.end_byte));
}
