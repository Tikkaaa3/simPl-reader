use std::{collections::HashMap, fs, io::Read, path::Path};

use html5ever::{parse_document, tendril::TendrilSink};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use sha2::{Digest, Sha256};

use crate::{BaseDirection, Document, ImageAsset, InlineStyle, Item, StyleRun};

const MAX_HTML_BYTES: u64 = 32 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 24_000_000;
const MAX_TOTAL_RGBA_BYTES: usize = 128 * 1024 * 1024;
const MAX_DOM_DEPTH: usize = 512;

fn is_unc_path(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        matches!(path.components().next(), Some(Component::Prefix(prefix))
            if matches!(prefix.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..)))
    }
    #[cfg(not(windows))]
    {
        let bytes = path.as_os_str().as_encoded_bytes();
        bytes.starts_with(b"//") || bytes.starts_with(b"\\\\")
    }
}

/// Load a local HTML or XHTML file using HTML5 tree construction and entity decoding.
pub fn load_html(path: &Path) -> Result<Document, String> {
    if is_unc_path(path) {
        return Err("Network/UNC HTML paths are unsupported".into());
    }
    let path = fs::canonicalize(path)
        .map_err(|e| format!("Cannot open HTML file {}: {e}", path.display()))?;
    if is_unc_path(&path) {
        return Err("Network/UNC HTML paths are unsupported".into());
    }
    let file = fs::File::open(&path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|e| format!("Cannot inspect {}: {e}", path.display()))?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_HTML_BYTES {
        return Err(format!(
            "HTML file is empty, not a regular file, or exceeds {MAX_HTML_BYTES} bytes: {}",
            path.display()
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_HTML_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_HTML_BYTES {
        return Err(format!(
            "HTML file is empty or exceeds {MAX_HTML_BYTES} bytes: {}",
            path.display()
        ));
    }
    if bytes.contains(&0) {
        return Err("HTML contains NUL bytes (binary or unsupported encoding)".into());
    }
    if bytes
        .iter()
        .any(|&b| b < 0x20 && !matches!(b, b'\t' | b'\r' | b'\n'))
    {
        return Err("HTML contains binary control bytes".into());
    }
    let source = std::str::from_utf8(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes))
        .map_err(|_| "HTML must be UTF-8 (legacy encodings are unsupported)".to_owned())?;
    if source.trim().is_empty() {
        return Err("HTML contains no content".into());
    }
    let dom: RcDom = parse_document(RcDom::default(), Default::default()).one(source);
    check_dom_depth(&dom.document)?;
    if let Some(head) = find_element(&dom.document, "head") {
        check_charset(&head)?;
    }
    let mut extractor = Extractor {
        parent: path.parent().ok_or("HTML file has no parent directory")?,
        items: Vec::new(),
        images: HashMap::new(),
        warnings: Vec::new(),
        current: TextBlock::default(),
        kind: BlockKind::Paragraph,
        next_id: 0,
        total_rgba_bytes: 0,
    };
    let mut title = String::new();
    find_title(&dom.document, &mut title);
    if let Some(body) = find_element(&dom.document, "body") {
        extractor.walk(&body, Context::default());
    }
    extractor.flush();
    if extractor.items.is_empty() {
        return Err(format!(
            "HTML has no readable body text or local images{}",
            extractor
                .warnings
                .first()
                .map(|warning| format!("; {warning}"))
                .unwrap_or_default()
        ));
    }
    let title = if title.trim().is_empty() {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into())
    } else {
        title.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    let Extractor {
        items,
        images,
        warnings,
        ..
    } = extractor;
    Ok(Document {
        path,
        title,
        fingerprint: format!("{:x}", Sha256::digest(&bytes)),
        items,
        images,
        warnings,
    })
}

fn check_dom_depth(root: &Handle) -> Result<(), String> {
    let mut stack = vec![(root.clone(), 0usize)];
    while let Some((node, depth)) = stack.pop() {
        if depth > MAX_DOM_DEPTH {
            // Avoid recursive RcDom destruction even for hostile deeply nested input.
            drain_tree(root);
            return Err(format!("HTML nesting exceeds {MAX_DOM_DEPTH} levels"));
        }
        stack.extend(
            node.children
                .borrow()
                .iter()
                .map(|child| (child.clone(), depth + 1)),
        );
    }
    Ok(())
}

fn drain_tree(root: &Handle) {
    let mut stack = vec![root.clone()];
    while let Some(node) = stack.pop() {
        stack.extend(std::mem::take(&mut *node.children.borrow_mut()));
    }
}

fn find_element(node: &Handle, name: &str) -> Option<Handle> {
    if let NodeData::Element { name: element, .. } = &node.data
        && element.local.as_ref() == name
    {
        return Some(node.clone());
    }
    for child in node.children.borrow().iter() {
        if let Some(found) = find_element(child, name) {
            return Some(found);
        }
    }
    None
}

fn find_title(node: &Handle, title: &mut String) {
    if let Some(head) = find_element(node, "head")
        && let Some(element) = find_element(&head, "title")
    {
        collect_text(&element, title);
    }
}

fn collect_text(node: &Handle, out: &mut String) {
    if let NodeData::Text { contents } = &node.data {
        out.push_str(&contents.borrow());
    }
    for child in node.children.borrow().iter() {
        collect_text(child, out);
    }
}

fn attr(node: &Handle, key: &str) -> Option<String> {
    let NodeData::Element { attrs, .. } = &node.data else {
        return None;
    };
    attrs
        .borrow()
        .iter()
        .find(|a| a.name.local.as_ref() == key)
        .map(|a| a.value.to_string())
}

fn check_charset(head: &Handle) -> Result<(), String> {
    if let NodeData::Element { name, .. } = &head.data
        && name.local.as_ref() == "meta"
    {
        let charset = attr(head, "charset").or_else(|| {
            attr(head, "content").and_then(|content| {
                content
                    .to_ascii_lowercase()
                    .split("charset=")
                    .nth(1)
                    .map(|value| value.trim_matches([' ', '\'', '"', ';']).to_owned())
            })
        });
        if let Some(value) = charset
            && !matches!(value.to_ascii_lowercase().as_str(), "utf-8" | "utf8")
        {
            return Err(format!(
                "Unsupported HTML encoding {value:?}; UTF-8 is required"
            ));
        }
    }
    for child in head.children.borrow().iter() {
        check_charset(child)?;
    }
    Ok(())
}

fn hidden_inline_style(style: &str) -> bool {
    style
        .split(';')
        .filter_map(|declaration| declaration.split_once(':'))
        .any(|(name, value)| {
            let value = value.split('!').next().unwrap_or(value).trim();
            (name.trim().eq_ignore_ascii_case("display") && value.eq_ignore_ascii_case("none"))
                || (name.trim().eq_ignore_ascii_case("visibility")
                    && value.eq_ignore_ascii_case("hidden"))
        })
}

#[derive(Clone, Copy, Default)]
struct Context {
    bold: bool,
    italic: bool,
    pre: bool,
    direction: Option<BaseDirection>,
    list_number: Option<i64>,
}

#[derive(Clone, Copy)]
enum BlockKind {
    Paragraph,
    Heading(u8),
}

#[derive(Default)]
struct TextBlock {
    text: String,
    styles: Vec<StyleRun>,
    pending_space: Option<Context>,
    direction: Option<BaseDirection>,
    pre: bool,
}

impl TextBlock {
    fn append(&mut self, source: &str, context: Context) {
        self.direction = self.direction.or(context.direction);
        self.pre |= context.pre;
        if context.pre {
            for c in source.chars() {
                if c == '\r' {
                    continue;
                }
                self.push(c, context);
            }
        } else {
            for c in source.chars() {
                if matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}') {
                    self.pending_space.get_or_insert(context);
                } else {
                    if let Some(space_context) = self.pending_space.take()
                        && !self.text.is_empty()
                        && !self.text.ends_with('\n')
                    {
                        self.push(' ', space_context);
                    }
                    self.push(c, context);
                }
            }
        }
    }

    fn push(&mut self, c: char, context: Context) {
        let start = self.text.len();
        self.text.push(c);
        if let Some(style) = match (context.bold, context.italic) {
            (true, true) => Some(InlineStyle::BoldItalic),
            (true, false) => Some(InlineStyle::Bold),
            (false, true) => Some(InlineStyle::Italic),
            _ => None,
        } {
            if let Some(last) = self.styles.last_mut()
                && last.style == style
                && last.end_byte == start
            {
                last.end_byte = self.text.len();
                return;
            }
            self.styles.push(StyleRun {
                start_byte: start,
                end_byte: self.text.len(),
                style,
            });
        }
    }

    fn line_break(&mut self) {
        self.pending_space = None;
        if !self.text.is_empty() && !self.text.ends_with('\n') {
            self.text.push('\n');
        }
    }
}

