# Iced reader

The only active UI path in simPl: a Windows local HTML/PDF/EPUB reader using Iced's
tiny-skia CPU renderer. See
[the roadmap](../../roadmap.md) for product scope and measurements. The old
fixture and empty-shell modes remain explicit diagnostics, not the normal app.

## Run

From the repository root (Windows PowerShell 5.1+):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 run
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 run -Fixture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 run -Large
```

The helper initializes a local x64 MSVC environment, builds release, and opens
the normal reader. `-Fixture` selects 1,000 body paragraphs and `-Large` selects
10,000; those diagnostic workloads also contain headings and an image.
Direct commands from an x64 Native Tools prompt:

```powershell
cargo build -p iced-shell --release --locked
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\pdfium.ps1
.\target\release\iced-shell.exe                    # welcome screen / native picker
.\target\release\iced-shell.exe "C:\Books\book.html"
.\target\release\iced-shell.exe "C:\Books\book.pdf"
.\target\release\iced-shell.exe "C:\Books\book.epub"
.\target\release\iced-shell.exe --shell-poc        # empty diagnostic shell
.\target\release\iced-shell.exe --reader-poc       # fixture diagnostic
.\target\release\iced-shell.exe --reader-poc-large
```

## Interface

Normal mode implements the supplied `design/` reference with Inter, Literata,
Material Symbols, blue accents, three Continue Reading cards, and a virtualized
cover library. The default window is 1280 × 800 logical pixels; the minimum is
540 × 360. Continue cards stack below 768 pixels, and the library uses six columns
above 1024, three above 640, otherwise two. Long chrome titles/paths stay within
their allocated space. PDF controls retain their compact two-row layout.

`ui.rs` owns shared typography/styles, `chrome.rs` the native draggable/resizable
frame, and `shelf.rs` the library. Static font faces and a small icon subset are
embedded; `assets/licenses/` records upstream provenance and licenses, copied into
the portable folder. No runtime font downloads, browser, WebView, animation timers,
blur effects, or GPU backend were added. Reading text uses Literata with the
existing OS fallback for Arabic, Hebrew, and CJK. The reading measure is at most
720 logical pixels, with 1.675 line spacing.
Keyboard focus markers cover the relevant readable metadata or help context.
A generation-checked, gated frame notification waits for the new widget layout;
the reveal operation then adjusts only enclosing vertical scrollables. It does
not scroll an unrelated reading viewport or keep idle redraws running. Reading
and PDF scrollables retain stable slots when panels or search/settings modals open.

The shelf's versioned `library.json` is independent of `recent.json`: 4,096 entries,
4 MiB serialized, no silent capacity eviction, atomic/coalesced background writes.
Missing storage imports legacy Recent metadata without opening source documents.
An invalid library is kept untouched and reported. Window exit drains pending
library writes as well as Recent writes. Separate instances retain last-writer
semantics rather than merging catalogs.

Explicit document opens collect author, file size, format, timestamp, and genuine
saved progress. EPUB declared artwork, HTML's first local image, and PDF page one
provide optional covers. The bounded PNG cache uses content fingerprints, central
crops for both card shapes, and at most 240 × 360 pixels / 512 KiB per file.
Only visible/overscan thumbnails are decoded; the renderer handles are dropped
when entering a document. Files with no artwork receive a typographic jacket.

**Ctrl+K** searches title/author/filename, with Up/Down and Enter selection.
The titlebar gear opens reflow settings; size applies to HTML/EPUB, is saved per
document, and defaults new documents in this session. **Library / Ctrl+W** returns
to the shelf. With no control focused, Space resumes its first continuing book.


## Opening, recent files, and keyboard controls

The normal picker, path argument and real file drop share one format dispatcher:
HTML/HTM/XHTML, PDF and EPUB, case-insensitive. Unsupported extensions are not
passed to the HTML parser. Cancelled/failed opens and auxiliary panels retain
the current document and native viewport; a modal picker or pending save cannot
be abandoned by a dropped file.
A failed position save keeps the document and anchor alive. A fresh close request
retries the save; Dismiss/Escape cancels the pending close instead of leaving an
invisible input guard. Close without saving is an explicit alternative.

**Recent / Ctrl+R** exposes up to 12 successful opens from the shelf or reader.
Each row has Open, Locate and Remove. **Tab / Shift+Tab** cycles an outlined
control; **Enter / Space** activates it. Keyboard traversal scrolls recent rows
and EPUB contents targets into view. **F1** shows help; Escape dismisses panels
and errors. PDF **Ctrl+L** focuses its page field and **Ctrl+F** fits width.
Focus borders belong to the button styles, so an opaque button cannot cover a
container-drawn keyboard indicator. This applies to both shared and PDF controls.

`reader-document::recent` owns versioned, atomically replaced
`%LOCALAPPDATA%\simPl\recent.json`, capped at 64 KiB / 12 entries, with bounded
paths/titles and Windows ordinal path deduplication. Missing history is empty;
invalid/inaccessible history warns, remains untouched on ordinary opens, and
requires explicit **Reset history** before replacement. History loads
and serialized/coalesced writes run off the UI thread. Successful accepted loads
update the MRU; failed/cancelled loads do not. Window close drains pending history
writes, but history errors do not trap exit. Separate instances follow atomic
last-writer behavior, not cross-process history merging.

Locate requires identical document kind and SHA-256 before transferring a typed
reading position to the new canonical path. A currently open matching document
supplies its newer live position; EPUB loads the saved chapter before committing
relocation. On success both stored paths are replaced, including at full library
capacity. Old position records remain as recovery data; Remove forgets the library
and Recent entries, not book files, thumbnails, or position records. An edited source
may be opened normally, but does not inherit a stale position.

## Local HTML reading

Normal use needs no repository fixtures. Open one local file through **Open file**,
Ctrl+O, a path argument, or file drop. A canceled picker leaves the current book
alone. Loading/parsing and position I/O run off the UI thread; obsolete open
results cannot replace a newer document. An open failure leaves an existing
document readable.

- HTML5 parsing with html5ever; XHTML is read with HTML5 tree construction, not
  XML validation. Input must be UTF-8.
- Headings, paragraphs, bold/italic/nested bold-italic, lists, link text,
  preformatted whitespace, basic table text, and local PNG/JPEG/GIF/WebP images.
  Missing/blocked assets produce warnings and available alt text.
- Bundled Literata with script-aware OS fallback, a fluid viewport, 12–36 px body size, and
  viewport-plus-overscan native layout. Full source items and the height index
  remain O(N); only visible/overscan rows become native text/image widgets.
- Wheel, Page Up/Down, and Space scroll; Ctrl+Home/End jumps to the ends.
  A−/A+ or Ctrl+−/+ changes size; Ctrl+0 resets it. Drag selects; Ctrl+A selects
  all source text; Ctrl+C or Copy copies in logical source order; Escape clears.
- Library/Ctrl+W saves and closes the document; native window close exits. Normal close,
  exit, or replacement atomically saves the content item, normalized intra-item
  location, and font size. Explicitly reopening unchanged source restores them;
  no file automatically opens at startup.

Position records are small versioned JSON files under
`%LOCALAPPDATA%\simPl\positions\`, keyed by the canonical Windows path. If
`LOCALAPPDATA` is absent/empty, the OS temporary directory is used as the base.
The source SHA-256 prevents applying an old location after edits. Corrupt saved
state warns but does not block reading. Save failures stay visible; a failed
close offers **Close without saving** rather than pretending the write succeeded.
An abrupt process kill is not a normal save.

This is not a browser: no scripts, network resources, remote fonts, link
navigation, interactive forms, or CSS layout. Image paths must be relative and
stay within the document directory after canonicalization; absolute, parent
traversal, file/remote URLs, and UNC paths are rejected. The HTML source and each
encoded image are limited to 32 MiB, each decoded image to 24 million pixels,
retained decoded RGBA to 128 MiB, and DOM nesting to 512 levels. These are input
safety limits, not a promise of constant total process memory.

## Local PDF reading

`reader-pdf` loads `pdfium.dll` only on first PDF use, by absolute path beside
the executable. It does not search the current directory or fall back to a
system installation. A missing DLL gives an open error without breaking HTML.
After staging a previously missing runtime, restart the reader.

The pinned runtime is non-V8 Windows x64 PDFium **156.0.8066.0**, Chromium
`8066`, from [pdfium-binaries](https://github.com/bblanchon/pdfium-binaries).
`pdfium-render 0.9.4` uses its `pdfium_7881` bindings without the default image
or thread-safe features; one serial worker owns all native handles.

- Continuous fixed-layout pages, virtualized to the viewport plus overscan.
  Prev/Next and the page field (Enter) navigate; wheel, Page Up/Down, Space,
  and Ctrl+Home/End work without reflowing PDF content.
- Zoom is 25–400%; 100% means 96 logical pixels per 72 PDF points.
  −/+, Ctrl+−/+, 100%/Ctrl+0, and Fit width preserve the page-relative anchor.
  Raster requests account for Windows display scaling.
- PDFium transforms glyph bounds through the same page-to-device mapping as
  rendering, including page crop and rotation. Mouse selection uses source
  glyph indices, survives raster eviction/zoom, and can extend across pages.
  Ctrl+A selects document text; Ctrl+C/Copy runs extraction off the UI thread.
  A focused page-number field owns its own editing shortcuts.
- Image-only pages display normally and explicitly report unavailable text.
  There is no OCR. Copy permissions are enforced. Extraction order is PDFium's;
  arbitrary column order, absent Unicode maps, and complex BiDi are not
  universally reconstructed.
- PDF position records use `*.pdf.json`: source fingerprint, zero-based page,
  page-relative vertical offset, relative horizontal scroll, and fit/explicit
  zoom. Normal close/exit/replacement saves them; explicit reopen restores them.

The application LRU holds at most **32 MiB** of RGBA and text-layer capacity,
with a secondary 128-page entry limit. Only one render and one copy request per
reader are in flight; closing it aborts those tasks and releases its cache.
Stale document/geometry replies cannot replace the active view. Cancellation
does not interrupt a native call already executing; queued/copy-page work checks
for canceled receivers. No timer polls or continuously redraws the idle PDF view.
PDFium and Iced's internal/transient allocations are outside the application
cache budget; 32 MiB is not a process-memory cap.

Input limits: 512 MiB source, 100,000 pages, 200,000 text glyphs per page, and
16 MiB copied text. Each raster is capped at 4,000,000 pixels and 8,192 pixels
per edge, proportionally reducing resolution rather than clipping the page.
At high zoom this can reduce sharpness. Invalid dimensions, parse failures,
and exceeded limits produce errors, not silent truncation. Password-required
PDFs are rejected. Form data and annotations are not rendered; JavaScript,
external actions, editing, and OCR are not enabled. PDFium is native code
running in-process, **not a sandbox or a total resource-limit boundary**.

## Local EPUB reading

`reader-document::epub` reads a retained ZIP file through `zip 8.6.0`, with
bounded OPF/container/navigation XML parsing through `roxmltree 0.21.1`.
EPUB 2 NCX and EPUB 3 navigation documents feed one contents model. Only the
linear spine is included; unsupported required spine media needs a supported
HTML/XHTML fallback or fails explicitly. Missing contents uses spine entries.

- **Prev chapter / Next chapter**, Ctrl+Page Up/Down: navigate the reading order.
- **Contents / Ctrl+T**: nested contents, virtualized to visible rows; targets
  preserve chapter and fragment identity. Escape closes the panel.
- The current chapter reuses HTML text layout, raster decoding, selection,
  copy and font controls. Ctrl+A/C selects/copies **only that chapter**.
- One chapter is retained by the UI. Loading/decompression runs on the task
  executor; archive access is serialized. Replaced or canceled load results
  cannot change the current book. Failed chapter loads retain the old chapter.
- Separate `*.epub.json` records contain whole-source SHA-256, canonical chapter
  href, item ID, intra-item fraction and font size. Close/exit/replacement and
  chapter navigation save them; reopening changed bytes discards the old anchor.
- Assets can use sibling paths such as `../Images/cover.jpg` inside the archive.
  This does not relax standalone HTML's document-directory confinement.
  Inline SVG wrappers expose raster image references through the same decoder;
  vector artwork, scripts, foreign-object content and hidden SVG text are not
  rendered. Publisher CSS/custom fonts and general links/footnotes are not applied.

Limits are checked before expensive parsing where applicable: **512 MiB encoded
archive**, **20,000 ZIP records**, **8 MiB central-directory metadata**, **512 MiB
total declared uncompressed data**, **8 MiB combined package/navigation XML**,
**100,000 nodes per XML document**, **4,096 linear chapters**, **10,000 contents
entries**, **64 contents nesting levels**, and **32 MiB per chapter/encoded asset**.
Decoded images share the HTML limits (24 million pixels each, 128 MiB aggregate
RGBA per chapter). These are bounds on inputs/application data, not total process
memory or native crash isolation.

Stored/deflated ZIP and UTF-8 XML/XHTML are supported. Unsafe paths, duplicate
members/aliases, encrypted ZIP members, decompressed-size mismatches, internal
DTD/entity declarations, DRM and fixed-layout content produce explicit errors.
Standard external DOCTYPE declarations do not trigger a network fetch. Known
font obfuscation is ignored with a warning because custom fonts are not loaded.
There is no filesystem extraction or current-directory/network asset fallback.

## Portable folder

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\package.ps1
# Once Cargo, PDFium and notice caches are populated:
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\package.ps1 -Offline
```

