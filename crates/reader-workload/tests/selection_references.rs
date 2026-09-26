//! Behavioral tests for the selection/copy reference seam, exercised
//! through the library's public interface.
//!
//! Golden expectations are checked-in UTF-8 files under
//! `fixtures/reader-workload/references/expected-copy/`, produced by the
//! authoring script from the documented copy semantics — independently
//! of the extraction logic under test.

use reader_workload::{
    Endpoint, ReferenceError, WorkloadSize, extract_copy_text, manifest, reference,
    selection_cases, validate_selection_cases, workload,
};

fn case_named(name: &str) -> reference::SelectionCase {
    selection_cases()
        .into_iter()
        .find(|case| case.name == name)
        .unwrap_or_else(|| panic!("curated case {name} must exist"))
}

fn golden_dir() -> std::path::PathBuf {
    let root = manifest::fixture_root().expect("fixture root resolves from the crate");
    root.join("references").join("expected-copy")
}

#[test]
fn every_curated_case_matches_its_independent_golden_file() {
    let w = workload(WorkloadSize::Small);
    for case in selection_cases() {
        let expected_path = golden_dir().join(format!("{}.txt", case.name));
        let expected = std::fs::read(&expected_path)
            .unwrap_or_else(|e| panic!("cannot read golden file {}: {e}", expected_path.display()));
        let actual = extract_copy_text(&w, &case.anchor, &case.focus)
            .unwrap_or_else(|e| panic!("case {} failed to extract: {e}", case.name));
        assert_eq!(
            actual.as_bytes(),
            expected.as_slice(),
            "case {} copied text differs from its golden file",
            case.name
        );
    }
}

#[test]
fn all_curated_endpoints_are_valid_and_on_recorded_characters() {
    let w = workload(WorkloadSize::Small);
    assert!(
        validate_selection_cases(&w, &selection_cases()).is_empty(),
        "curated selection cases must validate cleanly"
    );
}

#[test]
fn reversed_endpoints_yield_the_same_text() {
    let w = workload(WorkloadSize::Small);
    let case = &case_named("adjacent-paragraphs");
    let forward = extract_copy_text(&w, &case.anchor, &case.focus).unwrap();
    let backward = extract_copy_text(&w, &case.focus, &case.anchor).unwrap();
    assert_eq!(forward, backward);
    assert!(!forward.is_empty());
}

#[test]
fn collapsed_range_yields_empty_string() {
    let w = workload(WorkloadSize::Small);
    let case = &case_named("collapsed");
    let text = extract_copy_text(&w, &case.anchor, &case.focus).unwrap();
    assert!(text.is_empty());
    // Collapse at a different, non-zero offset behaves identically.
    let endpoint = Endpoint {
        item_id: "p-00001".to_string(),
        byte_offset: 3,
    };
    assert_eq!(
        extract_copy_text(&w, &endpoint, &endpoint),
        Ok(String::new())
    );
}

#[test]
fn image_items_contribute_no_text_and_no_separator() {
    let w = workload(WorkloadSize::Small);
    let case = &case_named("cross-image-heading");
    let text = extract_copy_text(&w, &case.anchor, &case.focus).unwrap();
    // p-0008 + LF + h-0002 + LF + p-0009 (image contributes nothing).
    let p8 = w.item_by_id("p-00008").unwrap().text().unwrap();
    let h2 = w.item_by_id("h-0002").unwrap().text().unwrap();
    let p9 = w.item_by_id("p-00009").unwrap().text().unwrap();
    assert_eq!(text, format!("{p8}\n{h2}\n{p9}"));
}

#[test]
fn no_terminal_lf_is_added() {
    let w = workload(WorkloadSize::Small);
    for case in selection_cases() {
        let text = extract_copy_text(&w, &case.anchor, &case.focus).unwrap();
        assert!(
            !text.ends_with('\n'),
            "case {} must not end with an added LF",
            case.name
        );
    }
}

#[test]
fn end_of_text_offset_is_valid() {
    let w = workload(WorkloadSize::Small);
    let text = w.item_by_id("p-00001").unwrap().text().unwrap();
    let end = Endpoint {
        item_id: "p-00001".to_string(),
        byte_offset: text.len(),
    };
    let selected = extract_copy_text(
        &w,
        &Endpoint {
            item_id: "p-00001".to_string(),
            byte_offset: 0,
        },
        &end,
    )
    .unwrap();
    assert_eq!(selected, text);
}

#[test]
fn unknown_item_id_is_reported() {
    let w = workload(WorkloadSize::Small);
    let err = extract_copy_text(
        &w,
        &Endpoint {
            item_id: "p-99999".to_string(),
            byte_offset: 0,
        },
        &Endpoint {
            item_id: "p-00001".to_string(),
            byte_offset: 0,
        },
    );
    assert_eq!(
        err,
        Err(ReferenceError::UnknownItemId {
            id: "p-99999".to_string()
        })
    );
}

#[test]
fn image_item_endpoint_is_not_selectable() {
    let w = workload(WorkloadSize::Small);
    let err = extract_copy_text(
        &w,
        &Endpoint {
            item_id: "img-0001".to_string(),
            byte_offset: 0,
        },
        &Endpoint {
            item_id: "p-00001".to_string(),
            byte_offset: 0,
        },
    );
    assert_eq!(
        err,
        Err(ReferenceError::ItemNotSelectable {
            id: "img-0001".to_string()
        })
    );
}