struct Extractor<'a> {
    parent: &'a Path,
    items: Vec<Item>,
    images: HashMap<String, ImageAsset>,
    warnings: Vec<String>,
    current: TextBlock,
    kind: BlockKind,
    next_id: usize,
    total_rgba_bytes: usize,
}

impl Extractor<'_> {
    fn id(&mut self) -> String {
        self.next_id += 1;
        format!("item-{:06}", self.next_id)
    }

    fn flush(&mut self) {
        let mut block = std::mem::take(&mut self.current);
        let trimmed = if block.pre {
            block.text.len()
        } else {
            block
                .text
                .trim_end_matches(|c: char| c.is_whitespace())
                .len()
        };
        block.text.truncate(trimmed);
        block.styles.retain_mut(|run| {
            run.end_byte = run.end_byte.min(trimmed);
            run.start_byte < run.end_byte
        });
        if block.text.trim().is_empty() {
            return;
        }
        let id = self.id();
        match self.kind {
            BlockKind::Heading(level) => self.items.push(Item::Heading {
                id,
                text: block.text,
                level,
            }),
            BlockKind::Paragraph => self.items.push(Item::Paragraph {
                id,
                base_direction: block
                    .direction
                    .unwrap_or_else(|| detect_direction(&block.text)),
                text: block.text,
                style_runs: block.styles,
            }),
        }
    }

    fn walk(&mut self, node: &Handle, mut context: Context) {
        let NodeData::Element { name, .. } = &node.data else {
            if let NodeData::Text { contents } = &node.data {
                self.current.append(&contents.borrow(), context);
            }
            return;
        };
        let tag = name.local.as_ref();
        if matches!(
            tag,
            "script"
                | "style"
                | "template"
                | "noscript"
                | "svg"
                | "canvas"
                | "iframe"
                | "object"
                | "audio"
                | "video"
                | "source"
        ) || attr(node, "hidden").is_some()
            || attr(node, "aria-hidden").as_deref() == Some("true")
            || attr(node, "style").is_some_and(|s| hidden_inline_style(&s))
        {
            return;
        }
        match attr(node, "dir").as_deref() {
            Some("rtl") => context.direction = Some(BaseDirection::Rtl),
            Some("ltr") => context.direction = Some(BaseDirection::Ltr),
            Some("auto") => context.direction = None,
            _ => {}
        }
        context.bold |= matches!(tag, "b" | "strong");
        context.italic |= matches!(tag, "i" | "em" | "cite");
        context.pre |= matches!(tag, "pre" | "textarea");
        let heading = match tag {
            "h1" => Some(1),
            "h2" => Some(2),
            "h3" => Some(3),
            "h4" => Some(4),
            "h5" => Some(5),
            "h6" => Some(6),
            _ => None,
        };
        let block = heading.is_some()
            || matches!(
                tag,
                "p" | "div"
                    | "section"
                    | "article"
                    | "header"
                    | "footer"
                    | "nav"
                    | "form"
                    | "blockquote"
                    | "pre"
                    | "ul"
                    | "ol"
                    | "li"
                    | "table"
                    | "thead"
                    | "tbody"
                    | "tfoot"
                    | "tr"
                    | "dl"
                    | "dt"
                    | "dd"
                    | "figure"
                    | "figcaption"
                    | "hr"
            );
        if block {
            self.flush();
        }
        let old_kind = self.kind;
        if let Some(level) = heading {
            self.kind = BlockKind::Heading(level);
        }
        if tag == "li" {
            let marker = context
                .list_number
                .map_or_else(|| "• ".to_owned(), |number| format!("{number}. "));
            self.current.append(&marker, context);
        }
        if tag == "img" {
            self.image(node, context);
        } else if tag == "br" {
            self.current.line_break();
        } else if tag == "hr" {
            self.current.append("────────", context);
        } else {
            let children = node.children.borrow();
            let reversed = tag == "ol" && attr(node, "reversed").is_some();
            let mut number = (tag == "ol").then(|| {
                attr(node, "start").and_then(|value| value.parse::<i64>().ok()).unwrap_or_else(|| {
                    if reversed {
                        children.iter().filter(|child| {
                            matches!(&child.data, NodeData::Element { name, .. } if name.local.as_ref() == "li")
                        }).count() as i64
                    } else { 1 }
                })
            });
            for child in children.iter() {
                let mut child_context = context;
                if matches!(tag, "ol" | "ul")
                    && matches!(&child.data, NodeData::Element { name, .. } if name.local.as_ref() == "li")
                {
                    child_context.list_number = number.map(|current| {
                        attr(child, "value")
                            .and_then(|value| value.parse::<i64>().ok())
                            .unwrap_or(current)
                    });
                    number = child_context
                        .list_number
                        .map(|current| current.saturating_add(if reversed { -1 } else { 1 }));
                }
                self.walk(child, child_context);
            }
        }
        if matches!(tag, "td" | "th") {
            self.current.append(" | ", context);
        }
        if block {
            self.flush();
            self.kind = old_kind;
        }
    }

    fn image(&mut self, node: &Handle, context: Context) {
        self.flush();
        let src = attr(node, "src").unwrap_or_default();
        let alt = attr(node, "alt").unwrap_or_default();
        let result = self.resolve_image(&src);
        match result {
            Ok((key, pixels)) => {
                if let Some(pixels) = pixels {
                    self.images.insert(key.clone(), pixels);
                }
                let id = self.id();
                self.items.push(Item::Image {
                    id,
                    asset_path: key,
                });
            }
            Err(error) => {
                self.warnings.push(format!("Image {src:?}: {error}"));
                if !alt.trim().is_empty() {
                    self.current.append(&alt, context);
                    self.flush();
                }
            }
        }
    }

    fn resolve_image(&mut self, src: &str) -> Result<(String, Option<ImageAsset>), String> {
        let url_path = src.split(['?', '#']).next().unwrap_or("");
        if url_path.is_empty()
            || url_path.starts_with('/')
            || url_path.starts_with('\\')
            || url_path.contains('\\')
            || url_path.contains(':')
        {
            return Err("blocked non-local or absolute image URL".into());
        }
        let decoded = percent_encoding::percent_decode_str(url_path)
            .decode_utf8()
            .map_err(|_| "invalid image filename encoding")?;
        if decoded.starts_with('/')
            || decoded.contains('\\')
            || decoded.contains(':')
            || decoded.contains('\0')
            || Path::new(decoded.as_ref()).components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::Prefix(_)
                        | std::path::Component::RootDir
                )
            })
        {
            return Err("blocked image path outside document directory".into());
        }
        let requested = self.parent.join(decoded.as_ref());
        let actual = fs::canonicalize(&requested).map_err(|e| {
            format!(
                "missing/unreadable local asset {}: {e}",
                requested.display()
            )
        })?;
        if is_unc_path(&actual) {
            return Err("blocked network/UNC image asset".into());
        }
        if !actual.starts_with(self.parent) {
            return Err("blocked image outside document directory".into());
        }
        let key = actual
            .strip_prefix(self.parent)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if self.images.contains_key(&key) {
            return Ok((key, None));
        }
        let file = fs::File::open(&actual).map_err(|e| e.to_string())?;
        let meta = file.metadata().map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.len() > MAX_IMAGE_BYTES {
            return Err("image exceeds encoded size limit or is not a file".into());
        }
        let mut encoded = Vec::with_capacity(meta.len() as usize);
        file.take(MAX_IMAGE_BYTES + 1)
            .read_to_end(&mut encoded)
            .map_err(|e| format!("cannot read image: {e}"))?;
        if encoded.len() as u64 > MAX_IMAGE_BYTES {
            return Err("image exceeds encoded size limit".into());
        }
        let identify = || {
            image::ImageReader::new(std::io::Cursor::new(encoded.as_slice()))
                .with_guessed_format()
                .map_err(|e| format!("cannot identify image: {e}"))
        };
        let (width, height) = identify()?
            .into_dimensions()
            .map_err(|e| format!("corrupt or unsupported image: {e}"))?;
        if width == 0
            || height == 0
            || width > 10_000
            || height > 10_000
            || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS
        {
            return Err("image exceeds pixel/dimension limit".into());
        }
        let length = (width as usize)
            .checked_mul(height as usize)
            .and_then(|size| size.checked_mul(4))
            .ok_or("image dimensions overflow")?;
        if self
            .total_rgba_bytes
            .checked_add(length)
            .is_none_or(|n| n > MAX_TOTAL_RGBA_BYTES)
        {
            return Err("document image allocation limit exceeded".into());
        }
        let mut reader = identify()?;
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(MAX_TOTAL_RGBA_BYTES as u64);
        limits.max_image_width = Some(10_000);
        limits.max_image_height = Some(10_000);
        reader.limits(limits);
        let image = reader
            .decode()
            .map_err(|e| format!("corrupt or unsupported image: {e}"))?;
        let rgba = image.to_rgba8().into_raw();
        self.total_rgba_bytes += rgba.len();
        Ok((
            key,
            Some(ImageAsset {
                width,
                height,
                rgba,
            }),
        ))
    }
}

