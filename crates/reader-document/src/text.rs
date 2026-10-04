//! Plain-text and Markdown sources become one generated HTML page at import time, so the
//! existing HTML reader (structure, images, links, find, positions) needs no second renderer.
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};
use std::path::Path;

/// Text and Markdown share the HTML size limit; conversion may grow Markdown slightly.
pub(crate) const MAX_TEXT_BYTES: u64 = 16 * 1024 * 1024;

pub(crate) fn is_text_extension(ext: &str) -> bool {
    matches!(ext, "txt" | "text" | "md" | "markdown")
}

fn is_markdown_extension(ext: &str) -> bool {
    matches!(ext, "md" | "markdown")
}

/// Decode UTF-8 (with or without BOM), UTF-16 with BOM, or else the system ANSI code page
/// (elsewhere: the code page of [`set_legacy_text_language`]).
pub(crate) fn decode(bytes: &[u8]) -> Result<String, String> {
    if let Some(rest) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        return std::str::from_utf8(rest)
            .map(str::to_owned)
            .map_err(|_| "The text file has a UTF-8 marker but is not valid UTF-8".to_owned());
    }
    let utf16 = |rest: &[u8], big_endian: bool| -> Result<String, String> {
        if !rest.len().is_multiple_of(2) {
            return Err("The UTF-16 text file is truncated".into());
        }
        let units = rest.chunks_exact(2).map(|pair| {
            if big_endian {
                u16::from_be_bytes([pair[0], pair[1]])
            } else {
                u16::from_le_bytes([pair[0], pair[1]])
            }
        });
        char::decode_utf16(units)
            .collect::<Result<String, _>>()
            .map_err(|_| "The UTF-16 text file contains invalid characters".to_owned())
    };
    if let Some(rest) = bytes.strip_prefix(&[0xff, 0xfe]) {
        return utf16(rest, false);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xfe, 0xff]) {
        return utf16(rest, true);
    }
    if bytes.contains(&0) {
        return Err("This looks like a binary file, not text".into());
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => Ok(text.to_owned()),
        Err(_) => Ok(legacy(bytes)),
    }
}

#[cfg(windows)]
fn legacy(bytes: &[u8]) -> String {
    use windows_sys::Win32::Globalization::{CP_ACP, MultiByteToWideChar};
    let Ok(len) = i32::try_from(bytes.len()) else {
        return latin1(bytes);
    };
    // SAFETY: both calls read `len` bytes from a live slice; the second writes at most
    // `capacity` UTF-16 units into a buffer of exactly that size.
    unsafe {
        let capacity = MultiByteToWideChar(CP_ACP, 0, bytes.as_ptr(), len, std::ptr::null_mut(), 0);
        if capacity <= 0 {
            return latin1(bytes);
        }
        let mut wide = vec![0_u16; capacity as usize];
        let written =
            MultiByteToWideChar(CP_ACP, 0, bytes.as_ptr(), len, wide.as_mut_ptr(), capacity);
        if written <= 0 {
            return latin1(bytes);
        }
        String::from_utf16_lossy(&wide[..written as usize])
    }
}

/// Without a system ANSI code page, decode with the Windows code page of the
/// reader's language, so a file written on that locale's desktop reads the same.
#[cfg(not(windows))]
fn legacy(bytes: &[u8]) -> String {
    let encoding = LEGACY
        .read()
        .ok()
        .and_then(|encoding| *encoding)
        .unwrap_or(encoding_rs::WINDOWS_1252);
    encoding.decode_without_bom_handling(bytes).0.into_owned()
}

#[cfg(not(windows))]
static LEGACY: std::sync::RwLock<Option<&'static encoding_rs::Encoding>> =
    std::sync::RwLock::new(None);

/// Choose the legacy text code page from a BCP 47 language tag (for example the
/// app locale, `tr-TR`). Windows decodes with its own ANSI code page, so this has
/// no effect there.
pub fn set_legacy_text_language(tag: &str) {
    #[cfg(not(windows))]
    if let Ok(mut legacy) = LEGACY.write() {
        *legacy = Some(legacy_encoding(tag));
    }
    #[cfg(windows)]
    let _ = tag;
}

