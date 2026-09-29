//! Private document copies. Deletion is restricted to marked, direct children of our store.
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

#[path = "html_bundle.rs"]
mod html_bundle;

const MARKER: &str = ".simpl-owned";
const MAX_BYTES: u64 = 512 * 1024 * 1024;

pub fn root() -> PathBuf {
    crate::position::storage_base()
        .join("simPl")
        .join("documents")
}

fn regular(path: &Path) -> Result<fs::Metadata, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err("Linked paths cannot be managed documents".into());
        }
    }
    if meta.file_type().is_symlink() {
        return Err("Linked paths cannot be managed documents".into());
    }
    Ok(meta)
}

fn owned_folder(root: &Path, path: &Path) -> Result<Option<PathBuf>, String> {
    let Some(parent) = path.parent() else {
        return Ok(None);
    };
    if !root.exists() {
        return Ok(None);
    }
    regular(root)?;
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    if !parent.exists() {
        return Ok(None);
    }
    let actual = fs::canonicalize(parent).map_err(|e| e.to_string())?;
    if actual.parent() != Some(root.as_path()) {
        return Ok(None);
    }
    regular(parent)?;
    let id = actual
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(None);
    }
    let marker = actual.join(MARKER);
    if !marker.exists() {
        return Ok(None);
    }
    let metadata = regular(&marker)?;
    if !metadata.is_file() || metadata.len() > 1024 {
        return Err("Invalid managed document marker".into());
    }
    let expected = fs::read_to_string(marker).map_err(|e| e.to_string())?;
    if path.file_name().and_then(|s| s.to_str()) != Some(expected.as_str()) {
        return Ok(None);
    }
    Ok(Some(actual))
}

pub fn import(path: &Path) -> Result<PathBuf, String> {
    import_at(&root(), path)
}

