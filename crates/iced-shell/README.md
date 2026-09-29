# Reader development

The product overview and build instructions live in the [root README](../../README.md).
This guide covers the native reader implementation and focused verification.

## Architecture

The active UI is Iced 0.14 with tiny-skia and softbuffer. WGPU is disabled.
`app.rs` coordinates the library, opening/saving, navigation and reader state.

| Module | Responsibility |
| --- | --- |
| `shelf`, `chrome`, `ui` | Library cards, window controls, shared palette and fonts |
| `book_style`, `book_zoom` | Minimal reading style and zoom without repagination |
| `book_pages`, `book_map` | Single-page boundaries and persistent global page maps |
| `virtual_reader`, `selection` | Visible content, height indexes and source-aware selection |
| `pdf_reader`, `pdf_page`, `document_scroll` | Original PDF rendering and navigation |
| `book_preview` | Opt-in production-widget rendering to PNG for visual QA |

`reader-document` owns HTML/EPUB structure, managed imports and saved state.
`reader-pdf` confines PDFium handles to one worker thread. Public results contain
owned data rather than native handles. Opening and conversion results are checked
against the active document/generation so stale work cannot replace a newer view.

## Pagination and persistence

- Book view mounts one physical page at a time. Vertical scrolling stays in that
  page; previous/next and the global page field change the page.
- PDF Book preserves source page counts and boundaries. Explicit view switches
  use the current page; reopening restores the saved Book position.
- Its page atlas starts from inexpensive height estimates and corrects the
  visible rows with native layout as the reader moves. Scrolling retains the
  layout state of rows that remain visible.
- EPUB/HTML use source page markers when available, otherwise a canonical first
  layout. Window resizing and zoom do not change those saved boundaries.
- The toolbar collapses to zero height. F8 and the title-bar control restore it;
  hidden controls are excluded from keyboard focus.
- Managed imports live below `%LOCALAPPDATA%\simPl\documents`. HTML book folders
  use an internal EPUB container, with source-format metadata so the shelf still
  identifies them as HTML. Removal is restricted to owned copies.
- The profile also contains `library.json`, `recent.json`, `positions`, `covers`,
  `page-maps` and `pdf-books`. Conversion and atlas caches are disposable;
  bookmarks and the managed library are user data.

PDF Book cache keys include the complete source SHA-256 and converter version.
The worker retains only its most recent conversion. Persisted JSON is bounded at
64 MiB and validated before use; invalid or obsolete data triggers reconstruction.
The first conversion still analyzes all source pages. This avoids repeated work
without weakening illustration detection. Blank source pages keep their slot
with empty text. Unsupported layout explanations are separate from blank pages.

## Checks