/// The Windows ANSI code page that desktop Windows uses for this language.
#[cfg(not(windows))]
fn legacy_encoding(tag: &str) -> &'static encoding_rs::Encoding {
    use encoding_rs::*;
    let tag = tag.to_ascii_lowercase().replace('_', "-");
    let mut parts = tag.split('-');
    let language = parts.next().unwrap_or_default();
    let subtags: Vec<&str> = parts.collect();
    let has = |subtag: &str| subtags.contains(&subtag);
    match language {
        "tr" | "az" => WINDOWS_1254,
        "el" => WINDOWS_1253,
        "sr" if has("latn") => WINDOWS_1250,
        "ru" | "uk" | "be" | "bg" | "sr" | "mk" | "kk" | "ky" | "tt" | "mn" => WINDOWS_1251,
        "pl" | "cs" | "sk" | "hu" | "ro" | "hr" | "sl" | "bs" | "sq" => WINDOWS_1250,
        "et" | "lv" | "lt" => WINDOWS_1257,
        "he" | "iw" => WINDOWS_1255,
        "ar" | "fa" | "ur" => WINDOWS_1256,
        "vi" => WINDOWS_1258,
        "th" => WINDOWS_874,
        "ja" => SHIFT_JIS,
        "ko" => EUC_KR,
        "zh" if has("hant") || has("tw") || has("hk") || has("mo") => BIG5,
        "zh" => GBK,
        _ => WINDOWS_1252,
    }
}

#[cfg(windows)]
fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&byte| char::from(byte)).collect()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The reader rejects control bytes other than tab and line breaks; a form feed or an escape
/// sequence in an old text file should not make the whole book unreadable.
fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || matches!(c, '\t' | '\n' | '\r'))
        .collect()
}

fn document(title: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>{}</title></head>\n<body>\n{body}</body></html>\n",
        escape(title)
    )
}

/// Blank lines separate paragraphs and single line breaks are only wrapping. A file without any
/// blank line is treated as one paragraph per line instead of one wall of text.
fn plain_body(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let text = text.trim();
    let mut body = String::new();
    let mut paragraph = |lines: &[&str]| {
        if !lines.is_empty() {
            body.push_str("<p>");
            body.push_str(&escape(&lines.join(" ")));
            body.push_str("</p>\n");
        }
    };
    if text.lines().any(|line| line.trim().is_empty()) {
        let mut lines = Vec::new();
        for line in text.lines() {
            if line.trim().is_empty() {
                paragraph(&lines);
                lines.clear();
            } else {
                lines.push(line.trim());
            }
        }
        paragraph(&lines);
    } else {
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            paragraph(&[line.trim()]);
        }
    }
    body
}

fn markdown_body(text: &str) -> (String, Option<String>) {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES;
    let mut first_heading = None;
    let mut in_heading = false;
    let mut heading_text = String::new();
    let events = Parser::new_ext(text, options).inspect(|event| match event {
        Event::Start(Tag::Heading { .. }) if first_heading.is_none() => {
            in_heading = true;
            heading_text.clear();
        }
        Event::Text(text) | Event::Code(text) if in_heading => heading_text.push_str(text),
        Event::End(TagEnd::Heading(_)) if in_heading => {
            in_heading = false;
            first_heading = Some(heading_text.trim().to_owned());
        }
        _ => {}
    });
    let mut body = String::new();
    html::push_html(&mut body, events);
    (body, first_heading.filter(|title| !title.is_empty()))
}

