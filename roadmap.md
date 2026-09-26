# simPl roadmap: a small local reader

## Product goal and present reality

Open a local book, read it, close it, and resume where you stopped. The reader
should appear quickly, use little disk and memory, remain idle while you read,
and work without a network connection. Start on Windows x64; add other desktop
platforms only in response to real demand.

The Windows application now reads local UTF-8 HTML/XHTML and PDF files.
HTML supports local images, selection/copy, reflow, and per-file resume.
PDF preserves page layout with continuous scrolling, navigation, zoom/fit-width,
text-layer selection/copy, and page-relative resume. Iced's tiny-skia CPU
renderer remains the only UI backend. The normal path works outside the repo.
Explicit fixture diagnostics, the Cosmic Text RTL correction, and optional
process RAM/CPU measurement remain available.

EPUB and recent files are not implemented. A licensed-dependency portable folder
can now be assembled; clean-machine qualification is still pending. These
completed reading slices are not completion of the whole MVP.

The MVP sequence is **real HTML reading and resume → PDF → EPUB → small
portable release**. Keep one active UI implementation and add only the code
needed by the next usable slice.

## MVP boundaries

| Area | MVP result |
|---|---|
| Opening | Local file dialog, path argument, and drag-and-drop; one open document |
| HTML | Local HTML/XHTML reading view for headings, paragraphs, lists, links, and local images |
| PDF | Original page layout, scrolling/page navigation, zoom, and fit-to-width |
| EPUB | DRM-free reflowable books with chapter order, table of contents, text, and images |
| Reading | Keyboard navigation; readable width and font size for reflowed text; selection/copy |
| Resume | Per-file content location on explicit reopen; a small recent-files list is still planned |
| Errors | Explain missing, corrupt, or unsupported files without freezing the UI |

HTML is not a browser: no JavaScript, network resources, remote fonts/images,
interactive forms, or pixel-perfect CSS. Fixed-layout and DRM-protected EPUB are outside
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

- Keep the single Rust/Iced UI with the **tiny-skia CPU renderer**. The measured
  CPU comparison justified removing the active WGPU path and its adapter logger;
  there is no runtime WGPU fallback. Keep checking first readable display,
  scrolling, RTL placement, copy, and idle CPU as real document support grows.
- The small reflow types, HTML parser, and position storage now live in
  `reader-document`; the application owns presentation and native file dialogs.
  Fixture workload generation is not the production loader. Reuse the real HTML
  path for EPUB, but give PDF its own page model rather than a universal AST.
- HTML5 parsing uses html5ever. EPUB needs ZIP package/spine parsing feeding the
  HTML path. `reader-pdf` now integrates PDFium through a lazy serial worker:
  non-V8 Windows x64 Chromium 8066 with `pdfium-render 0.9.4`. Native binaries,
  license notices, and the complete portable folder are part of this slice.
- Load a PDF engine or EPUB chapter when needed; bound decoded image/page
  caches by bytes. Keep costly work off the UI thread, and discard outdated
  results when opening another file. Limit archive expansion and image decoding;
  reject paths escaping the EPUB package and prevent HTML scripts or remote
  resources from running. Do not treat a Rust panic catcher as native PDF crash
  isolation.
- HTML positions are small atomically replaced per-file JSON records, keyed by
  canonical path with a source fingerprint, content item, intra-item fraction,
  and font size. Corrupt state warns without blocking the book; source edits
  invalidate an old position. Recent files are not implemented. PDF has separate
  page/offset/zoom records; EPUB will need chapter plus content position.
  A raw scroll pixel or percentage alone remains insufficient.

## Performance targets

Measure the same release build with representative real local books. A small
HTML smoke document is not a typical-book benchmark, and old fixture numbers
must not be relabeled as shipped-reader results.

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
committed or downloadable evidence**. They describe the old WGPU prototype,
not the current CPU reader.

### CPU/HTML release observation (2026-09-26)

The release executable was **6.83 MiB** (7,160,320 bytes). A copied executable
ran outside the repository without fixture assets, at 125% Windows display
scaling. The HTML sample was a 21,445-byte local file with 100 body paragraphs,
mixed Arabic/Hebrew/Japanese text, inline styles, lists, and a local PNG.

| Normal application scenario | Private working set | Private commit | Observed idle CPU, one logical core |
|---|---:|---:|---:|
| Welcome screen, no document | 8.03 MiB | 9.12 MiB | 0.00% |
| Local HTML sample, first reading view | 9.55 MiB | 10.80 MiB | 0.00% |

Each scenario had three 12-second root-process runs, sampled every 250 ms.
Memory values are the median of each run's last approximately five seconds,
then the median across the three runs. CPU uses cumulative process-time deltas
over those final 4.75–5.00-second windows; all six windows recorded no CPU-time
increase at the counter's resolution. All collections were valid; exit code 124
was the sampler's intentional duration-limit stop, not an application crash.

This shows substantially lower normal-window memory than the old WGPU baseline,
not a controlled comparison of identical product UIs or proof for typical large
books. Startup latency, cold launch, GPU/compositor memory, long reading sessions,
and a clean-machine portable package were not measured in this HTML run.

