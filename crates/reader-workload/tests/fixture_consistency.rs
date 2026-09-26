//! Consistency between the curated fixture text, the pinned BiDi
//! reference excerpt, and the reference documentation.

use reader_workload::{WorkloadSize, manifest, workload};

fn paragraph_text(id: &str) -> String {
    workload(WorkloadSize::Small)
        .item_by_id(id)
        .unwrap_or_else(|| panic!("paragraph {id} must exist"))
        .text()
        .expect("curated items are selectable")
        .to_string()
}

/// BidiCharacterTest-excerpt.txt line for a given original source line.
fn excerpt_line(original: usize) -> String {
    let root = manifest::fixture_root().expect("fixture root");
    let excerpt = std::fs::read_to_string(
        root.join("references")
            .join("BidiCharacterTest-excerpt.txt"),
    )
    .expect("excerpt readable");
    excerpt
        .lines()
        .find(|line| {
            !line.trim_start().starts_with('#')
                && line.split(';').next().map(str::trim).map(str::is_empty) != Some(true)
        })
        .map(|_| ())
        .ok_or(())
        .and_then(|_| {
            // The excerpt preserves source order; count data lines.
            let data: Vec<&str> = excerpt
                .lines()
                .filter(|line| !line.trim_start().starts_with('#') && !line.trim().is_empty())
                .collect();
            let order = [47, 48, 49, 142, 144, 230, 231, 250, 251, 252, 253];
            let position = order
                .iter()
                .position(|n| *n == original)
                .expect("cited excerpt line exists");
            data.get(position)
                .map(|line| line.trim_end_matches('\r').to_string())
                .ok_or(())
        })
        .expect("excerpt line present")
}

#[test]
fn curated_bidi_reference_paragraphs_match_pinned_excerpt_sequences() {
    // p-00003 must be exactly the code-point sequence of excerpt line 252
    // (BidiCharacterTest.txt line 252, pd = RTL).
    let line = excerpt_line(252);
    let fields: Vec<&str> = line.split(';').collect();
    let codes: Vec<u32> = fields[0]
        .split_whitespace()
        .map(|hex| u32::from_str_radix(hex, 16).expect("hex code point"))
        .collect();
    let expected: String = codes
        .iter()
        .map(|&c| char::from_u32(c).expect("valid scalar"))
        .collect();
    assert_eq!(
        paragraph_text("p-00003"),
        expected,
        "p-00003 must equal the pinned excerpt sequence"
    );
    // And its expected visual reordering is recorded in the excerpt.
    assert_eq!(fields[1], "1", "paragraph direction RTL");
    assert_eq!(
        fields[4].split_whitespace().collect::<Vec<_>>(),
        vec!["5", "6", "7", "4", "3", "2", "1", "0"]
    );

    // p-00004 must be exactly the sequence of excerpt line 49 (pd = RTL).
    let line = excerpt_line(49);
    let fields: Vec<&str> = line.split(';').collect();
    let codes: Vec<u32> = fields[0]
        .split_whitespace()
        .map(|hex| u32::from_str_radix(hex, 16).expect("hex code point"))
        .collect();
    let expected: String = codes
        .iter()
        .map(|&c| char::from_u32(c).expect("valid scalar"))
        .collect();
    assert_eq!(paragraph_text("p-00004"), expected);
    assert_eq!(fields[1], "1", "paragraph direction RTL");
    assert_eq!(
        fields[4].split_whitespace().collect::<Vec<_>>(),
        vec![
            "14", "15", "16", "13", "12", "11", "10", "9", "8", "5", "6", "7", "4", "3", "2", "1",
            "0"
        ]
    );
}

#[test]
fn golden_files_exist_for_every_case_with_exact_names() {
    let root = manifest::fixture_root().expect("fixture root");
    let dir = root.join("references").join("expected-copy");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("golden directory")
        .map(|entry| {
            entry
                .expect("entry")
                .path()
                .file_stem()
                .expect("stem")
                .to_string_lossy()
                .to_string()
        })
        .collect();
    names.sort();
    let mut expected: Vec<String> = reader_workload::selection_cases()
        .into_iter()
        .map(|case| case.name.to_string())
        .collect();
    expected.sort();
    assert_eq!(
        names, expected,
        "golden files must match curated cases exactly"
    );
    // No golden file carries a stray CRLF.
    for case in expected {
        let bytes = std::fs::read(dir.join(format!("{case}.txt"))).expect("golden bytes");
        assert!(
            !bytes.windows(2).any(|w| w == b"\r\n"),
            "{case} must be LF-only"
        );
    }
}

