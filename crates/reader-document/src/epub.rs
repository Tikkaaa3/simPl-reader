//! Bounded, file-backed EPUB 2/3 package reader. Archive members are never extracted.
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use parking_lot::Mutex;
use roxmltree::{Document as XmlDocument, Node, ParsingOptions};
use sha2::{Digest, Sha256};
use zip::{CompressionMethod, ZipArchive};

use crate::{
    Document, ImageAsset,
    html::{self, ResourceLoader},
};

const MAX_ARCHIVE: u64 = 512 * 1024 * 1024;
const MAX_ENTRIES: usize = 20_000;
const MAX_TOTAL: u64 = 512 * 1024 * 1024;
const MAX_RESOURCE: u64 = 32 * 1024 * 1024;
const MAX_METADATA: u64 = 8 * 1024 * 1024;
const MAX_CHAPTERS: usize = 4_096;
const MAX_TOC: usize = 10_000;
const MAX_TOC_DEPTH: usize = 64;
const MAX_XML_NODES: u32 = 100_000;
const XHTML: &str = "application/xhtml+xml";

#[derive(Clone, Debug)]
pub struct ChapterInfo {
    /// Canonical member path inside the package, not a filesystem path.
    pub href: String,
    pub title: String,
}

#[derive(Clone, Debug)]
pub struct TocEntry {
    pub label: String,
    pub chapter: usize,
    pub fragment: Option<String>,
    pub depth: usize,
}

#[derive(Debug)]
pub struct Chapter {
    pub document: Document,
    pub anchors: HashMap<String, String>,
}

#[derive(Clone, Debug)]
struct Entry {
    index: usize,
    size: u64,
}

#[derive(Debug)]
pub struct Epub {
    pub path: PathBuf,
    pub title: String,
    pub author: Option<String>,
    pub fingerprint: String,
    pub chapters: Vec<ChapterInfo>,
    pub contents: Vec<TocEntry>,
    pub warnings: Vec<String>,
    archive: Mutex<ZipArchive<File>>,
    cover_path: Option<(String, String)>,
    entries: HashMap<String, Entry>,
}

#[derive(Debug)]
struct ManifestItem {
    path: String,
    media: String,
    fallback: Option<String>,
}

#[derive(Debug)]
struct Package {
    title: String,
    author: Option<String>,
    cover_path: Option<(String, String)>,
    chapters: Vec<String>,
    ncx: Option<String>,
    nav: Option<String>,
}

/// Open a regular local EPUB file and retain its archive handle for lazy chapter reads.
pub fn open(path: &Path) -> Result<Epub, String> {
    if is_network_path(path) {
        return Err("Network/UNC EPUB paths are unsupported".into());
    }
    let path =
        fs::canonicalize(path).map_err(|e| format!("Cannot open EPUB {}: {e}", path.display()))?;
    if is_network_path(&path) {
        return Err("Network/UNC EPUB paths are unsupported".into());
    }
    let mut file =
        File::open(&path).map_err(|e| format!("Cannot open EPUB {}: {e}", path.display()))?;
    let stat = file
        .metadata()
        .map_err(|e| format!("Cannot inspect EPUB: {e}"))?;
    if !stat.is_file() || stat.len() == 0 || stat.len() > MAX_ARCHIVE {
        return Err("EPUB must be a nonempty regular file of at most 512 MiB".into());
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut remaining = MAX_ARCHIVE + 1;
    loop {
        let n = file
            .read(&mut buffer[..(remaining as usize).min(64 * 1024)])
            .map_err(|e| format!("Cannot fingerprint EPUB: {e}"))?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
        remaining -= n as u64;
        if remaining == 0 {
            return Err("EPUB exceeds 512 MiB".into());
        }
    }
    if MAX_ARCHIVE + 1 - remaining != stat.len() {
        return Err("EPUB file changed during fingerprinting".into());
    }
    let expected_entries = inspect_directory(&mut file, stat.len())?;
    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("Cannot rewind EPUB: {e}"))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("Invalid EPUB ZIP: {e}"))?;
    if archive.is_empty() || archive.len() != expected_entries {
        return Err("EPUB has no entries or contains duplicate ZIP member names".into());
    }
    let mut entries = HashMap::with_capacity(archive.len());
    let mut seen_paths = HashSet::with_capacity(archive.len());
    let mut total = 0u64;
    for i in 0..archive.len() {
        let entry = archive
            .by_index_raw(i)
            .map_err(|e| format!("Invalid ZIP entry {i}: {e}"))?;
        let name = member_path(entry.name())?;
        if entry.encrypted() {
            return Err(format!(
                "Password-protected ZIP entry is unsupported: {name}"
            ));
        }
        if !matches!(
            entry.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) {
            return Err(format!("Unsupported ZIP compression for {name}"));
        }
        if entry.compressed_size() > MAX_ARCHIVE {
            return Err(format!("ZIP entry exceeds archive bound: {name}"));
        }
        total = total
            .checked_add(entry.size())
            .ok_or("EPUB uncompressed size overflow")?;
        if total > MAX_TOTAL {
            return Err("EPUB declared uncompressed data exceeds 512 MiB".into());
        }
        if !entry.is_file() && !entry.is_dir() {
            return Err(format!("Non-regular ZIP entry is unsupported: {name}"));
        }
        if !seen_paths.insert(name.clone()) {
            return Err(format!(
                "Duplicate ZIP member path or file/directory alias: {name}"
            ));
        }
        if entry.is_file() {
            entries.insert(
                name,
                Entry {
                    index: i,
                    size: entry.size(),
                },
            );
        }
    }
    let mime = entries.get("mimetype").ok_or("EPUB is missing mimetype")?;
    if mime.index != 0
        || archive
            .by_index_raw(0)
            .map_err(|e| e.to_string())?
            .compression()
            != CompressionMethod::Stored
    {
        return Err("EPUB mimetype must be the first uncompressed ZIP entry".into());
    }
    if read_entry(&mut archive, mime, "mimetype", MAX_METADATA)? != b"application/epub+zip" {
        return Err("Invalid EPUB mimetype".into());
    }
    let mut metadata_budget = MAX_METADATA;
    let container = read_xml(
        &mut archive,
        &entries,
        "META-INF/container.xml",
        &mut metadata_budget,
    )?;
    let root_path = {
        let xml = parse_xml(&container)?;
        if !xml.root_element().has_tag_name((
            "urn:oasis:names:tc:opendocument:xmlns:container",
            "container",
        )) {
            return Err("Invalid EPUB container.xml".into());
        }
        let rootfile = xml
            .descendants()
            .find(|node| node.is_element() && node.tag_name().name() == "rootfile")
            .ok_or("EPUB container has no rootfile")?;
        if rootfile.attribute("media-type") != Some("application/oebps-package+xml") {
            return Err("Unsupported EPUB rootfile media type".into());
        }
        member_path(
            rootfile
                .attribute("full-path")
                .ok_or("EPUB rootfile has no path")?,
        )?
    };
    let opf = read_xml(&mut archive, &entries, &root_path, &mut metadata_budget)?;
    let package = parse_package(&opf, &root_path, &entries)?;
    // Older fixed-layout EPUBs often declare this outside the OPF.
    const DISPLAY_OPTIONS: &str = "META-INF/com.apple.ibooks.display-options.xml";
    if entries.contains_key(DISPLAY_OPTIONS) {
        let source = read_xml(
            &mut archive,
            &entries,
            DISPLAY_OPTIONS,
            &mut metadata_budget,
        )?;
        let xml = parse_xml(&source)?;
        if xml.descendants().any(|node| {
            node.is_element()
                && node.tag_name().name() == "option"
                && node.attribute("name") == Some("fixed-layout")
                && text_of(node) == "true"
        }) {
            return Err("Fixed-layout EPUB display options are unsupported".into());
        }
    }
    let mut warnings = vec!["Publisher CSS and custom fonts are not applied".to_owned()];
    if entries.contains_key("META-INF/encryption.xml") {
        let encryption = read_xml(
            &mut archive,
            &entries,
            "META-INF/encryption.xml",
            &mut metadata_budget,
        )?;
        check_encryption(&encryption, &entries, &mut warnings)?;
    }
    let mut chapters = Vec::with_capacity(package.chapters.len());
    let mut chapter_indices = HashMap::new();
    for path in package.chapters {
        chapter_indices.insert(path.clone(), chapters.len());
        chapters.push(ChapterInfo {
            title: file_label(&path),
            href: path,
        });
    }
    let mut contents = Vec::new();
    if let Some(nav) = package.nav {
        let xml = read_xml(&mut archive, &entries, &nav, &mut metadata_budget)?;
        contents = parse_nav(&xml, &nav, &chapter_indices)?;
    }
    if contents.is_empty()
        && let Some(ncx) = package.ncx
    {
        let xml = read_xml(&mut archive, &entries, &ncx, &mut metadata_budget)?;
        contents = parse_ncx(&xml, &ncx, &chapter_indices)?;
    }
    if contents.is_empty() {
        contents = chapters
            .iter()
            .enumerate()
            .map(|(chapter, info)| TocEntry {
                label: info.title.clone(),
                chapter,
                fragment: None,
                depth: 0,
            })
            .collect();
    }
    let mut labeled = vec![false; chapters.len()];
    for toc in &contents {
        let chapter = &mut chapters[toc.chapter];
        if !labeled[toc.chapter] && !toc.label.is_empty() {
            chapter.title.clone_from(&toc.label);
            labeled[toc.chapter] = true;
        }
    }
    Ok(Epub {
        path,
        title: package.title,
        author: package.author,
        fingerprint: format!("{:x}", hash.finalize()),
        chapters,
        contents,
        warnings,
        archive: Mutex::new(archive),
        cover_path: package.cover_path,
        entries,
    })
}