fn detect_direction(text: &str) -> BaseDirection {
    for c in text.chars() {
        if matches!(c as u32, 0x0590..=0x08ff | 0xfb1d..=0xfdff | 0xfe70..=0xfeff) {
            return BaseDirection::Rtl;
        }
        if c.is_alphabetic() {
            return BaseDirection::Ltr;
        }
    }
    BaseDirection::Ltr
}

#[cfg(test)]
mod tests {
    use super::load_html;
    use crate::{BaseDirection, InlineStyle, Item};
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "reader-html-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn html(&self, source: &str) -> PathBuf {
            let path = self.0.join("book.html");
            fs::write(&path, source).unwrap();
            path
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn html5_tree_entities_inline_styles_and_hidden_content() {
        let dir = TempDir::new();
        let path = dir.html("<!doctype html><head><title>Example &amp; notes</title><style>bad</style></head>\
            <body><h2>Section &lt;One&gt;</h2><p dir='rtl'>Hello <strong>bold <em>both</em></strong> after&nbsp;end<br>line</p>\
            <div>outer <p>nested</p> tail</div><script>hidden</script><aside hidden>hidden</aside></body>");
        let doc = load_html(&path).unwrap();
        assert_eq!(doc.path, fs::canonicalize(path).unwrap());
        assert_eq!(doc.title, "Example & notes");
        assert!(
            matches!(&doc.items[0], Item::Heading { text, level: 2, .. } if text == "Section <One>")
        );
        let Item::Paragraph {
            text,
            base_direction,
            style_runs,
            ..
        } = &doc.items[1]
        else {
            panic!("body paragraph")
        };
        assert_eq!(text, "Hello bold both after\u{a0}end\nline");
        assert_eq!(*base_direction, BaseDirection::Rtl);
        assert_eq!(style_runs.len(), 2);
        assert_eq!(
            (
                &text[style_runs[0].start_byte..style_runs[0].end_byte],
                style_runs[0].style
            ),
            ("bold ", InlineStyle::Bold)
        );
        assert_eq!(
            (
                &text[style_runs[1].start_byte..style_runs[1].end_byte],
                style_runs[1].style
            ),
            ("both", InlineStyle::BoldItalic)
        );
        let all = doc.items.iter().filter_map(Item::text).collect::<Vec<_>>();
        assert_eq!(
            all,
            ["Section <One>", text.as_str(), "outer", "nested", "tail"]
        );
    }