#[test]
fn visual_reference_table_resolves_every_mentioned_item_id() {
    let w = workload(WorkloadSize::Small);
    let root = manifest::fixture_root().expect("fixture root");
    let doc = std::fs::read_to_string(root.join("references").join("bidi-reference.md"))
        .expect("bidi reference doc readable");
    check_visual_table(&doc, &w)
        .unwrap_or_else(|e| panic!("visual reference table consistency: {e}"));
    // The table must keep referencing the curated items it documents.
    let count = distinct_item_references(&doc);
    assert!(
        count >= 12,
        "the visual reference table should reference the fixture items, found {count}"
    );
}

/// Extract the item column (first table cell) of every visual-reference
/// row and collect the identifier-like tokens in it.
fn item_column_tokens(doc: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for line in doc.lines().filter(|l| l.starts_with("| `")) {
        let first_cell = line
            .trim_start_matches('|')
            .split('|')
            .next()
            .unwrap_or_default();
        let mut token = String::new();
        for ch in first_cell.chars().chain(std::iter::once(' ')) {
            if ch.is_ascii_alphanumeric() || ch == '-' {
                token.push(ch);
            } else if !token.is_empty() {
                tokens.push(std::mem::take(&mut token));
            }
        }
    }
    tokens
}

/// Identifier-shaped tokens: lowercase letters, one hyphen, then digits.
/// Anything matching this shape in the item column is treated as an item
/// reference and must be either well-formed or a hard failure.
fn is_item_reference(token: &str) -> bool {
    let bytes = token.as_bytes();
    let hyphen = match token.find('-') {
        Some(i) => i,
        None => return false,
    };
    if hyphen == 0 || hyphen + 1 == bytes.len() {
        return false;
    }
    bytes[..hyphen].iter().all(|b| b.is_ascii_lowercase())
        && bytes[hyphen + 1..].iter().all(|b| b.is_ascii_digit())
}

fn is_exact_item_id(token: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    (token.starts_with("p-") && digits(&token[2..]) && token.len() == 7)
        || (token.starts_with("h-") && digits(&token[2..]) && token.len() == 6)
        || (token.starts_with("img-") && digits(&token[4..]) && token.len() == 8)
}

fn distinct_item_references(doc: &str) -> usize {
    let tokens = item_column_tokens(doc);
    let mut seen = std::collections::BTreeSet::new();
    for token in tokens {
        if is_exact_item_id(&token) {
            seen.insert(token);
        }
    }
    seen.len()
}

/// Every item reference in the visual table's item column must be
/// well-formed *and* resolve to an actual workload item. Malformed
/// references (e.g. the old four-digit form) are rejected, not skipped.
fn check_visual_table(doc: &str, workload: &reader_workload::Workload) -> Result<(), String> {
    for token in item_column_tokens(doc) {
        if !is_item_reference(&token) {
            continue;
        }
        if !is_exact_item_id(&token) {
            return Err(format!(
                "malformed item reference {token:?} in the visual reference table"
            ));
        }
        if workload.item_by_id(&token).is_none() {
            return Err(format!(
                "visual reference table mentions {token:?}, which does not exist in the workload"
            ));
        }
    }
    Ok(())
}

#[test]
fn excerpt_levels_field_is_recorded_for_the_exact_sequence_paragraphs() {
    // Beyond direction, code points, and visual order, the resolved
    // levels recorded in the pinned excerpt are asserted too.
    let line252 = excerpt_line(252);
    let fields: Vec<&str> = line252.split(';').collect();
    assert_eq!(
        fields[3].split_whitespace().collect::<Vec<_>>(),
        vec!["1", "1", "1", "1", "1", "2", "2", "2"],
        "line 252 levels"
    );
    let line49 = excerpt_line(49);
    let fields: Vec<&str> = line49.split(';').collect();
    assert_eq!(
        fields[3].split_whitespace().collect::<Vec<_>>(),
        vec![
            "1", "1", "1", "1", "1", "2", "2", "2", "1", "1", "1", "1", "1", "1", "2", "2", "2"
        ],
        "line 49 levels"
    );
}

#[test]
fn visual_table_checker_rejects_the_old_four_digit_form() {
    let w = workload(WorkloadSize::Small);
    let mutated = "| `p-0002` (old form) | LTR | x | y |
";
    let err = check_visual_table(mutated, &w).expect_err("malformed reference must be rejected");
    assert!(err.contains("p-0002") && err.contains("malformed"));
}

#[test]
fn visual_table_checker_rejects_unknown_five_digit_ids() {
    let w = workload(WorkloadSize::Small);
    let mutated = "| `p-09999` (unknown) | LTR | x | y |
";
    let err = check_visual_table(mutated, &w).expect_err("unknown id must be rejected");
    assert!(err.contains("p-09999"));
}