impl Epub {
    pub fn load_chapter(&self, index: usize) -> Result<Chapter, String> {
        let info = self
            .chapters
            .get(index)
            .ok_or("EPUB chapter index out of bounds")?;
        let mut archive = self.archive.lock();
        let bytes = read_named(&mut archive, &self.entries, &info.href, MAX_RESOURCE)?;
        let mut resources = EpubResources {
            chapter: &info.href,
            entries: &self.entries,
            archive: &mut archive,
        };
        let mut parsed = html::parse_html(self.path.clone(), &bytes, &mut resources)?;
        parsed.document.fingerprint.clone_from(&self.fingerprint);
        parsed.document.author.clone_from(&self.author);
        parsed.document.title.clone_from(&info.title);
        Ok(Chapter {
            document: parsed.document,
            anchors: parsed.anchors,
        })
    }

    /// Decode a declared EPUB2/3 cover on demand; missing covers are normal.
    /// Callers should consult the small persistent thumbnail cache first.
    pub fn cover(&self) -> Result<Option<ImageAsset>, String> {
        let Some((path, media)) = self.cover_path.as_ref() else {
            return Ok(None);
        };
        let mut archive = self.archive.lock();
        let encoded = read_named(&mut archive, &self.entries, path, MAX_RESOURCE)?;
        if media == "image/svg+xml" {
            let xml = std::str::from_utf8(&encoded).map_err(|_| "EPUB SVG cover must be UTF-8")?;
            let document = parse_xml(xml)?;
            let image = document.descendants().find(|node| {
                node.is_element()
                    && node.tag_name().name() == "image"
                    && node.tag_name().namespace() == Some("http://www.w3.org/2000/svg")
            });
            let Some(href) = image.and_then(|node| {
                node.attribute(("http://www.w3.org/1999/xlink", "href"))
                    .or_else(|| node.attribute("href"))
            }) else {
                return Ok(None);
            };
            let mut resources = EpubResources {
                chapter: path,
                entries: &self.entries,
                archive: &mut archive,
            };
            let key = resources.resolve(href)?;
            let raster = resources.load(&key)?;
            return html::decode_image(&raster, 128 * 1024 * 1024).map(Some);
        }
        if !matches!(
            media.as_str(),
            "image/png" | "image/jpeg" | "image/gif" | "image/webp"
        ) {
            return Ok(None);
        }
        html::decode_image(&encoded, 128 * 1024 * 1024).map(Some)
    }
}

struct EpubResources<'a> {
    chapter: &'a str,
    entries: &'a HashMap<String, Entry>,
    archive: &'a mut ZipArchive<File>,
}

impl ResourceLoader for EpubResources<'_> {
    fn resolve(&self, source: &str) -> Result<String, String> {
        let (path, fragment) = resolve_uri(self.chapter, source)?;
        if fragment.is_some() {
            return Err("Image URI fragments are unsupported".into());
        }
        if !self.entries.contains_key(&path) {
            return Err(format!("Missing EPUB resource: {path}"));
        }
        Ok(path)
    }

    fn load(&mut self, key: &str) -> Result<Vec<u8>, String> {
        read_named(self.archive, self.entries, key, MAX_RESOURCE)
    }
}

fn read_named(
    archive: &mut ZipArchive<File>,
    entries: &HashMap<String, Entry>,
    path: &str,
    limit: u64,
) -> Result<Vec<u8>, String> {
    let entry = entries
        .get(path)
        .ok_or_else(|| format!("Missing EPUB resource: {path}"))?;
    read_entry(archive, entry, path, limit)
}