The output is `target\portable\simPl\`: `simPl.exe`, `pdfium.dll`, and
`third-party\` notices. No fixture fonts or books are shipped. `.cargo/config.toml`
statically links the Windows x64 MSVC runtime. The app uses system fonts and
the document's own PDF fonts. Clean-machine/minimum-OS qualification remains
separate from same-host portable-folder smoke.

`pdfium.ps1` verifies a pinned archive length and SHA-256 even on cache hits;
`-Offline` never downloads. The notice collector uses the actual Windows
normal/build Cargo graph, preserves package notices, and retrieves omitted
upstream notices at their recorded revisions into an offline cache. For
`mac 0.1.1`, its original author/dual-license declarations accompany the
canonical text of its declared Apache-2.0 option. The pinned Rust standard
library's complete notice document and toolchain provenance are included.
Unresolved notices fail packaging; there is no silent incomplete-package mode.
The project's own license remains unspecified.

## Fixture diagnostics

`--reader-poc` / `--reader-poc-large` resolve
`fixtures/reader-workload/manifest.txt` from the current directory or an ancestor,
validate local fonts/images, and report missing/corrupt assets in the window.
Loading starts when the window opens, with no artificial delay. The fixture path
has no reading-position persistence and does not use product system-font roles.

- Dark, resizable native window; `Info` toggles an information panel, F1 opens
  it, Escape hides it, Tab/Shift+Tab traverse controls. `Exit` or native close exits.
- Reader width switches between 800 and 480 DIP; viewport height is capped at
  600 DIP by the old shared fixture recipe, not a future reader UX requirement.
- Viewport-plus-overscan row construction and layout for 1k/10k workloads.
  Full source content and a compact height index are still O(N). Counters do
  not prove that private Iced/Cosmic/OS caches are bounded.
- Native-hit-tested, document-spanning text selection and Ctrl+C; selection
  survives visible-row eviction and width/resize changes in the retained cases.
- Reader-only F5 unloads/reloads the fixture. Font registration may survive;
  this does not prove full memory reclamation after a real document closes.

## Focused verification

```powershell
cargo test -p iced-shell -p reader-document --all-targets --locked
powershell -NoProfile -ExecutionPolicy Bypass -File crates\iced-shell\tests\reader-poc.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File crates\iced-shell\tests\virtual-reader.ps1 -EvidenceDirectory target\virtual-fresh
powershell -NoProfile -ExecutionPolicy Bypass -File crates\iced-shell\tests\selection-copy.ps1 -EvidenceDirectory target\selection-fresh
```

Build release first. Interactive drivers require an idle, unobscured Windows
desktop, move the pointer/focus and own the processes they launch. Selection
verification overwrites the clipboard. Use fresh output directories. Run the
scenario relevant to the change; these are not mandatory sequential gates.

Additional tools: `tests/native-input.ps1` for shell input/lifecycle;
`tests/runtime-evidence.ps1` for CPU renderer/DPI/UI Automation observations;
`tests/bidi-diagnostic.ps1` for the opt-in BiDi matrix;
`tests/interaction-timing.ps1` for input/callback traces. Each accepts its
parameters at the top of the script. The timing analyzer's focused checks are
`tests/interaction-trace-tests.ps1`, `tests/interaction-source-tests.ps1` and
`tests/analyze-interaction-tests.py`.

## Keep the RTL correction until a verified replacement exists

Root `[patch.crates-io]` selects `patches/cosmic-text-0.15.0/`. This is the full
crates.io 0.15.0 source (MIT OR Apache-2.0), registry checksum
`173852283a9a57a3cbe365d86e74dc428a09c50421477d5ad6fe9d9509e37737`, with one
source correction in `src/shape.rs`: on RTL lines, reverse glyphs within each
compatible attribute run of RTL-level words, rather than across font/style
boundaries. License files remain with the source.
The upstream `sample/hello.txt` greeting list also omits its Turkish entry,
and the vendored `.gitattributes` stores binary assets directly rather than
requiring Git LFS. These packaging edits do not alter runtime code or licenses.

The original reader's pinned mixed-script `p-00003`/`p-00004` cases were visually
corrected at both widths. This is not general Unicode/font conformance. Removing
the override and running `cargo update --offline -p cosmic-text` restores the
known upstream behavior, **not an equivalent fix**. A dependency update or renderer
change must exercise original-window RTL pixels and real selection/copy again.

Historical screenshots and diagnostic dumps containing the former UI and
fixture text have been archived outside the repository, not translated or
presented as current evidence. The current English fixture is revision
`reader-workload-fx-3`; Arabic/Hebrew/CJK and combining-mark data remain
intentional Unicode regression inputs, not UI localization.

Use the drivers above to reproduce current behavior. Save generated captures
under `target/` or an OS temporary directory, not alongside source. The small
[timing trace fixture](tests/fixtures/interaction-trace/README.md) retains only
numeric callbacks and analyzer-required metadata from one historical run;
it is regression input, not a current performance report.

## Diagnostics and limitations

In explicit diagnostic modes, `ICED_SHELL_STARTUP_MARKERS=<path>` emits bounded
QPC markers; `ICED_SHELL_INTERACTION_TRACE=<new-path>` records optional reader
callbacks. These are **not** presented-frame timestamps or startup-budget proof,
and they do not instrument the normal HTML/PDF/EPUB path. Other `ICED_SHELL_*`
test-status/BiDi variables are explicit diagnostics; leave them absent in
ordinary use and resource measurements. The former adapter logger and its
environment gate have been removed.

Current features use Iced 0.14 with tiny-skia/softbuffer, advanced widgets,
decoded image handles, and the thread-pool executor. WGPU and unconditional
rendering are not enabled. Framework-owned worker/font caches and OS compositor
costs still exist. `Cargo.toml` and `Cargo.lock` are authoritative;
`dependency-inventory.txt` is a historical license snapshot, not an automatically
refreshed current graph or a portable release manifest.

The earlier diagnostic shell's UI Automation observation found no client control
descendants. Keyboard operation is not screen-reader support; product
accessibility has not been validated. Cross-DPI/multi-monitor and other-host
behavior remain unverified. The roadmap distinguishes the old WGPU baseline
from current CPU HTML/PDF/EPUB measurements. Timing diagnostics do not establish
presented-frame percentiles, dropped frames, or input-to-display latency.
