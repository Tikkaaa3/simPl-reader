//! Named selection/copy reference cases and the pure extraction seam.
//!
//! Positions are a stable text-item ID plus a zero-based UTF-8 byte
//! offset; ranges are start-inclusive/end-exclusive, always on character
//! boundaries. This is a fixture-only convention, not a production
//! locator contract, and the extraction here is a small reference
//! helper, not a general selection engine.

use crate::workload::Workload;
use reader_document::Endpoint;

/// Errors detected while resolving or validating selection references.
/// Validation returns errors as data; nothing is silently truncated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReferenceError {
    /// The endpoint's item ID does not exist in the workload.
    UnknownItemId { id: String },
    /// The endpoint names an image item, which has no selectable text.
    ItemNotSelectable { id: String },
    /// The offset is beyond the end of the item's text.
    OffsetOutOfRange {
        id: String,
        offset: usize,
        byte_length: usize,
    },
    /// The offset falls inside a multi-byte scalar (e.g. bisecting a
    /// base-plus-mark sequence).
    OffsetNotCharBoundary { id: String, offset: usize },
    /// The recorded first character at an endpoint no longer matches the
    /// curated fixture text (text drift at the recorded coordinate).
    EndpointCharMismatch {
        id: String,
        offset: usize,
        expected: Option<char>,
        actual: Option<char>,
    },
}

impl std::fmt::Display for ReferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReferenceError::UnknownItemId { id } => {
                write!(f, "unknown text item id {id:?}")
            }
            ReferenceError::ItemNotSelectable { id } => {
                write!(f, "item {id:?} is not selectable (no text)")
            }
            ReferenceError::OffsetOutOfRange {
                id,
                offset,
                byte_length,
            } => {
                write!(
                    f,
                    "offset {offset} out of range for item {id:?} (length {byte_length})"
                )
            }
            ReferenceError::OffsetNotCharBoundary { id, offset } => {
                write!(
                    f,
                    "offset {offset} is not a character boundary of item {id:?}"
                )
            }
            ReferenceError::EndpointCharMismatch {
                id,
                offset,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "item {id:?} @ {offset}: expected first char {expected:?}, found {actual:?}"
                )
            }
        }
    }
}

impl std::error::Error for ReferenceError {}

/// A named selection/copy reference case with independently recorded
/// expected output (golden files under
/// `fixtures/reader-workload/references/expected-copy/`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionCase {
    /// Stable case name (matches the golden file name).
    pub name: &'static str,
    /// What the case exercises.
    pub notes: &'static str,
    /// Anchor endpoint.
    pub anchor: Endpoint,
    /// Unicode scalar expected to start at the anchor (`None` at
    /// end-of-text).
    pub anchor_first_char: Option<char>,
    /// Focus endpoint.
    pub focus: Endpoint,
    /// Unicode scalar expected to start at the focus endpoint.
    pub focus_first_char: Option<char>,
}

/// The curated selection/copy reference cases, in fixture order.
pub fn selection_cases() -> Vec<SelectionCase> {
    crate::selection_cases::cases()
}

fn resolve(workload: &Workload, endpoint: &Endpoint) -> Result<(usize, usize), ReferenceError> {
    let index = workload
        .items()
        .iter()
        .position(|item| item.id() == endpoint.item_id)
        .ok_or_else(|| ReferenceError::UnknownItemId {
            id: endpoint.item_id.clone(),
        })?;
    let item = &workload.items()[index];
    let text = item
        .text()
        .ok_or_else(|| ReferenceError::ItemNotSelectable {
            id: endpoint.item_id.clone(),
        })?;
    let offset = endpoint.byte_offset;
    if offset > text.len() {
        return Err(ReferenceError::OffsetOutOfRange {
            id: endpoint.item_id.clone(),
            offset,
            byte_length: text.len(),
        });
    }
    if !text.is_char_boundary(offset) {
        return Err(ReferenceError::OffsetNotCharBoundary {
            id: endpoint.item_id.clone(),
            offset,
        });
    }
    Ok((index, offset))
}

fn endpoint_char(workload: &Workload, endpoint: &Endpoint) -> Result<Option<char>, ReferenceError> {
    let (index, offset) = resolve(workload, endpoint)?;
    let item = &workload.items()[index];
    let text = item.text().unwrap_or_default();
    if offset == text.len() {
        return Ok(None);
    }
    Ok(text[offset..].chars().next())
}

/// Extract the canonical plain-text copy string for a selection range.
///
/// Semantics: preserve logical character order and original
/// normalization; concatenate style runs without extra separators; join
/// selected text items (including headings) with one LF; image items
/// contribute no text and no additional separators; no terminal LF is
/// added unless present in the selected source text; reversing
/// anchor/focus yields the same text; a collapsed range yields the empty
/// string.
pub fn extract_copy_text(
    workload: &Workload,
    anchor: &Endpoint,
    focus: &Endpoint,
) -> Result<String, ReferenceError> {
    let anchor = resolve(workload, anchor)?;
    let focus = resolve(workload, focus)?;
    if anchor == focus {
        return Ok(String::new());
    }

    let (lo, hi) = if anchor <= focus {
        (anchor, focus)
    } else {
        (focus, anchor)
    };

    let mut parts: Vec<&str> = Vec::new();
    let items = workload.items();
    if lo.0 == hi.0 {
        let text = items[lo.0].text().unwrap_or_default();
        parts.push(&text[lo.1..hi.1]);
    } else {
        for (index, item) in items.iter().enumerate().skip(lo.0).take(hi.0 - lo.0 + 1) {
            let Some(text) = item.text() else {
                continue; // image blocks contribute no text, no separator
            };
            let piece = if index == lo.0 {
                &text[lo.1..]
            } else if index == hi.0 {
                &text[..hi.1]
            } else {
                text
            };
            // An item that contributes no characters (e.g. the focus at
            // byte 0 of a later item, or an anchor at end-of-text) adds no
            // text and no separator; joining it would fabricate a leading
            // or terminal LF that no source item contains.
            if !piece.is_empty() {
                parts.push(piece);
            }
        }
    }
    Ok(parts.join("\n"))
}

/// Validate the curated reference cases against a workload: every
/// endpoint must resolve, stay on character boundaries, and start at the
/// recorded first character. Returns one entry per detected problem, as
/// data (never panics, never truncates).
pub fn validate_selection_cases(
    workload: &Workload,
    cases: &[SelectionCase],
) -> Vec<(&'static str, ReferenceError)> {
    let mut errors = Vec::new();
    for case in cases {
        let endpoints = [
            (&case.anchor, case.anchor_first_char),
            (&case.focus, case.focus_first_char),
        ];
        for (endpoint, expected) in endpoints {
            let actual = endpoint_char(workload, endpoint);
            match actual {
                Ok(actual) => {
                    if actual != expected {
                        errors.push((
                            case.name,
                            ReferenceError::EndpointCharMismatch {
                                id: endpoint.item_id.clone(),
                                offset: endpoint.byte_offset,
                                expected,
                                actual,
                            },
                        ));
                    }
                }
                Err(err) => errors.push((case.name, err)),
            }
        }
    }
    errors
}