fn read_entry(
    archive: &mut ZipArchive<File>,
    entry: &Entry,
    path: &str,
    limit: u64,
) -> Result<Vec<u8>, String> {
    if entry.size > limit {
        return Err(format!("EPUB resource exceeds {limit} bytes: {path}"));
    }
    let mut file = archive
        .by_index(entry.index)
        .map_err(|e| format!("Cannot read EPUB resource {path}: {e}"))?;
    let mut data = Vec::with_capacity(entry.size as usize);
    file.by_ref()
        .take(limit + 1)
        .read_to_end(&mut data)
        .map_err(|e| format!("Cannot decompress EPUB resource {path}: {e}"))?;
    if data.len() as u64 != entry.size {
        return Err(format!(
            "EPUB resource has inconsistent decompressed length: {path}"
        ));
    }
    Ok(data)
}

fn read_xml(
    archive: &mut ZipArchive<File>,
    entries: &HashMap<String, Entry>,
    path: &str,
    remaining: &mut u64,
) -> Result<String, String> {
    let bytes = read_named(archive, entries, path, *remaining)?;
    *remaining -= bytes.len() as u64;
    String::from_utf8(bytes).map_err(|_| format!("EPUB XML must be UTF-8: {path}"))
}

fn parse_xml(source: &str) -> Result<XmlDocument<'_>, String> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    // roxmltree never resolves external DTDs without an explicit resolver.
    // Reject internal subsets first: even bounded input could otherwise expand
    // declared entities into much larger in-memory text values.
    reject_internal_dtd(source)?;
    XmlDocument::parse_with_options(
        source,
        ParsingOptions {
            allow_dtd: true,
            nodes_limit: MAX_XML_NODES,
            entity_resolver: None,
        },
    )
    .map_err(|e| format!("Invalid EPUB XML or unresolved external entity: {e}"))
}

fn reject_internal_dtd(source: &str) -> Result<(), String> {
    let bytes = source.as_bytes();
    let mut pos = 0;
    loop {
        while bytes.get(pos).is_some_and(u8::is_ascii_whitespace) {
            pos += 1;
        }
        if bytes[pos..].starts_with(b"<!--") {
            let end = bytes[pos + 4..]
                .windows(3)
                .position(|w| w == b"-->")
                .ok_or("Unclosed EPUB XML comment")?;
            pos += 4 + end + 3;
        } else if bytes[pos..].starts_with(b"<?") {
            let end = bytes[pos + 2..]
                .windows(2)
                .position(|w| w == b"?>")
                .ok_or("Unclosed EPUB XML processing instruction")?;
            pos += 2 + end + 2;
        } else if bytes[pos..].starts_with(b"<!DOCTYPE") {
            pos += b"<!DOCTYPE".len();
            let mut quote = None;
            loop {
                let Some(&byte) = bytes.get(pos) else {
                    return Err("Unclosed EPUB DOCTYPE".into());
                };
                match byte {
                    b'\'' | b'"' if quote.is_none() => quote = Some(byte),
                    _ if quote == Some(byte) => quote = None,
                    b'[' if quote.is_none() => {
                        return Err("Internal EPUB DTDs/entities are unsupported".into());
                    }
                    b'>' if quote.is_none() => return Ok(()),
                    _ => {}
                }
                pos += 1;
            }
        } else {
            return Ok(());
        }
    }
}

