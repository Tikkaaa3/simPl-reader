# simPl roadmap: a small local reader

## Product goal and present reality

Open a local book, read it, close it, and resume where you stopped. The reader
should appear quickly, use little disk and memory, remain idle while you read,
and work without a network connection. Start on Windows x64; add other desktop
platforms only in response to real demand.

Today there is a working Iced fixture prototype, **not a file reader**. It shows
1,000/10,000-paragraph local workloads, limits visible-row layout, supports
selection/copy, and has focused mixed-script and real-window checks. The
repository also retains a narrow Cosmic Text RTL correction and optional
process RAM/CPU measurement. No real HTML, PDF, or EPUB opening or parsing,
saved reading position, or distributable reader is implemented. A fixture demo
or a timing trace does not establish product behavior.

The MVP sequence is **real HTML reading and resume → PDF → EPUB → small
portable release**. Keep one active UI implementation and add only the code
needed by the next usable slice.

## MVP boundaries

| Area | Intended result; not implemented yet |
|---|---|
| Opening | Local file dialog, path argument, and drag-and-drop; one open document |
| HTML | Local HTML/XHTML reading view for headings, paragraphs, lists, links, and local images |
| PDF | Original page layout, scrolling/page navigation, zoom, and fit-to-width |
| EPUB | DRM-free reflowable books with chapter order, table of contents, text, and images |
| Reading | Keyboard navigation; readable width and font size for reflowed text; selection/copy |
| Resume | Small recent-files list and per-file position; offer to resume without silently reopening a file |
| Errors | Explain missing, corrupt, or unsupported files without freezing the UI |

HTML is not a browser: no JavaScript, network resources, remote fonts/images,
forms, or pixel-perfect CSS. Fixed-layout and DRM-protected EPUB are outside
scope. PDF is not reflowed or OCR'd: show scanned pages as pages, and expose
selection/copy only if the PDF engine has a usable text layer.

TXT/Markdown, other document formats, search, annotations, bookmarks, a cover
library, watched folders, metadata editing, a database/search index, a settings
center, themes, sync, accounts, OCR/AI, and plugins are not MVP prerequisites.
File associations and an installer can follow a portable release.

Unicode correctness fixtures may contain intentional Arabic, Hebrew, CJK, and
combining-character data. Product UI and repository prose are English; Turkish
fixture samples are not retained.

## Implementation direction

- Continue from the current Rust/Iced UI, without a second parallel UI
  framework. Its current memory cost is not approval for a lightweight release.
  Compare the available CPU-renderer option with the current path during the
  HTML slice, using the same content to check RAM, first readable display,
  scrolling, RTL placement, and copy behavior. Change renderer/UI only when a
  measured bottleneck warrants it.
- Grow the active application crate with ordinary modules as needed. Fixture
  workload data is not a production document model. Share a small reflow path
  between HTML and EPUB; render PDF with its own page model, not a universal
  document AST. Do not invent parser abstractions before real callers need them.
- Integrate maintained parsers and a PDF engine rather than building a browser,
  PDF parser, or shaping engine. html5ever is an HTML candidate; EPUB needs ZIP
  package/spine parsing feeding the HTML path. PDFium is a candidate, **not an
  integrated dependency**; its native binaries, redistribution licenses, and
  full package size must be addressed in the PDF slice.
- Load a PDF engine or EPUB chapter when needed; bound decoded image/page
  caches by bytes. Keep costly work off the UI thread, and discard outdated
  results when opening another file. Limit archive expansion and image decoding;
  reject paths escaping the EPUB package and prevent HTML scripts or remote
  resources from running. Do not treat a Rust panic catcher as native PDF crash
  isolation.
- Store recent files and stable per-document reading locations in a small
  atomically replaced local file. PDF needs page plus offset; EPUB needs chapter
  plus content position. A raw scroll pixel or percentage alone is insufficient.
  Corrupt saved state must not block opening a book. **None of this persistence
  exists in the prototype yet.**