#[test]
fn out_of_range_offset_is_reported_not_truncated() {
    let w = workload(WorkloadSize::Small);
    let text = w.item_by_id("p-00001").unwrap().text().unwrap();
    let err = extract_copy_text(
        &w,
        &Endpoint {
            item_id: "p-00001".to_string(),
            byte_offset: text.len() + 1,
        },
        &Endpoint {
            item_id: "p-00001".to_string(),
            byte_offset: 2,
        },
    );
    assert_eq!(
        err,
        Err(ReferenceError::OffsetOutOfRange {
            id: "p-00001".to_string(),
            offset: text.len() + 1,
            byte_length: text.len(),
        })
    );
}

#[test]
fn combining_mark_offset_is_reported_as_non_boundary() {
    let w = workload(WorkloadSize::Small);
    // p-0007 contains decomposed e + U+0301; the offset one byte into the
    // mark bisects the sequence and must be rejected.
    let text = w.item_by_id("p-00007").unwrap().text().unwrap();
    let mark_offset = text
        .char_indices()
        .find(|(i, c)| *c == '\u{0301}' && *i > 0)
        .map(|(i, _)| i)
        .expect("decomposed acute mark present");
    // The byte one into the mark's encoding is not a char boundary.
    let bisecting_offset = mark_offset + 1;
    let err = extract_copy_text(
        &w,
        &Endpoint {
            item_id: "p-00007".to_string(),
            byte_offset: mark_offset + 1,
        },
        &Endpoint {
            item_id: "p-00007".to_string(),
            byte_offset: mark_offset + 2,
        },
    );
    assert_eq!(
        err,
        Err(ReferenceError::OffsetNotCharBoundary {
            id: "p-00007".to_string(),
            offset: bisecting_offset,
        })
    );
}

#[test]
fn reference_module_reports_errors_with_useful_messages() {
    let err = ReferenceError::OffsetOutOfRange {
        id: "p-00001".to_string(),
        offset: 9,
        byte_length: 8,
    };
    let message = err.to_string();
    assert!(message.contains("p-00001") && message.contains("9"));
}

#[test]
fn zero_offset_focus_in_next_item_adds_no_terminal_lf() {
    let w = workload(WorkloadSize::Small);
    let p1 = w.item_by_id("p-00001").unwrap().text().unwrap().to_string();
    // Forward and reversed: the focus item at byte 0 selects nothing, so
    // the copied text is exactly the first paragraph, without LF.
    let forward = extract_copy_text(
        &w,
        &Endpoint {
            item_id: "p-00001".to_string(),
            byte_offset: 0,
        },
        &Endpoint {
            item_id: "p-00002".to_string(),
            byte_offset: 0,
        },
    )
    .unwrap();
    assert_eq!(forward, p1);
    let backward = extract_copy_text(
        &w,
        &Endpoint {
            item_id: "p-00002".to_string(),
            byte_offset: 0,
        },
        &Endpoint {
            item_id: "p-00001".to_string(),
            byte_offset: 0,
        },
    )
    .unwrap();
    assert_eq!(backward, p1);
}

#[test]
fn empty_cross_item_range_yields_empty_string() {
    let w = workload(WorkloadSize::Small);
    let p1_len = w.item_by_id("p-00001").unwrap().text().unwrap().len();
    // First item's end-of-text to next item's byte 0 selects no characters
    // in either direction: the result is the empty string, not a lone LF.
    for direction in [0, 1] {
        let (a_item, a_off, f_item, f_off) = if direction == 0 {
            ("p-00001", p1_len, "p-00002", 0)
        } else {
            ("p-00002", 0, "p-00001", p1_len)
        };
        let result = extract_copy_text(
            &w,
            &Endpoint {
                item_id: a_item.to_string(),
                byte_offset: a_off,
            },
            &Endpoint {
                item_id: f_item.to_string(),
                byte_offset: f_off,
            },
        );
        assert_eq!(result, Ok(String::new()));
    }
}

#[test]
fn empty_leading_piece_adds_no_leading_separator() {
    let w = workload(WorkloadSize::Small);
    let p1_len = w.item_by_id("p-00001").unwrap().text().unwrap().len();
    let p2 = w.item_by_id("p-00002").unwrap().text().unwrap().to_string();
    // Anchor at end-of-text of the first item selects nothing there; the
    // copy starts with the focus item's selected characters. The focus
    // offset is the first character boundary at or after byte 5 (the text
    // starts with multi-byte Arabic scalars).
    let focus_offset = p2
        .char_indices()
        .map(|(i, _)| i)
        .find(|i| *i >= 5)
        .unwrap_or(p2.len());
    let result = extract_copy_text(
        &w,
        &Endpoint {
            item_id: "p-00001".to_string(),
            byte_offset: p1_len,
        },
        &Endpoint {
            item_id: "p-00002".to_string(),
            byte_offset: focus_offset,
        },
    )
    .unwrap();
    assert_eq!(result, &p2[..focus_offset]);
}
