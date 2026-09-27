//! Wrap a local HTML book folder in a private EPUB, preserving relative resources.
use super::*;
use html5ever::{parse_document, tendril::TendrilSink};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use zip::{ZipWriter, write::SimpleFileOptions};

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn metadata(node: &Handle, title: &mut String, author: &mut String, depth: usize) {
    if depth > 512 {
        return;
    }
    if let NodeData::Element { name, attrs, .. } = &node.data {
        if name.local.as_ref() == "title" {
            for child in node.children.borrow().iter() {
                if let NodeData::Text { contents } = &child.data {
                    title.push_str(&contents.borrow());
                }
            }
        }
        if name.local.as_ref() == "meta" {
            let attrs = attrs.borrow();
            if attrs
                .iter()
                .any(|a| a.name.local.as_ref() == "name" && a.value.eq_ignore_ascii_case("author"))
                && let Some(value) = attrs.iter().find(|a| a.name.local.as_ref() == "content")
            {
                *author = value.value.to_string();
            }
        }
    }
    for child in node.children.borrow().iter() {
        metadata(child, title, author, depth + 1);
    }
}

fn collect(base: &Path, dir: &Path, files: &mut Vec<PathBuf>, depth: usize) -> Result<(), String> {
    if depth > 32 {
        return Err("HTML book folders are nested too deeply".into());
    }
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let meta = regular(&path)?;
        if meta.is_dir() {
            collect(base, &path, files, depth + 1)?;
        } else if meta.is_file() {
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if matches!(
                ext.as_str(),
                "html"
                    | "htm"
                    | "xhtml"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "webp"
                    | "svg"
                    | "css"
                    | "woff"
                    | "woff2"
                    | "ttf"
            ) {
                files.push(
                    path.strip_prefix(base)
                        .map_err(|e| e.to_string())?
                        .to_path_buf(),
                );
                if files.len() > 19_000 {
                    return Err("HTML book has too many files".into());
                }
            }
        }
    }
    Ok(())
}

