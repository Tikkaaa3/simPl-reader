//! A persistent, canonical page atlas. Chapters keep only their own live widgets/images.
use super::*;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

pub const PAPER: f32 = 720.0;
pub const MARGIN: f32 = 48.0;
pub const TEXT: f32 = PAPER - MARGIN * 2.0;
const VERSION: u32 = 1;
const MAX_CACHE: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Section {
    pub heights: Vec<f32>,
    pub pages: Vec<book_pages::Page>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Atlas {
    version: u32,
    pub fingerprint: String,
    pub sections: Vec<Section>,
    pub total: usize,
    pub source_pages: bool,
    #[serde(default)]
    pdf_book_version: u32,
    #[serde(skip)]
    pub supplements: HashMap<usize, Section>,
}
impl Atlas {
    pub fn section(&self, index: usize) -> Option<&Section> {
        self.sections
            .get(index)
            .or_else(|| self.supplements.get(&index))
    }
    pub fn target(&self, value: &str) -> Option<(usize, usize)> {
        let value = value.trim();
        let by_label = self.sections.iter().enumerate().find_map(|(s, section)| {
            section
                .pages
                .iter()
                .position(|p| p.label.eq_ignore_ascii_case(value))
                .map(|p| (s, p))
        });
        by_label.or_else(|| {
            value
                .parse::<usize>()
                .ok()
                .filter(|n| *n > 0 && *n <= self.total)
                .and_then(|n| {
                    self.sections.iter().enumerate().find_map(|(s, section)| {
                        section
                            .pages
                            .iter()
                            .position(|p| p.number as usize + 1 == n)
                            .map(|p| (s, p))
                    })
                })
        })
    }
    fn valid(&self, fingerprint: &str, count: usize) -> bool {
        self.version == VERSION
            && self.fingerprint == fingerprint
            && self.sections.len() == count
            && self.total > 0
            && self.total <= 100_000
            && self.sections.iter().all(|s| {
                !s.heights.is_empty()
                    && s.heights
                        .iter()
                        .all(|h| h.is_finite() && *h > 0.0 && *h < 100_000_000.0)
                    && !s.pages.is_empty()
                    && s.pages.iter().all(|p| {
                        p.number < self.total as u32
                            && p.label.len() <= 1024
                            && p.rows.start <= p.rows.end
                            && p.rows.end <= s.heights.len()
                            && p.content.start.is_finite()
                            && p.content.end.is_finite()
                            && p.content.start >= 0.0
                            && p.content.end >= p.content.start
                            && p.top.is_finite()
                            && p.top >= 0.0
                            && p.height.is_finite()
                            && p.height > 0.0
                    })
            })
    }
}
fn cache_path(fingerprint: &str) -> PathBuf {
    reader_document::managed::root()
        .parent()
        .unwrap()
        .join("page-maps")
        .join(format!("v{VERSION}-{fingerprint}.json"))
}
fn storage_enabled() -> bool {
    // Tests only persist when the visual-QA harness explicitly names an isolated store.
    !cfg!(test)
        || std::env::var_os("SIMPL_PREVIEW_STORE")
            .is_some_and(|store| Some(store) == std::env::var_os("LOCALAPPDATA"))
}
pub(super) fn cached(book: &Book, count: usize) -> Option<Atlas> {
    if !storage_enabled()
        || book.fingerprint.len() != 64
        || !book.fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return None;
    }
    let path = cache_path(&book.fingerprint);
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_CACHE {
        return None;
    }
    let mut data = Vec::new();
    file.take(MAX_CACHE + 1).read_to_end(&mut data).ok()?;
    if data.len() as u64 > MAX_CACHE {
        return None;
    }
    let mut atlas: Atlas = serde_json::from_slice(&data).ok()?;
    // Display-only migration: retain every saved boundary and global page identity.
    for section in &mut atlas.sections {
        for page in &mut section.pages {
            page.label = reader_document::display_page_label(&page.label);
        }
    }
    (atlas.valid(&book.fingerprint, count)
        && atlas.pdf_book_version
            == book
                .pdf_source
                .as_ref()
                .map_or(0, |_| reader_pdf::book::VERSION))
    .then_some(atlas)
}
fn persist(atlas: &Atlas) -> Result<(), String> {
    // Synthetic unit-test documents have no content fingerprint and never touch user storage.
    if !storage_enabled()
        || atlas.fingerprint.len() != 64
        || !atlas.fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Ok(());
    }
    let path = cache_path(&atlas.fingerprint);
    let bytes = serde_json::to_vec(atlas).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_CACHE {
        return Err("Page map exceeds the storage limit".into());
    }
    std::fs::create_dir_all(path.parent().unwrap())
        .map_err(|e| format!("Cannot save fixed pages: {e}"))?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp = path.with_extension(format!("{}-{nonce}.tmp", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        if path.exists() {
            // A valid map would already have been loaded; retain damaged cache bytes for diagnosis.
            let old = path.with_extension(format!("{nonce}.invalid"));
            std::fs::rename(&path, old)?;
        }
        std::fs::rename(&temp, &path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(|e| format!("Cannot save fixed pages: {e}"))
}

pub fn build(book: Arc<Book>, cancel: &AtomicBool) -> Result<Option<Atlas>, String> {
    let Some(mut atlas) = build_spine(book.clone(), cancel)? else {
        return Ok(None);
    };
    if let Some(chapter) = &book.epub
        && chapter.index >= chapter.document.chapters.len()
    {
        let Some(heights) = measure_book(book.clone(), TEXT, DEFAULT_FONT_SIZE, cancel) else {
            return Ok(None);
        };
        let index = virtual_reader::HeightIndex::new(heights.clone());
        let lines = book
            .items
            .iter()
            .map(|item| {
                item.text().map(|_| {
                    let semantics = book.structure.get(item.id());
                    book_pages::Lines {
                        height: MINIMAL.block_size(item, semantics, DEFAULT_FONT_SIZE)
                            * MINIMAL.line_height,
                        top: MINIMAL
                            .block_padding(item, semantics, DEFAULT_FONT_SIZE, TEXT)
                            .top,
                    }
                })
            })
            .collect::<Vec<_>>();
        let mut pages = book_pages::reflow(&index, PAPER, &[], &lines);
        for page in &mut pages {
            page.label = "Note".into();
        }
        atlas
            .supplements
            .insert(chapter.index, Section { heights, pages });
    }
    Ok(Some(atlas))
}

fn build_spine(book: Arc<Book>, cancel: &AtomicBool) -> Result<Option<Atlas>, String> {
    let count = book.epub.as_ref().map_or(1, |c| c.document.chapters.len());
    if let Some(atlas) = cached(&book, count) {
        return Ok(Some(atlas));
    }
    let mut atlas = Atlas {
        version: VERSION,
        fingerprint: book.fingerprint.clone(),
        sections: Vec::new(),
        total: 0,
        source_pages: book.pdf_source.is_some(),
        pdf_book_version: book
            .pdf_source
            .as_ref()
            .map_or(0, |_| reader_pdf::book::VERSION),
        supplements: HashMap::new(),
    };
    let mut sources: Vec<(usize, usize, String)> = Vec::new();
    for section in 0..count {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let current = if let Some(epub) = &book.epub {
            if section == epub.index {
                book.clone()
            } else {
                Arc::new(load_epub_chapter(epub.document.clone(), section, None)?)
            }
        } else {
            book.clone()
        };
        let Some(heights) = measure_book(current.clone(), TEXT, DEFAULT_FONT_SIZE, cancel) else {
            return Ok(None);
        };
        let geometry = virtual_reader::HeightIndex::new(heights.clone());
        let headings = if current.epub.is_none() {
            current
                .items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| {
                    matches!(item, Item::Heading { level: 1 | 2, .. }).then_some(i)
                })
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        let lines = current
            .items
            .iter()
            .map(|item| {
                item.text().map(|_| {
                    let semantics = current.structure.get(item.id());
                    book_pages::Lines {
                        height: MINIMAL.block_size(item, semantics, DEFAULT_FONT_SIZE)
                            * MINIMAL.line_height,
                        top: MINIMAL
                            .block_padding(item, semantics, DEFAULT_FONT_SIZE, TEXT)
                            .top,
                    }
                })
            })
            .collect::<Vec<_>>();
        let mut pages = if let Some(source) = &current.pdf_source {
            book_pages::layout(
                source.conversion.blocks.iter().map(|b| b.sources[0].page),
                &geometry,
                PAPER,
            )
        } else {
            book_pages::reflow(&geometry, PAPER, &headings, &lines)
        };
        for page in &mut pages {
            page.number += atlas.total as u32;
            page.label = (page.number + 1).to_string();
        }
        atlas.total += pages.len();
        if atlas.total > 100_000 {
            return Err("Book exceeds 100,000 pages".into());
        }
        let targets = if let Some(epub) = &book.epub
            && !epub.document.page_list.is_empty()
        {
            epub.document
                .page_list
                .iter()
                .filter(|p| p.chapter == section)
                .map(|p| (p.label.clone(), p.fragment.clone()))
                .collect::<Vec<_>>()
        } else {
            current
                .page_breaks
                .iter()
                .map(|(label, target)| (label.clone(), Some(target.clone())))
                .collect()
        };
        for (label, fragment) in targets {
            let row = match fragment {
                None => 0,
                Some(id) => current
                    .anchors
                    .get(&id)
                    .and_then(|id| current.items.iter().position(|i| i.id() == id))
                    .ok_or_else(|| format!("Source page target is missing: {label}"))?,
            };
            if sources
                .last()
                .is_some_and(|(s, r, _)| (*s, *r) > (section, row))
            {
                return Err("Source page list is out of reading order".into());
            }
            sources.push((section, row, label));
        }
        atlas.sections.push(Section { heights, pages });
    }
    if !sources.is_empty() {
        atlas.source_pages = true;
        atlas.total = sources.len();
        let mut inherited = 0;
        for (section, layout) in atlas.sections.iter_mut().enumerate() {
            let index = virtual_reader::HeightIndex::new(layout.heights.clone());
            let mut cuts = vec![(0.0, inherited)];
            let mut first_target = true;
            for (number, (_, row, _)) in sources.iter().enumerate().filter(|(_, p)| p.0 == section)
            {
                let offset = index.start(*row);
                if first_target && offset == 0.0 {
                    cuts[0] = (offset, number);
                } else {
                    cuts.push((offset, number));
                }
                inherited = number;
                first_target = false;
            }
            let mut top = book_pages::GAP;
            layout.pages = cuts
                .iter()
                .enumerate()
                .map(|(i, (start, number))| {
                    let end = cuts.get(i + 1).map_or(index.total(), |c| c.0);
                    let height =
                        (end - start + book_pages::TOP + book_pages::BOTTOM).max(PAPER * 1.414);
                    let page = book_pages::Page {
                        number: *number as u32,
                        label: reader_document::display_page_label(&sources[*number].2),
                        rows: index.window(*start, end - start, 0.0),
                        content: *start..end,
                        top,
                        height,
                    };
                    top += height + book_pages::GAP;
                    page
                })
                .collect();
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return Ok(None);
    }
    persist(&atlas)?;
    Ok(Some(atlas))
}
