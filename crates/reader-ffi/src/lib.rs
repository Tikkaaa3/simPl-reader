//! UniFFI surface for the Android app. Kotlin bindings are generated from the
//! compiled library (`uniffi-bindgen generate --library`), so every exported
//! item is declared here with proc-macros; there is no UDL file.

use reader_document::library::SourceFormat;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

mod layout;
pub use layout::*;
mod library;
pub use library::*;
mod reader;
pub use reader::*;

uniffi::setup_scaffolding!();

/// Identifies the native core the app loaded.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct BuildInfo {
    /// Workspace crate version of the core.
    pub core_version: String,
    /// Operating system the library was compiled for (`android` on devices).
    pub os: String,
    /// CPU architecture the library was compiled for (`aarch64`, `x86_64`).
    pub arch: String,
    /// Whether the library carries debug assertions (Cargo dev profile).
    pub debug: bool,
}

#[uniffi::export]
pub fn build_info() -> BuildInfo {
    BuildInfo {
        core_version: env!("CARGO_PKG_VERSION").to_owned(),
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        debug: cfg!(debug_assertions),
    }
}

/// A core operation failed; `reason` is shown to the reader as is.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Error)]
pub enum CoreError {
    Failed { reason: String },
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed { reason } => f.write_str(reason),
        }
    }
}

impl std::error::Error for CoreError {}

impl From<String> for CoreError {
    fn from(reason: String) -> Self {
        Self::Failed { reason }
    }
}

/// Prepare the core before any other call: `data_dir` keeps the library and
/// reading state, `cache_dir` disposable conversions, and `language` (a BCP 47
/// tag such as `tr-TR`) picks the code page for legacy, non-Unicode text files.
#[uniffi::export]
pub fn initialize(data_dir: String, cache_dir: String, language: String) -> Result<(), CoreError> {
    reader_profile::configure(Path::new(&data_dir), Path::new(&cache_dir))?;
    reader_document::set_legacy_text_language(&language);
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum DocumentFormat {
    Epub,
    Pdf,
    Html,
    Text,
    Markdown,
}

impl From<SourceFormat> for DocumentFormat {
    fn from(format: SourceFormat) -> Self {
        match format {
            SourceFormat::Epub => Self::Epub,
            SourceFormat::Pdf => Self::Pdf,
            SourceFormat::Html => Self::Html,
            SourceFormat::Text => Self::Text,
            SourceFormat::Markdown => Self::Markdown,
        }
    }
}

/// What the core read from a document without laying it out.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DocumentSummary {
    /// The format the reader imported (TXT/Markdown stay labelled as such).
    pub format: DocumentFormat,
    pub title: String,
    pub author: Option<String>,
    /// SHA-256 of the source, the key of positions and annotations.
    pub fingerprint: String,
    /// EPUB spine chapters; 1 for single-section documents.
    pub chapters: u32,
    /// Source pages: PDF pages, or publisher page labels of EPUB/HTML (0 if none).
    pub source_pages: u32,
}

/// Copy a document into the managed library (TXT/Markdown become a private
/// HTML page) and return the path the reader opens from now on.
#[uniffi::export]
pub fn import_document(path: String) -> Result<String, CoreError> {
    let managed = reader_document::managed::import(Path::new(&path))?;
    Ok(managed.to_string_lossy().into_owned())
}

/// Open a PDF, EPUB or HTML document (managed or not) and summarize it.
#[uniffi::export]
pub fn inspect_document(path: String) -> Result<DocumentSummary, CoreError> {
    let path = PathBuf::from(path);
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let managed = reader_document::managed::source_kind(&path).map(DocumentFormat::from);
    let summary = match extension.as_str() {
        "pdf" => {
            let document = complete(reader_pdf::open(path))?;
            DocumentSummary {
                format: DocumentFormat::Pdf,
                title: document.title.clone(),
                author: document.author.clone(),
                fingerprint: document.fingerprint.clone(),
                chapters: 1,
                source_pages: count(document.pages.len()),
            }
        }
        "epub" => {
            let epub = reader_document::epub::open(&path)?;
            DocumentSummary {
                format: managed.unwrap_or(DocumentFormat::Epub),
                title: epub.title.clone(),
                author: epub.author.clone(),
                fingerprint: epub.fingerprint.clone(),
                chapters: count(epub.chapters.len()),
                source_pages: count(epub.page_list.len()),
            }
        }
        "html" | "htm" | "xhtml" => {
            let document = reader_document::load_html(&path)?;
            DocumentSummary {
                format: managed.unwrap_or(DocumentFormat::Html),
                title: document.title,
                author: document.author,
                fingerprint: document.fingerprint,
                chapters: 1,
                source_pages: count(document.page_breaks.len()),
            }
        }
        "txt" | "text" | "md" | "markdown" => {
            return Err("Import text and Markdown files before opening them"
                .to_owned()
                .into());
        }
        _ => return Err(format!("Unsupported document: {}", path.display()).into()),
    };
    Ok(summary)
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Drive one of the PDF worker's futures to completion on the calling thread.
/// Callers are Kotlin background threads; the reply comes from the worker thread.
fn complete<T>(future: impl Future<Output = T>) -> T {
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::park(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_info_describes_this_build() {
        let info = build_info();
        assert_eq!(info.core_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.os, std::env::consts::OS);
        assert_eq!(info.arch, std::env::consts::ARCH);
        assert_eq!(info.debug, cfg!(debug_assertions));
    }

    #[test]
    fn unsupported_and_unimported_documents_are_reported() {
        assert!(matches!(
            inspect_document("notes.txt".into()),
            Err(CoreError::Failed { reason }) if reason.contains("Import")
        ));
        assert!(inspect_document("archive.zip".into()).is_err());
    }

    #[test]
    fn html_documents_are_summarized() {
        let path = std::env::temp_dir().join(format!("simpl-ffi-{}.html", std::process::id()));
        std::fs::write(
            &path,
            "<title>Harbour</title><h1>Harbour</h1><p>The boats came in.</p>",
        )
        .unwrap();
        let summary = inspect_document(path.to_string_lossy().into_owned());
        let _ = std::fs::remove_file(&path);
        let summary = summary.unwrap();
        assert_eq!(summary.format, DocumentFormat::Html);
        assert_eq!(summary.title, "Harbour");
        assert_eq!(summary.chapters, 1);
        assert_eq!(summary.fingerprint.len(), 64);
    }
}