pub(super) fn import(root: &Path, source: &Path) -> Result<PathBuf, String> {
    let mut files = Vec::new();
    collect(source, source, &mut files, 0)?;
    files.sort();
    let mut chapters: Vec<_> = files
        .iter()
        .filter(|p| {
            p.components().count() == 1
                && matches!(
                    p.extension().and_then(|e| e.to_str()),
                    Some("html" | "htm" | "xhtml")
                )
        })
        .cloned()
        .collect();
    if chapters.is_empty() {
        return Err("This folder contains no HTML book pages".into());
    }
    if chapters.len() > 4096 {
        return Err("HTML book has too many chapters".into());
    }
    let mut hash = Sha256::new();
    hash.update(b"simpl-html-folder-v1");
    hash.update(source.to_string_lossy().as_bytes());
    let mut total = 0u64;
    let mut data = Vec::new();
    for relative in &files {
        let file = fs::File::open(source.join(relative)).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 32 * 1024 * 1024 {
            return Err("An HTML book resource exceeds 32 MiB".into());
        }
        total += bytes.len() as u64;
        if total > MAX_BYTES {
            return Err("HTML book contents exceed the 512 MiB import limit".into());
        }
        hash.update(relative.to_string_lossy().as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(&bytes);
        data.push((relative.clone(), bytes));
    }
    // Exported books often have an index that duplicates their front matter.
    if let Some((_, index)) = data.iter().find(|(p, _)| p == Path::new("index.html"))
        && chapters.iter().any(|p| {
            p != Path::new("index.html")
                && data.iter().any(|(name, bytes)| name == p && bytes == index)
        })
    {
        chapters.retain(|p| p != Path::new("index.html"));
    }
    let first = data.iter().find(|(p, _)| p == &chapters[0]).unwrap();
    let dom: RcDom = parse_document(RcDom::default(), Default::default())
        .one(String::from_utf8_lossy(&first.1).into_owned());
    let mut title = String::new();
    let mut author = String::new();
    metadata(&dom.document, &mut title, &mut author, 0);
    if title.is_empty() {
        title = source
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
    }
    // Section titles from HTML exports commonly share a book-title suffix.
    if let Some((_, suffix)) = title.rsplit_once(" — ") {
        let common = chapters.iter().skip(1).take(3).all(|path| {
            let (_, bytes) = data.iter().find(|(name, _)| name == path).unwrap();
            let dom: RcDom = parse_document(RcDom::default(), Default::default())
                .one(String::from_utf8_lossy(bytes).into_owned());
            let mut chapter_title = String::new();
            metadata(&dom.document, &mut chapter_title, &mut String::new(), 0);
            chapter_title.ends_with(&format!(" — {suffix}"))
        });
        if chapters.len() > 1 && common {
            title = suffix.to_owned();
        }
    }
    let id = format!("{:x}", hash.finalize());
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    regular(root)?;
    let folder = root.join(&id);
    let target = folder.join("book.epub");
    if folder.exists() {
        if owned_folder(root, &target)?.is_some() && regular(&target)?.is_file() {
            fs::write(folder.join(".simpl-source-format"), "html").map_err(|e| e.to_string())?;
            return Ok(target);
        }
        return Err("An incomplete HTML book import already exists".into());
    }
    fs::create_dir(&folder).map_err(|e| e.to_string())?;
    let result = (|| -> Result<(), String> {
        let mut zip = ZipWriter::new(fs::File::create(&target).map_err(|e| e.to_string())?);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut write = |name: &str, bytes: &[u8], stored: bool| -> Result<(), String> {
            zip.start_file(
                name,
                if stored {
                    options.compression_method(zip::CompressionMethod::Stored)
                } else {
                    options
                },
            )
            .map_err(|e| e.to_string())?;
            zip.write_all(bytes).map_err(|e| e.to_string())
        };
        write("mimetype", b"application/epub+zip", true)?;
        write("META-INF/container.xml", b"<container xmlns='urn:oasis:names:tc:opendocument:xmlns:container' version='1.0'><rootfiles><rootfile full-path='_simpl.opf' media-type='application/oebps-package+xml'/></rootfiles></container>", false)?;
        let mut manifest = String::new();
        let mut spine = String::new();
        let mut nav = String::new();
        for (i, (path, bytes)) in data.iter().enumerate() {
            let name = path.to_string_lossy().replace('\\', "/");
            let href = name
                .split('/')
                .map(|s| {
                    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC)
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("/");
            let kind = match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
                "html" | "htm" | "xhtml" => "application/xhtml+xml",
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                "svg" => "image/svg+xml",
                "css" => "text/css",
                _ => "application/octet-stream",
            };
            manifest.push_str(&format!(
                "<item id='r{i}' href='{}' media-type='{kind}'/>",
                xml(&href)
            ));
            if chapters.contains(path) {
                spine.push_str(&format!("<itemref idref='r{i}'/>"));
                let dom: RcDom = parse_document(RcDom::default(), Default::default())
                    .one(String::from_utf8_lossy(bytes).into_owned());
                let mut label = String::new();
                metadata(&dom.document, &mut label, &mut String::new(), 0);
                nav.push_str(&format!(
                    "<li><a href='{}'>{}</a></li>",
                    xml(&href),
                    xml(&label)
                ));
            }
            write(&name, bytes, false)?;
        }
        let opf = format!(
            "<package xmlns='http://www.idpf.org/2007/opf' version='3.0' unique-identifier='id'><metadata xmlns:dc='http://purl.org/dc/elements/1.1/'><dc:identifier id='id'>{id}</dc:identifier><dc:title>{}</dc:title><dc:creator>{}</dc:creator><dc:language>en</dc:language></metadata><manifest>{manifest}<item id='nav' href='_simpl-nav.xhtml' media-type='application/xhtml+xml' properties='nav'/></manifest><spine>{spine}</spine></package>",
            xml(&title),
            xml(&author)
        );
        write("_simpl.opf", opf.as_bytes(), false)?;
        write("_simpl-nav.xhtml", format!("<html xmlns='http://www.w3.org/1999/xhtml' xmlns:epub='http://www.idpf.org/2007/ops'><head><title>Contents</title></head><body><nav epub:type='toc'><ol>{nav}</ol></nav></body></html>").as_bytes(), false)?;
        zip.finish().map_err(|e| e.to_string())?;
        // Validate the exact package before marking it as owned/reusable.
        crate::epub::open(&target)?;
        fs::write(folder.join(".simpl-source-format"), "html").map_err(|e| e.to_string())?;
        fs::write(folder.join(MARKER), "book.epub").map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let actual = fs::canonicalize(&folder).map_err(|e| e.to_string())?;
        if actual.parent() == Some(fs::canonicalize(root).map_err(|e| e.to_string())?.as_path())
            && check_tree(&actual).is_ok()
        {
            let _ = fs::remove_dir_all(actual);
        }
    }
    result.map(|_| target)
}
