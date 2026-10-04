//! Shared canonical atlas, retaining the desktop module path.
pub use reader_layout::atlas::*;

fn storage_enabled() -> bool {
    !cfg!(test)
        || std::env::var_os("SIMPL_PREVIEW_STORE")
            .is_some_and(|store| Some(store) == std::env::var_os("LOCALAPPDATA"))
}
pub fn build(
    book: std::sync::Arc<super::Book>,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Option<Atlas>, String> {
    reader_layout::atlas::build_with_cache(book, cancel, storage_enabled())
}
