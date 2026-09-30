# simPl feature roadmap

> Short, ordered list of next features. Each item states the goal, what to
> research first, and the main risk. It is deliberately not a design: detailed
> plans are written when an item starts. Completed history lives in
> [roadmap.md](roadmap.md) and [book-parser-roadmap.md](book-parser-roadmap.md).

Order is by value against effort and by dependency: cheap UI consistency first,
features that reuse existing pipelines next, features that need new storage or
new dependencies last.

## 1. Document view toolbar matches Book view

**Status: done.** Library and Book on the left, page controls in the centre, zoom and find on the right; fit width moved to Ctrl+Shift+F (toggles back).

**Goal.** The PDF Document toolbar should look like the Book toolbar: page size
and zoom controls grouped at the right, page counter centered, Library and mode
switch at the left.

**Research.**
- Book layout is `document_toolbar` in `crates/iced-shell/src/app.rs`
  (left / centered navigation / right zoom, using `FillPortion(1)` columns).
- PDF controls come from `pdf_reader::Reader::toolbar`; check which controls
  exist (page input, zoom, fit width) and reuse the same widgets and styles.
- The narrow layout (`window_size.width < 1050`) stacks two rows; decide whether
  the new layout still needs it.

**Risk.** Keyboard focus order and the existing toolbar tests.

## 2. In-book search (Ctrl+F)

**Status: done** for HTML, EPUB and PDF (Book and Document views). Ctrl+F toggles the find bar; matches scroll to the exact line.

**Goal.** Search inside the open book, step through matches, highlight them on
the page.

**Research.**
- Where searchable text lives: reflow `Item` text for HTML/EPUB (EPUB loads one
  chapter at a time) and the PDFium text layer for PDF.
- Reuse selection rendering for highlights; check `selection.rs` and
  `book_map.rs`.
- Match navigation must resolve to a global page in the stable page map.

**Risk.** EPUB chapters that are not loaded yet; large PDFs (search off the UI
thread, cancellable).

## 3. Reading appearance settings

**Status: partial.** Default, Soft, Clear and Compact themes are bundled, with
global persisted selection, fonts, spacing and light/dark colors. Per-book
preferences, independent spacing/margin controls and two-column pages remain open.

**Goal.** More themes (sepia), line spacing, margins, font choice (Literata is
already bundled), optional two-column pages. Remembered per book.

**Research.**
- Existing settings: `preferences.rs` and `book_style.rs`.
- Which options change pagination (spacing, margins, columns) and therefore
  must rebuild the global page map, versus which only restyle.
- Per-book storage next to the reading position in `position.rs`.

**Risk.** Zoom currently avoids repagination; new options must not break that
guarantee or the page-number stability.

## 4. TXT and Markdown support

**Status: done.** `.txt`, `.text`, `.md` and `.markdown` are converted to one generated HTML page when imported (`crates/reader-document/src/text.rs`, `managed.rs`), so the HTML reader, find, page map and positions work unchanged. Markdown uses `pulldown-cmark` 0.13.4 (MIT; with `pulldown-cmark-escape` and `unicase`, all permissive, notices collected by `scripts/collect-licenses.ps1`). Encodings: UTF-8 (BOM or not), UTF-16 with a BOM, otherwise the Windows ANSI code page. Local Markdown images are copied beside the page. The shelf retains distinct TXT and Markdown labels, including repaired legacy imports. Known limit: an edited source is not re-imported (same as HTML today). The installer registers `.txt`/`.md` alongside PDF, HTML and EPUB; isolated installation checks cover all five extensions.

**Goal.** Open `.txt` and `.md` files in the same reader as HTML.

**Research.**
- Convert to the existing HTML/reflow `Item` model rather than adding a new
  renderer; a Markdown crate (for example `pulldown-cmark`) is the likely
  dependency. Check its license against the shipped notices.
- Text encoding detection beyond UTF-8.
- Extension lists in `managed.rs`, `document_kind` in `app.rs`, and the
  installer file associations (`installer/simPl.iss`).

**Risk.** Dependency size and license inventory updates.

## 5. Bookmarks, highlights and notes

