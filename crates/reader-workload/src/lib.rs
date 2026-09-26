//! Deterministic, UI-independent reader fixture workloads for the Iced prototype.
//!
//! This crate is **fixture-only data**: it produces the 1,000- and
//! 10,000-body-paragraph benchmark workloads, their styles/headings/image
//! references, the pinned asset manifest, the shared layout recipe, and
//! the named selection/copy reference cases. It is not a document model,
//! not a parser, not a selection engine, and has no UI-framework,
//! parser, or hashing dependencies. Generation is deterministic and
//! touches neither the filesystem, the clock, randomness, locale, nor
//! the network.
//!
//! The Iced reader adapter and tests call [`workload`], inspect ordered
//! `reader_document::Item` content, consult [`recipe::LAYOUT_RECIPE`] and the
//! [`manifest`] records, and load assets explicitly after their shell
//! frame using [`manifest::find_fixture_root`].

#![forbid(unsafe_code)]

mod curated;
mod selection_cases;

pub mod manifest;
pub mod recipe;
pub mod reference;
pub mod workload;

pub use manifest::{AssetRecord, AssetValidationError, Manifest, ManifestError};
pub use recipe::{LAYOUT_RECIPE, LayoutRecipe};
pub use reference::{
    ReferenceError, SelectionCase, extract_copy_text, selection_cases, validate_selection_cases,
};
pub use workload::{
    BODY_PARAGRAPHS_LARGE, BODY_PARAGRAPHS_SMALL, Workload, WorkloadSize, workload,
};

/// Fixture revision shared by the manifest and every generated workload.
pub const FIXTURE_REVISION: &str = curated::FIXTURE_REVISION;

/// Portable, fixture-root-relative path of the workload image asset.
pub use curated::{IMAGE_ASSET_PATH, IMAGE_INTRINSIC};
