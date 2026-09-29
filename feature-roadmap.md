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

**Goal.** Shelves such as "To read", "Finished", "Study"; assign from a small
menu, filter and sort by them.

**Research.**
- `library.rs` entry format and its entry limit; add tags without breaking
  existing library files.
- Shelf UI, sorting and filter controls in `shelf.rs`.
- Interaction with favourites (possibly a built-in collection).

**Risk.** Library file migration and the current bounded-size validation.

## 7. Dictionary and translation lookup

**Goal.** Double-click a word to see a definition in a small popup, offline.

**Research.**
- Offline dictionary source and its license; format and size (user-supplied
  dictionary files may be safer than bundling).
- Word boundaries with the existing Unicode and RTL handling in selection.
- Popup placement in the Iced UI.

**Risk.** Dictionary licensing and install size.

## 8. Text to speech

**Goal.** Read the selection, page or chapter aloud with the built-in Windows
speech engine, with play, pause and stop.

**Research.**
- Windows speech API access from Rust (SAPI or WinRT `SpeechSynthesizer`) and
  the crate or `windows` features it needs.
- Feeding page text in order, and following along by page turn or highlight.
- Behavior when the book or page changes mid-speech.

**Risk.** Windows-only, so the platform boundary in `platform.rs` must stay
clean; voice availability varies per machine.