    #[test]
    fn local_image_pixels_percent_escapes_and_missing_assets() {
        let dir = TempDir::new();
        let pixel = image::RgbaImage::from_pixel(2, 1, image::Rgba([1, 2, 3, 255]));
        pixel.save(dir.0.join("cover art.png")).unwrap();
        fs::write(dir.0.join("broken.png"), b"not a png").unwrap();
        let path = dir.html("<html><body><img src='cover%20art.png'><img src='broken.png' alt='Broken cover'>\
            <img src='missing.png' alt='Missing cover'><img src='https://example.test/image.png' alt='Remote cover'>\
            <img src='../outside.png' alt='Blocked cover'></body></html>");
        let doc = load_html(&path).unwrap();
        assert!(
            matches!(&doc.items[0], Item::Image { asset_path, .. } if asset_path == "cover art.png")
        );
        let asset = &doc.images["cover art.png"];
        assert_eq!((asset.width, asset.height), (2, 1));
        assert_eq!(asset.rgba, [1, 2, 3, 255, 1, 2, 3, 255]);
        assert_eq!(
            doc.items.iter().filter_map(Item::text).collect::<Vec<_>>(),
            [
                "Broken cover",
                "Missing cover",
                "Remote cover",
                "Blocked cover"
            ]
        );
        assert_eq!(doc.warnings.len(), 4);
        assert!(doc.warnings[0].contains("corrupt or unsupported"));
        assert!(doc.warnings[1].contains("missing/unreadable"));
        assert!(doc.warnings[2].contains("blocked non-local"));
        assert!(doc.warnings[3].contains("blocked image path"));
    }