Machine-local, untracked evidence is under `target/cpu-html-measure/`,
`target/html-reader-smoke/`, `target/cpu-html-selection/`, and
`target/cpu-html-virtual-release/`. The measured executable's SHA-256 was
`8cd8ad0dd4ffc6d5ceae5c7646017217f3d048d8ceaa3fdbe4ebed6d8d1c89f1`.

### PDF portable-release observation

The static-CRT Windows x64 portable folder contains **333 files / 17.49 MiB**
(18,335,781 bytes), including `simPl.exe` (9,033,728 bytes), `pdfium.dll`
(7,380,992 bytes), and Rust, standard-library, and native dependency notices.
There are no bundled fixture fonts or books. PE imports were inspected: the
executable needs no separate VC++ runtime DLL. This is a same-host measurement,
not clean-machine qualification.

The complete folder was copied to an OS temporary directory outside the repo.
Native mouse/keyboard and clipboard checks at 125% display scaling exercised
text selection in both directions, crop/rotation, full-document text extraction,
page navigation, zoom/fit-width, resize-aware resume, cache-evicted selection,
wheel-extended cross-page selection, copy cancellation, scanned pages, corrupt
input, and switching back to HTML. A 5,000-page generated PDF reached its final
page; the real book was Adobe's 756-page
[PDF 32000-1:2008 specification](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf).
Rendered screenshots were inspected rather than treating a successful open as
rendering proof. The empty application was checked not to load PDFium.
The real PDF was also read across pages 99–101, zoomed to 125%, closed, and
resumed at the same page-relative location after resizing. A missing adjacent
DLL left HTML readable and did not load a DLL placed in the working directory.

| Normal application scenario | Private working set | Private commit | Observed idle CPU, one logical core |
|---|---:|---:|---:|
| Welcome screen, no document | 8.07 MiB | 9.16 MiB | 0.00% |
| Tiny HTML switching-smoke file | 8.05 MiB | 9.13 MiB | 0.00% |
| Generated text PDF, 12 pages | 23.77 MiB | 25.22 MiB | 0.00% |
| Generated image-only PDF, 3 pages | 22.36 MiB | 23.58 MiB | 0.00% |
| Generated long PDF, 5,000 pages | 33.16 MiB | 34.73 MiB | 0.00% |
| Adobe specification, 756 pages | 35.71 MiB | 37.32 MiB | 0.00% |

All documents were measured at the first reading view with default fit-width
and a fresh measurement profile. Each scenario had three 12-second root-process
runs, sampled every 250 ms on Windows build 26200.9457, x86_64. Memory is the
median over each run's final approximately five seconds, then the median across
three runs. CPU is the cumulative process-time delta over the same live-sample
window (4.74–5.00 seconds), normalized to one logical core. All 18 windows had
no CPU-time increase at the counter's resolution. Terminal teardown samples
were excluded. All collections were valid; exit code 124 was the sampler's
intentional duration-limit stop, not an application crash. The tiny HTML input
is not the earlier 100-paragraph sample or a typical-book benchmark.

These cases meet the PDF memory, empty-window, package-size, and idle-CPU
targets, so **retain PDFium; no alternative-engine investigation is triggered**.
The 32 MiB application cache does not bound native or renderer memory.
Startup-to-visible-content latency, cold/warm launch budgets, GPU/compositor
memory, long-session/repeated-cycle growth, broad PDF fidelity, and clean-machine
compatibility remain unmeasured; this is not proof that every performance
target or every PDF is covered.

Machine-local, untracked evidence: `target/portable/package-facts.json`,
`target/pdf-measure-51c3e4ee1e3d40e2811d666a7fce6527/` (manifests, samples,
summary), and the external smoke directory
`%TEMP%\simPl-pdf-smoke-6b670af1066d4bcbb075b67a1820515b\`
(copied runtime, inputs, position records, result, screenshots). Supplemental
real-book resume and DLL-isolation screenshots/results are in
`%TEMP%\simPl-pdf-smoke-da578ee508a640b78fea2ff2be25fc31\`. These are not
committed or downloadable artifacts. The measured executable's SHA-256 was
`5180dfc183d23a5384642245de5f2d10b0e195a3bb772021587cfc1c3ab6315c`.

## Delivery order

### 1. HTML: first usable vertical slice

- [x] Open a local HTML/XHTML file by dialog or path argument and display
      readable text and local images without a repository fixture.
- [x] Scroll, adjust font size, select/copy text, and report corrupt files.
- [x] Return to a stable content location after close/reopen, including after
      reasonable viewport changes.
- [x] Remove demo loading delays and fixture requirements from the product path.
- [x] Measure and reduce renderer/memory costs while preserving RTL correctness.

**Done when:** from another working directory, a real HTML file can be opened,
read, closed, and resumed. A standalone parser or AST is not the deliverable.

### 2. PDF: read a real book early

- [x] Load the PDF engine on demand; show the first page, navigation, and zoom.
- [x] Bound page cache; save position; handle long, scanned, and corrupt files.
- [x] Copy/select actual text where available and state when it is not.
- [x] Include licensed native components in the portable package and measure it.

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