**Status: done.** A selection opens a small highlight and note menu; right-click
also offers page bookmarks and removal of an existing highlight. The list is a
right sidebar with a persistent edge toggle. Overlapping highlights of the same
color merge without darkening, while different colors remain independent.
Ctrl+H, Ctrl+D and Ctrl+B provide keyboard access
without another toolbar control. A versioned file per book fingerprint stores
bookmarks, highlights and notes under the local simPl profile. Reflow highlights
use item IDs and UTF-8 offsets, with selected-text recovery if IDs change; PDF
highlights use page and glyph indices. Notes can be edited from a highlight or
from the list, and saved entries navigate back to their passage or page.

**Goal.** Bookmark a page, highlight selected text, attach a note. Data
follows the book's fingerprint so it survives moves and re-imports.

**Research.**
- Storage: one small file per fingerprint, mirroring `position.rs`; decide the
  format and versioning.
- Anchoring: reflow items and PDF text ranges differ. Find a stable anchor
  (item id plus offsets) that survives zoom and font changes.
- Reuse selection and the highlight rendering from item 2.

**Risk.** Anchors breaking when parsing changes; keep records versioned.

## 6. Collections and tags

**Status: shelves done; free-form tags remain open.** Create, rename and remove
custom shelves, assign books from their menu and filter the library. Shelf storage
is versioned and retains the existing favourites behavior.

**Goal.** Shelves such as "To read", "Finished", "Study"; assign from a small
menu, filter and sort by them.

**Research.**
- `library.rs` entry format and its entry limit; add tags without breaking
  existing library files.
- Shelf UI, sorting and filter controls in `shelf.rs`.
- Interaction with favourites (possibly a built-in collection).

**Risk.** Library file migration and the current bounded-size validation.

## 7. Dictionary and translation lookup

**Status: done for downloadable offline word lookup.** Double-click or select a short phrase
to open an offline card in Book and PDF Document views. Persisted settings select
automatic/manual mode and valid input/output pairs; right-click → Translate works
in manual mode. Thirteen directions cover English ↔ Turkish, Spanish, German,
French, Japanese, Chinese and Korean → English. Chinese → English adds CC-CEDICT
for both scripts. Lookups run on workers with cancellation and generation guards.
All 13 directions are optional downloads, with per-direction size/status, progress,
cancel/retry, removal and verified ZIP import in Settings. Missing-word cards can
download the needed pair and retry in place. HTTPS downloads use an immutable
data release and pinned SHA-256 checksums; installed files work offline. Data
attribution and CC BY-SA 4.0 notices ship with each package and the application.
Sentence translation, Argos plugins, arbitrary third-party dictionary formats and
full morphology remain open. See [downloadable data](assets/dictionaries/README.md).

**Goal.** Double-click a word to see a definition in a small popup, offline.

**Research.**
- Offline dictionary source and its license; format and size (user-supplied
  dictionary files may be safer than bundling).
- Word boundaries with the existing Unicode and RTL handling in selection.
- Popup placement in the Iced UI.

**Risk.** Dictionary licensing and install size.

## 8. Text to speech

**Status: done for Windows.** Offline SAPI uses installed classic and OneCore
voices. Listen (Ctrl+Shift+U) reads a selection or continues from the current page;
Listen toggles it off. Pause/resume and rate controls remain in the player. Voice
and rate are persisted. Reading follows pages and asynchronous EPUB chapter loads;
PDF selection/page replies are tied to the active session. Image-only pages are
skipped. Automatic language selection is a heuristic, with a Windows default fallback.

**Goal.** Read the selection, page or chapter aloud with the built-in Windows
speech engine, with a Listen toggle and pause/resume.

**Research.**
- Windows speech API access from Rust (SAPI or WinRT `SpeechSynthesizer`) and
  the crate or `windows` features it needs.
- Feeding page text in order, and following along by page turn or highlight.
- Behavior when the book or page changes mid-speech.

**Risk.** Windows-only, so the platform boundary in `platform.rs` must stay
clean; voice availability varies per machine.

## Release consolidation — 2026-09-29

The 0.1.1 work includes bounded, cancellable background search for large books,
shared page/contents data on frequent UI paths, and PDF raster rendering after a
text-layer error. See [stabilization-report.md](stabilization-report.md) for checks,
measurements and remaining release qualification. Next priorities are independent
Windows/accessibility QA, profile backup and annotation export. Offline word
translation and verified optional dictionary downloads are now implemented.
Per-book appearance can follow once its storage and page-map rules are specified.
The [final local reader review](reader-release-review-2026-09-30.md) compares the
current product with other readers and separates release qualification from
future features.