## Performance targets, not current achievements

Measure the same release build and representative real local books once file
opening exists. Do not relabel prototype numbers as a shipped-reader result.

| Metric | Initial target |
|---|---:|
| Warm launch to usable window | ≤ 200 ms median |
| Cold launch after restart to usable window | ≤ 500 ms |
| Small local book to first readable content | ≤ 1 s |
| Empty-window private working set | ≤ 50 MiB |
| Typical HTML/EPUB private working set | ≤ 80 MiB |
| Typical PDF private working set | ≤ 150 MiB |
| Portable package including required DLLs, fonts, and licenses | ≤ 50 MiB |
| CPU while reading | Near zero; no continuous redraw/polling |

Bound page/image caches for large books and check repeated open/close cycles for
unbounded growth. Report private commit separately from private working set;
without GPU measurement neither is total memory. Development fixtures and
`target/` are not part of the portable package. Measure startup to visible,
usable content, not merely an app callback or marker. Short runs do not justify
cold-launch or percentile claims.

### Historical single-run baseline (2026-09-26)

Before the repository cleanup, release fixture runs on one Windows machine
sampled every 250 ms over separate 10/12-second windows. Median of roughly the
last five seconds: empty shell **158.28 MiB private working set / 307.72 MiB
private commit**; 1,000-paragraph fixture **173.29 MiB / 325.88 MiB**. The
release executable was **13.56 MiB**. The corresponding one-logical-core CPU
observations were approximately 0% / 0.33%. These were root-process, single-run
measurements, not startup, GPU, real-book, or package-size results.

After cleanup, a rebuilt release executable was **11.23 MiB**. Another single
10-second empty-window run, sampled every 250 ms with a median over roughly the
last five seconds, gave **158.46 MiB private working set / 308.68 MiB private
commit**. The smaller executable did not resolve high empty-window memory use.
Original raw outputs at `target/reset-baseline-{empty,reader}/` and
`target/reset-after-empty/` were untracked machine-local artifacts, **not
committed or downloadable evidence**. These numbers record past observations;
repeat measurements on the next real-file slice.

## Delivery order

### 1. HTML: first usable vertical slice

- [ ] Open a local HTML/XHTML file by dialog or path argument and display
      readable text and local images without a repository fixture.
- [ ] Scroll, adjust font size, select/copy text, and report corrupt files.
- [ ] Return to a stable content location after close/reopen, including after
      reasonable viewport changes.
- [ ] Remove demo loading delays and fixture requirements from the product path.
- [ ] Measure and reduce renderer/memory costs while preserving RTL correctness.

**Done when:** from another working directory, a real HTML file can be opened,
read, closed, and resumed. A standalone parser or AST is not the deliverable.

### 2. PDF: read a real book early

- [ ] Load the PDF engine on demand; show the first page, navigation, and zoom.
- [ ] Bound page cache; save position; handle long, scanned, and corrupt files.
- [ ] Copy/select actual text where available and state when it is not.
- [ ] Include licensed native components in the portable package and measure it.

**Done when:** a real PDF book can be read for several pages, zoomed, closed,
and resumed in the same place.

### 3. EPUB: reuse reflow reading

- [ ] Parse ZIP package/spine, navigate chapters and contents, display local images.
- [ ] Load chapters on demand and preserve position across font/viewport changes.
- [ ] Clearly reject unsupported DRM/fixed-layout books and bound hostile archives.

**Done when:** a multichapter EPUB can be navigated end to end and resumed
without building a second HTML/layout system.

### 4. Portable MVP

- [ ] Make opening by dialog, path, and drag/drop consistent across all three formats.
- [ ] Finish recent files/resume, keyboard navigation, and missing/moved-file handling.
- [ ] Exercise open/read/close/resume on real HTML, PDF, and EPUB files.
- [ ] Run the portable release on a clean Windows machine without the repo or build tools.
- [ ] Measure startup, RAM, idle CPU, and full package size; resolve or explicitly
      account for deviations from the targets above.