/// The generated page for `source` (a `.txt`, `.text`, `.md` or `.markdown` file's bytes).
pub(crate) fn to_html(source: &Path, ext: &str, bytes: &[u8]) -> Result<String, String> {
    let text = clean(&decode(bytes)?);
    if text.trim().is_empty() {
        return Err("The text file is empty".into());
    }
    let stem = source
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".into());
    if is_markdown_extension(ext) {
        let (body, heading) = markdown_body(&text);
        Ok(document(&heading.unwrap_or(stem), &body))
    } else {
        Ok(document(&stem, &plain_body(&text)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_utf8_utf16_and_legacy_bytes() {
        assert_eq!(decode("Şiir ğ".as_bytes()).unwrap(), "Şiir ğ");
        assert_eq!(decode(b"\xef\xbb\xbfabc").unwrap(), "abc");
        assert_eq!(decode(b"\xff\xfeh\0i\0").unwrap(), "hi");
        assert_eq!(decode(b"\xfe\xff\0h\0i").unwrap(), "hi");
        assert!(decode(b"\xff\xfeh\0i").is_err());
        assert!(decode(b"text\0text").is_err());
        // Not valid UTF-8: decoded with a legacy code page instead of failing.
        let legacy = decode(b"caf\xe9").unwrap();
        assert!(legacy.starts_with("caf") && legacy.chars().count() == 4);
    }

    #[cfg(not(windows))]
    #[test]
    fn legacy_text_follows_the_language_code_page() {
        use encoding_rs::*;
        assert_eq!(legacy_encoding("tr-TR"), WINDOWS_1254);
        assert_eq!(legacy_encoding("sr_Latn_RS"), WINDOWS_1250);
        assert_eq!(legacy_encoding("sr-RS"), WINDOWS_1251);
        assert_eq!(legacy_encoding("zh-Hant-HK"), BIG5);
        assert_eq!(legacy_encoding("zh-CN"), GBK);
        assert_eq!(legacy_encoding("en-US"), WINDOWS_1252);
        assert_eq!(legacy_encoding(""), WINDOWS_1252);
        // The process-wide choice is exercised in one test to avoid races.
        set_legacy_text_language("tr-TR");
        assert_eq!(decode(b"\xd0\xfeiir").unwrap(), "Ğşiir");
        set_legacy_text_language("en-US");
        assert_eq!(decode(b"caf\xe9").unwrap(), "café");
    }

    #[test]
    fn plain_text_joins_wrapped_lines_and_escapes_markup() {
        let html = plain_body("First line\nwraps here.\r\n\r\nSecond <b>paragraph</b> & more.\n");
        assert_eq!(
            html,
            "<p>First line wraps here.</p>\n<p>Second &lt;b&gt;paragraph&lt;/b&gt; &amp; more.</p>\n"
        );
    }

    #[test]
    fn plain_text_without_blank_lines_keeps_one_paragraph_per_line() {
        assert_eq!(
            plain_body("one\ntwo\nthree"),
            "<p>one</p>\n<p>two</p>\n<p>three</p>\n"
        );
        assert_eq!(plain_body("only line"), "<p>only line</p>\n");
    }

    #[test]
    fn control_characters_are_removed_but_line_breaks_stay() {
        assert_eq!(clean("a\x0cb\x1b[0m\tc\nd"), "ab[0m\tc\nd");
    }

    #[test]
    fn markdown_uses_the_first_heading_as_title_and_renders_structure() {
        let page = to_html(
            Path::new("C:/notes/readme.md"),
            "md",
            b"# My *Notes*\n\ntext with `code`\n\n- a\n- b\n\n| x | y |\n|---|---|\n| 1 | 2 |\n",
        )
        .unwrap();
        assert!(page.contains("<title>My Notes</title>"));
        assert!(page.contains("<h1>My <em>Notes</em></h1>"));
        assert!(page.contains("<li>a</li>"));
        assert!(page.contains("<table>"));
        let untitled = to_html(Path::new("plain.markdown"), "markdown", b"just text").unwrap();
        assert!(untitled.contains("<title>plain</title>"));
    }

    #[test]
    fn text_title_is_the_escaped_file_name_and_empty_files_are_rejected() {
        let page = to_html(Path::new("A & B.txt"), "txt", b"hello").unwrap();
        assert!(page.contains("<title>A &amp; B</title>"));
        assert!(to_html(Path::new("empty.txt"), "txt", b" \n\n").is_err());
    }

    #[test]
    fn generated_pages_load_as_reflow_documents() {
        let dir = std::env::temp_dir().join(format!("simpl-text-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("book.html");
        let markdown = "# Title\n\nParagraph one.\n\n## Section\n\nParagraph two.\n";
        std::fs::write(
            &path,
            to_html(Path::new("n.md"), "md", markdown.as_bytes()).unwrap(),
        )
        .unwrap();
        let document = crate::load_html(&path).unwrap();
        assert_eq!(document.title, "Title");
        assert!(document.items.len() >= 4);
        std::fs::write(
            &path,
            to_html(Path::new("n.txt"), "txt", b"A\n\nB").unwrap(),
        )
        .unwrap();
        assert_eq!(crate::load_html(&path).unwrap().items.len(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