fn parse_package(
    xml: &str,
    path: &str,
    entries: &HashMap<String, Entry>,
) -> Result<Package, String> {
    let doc = parse_xml(xml)?;
    let package = doc.root_element();
    if package.tag_name().name() != "package"
        || package.tag_name().namespace() != Some("http://www.idpf.org/2007/opf")
    {
        return Err("Invalid EPUB OPF package".into());
    }
    let version = package
        .attribute("version")
        .ok_or("OPF package has no version")?;
    if !version.starts_with("2.") && !version.starts_with("3.") {
        return Err(format!("Unsupported EPUB package version: {version}"));
    }
    let metadata = child(package, "metadata").ok_or("OPF metadata missing")?;
    let title = metadata
        .descendants()
        .find(|n| {
            n.is_element()
                && n.tag_name().name() == "title"
                && n.tag_name().namespace() == Some("http://purl.org/dc/elements/1.1/")
        })
        .map(text_of)
        .unwrap_or_default();
    let title = if title.is_empty() {
        file_label(path)
    } else {
        title
    };
    let author = metadata
        .descendants()
        .find(|n| {
            n.is_element()
                && n.tag_name().name() == "creator"
                && n.tag_name().namespace() == Some("http://purl.org/dc/elements/1.1/")
        })
        .map(text_of)
        .filter(|author| !author.is_empty());
    let layout_fixed = metadata.descendants().any(|n| {
        n.is_element()
            && n.tag_name().name() == "meta"
            && n.attribute("refines").is_none()
            && (n.attribute("property") == Some("rendition:layout")
                && text_of(n) == "pre-paginated"
                || n.attribute("name") == Some("fixed-layout")
                    && n.attribute("content") == Some("true"))
    });
    let manifest = child(package, "manifest").ok_or("OPF manifest missing")?;
    let package_base = xml_base(path, package)?;
    let manifest_base = xml_base(&package_base, manifest)?;
    let mut items = HashMap::new();
    let cover_id = metadata
        .descendants()
        .find(|n| {
            n.is_element() && n.tag_name().name() == "meta" && n.attribute("name") == Some("cover")
        })
        .and_then(|n| n.attribute("content"));
    let mut cover_path = None;
    let mut nav = None;
    for item in manifest
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "item")
    {
        let id = item.attribute("id").ok_or("OPF item lacks id")?;
        if id.is_empty() {
            return Err("OPF item id is empty".into());
        }
        let href = item.attribute("href").ok_or("OPF item lacks href")?;
        let (resource, fragment) = resolve_uri(&xml_base(&manifest_base, item)?, href)?;
        if fragment.is_some() {
            return Err(format!("OPF item href contains a fragment: {href}"));
        }
        if !entries.contains_key(&resource) {
            return Err(format!("OPF manifest resource is missing: {resource}"));
        }
        let media = item
            .attribute("media-type")
            .ok_or("OPF item lacks media-type")?
            .to_owned();
        if item.attribute("properties").is_some_and(|properties| {
            properties
                .split_whitespace()
                .any(|property| property == "cover-image")
        }) {
            cover_path = Some((resource.clone(), media.clone()));
        }
        let is_nav = item
            .attribute("properties")
            .is_some_and(|p| p.split_whitespace().any(|s| s == "nav"));
        if is_nav {
            if nav.replace(resource.clone()).is_some() {
                return Err("Multiple EPUB navigation documents".into());
            }
            if media != XHTML {
                return Err("EPUB navigation document must be XHTML".into());
            }
        }
        if items
            .insert(
                id.to_owned(),
                ManifestItem {
                    path: resource,
                    media,
                    fallback: item.attribute("fallback").map(str::to_owned),
                },
            )
            .is_some()
        {
            return Err(format!("Duplicate OPF manifest id: {id}"));
        }
    }
    if cover_path.is_none() {
        cover_path = cover_id
            .and_then(|id| items.get(id))
            .map(|item| (item.path.clone(), item.media.clone()));
    }
    let spine = child(package, "spine").ok_or("OPF spine missing")?;
    let ncx = spine
        .attribute("toc")
        .map(|id| {
            items
                .get(id)
                .ok_or_else(|| format!("Missing NCX manifest item {id}"))
        })
        .transpose()?
        .map(|item| {
            if item.media != "application/x-dtbncx+xml" {
                return Err("EPUB spine toc is not NCX".to_owned());
            }
            Ok(item.path.clone())
        })
        .transpose()?;
    let mut layout_overrides = HashMap::new();
    for node in metadata.descendants().filter(|node| {
        node.is_element()
            && node.tag_name().name() == "meta"
            && node.attribute("property") == Some("rendition:layout")
    }) {
        if let Some(reference) = node
            .attribute("refines")
            .and_then(|value| value.strip_prefix('#'))
            && layout_overrides.insert(reference, text_of(node)).is_some()
        {
            return Err("Duplicate EPUB layout refinement".into());
        }
    }
    let mut chapters = Vec::new();
    let mut seen = HashSet::new();
    for itemref in spine
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "itemref")
    {
        if itemref.attribute("linear") == Some("no") {
            continue;
        }
        let id = itemref
            .attribute("idref")
            .ok_or("OPF spine item lacks idref")?;
        let props = itemref.attribute("properties").unwrap_or("");
        let override_layout = itemref
            .attribute("id")
            .and_then(|id| layout_overrides.get(id))
            .or_else(|| layout_overrides.get(id))
            .map(String::as_str);
        let reflow = props
            .split_whitespace()
            .any(|p| p == "rendition:layout-reflowable")
            || override_layout == Some("reflowable");
        let fixed = props
            .split_whitespace()
            .any(|p| p == "rendition:layout-pre-paginated")
            || override_layout == Some("pre-paginated");
        if fixed || (layout_fixed && !reflow) {
            return Err(format!("Fixed-layout EPUB spine item is unsupported: {id}"));
        }
        let mut current = id;
        let mut chain = HashSet::new();
        let item = loop {
            if !chain.insert(current) {
                return Err(format!("OPF fallback cycle for spine item: {id}"));
            }
            let item = items
                .get(current)
                .ok_or_else(|| format!("Missing OPF spine/fallback item: {current}"))?;
            if item.media == XHTML || item.media == "text/html" {
                break item;
            }
            current = item.fallback.as_deref().ok_or_else(|| {
                format!(
                    "Unsupported required EPUB spine media type {} (item {id})",
                    item.media
                )
            })?;
        };
        if seen.insert(item.path.clone()) {
            if chapters.len() == MAX_CHAPTERS {
                return Err("EPUB exceeds 4096 reading-order chapters".into());
            }
            chapters.push(item.path.clone());
        } else {
            return Err(format!("Repeated EPUB spine resource: {}", item.path));
        }
    }
    if chapters.is_empty() {
        return Err("EPUB has no readable linear spine chapters".into());
    }
    Ok(Package {
        author,
        cover_path,
        title,
        chapters,
        nav,
        ncx,
    })
}

fn check_encryption(
    xml: &str,
    entries: &HashMap<String, Entry>,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let doc = parse_xml(xml)?;
    if doc.root_element().tag_name().name() != "encryption" {
        return Err("Invalid EPUB encryption.xml".into());
    }
    for encrypted in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "EncryptedData")
    {
        let algorithm = encrypted
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "EncryptionMethod")
            .and_then(|n| n.attribute("Algorithm"))
            .ok_or("EPUB encrypted resource lacks algorithm")?;
        let uri = encrypted
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "CipherReference")
            .and_then(|n| n.attribute("URI"))
            .ok_or("EPUB encrypted resource lacks URI")?;
        let (path, fragment) = resolve_uri("encryption.xml", uri)?;
        if fragment.is_some() || !entries.contains_key(&path) {
            return Err(format!("Invalid encrypted resource: {uri}"));
        }
        let font = [".otf", ".ttf", ".woff", ".woff2"]
            .iter()
            .any(|extension| path.to_ascii_lowercase().ends_with(extension));
        if font
            && matches!(
                algorithm,
                "http://www.idpf.org/2008/embedding" | "http://ns.adobe.com/pdf/enc#RC"
            )
        {
            warnings.push(format!("Obfuscated EPUB font is unsupported: {path}"));
        } else {
            return Err(format!(
                "DRM/encrypted EPUB content is unsupported: {path} ({algorithm})"
            ));
        }
    }
    Ok(())
}

fn parse_nav(
    xml: &str,
    path: &str,
    chapters: &HashMap<String, usize>,
) -> Result<Vec<TocEntry>, String> {
    let doc = parse_xml(xml)?;
    let nav = doc
        .descendants()
        .find(|n| {
            n.is_element()
                && n.tag_name().name() == "nav"
                && n.attributes()
                    .any(|a| a.name() == "type" && a.value().split_whitespace().any(|v| v == "toc"))
        })
        .ok_or("EPUB navigation document has no table of contents")?;
    let list = nav
        .children()
        .find(|n| n.is_element() && matches!(n.tag_name().name(), "ol" | "ul"))
        .ok_or("EPUB navigation table of contents has no list")?;
    let mut result = Vec::new();
    visit_nav_list(list, path, chapters, 0, &mut result)?;
    Ok(result)
}

fn visit_nav_list(
    list: Node<'_, '_>,
    path: &str,
    chapters: &HashMap<String, usize>,
    depth: usize,
    out: &mut Vec<TocEntry>,
) -> Result<(), String> {
    if depth > MAX_TOC_DEPTH {
        return Err("EPUB TOC nesting exceeds 64 levels".into());
    }
    for li in list
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "li")
    {
        if let Some(link) = li
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "a")
            && let Some(href) = link.attribute("href")
        {
            add_toc(out, chapters, path, href, text_of(link), depth)?;
        }
        for sub in li
            .children()
            .filter(|n| n.is_element() && matches!(n.tag_name().name(), "ol" | "ul"))
        {
            visit_nav_list(sub, path, chapters, depth + 1, out)?;
        }
    }
    Ok(())
}