    #[test]
    fn input_encoding_empty_body_and_preformatted_lists_tables() {
        let dir = TempDir::new();
        for source in [
            "",
            "<html><script>only hidden</script></html>",
            "<meta charset=windows-1252><p>text</p>",
        ] {
            assert!(load_html(&dir.html(source)).is_err(), "{source}");
        }
        fs::write(dir.0.join("book.html"), [0xff, 0xfe, 0x00, 0x00]).unwrap();
        assert!(load_html(&dir.0.join("book.html")).is_err());
        let doc = load_html(&dir.html(
            "<pre>  x\n  y</pre><ul><li>first</li><li>second</li></ul>\
            <table><tr><td>A</td><td>B</td></tr></table>",
        ))
        .unwrap();
        let text = doc.items.iter().filter_map(Item::text).collect::<Vec<_>>();
        assert_eq!(text, ["  x\n  y", "• first", "• second", "A | B |"]);
        let repaired = load_html(&dir.html("<p>First &amp; second<p>Third</p>")).unwrap();
        assert_eq!(
            repaired
                .items
                .iter()
                .filter_map(Item::text)
                .collect::<Vec<_>>(),
            ["First & second", "Third"]
        );
    }

    #[test]
    fn ordered_lists_visible_navigation_and_css_hidden_declarations() {
        let dir = TempDir::new();
        let doc = load_html(&dir.html(
            "<nav><a href='#chap'>Contents</a></nav><form><label>Author</label></form>\
             <ol start='4'><li>four</li><li value='8'>eight</li><li>nine</li></ol>\
             <ol reversed><li>two</li><li>one</li></ol>\
             <p style='background:url(display:none)'>Visible</p>\
             <p style=' DISPLAY : none ! important '>Hidden</p>\
             <p style='visibility : hidden'>Also hidden</p>",
        ))
        .unwrap();
        assert_eq!(
            doc.items.iter().filter_map(Item::text).collect::<Vec<_>>(),
            [
                "Contents", "Author", "4. four", "8. eight", "9. nine", "2. two", "1. one",
                "Visible"
            ]
        );
    }
}