fn import_at(root: &Path, path: &Path) -> Result<PathBuf, String> {
    if owned_folder(root, path)?.is_some() {
        return Ok(path.to_path_buf());
    }
    let source = fs::canonicalize(path).map_err(|e| format!("Cannot read document: {e}"))?;
    let meta = regular(&source)?;
    if meta.is_dir() {
        return html_bundle::import(root, &source);
    }
    if !meta.is_file() {
        return Err("Choose a document file or an HTML book folder".into());
    }
    if meta.len() > MAX_BYTES {
        return Err("Document exceeds the 512 MiB import limit".into());
    }
    let ext = source
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let text_source = crate::text::is_text_extension(&ext);
    let source_format = match ext.as_str() {
        "txt" | "text" => Some("txt"),
        "md" | "markdown" => Some("markdown"),
        _ => None,
    };
    if !text_source && !matches!(ext.as_str(), "html" | "htm" | "xhtml" | "pdf" | "epub") {
        return Err("Unsupported document format".into());
    }
    let source_name = source
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Invalid document filename")?;
    // Text and Markdown are stored as the generated HTML page, read by the HTML reader.
    let generated_name;
    let name = if text_source {
        generated_name = format!(
            "{}.html",
            source
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("Invalid document filename")?
        );
        generated_name.as_str()
    } else {
        source_name
    };
    let mut hash = Sha256::new();
    hash.update(source.to_string_lossy().as_bytes());
    let mut content = Sha256::new();
    let mut generated = None;
    if text_source {
        if meta.len() > crate::text::MAX_TEXT_BYTES {
            return Err("Text file exceeds the 16 MiB import limit".into());
        }
        let mut bytes = Vec::new();
        fs::File::open(&source)
            .map_err(|e| e.to_string())?
            .take(crate::text::MAX_TEXT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > crate::text::MAX_TEXT_BYTES {
            return Err("Text file grew beyond the import limit".into());
        }
        generated = Some(crate::text::to_html(&source, &ext, &bytes)?);
    } else {
        let mut file = fs::File::open(&source).map_err(|e| e.to_string())?;
        let mut buffer = [0; 64 * 1024];
        let mut read = 0;
        loop {
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            read += n as u64;
            if read > MAX_BYTES {
                return Err("Document grew beyond the import limit".into());
            }
            hash.update(&buffer[..n]);
            content.update(&buffer[..n]);
        }
    }
    let folder = root.join(format!("{:x}", hash.finalize()));
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    regular(root)?;
    let target = folder.join(name);
    if folder.exists() {
        if owned_folder(root, &target)?.is_some() && regular(&target)?.is_file() {
            // Repair metadata on copies imported before source formats were retained.
            if let Some(format) = source_format {
                fs::write(folder.join(".simpl-source-format"), format)
                    .map_err(|e| e.to_string())?;
            }
            return Ok(target);
        }
        return Err("An incomplete import already occupies this document folder".into());
    }
    if matches!(ext.as_str(), "pdf" | "epub")
        && let Some(existing) = find_copy(root, &ext, meta.len(), &content.finalize())?
    {
        return Ok(existing);
    }
    let assets = if let Some(page) = &generated {
        crate::html::load_html_bytes(source.clone(), page.as_bytes())?
            .images
            .into_keys()
            .collect::<Vec<_>>()
    } else if matches!(ext.as_str(), "html" | "htm" | "xhtml") {
        crate::load_html(&source)?
            .images
            .into_keys()
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    fs::create_dir(&folder).map_err(|e| e.to_string())?;
    let copied = (|| {
        if let Some(page) = &generated {
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|e| e.to_string())?;
            output
                .write_all(page.as_bytes())
                .map_err(|e| e.to_string())?;
        } else {
            copy(&source, &target)?;
        }
        let source_parent = source.parent().ok_or("Missing source directory")?;
        let mut total = generated
            .as_ref()
            .map_or(meta.len(), |page| page.len() as u64);
        for key in assets {
            let relative = Path::new(&key);
            if !relative
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
            {
                return Err("Invalid local image path".into());
            }
            let original =
                fs::canonicalize(source_parent.join(relative)).map_err(|e| e.to_string())?;
            if !original.starts_with(source_parent) {
                return Err("Image outside document directory".into());
            }
            total += regular(&original)?.len();
            if total > MAX_BYTES {
                return Err("HTML assets exceed import limit".into());
            }
            let destination = folder.join(relative);
            fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
            if destination != target {
                copy(&original, &destination)?;
            }
        }
        if let Some(format) = source_format {
            fs::write(folder.join(".simpl-source-format"), format).map_err(|e| e.to_string())?;
        }
        fs::write(folder.join(MARKER), name).map_err(|e| e.to_string())?;
        Ok(target.clone())
    })();
    if copied.is_err() {
        // Newly created folder only; never a caller-provided recursive deletion target.
        let canonical = fs::canonicalize(&folder).map_err(|e| e.to_string())?;
        let base = fs::canonicalize(root).map_err(|e| e.to_string())?;
        if canonical.parent() == Some(base.as_path()) && check_tree(&canonical).is_ok() {
            let _ = fs::remove_dir_all(canonical);
        }
    }
    copied
}

/// The same single-file book imported from another location reuses its existing managed copy.
fn find_copy(root: &Path, ext: &str, len: u64, digest: &[u8]) -> Result<Option<PathBuf>, String> {
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let folder = entry.map_err(|e| e.to_string())?.path();
        let Ok(marker) = fs::read_to_string(folder.join(MARKER)) else {
            continue;
        };
        let candidate = folder.join(&marker);
        let same_format = candidate
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case(ext));
        if !same_format || owned_folder(root, &candidate)?.is_none() {
            continue;
        }
        let Ok(meta) = regular(&candidate) else {
            continue;
        };
        if !meta.is_file() || meta.len() != len {
            continue;
        }
        let mut file = fs::File::open(&candidate).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut buffer = [0; 64 * 1024];
        loop {
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        if hash.finalize().as_slice() == digest {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn copy(source: &Path, target: &Path) -> Result<(), String> {
    let mut input = fs::File::open(source).map_err(|e| e.to_string())?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|e| e.to_string())?;
    let copied = std::io::copy(
        &mut Read::by_ref(&mut input).take(MAX_BYTES + 1),
        &mut output,
    )
    .map_err(|e| e.to_string())?;
    if copied > MAX_BYTES {
        return Err("Document grew beyond the import limit".into());
    }
    output.flush().map_err(|e| e.to_string())
}

fn check_tree(folder: &Path) -> Result<(), String> {
    regular(folder)?;
    for entry in fs::read_dir(folder).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if regular(&path)?.is_dir() {
            check_tree(&path)?;
        }
    }
    Ok(())
}

/// External/legacy documents are unlisted only. Their source files are never deleted.
pub fn remove(path: &Path) -> Result<(), String> {
    remove_at(&root(), path)
}
fn remove_at(root: &Path, path: &Path) -> Result<(), String> {
    if let Some(folder) = owned_folder(root, path)? {
        check_tree(&folder)?;
        fs::remove_dir_all(folder).map_err(|e| format!("Cannot remove managed copy: {e}"))?;
    }
    Ok(())
}

/// Original user-facing format can differ from the internal chapter container.
pub fn source_kind(path: &Path) -> Option<crate::library::SourceFormat> {
    source_kind_at(&root(), path)
}

fn source_kind_at(root: &Path, path: &Path) -> Option<crate::library::SourceFormat> {
    use crate::library::SourceFormat;
    let ext = path.extension()?.to_str()?;
    if !ext.eq_ignore_ascii_case("html") && path.file_name()?.to_str()? != "book.epub" {
        return None;
    }
    owned_folder(root, path).ok()??;
    let parent = path.parent()?;
    match fs::read_to_string(parent.join(".simpl-source-format"))
        .ok()
        .as_deref()
    {
        Some("html") => return Some(SourceFormat::Html),
        Some("txt") => return Some(SourceFormat::Text),
        Some("markdown") => return Some(SourceFormat::Markdown),
        _ => {}
    }
    if path.file_name()?.to_str()? != "book.epub" {
        return None;
    }
    // Recognize packages produced before source-format metadata was introduced.
    let mut archive = zip::ZipArchive::new(fs::File::open(path).ok()?).ok()?;
    if archive.by_name("_simpl.opf").is_ok() && archive.by_name("_simpl-nav.xhtml").is_ok() {
        Some(SourceFormat::Html)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_is_idempotent_and_remove_never_touches_original_or_unmarked_files() {
        let dir = std::env::temp_dir().join(format!(
            "simpl-import-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("book.html");
        fs::write(&source, "<h1>Chapter</h1><p>Keep this original.</p>").unwrap();
        fs::create_dir(dir.join("images")).unwrap();
        fs::write(
            dir.join("images/picture.png"),
            include_bytes!("../../../fixtures/reader-workload/assets/images/reader-sample.png"),
        )
        .unwrap();
        fs::write(
            &source,
            "<h1>Chapter</h1><p>Keep this original.</p><img src='images/picture.png'>",
        )
        .unwrap();
        let root = dir.join("managed");
        let target = import_at(&root, &source).unwrap();
        assert_ne!(target, source);
        assert!(
            target
                .parent()
                .unwrap()
                .join("images/picture.png")
                .is_file()
        );
        assert_eq!(crate::load_html(&target).unwrap().images.len(), 1);
        assert_eq!(fs::read(&target).unwrap(), fs::read(&source).unwrap());
        assert_eq!(import_at(&root, &source).unwrap(), target);
        remove_at(&root, &source).unwrap();
        assert!(source.exists());
        remove_at(&root, &target).unwrap();
        assert!(!target.exists());
        assert!(source.exists());
        let unowned = root.join("a".repeat(64));
        fs::create_dir_all(&unowned).unwrap();
        let unrelated = unowned.join("other.pdf");
        fs::write(&unrelated, b"unowned").unwrap();
        remove_at(&root, &unrelated).unwrap();
        assert!(unrelated.exists());
        let bundle = dir.join("html-book");
        fs::create_dir(&bundle).unwrap();
        let first = "<title>My Book</title><h1>First</h1><a href='02.html#end'>Next</a>";
        fs::write(bundle.join("index.html"), first).unwrap();
        fs::write(bundle.join("01.html"), first).unwrap();
        fs::write(
            bundle.join("02.html"),
            "<h1 id='end'>Last chapter</h1><img src='picture.png'>",
        )
        .unwrap();
        fs::write(
            bundle.join("picture.png"),
            include_bytes!("../../../fixtures/reader-workload/assets/images/reader-sample.png"),
        )
        .unwrap();
        let packaged = import_at(&root, &bundle).unwrap();
        let epub = crate::epub::open(&packaged).unwrap();
        assert_eq!(epub.chapters.len(), 2);
        assert_eq!(epub.title, "My Book");
        assert_eq!(
            epub.resolve_link(0, "02.html#end").unwrap(),
            (1, Some("end".into()))
        );
        assert_eq!(epub.load_chapter(1).unwrap().document.images.len(), 1);
        drop(epub);
        assert_eq!(import_at(&root, &bundle).unwrap(), packaged);
        remove_at(&root, &packaged).unwrap();
        assert!(!packaged.exists());
        assert!(bundle.join("02.html").is_file());
        let empty = dir.join("empty-folder");
        fs::create_dir(&empty).unwrap();
        assert!(import_at(&root, &empty).unwrap_err().contains("no HTML"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn text_and_markdown_import_as_generated_html_with_their_images() {
        let dir = std::env::temp_dir().join(format!(
            "simpl-import-text-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(dir.join("img")).unwrap();
        fs::write(
            dir.join("img/dot.png"),
            include_bytes!("../../../fixtures/reader-workload/assets/images/reader-sample.png"),
        )
        .unwrap();
        let markdown = dir.join("Notes.MD");
        fs::write(
            &markdown,
            "# Field notes\n\nSome text.\n\n![dot](img/dot.png)\n",
        )
        .unwrap();
        let plain = dir.join("story.txt");
        fs::write(&plain, "Once upon\na time.\n\nThe end.\n").unwrap();
        let root = dir.join("managed");

        // An external HTML file with a similarly named sidecar is not our converted copy.
        let external = dir.join("external.html");
        fs::write(&external, "<p>Original HTML</p>").unwrap();
        fs::write(dir.join(".simpl-source-format"), "txt").unwrap();
        assert_eq!(source_kind_at(&root, &external), None);

        let md = import_at(&root, &markdown).unwrap();
        assert_eq!(md.file_name().unwrap(), "Notes.html");
        assert_eq!(
            source_kind_at(&root, &md),
            Some(crate::library::SourceFormat::Markdown)
        );
        assert!(md.parent().unwrap().join("img/dot.png").is_file());
        let document = crate::load_html(&md).unwrap();
        assert_eq!(document.title, "Field notes");
        assert_eq!(document.images.len(), 1);
        assert_eq!(import_at(&root, &markdown).unwrap(), md);

        let txt = import_at(&root, &plain).unwrap();
        assert_eq!(
            source_kind_at(&root, &txt),
            Some(crate::library::SourceFormat::Text)
        );
        assert_eq!(crate::load_html(&txt).unwrap().title, "story");
        assert_eq!(crate::load_html(&txt).unwrap().items.len(), 2);

        // Reimport repairs legacy metadata without replacing content or changing its path.
        let marker = txt.parent().unwrap().join(".simpl-source-format");
        let bytes = fs::read(&txt).unwrap();
        fs::remove_file(marker).unwrap();
        assert_eq!(source_kind_at(&root, &txt), None);
        assert_eq!(import_at(&root, &plain).unwrap(), txt);
        assert_eq!(
            source_kind_at(&root, &txt),
            Some(crate::library::SourceFormat::Text)
        );
        assert_eq!(fs::read(&txt).unwrap(), bytes);

        remove_at(&root, &md).unwrap();
        assert!(!md.exists());
        assert!(markdown.exists() && plain.exists());
        let binary = dir.join("blob.txt");
        fs::write(&binary, b"ab\0cd").unwrap();
        assert!(import_at(&root, &binary).is_err());
        // An image outside the source folder is left out rather than copied.
        fs::write(dir.join("escape.md"), "![x](../outside.png)").unwrap();
        let escaped = import_at(&root, &dir.join("escape.md")).unwrap();
        assert_eq!(fs::read_dir(escaped.parent().unwrap()).unwrap().count(), 3);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn same_single_file_book_from_another_path_reuses_the_managed_copy() {
        let dir = std::env::temp_dir().join(format!(
            "simpl-import-dedupe-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(dir.join("a")).unwrap();
        fs::create_dir_all(dir.join("b")).unwrap();
        let root = dir.join("managed");
        for (name, other) in [
            ("book.pdf", b"%PDF changed"),
            ("book.epub", b"epub changed"),
        ] {
            let bytes = format!("same bytes for {name}");
            fs::write(dir.join("a").join(name), &bytes).unwrap();
            fs::write(dir.join("b").join(name), &bytes).unwrap();
            fs::write(dir.join("b").join(format!("other-{name}")), other).unwrap();
            let first = import_at(&root, &dir.join("a").join(name)).unwrap();
            assert_eq!(import_at(&root, &dir.join("b").join(name)).unwrap(), first);
            let different = import_at(&root, &dir.join("b").join(format!("other-{name}"))).unwrap();
            assert_ne!(different, first);
        }
        assert_eq!(fs::read_dir(&root).unwrap().count(), 4);
        fs::remove_dir_all(dir).unwrap();
    }
}