fn parse_ncx(
    xml: &str,
    path: &str,
    chapters: &HashMap<String, usize>,
) -> Result<Vec<TocEntry>, String> {
    let doc = parse_xml(xml)?;
    if doc.root_element().tag_name().name() != "ncx" {
        return Err("Invalid EPUB NCX".into());
    }
    let map = doc
        .root_element()
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "navMap")
        .ok_or("EPUB NCX has no navMap")?;
    let mut result = Vec::new();
    visit_ncx(map, path, chapters, 0, &mut result)?;
    Ok(result)
}

fn visit_ncx(
    parent: Node<'_, '_>,
    path: &str,
    chapters: &HashMap<String, usize>,
    depth: usize,
    out: &mut Vec<TocEntry>,
) -> Result<(), String> {
    if depth > MAX_TOC_DEPTH {
        return Err("EPUB TOC nesting exceeds 64 levels".into());
    }
    for point in parent
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "navPoint")
    {
        if let Some(src) = child(point, "content").and_then(|n| n.attribute("src")) {
            let label = child(point, "navLabel").map(text_of).unwrap_or_default();
            add_toc(out, chapters, path, src, label, depth)?;
        }
        visit_ncx(point, path, chapters, depth + 1, out)?;
    }
    Ok(())
}

fn add_toc(
    out: &mut Vec<TocEntry>,
    chapters: &HashMap<String, usize>,
    base: &str,
    href: &str,
    label: String,
    depth: usize,
) -> Result<(), String> {
    let (path, fragment) = resolve_uri(base, href)?;
    if let Some(&chapter) = chapters.get(&path) {
        if out.len() == MAX_TOC {
            return Err("EPUB exceeds 10,000 table of contents entries".into());
        }
        out.push(TocEntry {
            label: if label.is_empty() {
                file_label(&path)
            } else {
                label
            },
            chapter,
            fragment,
            depth,
        });
    }
    Ok(())
}

fn child<'a, 'input: 'a>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name() == name)
}

fn text_of(node: Node<'_, '_>) -> String {
    let mut result = String::new();
    let mut space = false;
    for value in node
        .descendants()
        .filter(|node| node.is_text())
        .filter_map(|node| node.text())
    {
        for character in value.chars() {
            if character.is_whitespace() {
                space = !result.is_empty();
            } else {
                if space {
                    result.push(' ');
                }
                result.push(character);
                space = false;
            }
        }
    }
    result
}

fn file_label(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.')
        .map_or(name, |(stem, _)| stem)
        .replace(&['_', '-'][..], " ")
}

/// An XML base is a URI relative to the containing resource, not a host path.
fn xml_base(base: &str, node: Node<'_, '_>) -> Result<String, String> {
    let Some(value) = node.attribute(("http://www.w3.org/XML/1998/namespace", "base")) else {
        return Ok(base.to_owned());
    };
    let (mut result, fragment) = resolve_uri(base, value)?;
    if fragment.is_some() {
        return Err("EPUB xml:base cannot contain a fragment".into());
    }
    if value.ends_with('/') {
        result.push('/');
    }
    Ok(result)
}

fn member_path(path: &str) -> Result<String, String> {
    if path.len() > 4096 || path.ends_with("//") {
        return Err(format!("Unsafe or overlong ZIP member path: {path}"));
    }
    let path = path.strip_suffix('/').unwrap_or(path);
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\\')
        || path.contains('\0')
        || path.contains(':')
        || path.contains('?')
    {
        return Err(format!("Unsafe ZIP member path: {path}"));
    }
    if path.split('/').any(|part| {
        part.is_empty() || part == "." || part == ".." || part.chars().any(char::is_control)
    }) {
        return Err(format!("Unsafe ZIP member path: {path}"));
    }
    Ok(path.to_owned())
}

/// Decode URI escapes exactly once, then fold relative parent segments inside the archive.
fn resolve_uri(base: &str, href: &str) -> Result<(String, Option<String>), String> {
    let (raw_path, raw_fragment) = href
        .split_once('#')
        .map_or((href, None), |(p, f)| (p, Some(f)));
    if raw_path.contains('?')
        || raw_path.starts_with('/')
        || raw_path.starts_with('\\')
        || raw_path.contains('\\')
        || raw_path.contains(':')
    {
        return Err(format!(
            "External or absolute EPUB URI is unsupported: {href}"
        ));
    }
    fn decode(value: &str) -> Result<String, String> {
        let bytes = value.as_bytes();
        for (i, b) in bytes.iter().enumerate() {
            if *b == b'%'
                && (i + 2 >= bytes.len()
                    || !bytes[i + 1].is_ascii_hexdigit()
                    || !bytes[i + 2].is_ascii_hexdigit())
            {
                return Err("Malformed percent encoding in EPUB URI".into());
            }
        }
        percent_encoding::percent_decode_str(value)
            .decode_utf8()
            .map(|s| s.into_owned())
            .map_err(|_| "EPUB URI is not UTF-8".into())
    }
    let decoded = decode(raw_path)?;
    if decoded.starts_with('/')
        || decoded.starts_with('\\')
        || decoded.contains('\\')
        || decoded.contains(':')
        || decoded.contains('?')
        || decoded.contains('\0')
    {
        return Err(format!(
            "External or absolute EPUB URI is unsupported: {href}"
        ));
    }
    let mut parts: Vec<&str> = base.split('/').collect();
    if raw_path.is_empty() { /* An empty URI path refers to its base resource. */
    } else {
        parts.pop();
        for part in decoded.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    if parts.pop().is_none() {
                        return Err(format!("EPUB URI escapes archive: {href}"));
                    }
                }
                _ if part.chars().any(char::is_control) => {
                    return Err(format!("Invalid EPUB URI: {href}"));
                }
                _ => parts.push(part),
            }
        }
    }
    let result = parts.join("/");
    if result.is_empty() || result.len() > 4096 {
        return Err("EPUB URI resolves to archive root or overlong path".into());
    }
    let fragment = raw_fragment
        .map(decode)
        .transpose()?
        .filter(|s| !s.is_empty());
    Ok((result, fragment))
}

fn is_network_path(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        matches!(path.components().next(), Some(Component::Prefix(p))
            if matches!(p.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..) | Prefix::DeviceNS(..)))
    }
    #[cfg(not(windows))]
    {
        let b = path.as_os_str().as_encoded_bytes();
        b.starts_with(b"//") || b.starts_with(b"\\\\")
    }
}