From the repository root:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 check -Offline
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 build -Offline
```

Omit `-Offline` for the initial setup. `check` runs formatting, workspace Clippy
with warnings denied, and workspace tests. `build` stages the pinned PDF runtime.
For direct Cargo commands, initialize an x64 MSVC Native Tools environment first.

Run process timing tests without competing builds or heavy image conversion.
Use Windows PowerShell 5.1 for the repository scripts and their child processes.
Generated screenshots, PDFs, reports and local test profiles belong in `target/`.
Do not commit personal books or user library data.

## Book appearance visual QA

The ignored tests in `book_preview.rs` paint the real widget tree to PNG using
tiny-skia. They require no OS input and are separate from desktop interaction tests.
Generate the authored EPUB with `python fixtures/book-structure/build_epub.py`, then:

```powershell
$env:SIMPL_PREVIEW_HTML = Join-Path $PWD 'fixtures\book-structure\structured.html'
$env:SIMPL_PREVIEW_EPUB = Join-Path $PWD 'target\book-milestone2\structured.epub'
$env:SIMPL_PREVIEW_OUTPUT = Join-Path $PWD 'target\book-previews'
cargo test -p iced-shell --bin iced-shell --locked --offline render_book_previews -- --ignored --nocapture
```

Optional `SIMPL_PREVIEW_ROW` and `SIMPL_PREVIEW_CHAPTER` select a zero-based
content row or EPUB spine section. These previews verify widget layout, not
native focus, DPI behavior or displayed-frame timing.

## PDF Book conversion and QA

The converter reconstructs prose and headings from the existing text layer.
It retains relative heading size, bold and italic runs, centered titles,
numbered entries, Contents indentation and right-aligned folios. Same-document
PDF links in reconstructed text jump to their source page. Printed Roman or
Arabic footers become page labels when supported by the PDF label or a sequence.
Two-column pages (dictionaries, encyclopedias) are read left column first; in
columns, extra leading between lines also separates entries. Full-page scan
analysis masks OCR glyphs and locates illustration regions. Scans are only
rasterized for this when their text leaves room for an illustration inside the
book's usual text block. Illustrations keep their width and horizontal position
relative to that text block (or their column). Only active-page illustrations are
rasterized at reading resolution. Ordinary prose remains selectable. There is
no OCR engine. Complex tables, ambiguous multi-column layouts and unreliable
text use the original page image in their source slot; text on those pages is
selectable in Document mode. Extraction restrictions are respected.

Conversion limits are 5,000 pages, 64 MiB of extracted UTF-8 text and 1,000,000 source
lines. Cancellation is checked between pages and during reconstruction. Source
anchors contain converter version, page and original byte offset; older versions
migrate through the source page. PDF atlas caches also carry converter version.

[Authored PDF fixtures](../../fixtures/pdf-book/README.md) cover prose, illustrations,
columns and extraction permissions. The `render_pdf_book_previews` ignored test
also has assertions specific to the 111-page Alice PDF documented in that fixture
README; do not treat it as a generic arbitrary-PDF test.

To measure conversion independently of UI layout, stage PDFium beside the example:

```powershell
cargo build -p reader-pdf --example book_timing --release --locked --offline
Copy-Item target\release\pdfium.dll target\release\examples\pdfium.dll
$env:LOCALAPPDATA = Join-Path $PWD 'target\pdf-timing-profile'
$env:SIMPL_PDF_TIMING = '1'
.\target\release\examples\book_timing.exe 'C:\Books\book.pdf'
```

Use a disposable shell for these environment overrides. The example compares
conversion data across first load, same-session retrieval and reopen. Stage timings
separate text extraction, object inspection, scan rasterization and region detection.
They exclude file opening, UI layout and display latency.

`pdf_book_cache_and_blank_pages` is an opt-in regression for the locally supplied
440-page Pride and Prejudice and 359-page Sherlock Holmes PDFs, in that order.
`SIMPL_PDF_PATHS` points to a JSON array of their absolute paths;
`SIMPL_PREVIEW_OUTPUT` names the PNG directory. Set `SIMPL_PREVIEW_STORE` and
`LOCALAPPDATA` to the same isolated directory, and copy `pdfium.dll` into
`target\debug\deps` before running the test. It checks identical cached content,
source page counts, blank page 3, prose pages and Sherlock's page 22 illustration.
Those books are not repository fixtures.

## Desktop and selection checks

After a release build, choose the driver appropriate to the change:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File crates\iced-shell\tests\reader-poc.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File crates\iced-shell\tests\virtual-reader.ps1 -EvidenceDirectory target\virtual-fresh
powershell -NoProfile -ExecutionPolicy Bypass -File crates\iced-shell\tests\selection-copy.ps1 -EvidenceDirectory target\selection-fresh
```

These drivers own the processes they launch and require an idle, unobscured Windows
desktop. They move focus/pointer state; selection checks overwrite the clipboard.
Use fresh output directories. Additional drivers under `tests/` cover native input,
runtime evidence, BiDi and interaction timing; inspect their parameters before use.

`--reader-poc` and `--reader-poc-large` load authored 1,000/10,000-paragraph workloads
from `fixtures/reader-workload`. They are diagnostic modes, not the normal reader.
The product does not need those fixtures. `SHELL_STARTUP_MARKERS=1` emits optional
QPC diagnostic markers; these are not presented-frame timestamps.

## Keep the RTL correction until a verified replacement exists

Root `[patch.crates-io]` selects `patches/cosmic-text-0.15.0/`. It retains the upstream
MIT/Apache-2.0 notices and fixes `src/shape.rs` to reverse glyphs within compatible
attribute runs of RTL-level words rather than across font/style boundaries.
The base registry checksum is
`173852283a9a57a3cbe365d86e74dc428a09c50421477d5ad6fe9d9509e37737`.
The vendored greeting sample omits its Turkish entry and binary assets are stored
without Git LFS; neither edit changes runtime code or license terms.

Removing the override restores the known upstream behavior, not an equivalent fix.
Any renderer/dependency replacement must pass the retained mixed-script visual and
selection/copy cases. The verified cases do not establish general Unicode conformance.

## Qualification boundaries

Keyboard navigation is implemented; screen-reader support is not validated.
Independent clean-Windows packaging and cross-DPI/multi-monitor qualification remain
open. Native previews and internal timing markers do not establish input-to-display
latency, dropped frames or full memory reclamation. The [roadmap](../../roadmap.md)
tracks these boundaries. `Cargo.toml` and `Cargo.lock` are the dependency authority;
old measurement and license snapshots are not current release manifests.
