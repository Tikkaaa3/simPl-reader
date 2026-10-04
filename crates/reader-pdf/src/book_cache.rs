//! Disposable, versioned reconstruction cache keyed by the source file's SHA-256.
use crate::book::{BlockLayout, Conversion, VERSION};
use std::{fs, io::Read, path::PathBuf};

const MAX_CACHE: u64 = 256 * 1024 * 1024;

fn path(key: &str) -> Option<PathBuf> {
    if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(
        reader_profile::cache_base()?
            .join("simPl")
            .join("pdf-books")
            .join(format!("v{VERSION}-{key}.json")),
    )
}

pub(crate) fn load(key: &str, pages: usize) -> Option<Conversion> {
    let file = fs::File::open(path(key)?).ok()?;
    if file.metadata().ok()?.len() > MAX_CACHE {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_CACHE + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_CACHE {
        return None;
    }
    let conversion: Conversion = serde_json::from_slice(&bytes).ok()?;
    valid(&conversion, pages).then_some(conversion)
}

fn valid(book: &Conversion, pages: usize) -> bool {
    if pages == 0 || pages > crate::book::MAX_PAGES || book.blocks.len() > 1_000_000 {
        return false;
    }
    let mut represented = vec![false; pages];
    for block in &book.blocks {
        if block.sources.is_empty()
            || !block.size_ratio.is_finite()
            || block.size_ratio <= 0.0
            || !block.top_gap.is_finite()
            || !(0.0..=1.0).contains(&block.top_gap)
            || !match block.layout {
                BlockLayout::Flow | BlockLayout::Centered | BlockLayout::Right => true,
                BlockLayout::List { indent } | BlockLayout::Inset { indent } => {
                    indent.is_finite() && (0.0..=1.0).contains(&indent)
                }
                BlockLayout::Toc {
                    number_start,
                    indent,
                } => {
                    number_start > 0
                        && number_start < block.text.len()
                        && block.text.is_char_boundary(number_start)
                        && indent.is_finite()
                        && (0.0..=1.0).contains(&indent)
                }
            }
            || !block.styles.iter().all(|style| {
                style.start < style.end
                    && style.end <= block.text.len()
                    && block.text.is_char_boundary(style.start)
                    && block.text.is_char_boundary(style.end)
            })
            || !block.links.iter().all(|link| {
                link.start < link.end
                    && link.end <= block.text.len()
                    && block.text.is_char_boundary(link.start)
                    && block.text.is_char_boundary(link.end)
                    && link
                        .href
                        .strip_prefix("pdf-page:")
                        .and_then(|value| value.parse::<usize>().ok())
                        .is_some_and(|page| page < pages)
            })
        {
            return false;
        }
        for source in &block.sources {
            let Some(page) = represented.get_mut(source.page as usize) else {
                return false;
            };
            if source.start > source.end || !source.top.is_finite() || !source.bottom.is_finite() {
                return false;
            }
            *page = true;
        }
    }
    let ids: std::collections::HashSet<&str> = book.blocks.iter().map(|b| b.id.as_str()).collect();
    represented.iter().all(|p| *p)
        && book
            .fallback_pages
            .iter()
            .all(|page| (*page as usize) < pages)
        && book
            .page_labels
            .iter()
            .all(|(page, label)| (*page as usize) < pages && label.len() <= 256)
        && book.illustrations.iter().all(|(id, rect)| {
            ids.contains(id.as_str())
                && rect.left >= 0.0
                && rect.top >= 0.0
                && rect.right <= 1.0
                && rect.bottom <= 1.0
                && rect.left < rect.right
                && rect.top < rect.bottom
        })
        && book.placements.iter().all(|(id, placement)| {
            ids.contains(id.as_str())
                && placement.offset.is_finite()
                && placement.width.is_finite()
                && (0.0..=1.0).contains(&placement.offset)
                && (0.0..=1.0).contains(&placement.width)
                && placement.offset + placement.width <= 1.001
        })
}

pub(crate) fn save(key: &str, book: &Conversion) {
    let Some(path) = path(key) else {
        return;
    };
    let Ok(bytes) = serde_json::to_vec(book) else {
        return;
    };
    if bytes.len() as u64 > MAX_CACHE {
        return;
    }
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    if fs::write(&temporary, bytes).is_ok() {
        let _ = fs::rename(&temporary, &path);
    }
    let _ = fs::remove_file(temporary);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_preserves_blank_source_pages_and_rejects_missing_pages() {
        let mut builder = crate::book::Builder::default();
        builder
            .push(0, crate::TextLayer::new(String::new(), vec![]))
            .unwrap();
        let book = builder.finish(|| false).unwrap();
        let restored: Conversion =
            serde_json::from_slice(&serde_json::to_vec(&book).unwrap()).unwrap();
        assert!(valid(&restored, 1));
        assert!(!valid(&restored, 2));
        assert!(restored.blocks[0].text.is_empty());
        assert!(path("../invalid").is_none());
    }
}