/// Bound ZIP central-directory parsing before handing bytes to zip, which indexes
/// equal raw names by replacement. Compare this raw record count with its index.
fn inspect_directory(file: &mut File, file_len: u64) -> Result<usize, String> {
    fn at(file: &mut File, offset: u64, data: &mut [u8]) -> Result<(), String> {
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| format!("Cannot seek ZIP: {e}"))?;
        file.read_exact(data)
            .map_err(|e| format!("Invalid ZIP directory: {e}"))
    }
    fn u16_at(b: &[u8], i: usize) -> u16 {
        u16::from_le_bytes([b[i], b[i + 1]])
    }
    fn u32_at(b: &[u8], i: usize) -> u32 {
        u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
    }
    fn u64_at(b: &[u8], i: usize) -> u64 {
        u64::from_le_bytes([
            b[i],
            b[i + 1],
            b[i + 2],
            b[i + 3],
            b[i + 4],
            b[i + 5],
            b[i + 6],
            b[i + 7],
        ])
    }

    if file_len < 22 {
        return Err("EPUB has no ZIP end-of-directory record".into());
    }
    let trailer_len = file_len.min(65_557) as usize;
    let mut trailer = vec![0; trailer_len];
    at(file, file_len - trailer_len as u64, &mut trailer)?;
    let end = (0..=trailer_len - 22)
        .rev()
        .find(|&i| {
            trailer[i..].starts_with(b"PK\x05\x06")
                && i + 22 + u16_at(&trailer, i + 20) as usize == trailer_len
        })
        .ok_or("EPUB has no valid ZIP end-of-directory record")?;
    let record = &trailer[end..end + 22];
    if u16_at(record, 4) != 0 || u16_at(record, 6) != 0 || u16_at(record, 8) != u16_at(record, 10) {
        return Err("Multi-disk ZIP archives are unsupported".into());
    }
    let end_offset = file_len - trailer_len as u64 + end as u64;
    let mut count = u16_at(record, 10) as u64;
    let mut dir_size = u32_at(record, 12) as u64;
    let mut dir_offset = u32_at(record, 16) as u64;
    let zip64 =
        count == u16::MAX as u64 || dir_size == u32::MAX as u64 || dir_offset == u32::MAX as u64;
    if zip64 {
        if end_offset < 20 {
            return Err("Missing ZIP64 directory locator".into());
        }
        let mut locator = [0u8; 20];
        at(file, end_offset - 20, &mut locator)?;
        if &locator[..4] != b"PK\x06\x07" || u32_at(&locator, 4) != 0 || u32_at(&locator, 16) != 1 {
            return Err("Invalid or multi-disk ZIP64 directory locator".into());
        }
        let zip64_offset = u64_at(&locator, 8);
        if zip64_offset > end_offset - 20 || end_offset - 20 - zip64_offset < 56 {
            return Err("ZIP64 directory record is outside archive".into());
        }
        let mut record64 = [0u8; 56];
        at(file, zip64_offset, &mut record64)?;
        let zip64_record_size = u64_at(&record64, 4);
        if zip64_record_size > MAX_METADATA
            || zip64_offset
                .checked_add(12)
                .and_then(|start| start.checked_add(zip64_record_size))
                .is_none_or(|end| end > end_offset - 20)
        {
            return Err("ZIP64 directory record exceeds archive metadata bounds".into());
        }
        if &record64[..4] != b"PK\x06\x06"
            || u64_at(&record64, 4) < 44
            || u32_at(&record64, 16) != 0
            || u32_at(&record64, 20) != 0
            || u64_at(&record64, 24) != u64_at(&record64, 32)
        {
            return Err("Invalid or multi-disk ZIP64 directory record".into());
        }
        count = u64_at(&record64, 32);
        dir_size = u64_at(&record64, 40);
        dir_offset = u64_at(&record64, 48);
    }
    if count == 0 || count > MAX_ENTRIES as u64 {
        return Err("EPUB has no entries or exceeds 20,000 ZIP entries".into());
    }
    if dir_size > MAX_METADATA
        || dir_offset
            .checked_add(dir_size)
            .is_none_or(|end| end > end_offset)
    {
        return Err("EPUB ZIP central directory exceeds 8 MiB or archive bounds".into());
    }
    let directory_end = dir_offset + dir_size;
    let mut position = dir_offset;
    let mut names = HashSet::with_capacity(count as usize);
    for _ in 0..count {
        if position
            .checked_add(46)
            .is_none_or(|end| end > directory_end)
        {
            return Err("Truncated ZIP central-directory entry".into());
        }
        let mut header = [0u8; 46];
        at(file, position, &mut header)?;
        if &header[..4] != b"PK\x01\x02" {
            return Err("Invalid ZIP central-directory entry".into());
        }
        let name_len = u16_at(&header, 28) as usize;
        let record_len =
            46 + name_len + u16_at(&header, 30) as usize + u16_at(&header, 32) as usize;
        if name_len == 0
            || name_len > 4097
            || position
                .checked_add(record_len as u64)
                .is_none_or(|end| end > directory_end)
        {
            return Err("Invalid or overlong ZIP central-directory entry".into());
        }
        let mut name = vec![0u8; name_len];
        at(file, position + 46, &mut name)?;
        if !names.insert(name) {
            return Err("Duplicate ZIP member names".into());
        }
        position += record_len as u64;
    }
    if position != directory_end {
        return Err("ZIP central-directory entry count does not match its size".into());
    }
    Ok(count as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Cursor, Write},
        sync::atomic::{AtomicU64, Ordering},
    };
    use zip::write::SimpleFileOptions;

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);
    struct TestEpub(PathBuf);
    impl Drop for TestEpub {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    fn fixture(extra: &[(&str, &[u8])]) -> TestEpub {
        let file = TestEpub(std::env::temp_dir().join(format!(
            "reader-epub-{}-{}.epub",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed),
        )));
        let out = File::create(&file.0).unwrap();
        let mut zip = zip::ZipWriter::new(out);
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file("mimetype", stored).unwrap();
        zip.write_all(b"application/epub+zip").unwrap();
        zip.start_file("META-INF/container.xml", deflated).unwrap();
        zip.write_all(br#"<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
        for (name, bytes) in extra {
            zip.start_file(*name, deflated).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        file
    }

    fn chapter_one() -> &'static [u8] {
        br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h1 id="start">First chapter</h1><p id="middle">Alpha passage</p><img src="../images/dot.png"/></body></html>"#
    }
    fn chapter_two() -> &'static [u8] {
        br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h2 id="two">Second chapter</h2><p>Beta passage</p></body></html>"#
    }
    fn opf3() -> &'static [u8] {
        br#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Unicode book</dc:title></metadata><manifest><item id="a" href="text/Caf%C3%A9%23one.xhtml" media-type="application/xhtml+xml"/><item id="b" href="text/two.xhtml" media-type="application/xhtml+xml"/><item id="image" href="images/dot.png" media-type="image/png"/><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/></manifest><spine><itemref idref="a"/><itemref idref="b"/></spine></package>"#
    }
    fn nav3() -> &'static [u8] {
        br##"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol><li><a href="text/Caf%C3%A9%23one.xhtml#start">Chapter One</a><ol><li><a href="text/Caf%C3%A9%23one.xhtml#middle">Middle</a></li></ol></li><li><a href="text/two.xhtml#two">Chapter Two</a></li></ol></nav></body></html>"##
    }
    fn png() -> Vec<u8> {
        let mut result = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(1, 1)
            .write_to(&mut result, image::ImageFormat::Png)
            .unwrap();
        result.into_inner()
    }
    fn book3() -> TestEpub {
        let image = png();
        fixture(&[
            ("OEBPS/book.opf", opf3()),
            ("OEBPS/nav.xhtml", nav3()),
            ("OEBPS/text/Café#one.xhtml", chapter_one()),
            ("OEBPS/text/two.xhtml", chapter_two()),
            ("OEBPS/images/dot.png", &image),
        ])
    }

    #[test]
    fn epub3_spine_nested_nav_images_and_fragments() {
        let file = book3();
        let book = open(&file.0).unwrap();
        assert_eq!(book.title, "Unicode book");
        assert_eq!(book.chapters.len(), 2);
        assert_eq!(book.chapters[0].href, "OEBPS/text/Café#one.xhtml");
        assert_eq!(book.chapters[0].title, "Chapter One");
        assert_eq!(
            book.contents
                .iter()
                .map(|e| (e.chapter, e.depth, e.fragment.as_deref()))
                .collect::<Vec<_>>(),
            [
                (0, 0, Some("start")),
                (0, 1, Some("middle")),
                (1, 0, Some("two"))
            ]
        );
        let first = book.load_chapter(0).unwrap();
        assert!(
            first
                .document
                .items
                .iter()
                .filter_map(crate::Item::text)
                .any(|s| s.contains("Alpha passage"))
        );
        assert_eq!(first.document.images.len(), 1);
        assert!(first.anchors.contains_key("middle"));
        assert_eq!(first.document.path, book.path);
        assert_eq!(first.document.fingerprint, book.fingerprint);
        let second = book.load_chapter(1).unwrap();
        assert!(
            second
                .document
                .items
                .iter()
                .filter_map(crate::Item::text)
                .any(|s| s.contains("Beta passage"))
        );
        assert_eq!(
            book.load_chapter(2).unwrap_err(),
            "EPUB chapter index out of bounds"
        );
    }

    #[test]
    fn epub2_ncx_resolves_spine_fragments() {
        let opf = br#"<package xmlns="http://www.idpf.org/2007/opf" version="2.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Two</dc:title></metadata><manifest><item id="a" href="text/a.xhtml" media-type="application/xhtml+xml"/><item id="b" href="text/b.xhtml" media-type="application/xhtml+xml"/><item id="toc" href="toc.ncx" media-type="application/x-dtbncx+xml"/></manifest><spine toc="toc"><itemref idref="a"/><itemref idref="b"/></spine></package>"#;
        let ncx = br##"<!DOCTYPE ncx PUBLIC "-//NISO//DTD ncx 2005-1//EN" "https://example.invalid/ncx-2005-1.dtd"><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap><navPoint><navLabel><text>One</text></navLabel><content src="text/a.xhtml#start"/><navPoint><navLabel><text>Subsection</text></navLabel><content src="text/a.xhtml#middle"/></navPoint></navPoint><navPoint><navLabel><text>Two</text></navLabel><content src="text/b.xhtml#two"/></navPoint></navMap></ncx>"##;
        let file = fixture(&[
            ("OEBPS/book.opf", opf),
            ("OEBPS/toc.ncx", ncx),
            ("OEBPS/text/a.xhtml", chapter_one()),
            ("OEBPS/text/b.xhtml", chapter_two()),
        ]);
        let book = open(&file.0).unwrap();
        assert_eq!(book.contents.len(), 3);
        assert_eq!(book.contents[1].depth, 1);
        assert_eq!(book.contents[1].fragment.as_deref(), Some("middle"));
        assert_eq!(book.contents[2].chapter, 1);
        assert!(book.load_chapter(1).unwrap().anchors.contains_key("two"));
    }

    #[test]
    fn missing_toc_uses_spine_instead_of_empty_contents() {
        let opf = br#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata/><manifest><item id="a" href="text/a.xhtml" media-type="application/xhtml+xml"/><item id="b" href="text/b.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="a"/><itemref idref="b"/></spine></package>"#;
        let file = fixture(&[
            ("OEBPS/book.opf", opf),
            ("OEBPS/text/a.xhtml", chapter_one()),
            ("OEBPS/text/b.xhtml", chapter_two()),
        ]);
        let book = open(&file.0).unwrap();
        assert_eq!(
            book.contents
                .iter()
                .map(|entry| entry.chapter)
                .collect::<Vec<_>>(),
            [0, 1]
        );
    }

    #[test]
    fn unsafe_members_external_urls_and_unbounded_metadata_fail_closed() {
        let path = fixture(&[("../escape.xhtml", b"secret")]);
        assert!(open(&path.0).unwrap_err().contains("Unsafe ZIP member"));
        assert!(resolve_uri("OEBPS/text/a.xhtml", "../../../other.xhtml").is_err());
        assert_eq!(
            resolve_uri("OEBPS/text/a.xhtml", "../../other.xhtml")
                .unwrap()
                .0,
            "other.xhtml"
        );
        assert!(resolve_uri("OEBPS/text/a.xhtml", "https://example.org/image.png").is_err());
        assert!(resolve_uri("OEBPS/text/a.xhtml", "%2f%2fhost/file").is_err());
        assert!(resolve_uri("OEBPS/text/a.xhtml", "..%5c..%5cimage.png").is_err());
        assert_eq!(
            resolve_uri("OEBPS/text/a.xhtml", "../images/image%252e.png")
                .unwrap()
                .0,
            "OEBPS/images/image%2e.png"
        );
        let file = book3();
        let mut bytes = fs::read(&file.0).unwrap();
        let central = bytes.windows(4).position(|b| b == b"PK\x01\x02").unwrap();
        bytes[central + 24..central + 28].copy_from_slice(&(MAX_TOTAL as u32).to_le_bytes());
        fs::write(&file.0, bytes).unwrap();
        assert!(open(&file.0).is_err());
    }

    #[test]
    fn encryption_fixed_layout_and_unsupported_spine_are_explicit() {
        let enc = br#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><EncryptedData><EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#aes256-cbc"/><CipherReference URI="OEBPS/text/a.xhtml"/></EncryptedData></encryption>"#;
        let opf = br#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata><meta property="rendition:layout">pre-paginated</meta></metadata><manifest><item id="a" href="text/a.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="a"/></spine></package>"#;
        let file = fixture(&[
            ("OEBPS/book.opf", opf),
            ("OEBPS/text/a.xhtml", chapter_one()),
        ]);
        assert!(open(&file.0).unwrap_err().contains("Fixed-layout"));
        let opf = br#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata/><manifest><item id="a" href="text/a.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="a"/></spine></package>"#;
        let file = fixture(&[
            ("OEBPS/book.opf", opf),
            ("OEBPS/text/a.xhtml", chapter_one()),
            ("META-INF/encryption.xml", enc),
        ]);
        assert!(open(&file.0).unwrap_err().contains("DRM/encrypted"));
        let opf = br#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata/><manifest><item id="a" href="part.svg" media-type="image/svg+xml"/></manifest><spine><itemref idref="a"/></spine></package>"#;
        let file = fixture(&[("OEBPS/book.opf", opf), ("OEBPS/part.svg", b"<svg/>")]);
        assert!(
            open(&file.0)
                .unwrap_err()
                .contains("Unsupported required EPUB spine media")
        );
    }

    #[test]
    fn supported_fallback_and_obfuscated_font_still_open_reflowable_chapter() {
        let opf = br##"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata><meta property="rendition:layout">pre-paginated</meta><meta refines="#fallback" property="rendition:layout">reflowable</meta></metadata><manifest><item id="cover" href="cover.svg" media-type="image/svg+xml" fallback="fallback"/><item id="fallback" href="text/a.xhtml" media-type="application/xhtml+xml"/><item id="font" href="fonts/book.otf" media-type="application/vnd.ms-opentype"/></manifest><spine><itemref idref="cover" properties="rendition:layout-reflowable"/></spine></package>"##;
        let enc = br#"<encryption><EncryptedData><EncryptionMethod Algorithm="http://www.idpf.org/2008/embedding"/><CipherReference URI="OEBPS/fonts/book.otf"/></EncryptedData></encryption>"#;
        let file = fixture(&[
            ("OEBPS/book.opf", opf),
            ("OEBPS/cover.svg", b"<svg/>"),
            ("OEBPS/text/a.xhtml", chapter_one()),
            ("OEBPS/fonts/book.otf", b"font"),
            ("META-INF/encryption.xml", enc),
        ]);
        let book = open(&file.0).unwrap();
        assert_eq!(book.chapters[0].href, "OEBPS/text/a.xhtml");
        assert!(
            book.warnings
                .iter()
                .any(|w| w.contains("Obfuscated EPUB font"))
        );
        assert_eq!(book.contents[0].chapter, 0);
        assert!(book.load_chapter(0).unwrap().anchors.contains_key("start"));
    }

    #[test]
    fn declared_epub2_and_epub3_covers_and_authors() {
        let image = png();
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><image xlink:href="cover.png"/></svg>"#;
        for (version, metadata, cover_manifest, cover_path, cover_bytes) in [
            (
                "2.0",
                r#"<dc:creator> Mary Shelley </dc:creator><meta name="cover" content="art"/>"#,
                r#"<item id="art" href="cover.png" media-type="image/png"/>"#,
                "OEBPS/cover.png",
                image.as_slice(),
            ),
            (
                "3.0",
                "<dc:creator>Octavia Butler</dc:creator>",
                r#"<item id="art" href="cover.svg" media-type="image/svg+xml" properties="cover-image"/><item id="raster" href="cover.png" media-type="image/png"/>"#,
                "OEBPS/cover.svg",
                svg.as_slice(),
            ),
        ] {
            let opf = format!(
                r#"<package xmlns="http://www.idpf.org/2007/opf" version="{version}"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Real title</dc:title>{metadata}</metadata><manifest><item id="text" href="chapter.xhtml" media-type="application/xhtml+xml"/>{cover_manifest}</manifest><spine><itemref idref="text"/></spine></package>"#
            );
            let mut members = vec![
                ("OEBPS/book.opf", opf.as_bytes()),
                ("OEBPS/chapter.xhtml", chapter_two()),
                (cover_path, cover_bytes),
            ];
            if version == "3.0" {
                members.push(("OEBPS/cover.png", image.as_slice()));
            }
            let file = fixture(&members);
            let book = open(&file.0).unwrap();
            assert_eq!(book.title, "Real title");
            assert_eq!(
                book.author.as_deref(),
                Some(if version == "2.0" {
                    "Mary Shelley"
                } else {
                    "Octavia Butler"
                })
            );
            assert_eq!(
                book.cover()
                    .unwrap()
                    .map(|asset| (asset.width, asset.height)),
                Some((1, 1))
            );
        }
    }

    #[test]
    fn broken_declared_cover_does_not_prevent_opening_text() {
        let opf = br#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata/><manifest><item id="text" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="cover" href="broken.png" media-type="image/png" properties="cover-image"/></manifest><spine><itemref idref="text"/></spine></package>"#;
        let file = fixture(&[
            ("OEBPS/book.opf", opf),
            ("OEBPS/chapter.xhtml", chapter_two()),
            ("OEBPS/broken.png", b"not an image"),
        ]);
        let book = open(&file.0).unwrap();
        assert!(book.cover().is_err());
        assert!(!book.load_chapter(0).unwrap().document.items.is_empty());
    }

    #[test]
    fn declared_zip_entry_count_is_checked_before_archive_parsing() {
        let file = book3();
        let mut bytes = fs::read(&file.0).unwrap();
        let eocd = bytes.windows(4).rposition(|b| b == b"PK\x05\x06").unwrap();
        bytes[eocd + 8..eocd + 12].copy_from_slice(&[0x21, 0x4e, 0x21, 0x4e]);
        fs::write(&file.0, bytes).unwrap();
        assert!(open(&file.0).unwrap_err().contains("20,000 ZIP entries"));
    }

    #[test]
    fn xml_dtd_is_rejected_without_entity_expansion() {
        let opf = br#"<!DOCTYPE package [<!ENTITY payload "external">]><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata/><manifest><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="a"/></spine></package>"#;
        let file = fixture(&[("OEBPS/book.opf", opf), ("OEBPS/a.xhtml", chapter_one())]);
        assert!(open(&file.0).unwrap_err().contains("DTDs"));
    }
}
