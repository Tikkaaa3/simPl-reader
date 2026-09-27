# simPl roadmap: a small local reader

## Product goal and present reality

Open a local book, read it, close it, and resume where you stopped. The reader
should appear quickly, use little disk and memory, remain idle while you read,
and work without a network connection. Start on Windows x64; add other desktop
platforms only in response to real demand.

The Windows application now reads local UTF-8 HTML/XHTML, PDF, and reflowable
EPUB 2/3 files. HTML supports local images, selection/copy, reflow, and per-file
resume. EPUB reuses that view with on-demand chapters, nested contents and
chapter-relative resume. PDF preserves page layout with continuous scrolling,
navigation, zoom/fit-width, text-layer selection/copy, and page-relative resume.
Iced's tiny-skia CPU renderer remains the only UI backend. The normal path works
outside the repo. Explicit fixture diagnostics, the Cosmic Text RTL correction,
and optional process RAM/CPU measurement remain available.

The core local-reading MVP now includes a bounded recent-files list, keyboard
access to its controls, and fingerprint-checked moved-file recovery. A complete
licensed-dependency portable folder can be assembled. Independent clean-Windows
qualification is still blocked by the available environment.

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
| Resume | Per-file content location on explicit reopen, a 12-entry recent list, and verified moved-file recovery |
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
  Fixture workload generation is not the production loader. EPUB now reuses
  the HTML extractor/resource decoder; PDF retains its own page model.
- HTML5 parsing uses html5ever. EPUB uses file-backed `zip 8.6.0` and bounded
  `roxmltree 0.21.1` package/navigation parsing; one chapter is loaded at a time.
  `reader-pdf` integrates PDFium through a lazy serial worker: non-V8 Windows x64
  Chromium 8066 with `pdfium-render 0.9.4`. The portable folder includes native
  binaries and the complete shipped dependency notices.
- Load a PDF engine or EPUB chapter when needed; bound decoded image/page
  caches by bytes. Keep costly work off the UI thread, and discard outdated
  results when opening another file. Limit archive expansion and image decoding;
  reject paths escaping the EPUB package and prevent HTML scripts or remote
  resources from running. Do not treat a Rust panic catcher as native PDF crash
  isolation.
- HTML positions are small atomically replaced per-file JSON records, keyed by
  canonical path with a source fingerprint, content item, intra-item fraction,
  and font size. Corrupt state warns without blocking the book; source edits
  invalidate an old position. PDF has separate page/offset/zoom records; EPUB
  adds chapter href plus content item/font size. A raw scroll pixel or percentage
  alone remains insufficient.
- Recent history is a bounded, versioned atomic JSON file, loaded asynchronously
  and saved through a serialized/coalesced UI queue. It records successful
  accepted opens only. Locate verifies kind/fingerprint before transferring a
  position, preferring a currently open book's newer live position. Unreadable
  history requires an explicit reset, not silent replacement.

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

### EPUB portable-release observation

The EPUB release keeps the shared CPU reflow renderer and loads one spine chapter
at a time. Its offline portable folder contains **345 files / 18,802,241 bytes
(17.93 MiB)**, including the **9,439,232-byte executable (9.00 MiB)**, PDFium,
and all required notices for 176 shipped Rust dependencies and native components.
There are no bundled font files or additional VC++ runtime imports.

Native verification of the copied folder outside the repository passed at 125%
display scaling: EPUB3 nested contents and EPUB2 NCX fragments, source-order
chapter navigation, Unicode picker paths, local images, exact selection/copy,
20 px font and resized resume, and HTML/PDF switching. Failed opens preserve both
the existing chapter and its item/fraction position; a smoke-discovered native
scroll-widget reset was fixed and rechecked for each rejected input. EPUB-only
reading did not load PDFium. All 190 workspace tests and Clippy passed.

The real book was [Project Gutenberg's Alice](https://www.gutenberg.org/ebooks/11),
downloaded from [the EPUB3 images endpoint](https://www.gutenberg.org/ebooks/11.epub3.images):
188,960 bytes, SHA-256
`12cbc3610260503383ad7ecf800beb0d885a6011e15900b2bf61cecfc571d6c8`.
Its SVG-wrapped raster cover displayed, all 15 linear spine sections were
traversed, and chapter III text was copied and resumed after resizing.

Nine valid root-process collections used three 12-second runs per scenario,
sampled every 250 ms. Each row is the median of three per-run memory medians in
the last approximately five seconds of live samples (actual windows 4.74–5.00 s).
CPU uses cumulative user/kernel time over that same window, normalized to one
logical processor. Terminal samples are excluded; exit code 124 is the sampler's
intentional duration stop, not an application crash.

| Scenario | Private working set | Private commit | Idle CPU |
| --- | ---: | ---: | ---: |
| Empty welcome window | 8.10 MiB | 9.19 MiB | 0.00% observed |
| Alice raster cover, default 18 px | 14.95 MiB | 16.25 MiB | 0.00% observed |
| Alice chapter III, restored 20 px | 9.57 MiB | 10.75 MiB | 0.00% observed |

Each run used a fresh profile; chapter III profiles were seeded from the native
smoke's actual saved position records for that same book path. All nine idle
windows had no CPU-time counter increase at the available counter resolution.
These observations meet the current empty-window, typical-EPUB, idle-CPU and
package-size targets; they do not measure peak, total-system or GPU memory.

This is not arbitrary EPUB/CSS/SVG fidelity, a hostile-file sandbox, startup
latency, long-session memory, accessibility, or clean-machine compatibility
proof. Publisher CSS/fonts, vector SVG rendering, non-linear auxiliary sections,
general internal links/footnotes, DRM and fixed-layout books remain unsupported.

Machine-local, untracked evidence: `target/portable/epub-package-facts.json`,
`target/epub-measure-7dfea3f69b3646098b3435897c3c288a/` (manifests, samples,
summary), and
`%TEMP%\simPl-epub-smoke-9c2c2dba5af54b228c2e281322df1bff\` (copied runtime,
inputs, position records, result and inspected screenshots). These are not
committed or downloadable artifacts. The final executable's SHA-256 is
`3d1c87f2370b6010d0af8c6ffc089da01c526e6a2e9ec41509a63ae4ba5b75a5`.

### Portable MVP qualification

The recent-files build keeps the same CPU renderer and lazy PDFium engine.
Its complete portable folder contains **345 files / 18,896,449 bytes (18.02 MiB)**:
the executable is **9,533,440 bytes**, and PDFium is **7,380,992 bytes**.
The folder includes the required native notices and notices for 176 shipped Rust
dependencies, with no bundled fonts or additional VC++ runtime DLL imports.
The offline format check, workspace Clippy with warnings denied, and all **197
tests** passed. Two added state-transition regressions failed before the
save-error correction and pass afterward.

The copied folder passed native desktop verification outside the repository,
with a fresh profile, an unrelated working directory, a System32/Windows-only
PATH, and 125% display scaling. HTML/PDF/EPUB opened by real command-line
arguments, native picker, and OLE file drop, including Unicode names and uppercase
extensions. Recent ordering/deduplication, the 12-entry limit, keyboard traversal
through the last row, missing-file Locate, rejection of a different book, and
relocation of a file moved while still open all passed. Rejected opens retained
the current source text and reading anchor. A real locked position record proved
save-failure retention, explicit retry, and Escape cancellation; corrupt history
remained untouched until native Reset, and unwritable history did not trap exit.
Shared and PDF keyboard focus borders were checked on the actual desktop.

Real-book round trips used a local UTF-8 snapshot of
[Gutenberg's Alice HTML](https://www.gutenberg.org/cache/epub/11/pg11-images.html),
the Alice EPUB identified above, and the 756-page Adobe specification. HTML
prose at 20 px, EPUB chapter III at 20 px, and PDF page 100 at 100% zoom survived
close/recent/reopen and resizing. Native Tab/Shift+Tab, Enter/Space, F1/Escape,
EPUB contents arrows, PDF page editing, and fit-width were exercised.

| Warm launch scenario | Median to visible toolbar and content |
| --- | ---: |
| Empty welcome window | 85.9 ms |
| Alice HTML, initial view | 91.2 ms |
| Adobe PDF, first page | 149.9 ms |
| Alice EPUB, raster cover | 101.8 ms |

Each median uses five fresh processes/profiles with warm OS/file caches.
QPC timing starts immediately before process creation and stops when two
desktop-pixel regions match stable references: the enabled Open control and
welcome/document content. Polling/capture overhead is included; no app callback,
startup marker, or forced offscreen paint substitutes for visible output.
The native picker responded after every capture, but input-to-response latency
was not timed. These visible-start medians are below the 200 ms warm-window and
1 s content budgets; they do not establish a cold-start or percentile result.

| Reading scenario | Private working set | Private commit | Observed idle CPU |
| --- | ---: | ---: | ---: |
| Empty welcome window | 8.18 MiB | 9.28 MiB | 0.00% |
| Welcome with 12 recent entries | 8.85 MiB | 9.94 MiB | 0.00% |
| Alice HTML prose, restored 20 px | 14.71 MiB | 16.20 MiB | 0.00% |
| Adobe PDF page 100, restored 100% zoom | 43.85 MiB | 45.50 MiB | 0.00% |
| Alice EPUB chapter III, restored 20 px | 10.27 MiB | 11.55 MiB | 0.00% |

These are 15 valid root-process collections on Windows build 26200.9457:
three 12-second runs per scenario, sampled every 250 ms. Each profile was fresh;
reading/recent cases were seeded from the native smoke's real saved records.
Memory uses each run's final approximately five-second live-sample median,
then the median of three runs. CPU is the cumulative user/kernel time delta
over the same QPC window (4.75–5.00 s), normalized to one logical core.
All 15 windows had zero CPU-time increase at the counter's resolution.
Terminal samples are excluded; exit 124 is the intentional duration stop.
A launcher-status anomaly prompted an extra PDF collection; it is retained
separately, and the table uses the original three runs for every scenario.
The measured memory, idle-CPU and package-size cases meet their targets.
Neither private metric is total-system/GPU memory or a peak-memory bound.

Six mixed-format open/read/close cycles ended at **25.91–27.48 MiB private
working set / 30.11–32.70 MiB private commit**. This short sequence did not show
monotonic growth; it is not a long-session leak or peak-memory bound.

**Environment-limited qualification:** an independent clean Windows run remains
blocked. Windows Sandbox is disabled and its executable is absent; Hyper-V
management access is denied, and non-admin virtualization enumeration exposes
only the host. No configured SSH test host or installed alternate VM tool was
available. No feature installation, elevation, OS restart, or termination of
unrelated processes was attempted. A separate accessible clean Windows machine
or VM is required to close that acceptance item. The cold-launch-after-restart
target is also unmeasured under the no-restart constraint.

Machine-local, untracked evidence: `target/portable/mvp-package-facts.json`,
`target/mvp-measure-9dedaeeff9bc4a14984d012de86594b3/` (manifests, samples,
summary), and
`%TEMP%\simPl-mvp-smoke-eb62081ca08c48719a390d41f998ec25\` (copied runtime,
inputs, native results, position records, inspected screenshots, focus evidence,
and visible-startup captures/results). These are not committed or downloadable
artifacts. The verified executable's SHA-256 is
`8b1e4679b2a7491a7880be5bc39694bee18b20c3f6844d3a8ac002edbffc45e2`.

### Modern reader UI

**Historical baseline, before the supplied design:** the reader used graphite/sage chrome, system Segoe UI,
semibold headings, restrained cards and thin scrollbars. Welcome contracted at
small heights; PDF controls used one row above the compact breakpoint and two
below it. Long labels could not cover adjacent actions. That pass added no dependency,
bundled font, animation timer, blur effect, GPU backend, or larger document/page cache.
The custom theme is initialized once.

Offline format/Clippy checks and **201 tests** passed, including four regressions
for nested focus geometry, reverse traversal, already-visible controls and tall
help context. Native verification used the copied complete folder, an unrelated
working directory, a restricted PATH, isolated profiles, and 125% Windows scaling.
It covered default and 540 × 360 minimum layouts; mouse/native-picker and command-
line opening; exact 35,663-character Unicode-path HTML copy; all 12 recent rows,
reverse Tab and Enter/Space; stacked help/contents; real EPUB chapter navigation;
PDF page 100, page-field focus, actual size and fit width; resized position/font
resume; rejected opens; and fingerprint-checked moved-file recovery. Screenshots
were inspected. This UI pass did not re-qualify Winit's OLE drag/drop gesture.

The small-window smoke exposed hidden focus in nested panels. Focus reveal now
waits for the new widget layout, scrolls only ancestors of the target, and includes
the recent row's metadata or the help context. Its frame notification is gated,
not a continuous redraw subscription. With recent-list keyboard focus and help
open, a separate 5.000-second observation recorded zero user/kernel CPU increase.

**Paired warm visible-start measurements, same development host:**

| Scenario | Previous UI | New UI | Observed difference |
| --- | ---: | ---: | ---: |
| Empty welcome | 75.1 ms | 77.7 ms | +2.5 ms |
| Alice HTML, first view | 88.6 ms | 95.7 ms | +7.1 ms |
| Adobe PDF, first page | 153.5 ms | 148.8 ms | −4.7 ms |
| Alice EPUB, cover | 97.2 ms | 113.9 ms | +16.7 ms |

Each number is a five-process median with fresh profiles and warm OS/file caches.
Timing begins immediately before process creation and ends when two high-contrast
desktop regions (header and welcome/document content) match settled references.
Capture/polling overhead is included. The native picker responded after every
capture; this does not measure input-to-display latency. The largest observed
increase was 16.7 ms for EPUB; all medians remain under 200 ms. This small sample
does not establish a speedup, percentile guarantee, or cold-start result.

| First-view scenario | Previous private WS | New private WS | Previous private commit | New private commit |
| --- | ---: | ---: | ---: | ---: |
| Empty welcome | 8.15 MiB | 8.19 MiB | 9.25 MiB | 9.27 MiB |
| Welcome, 12 recent entries | 9.02 MiB | 9.15 MiB | 10.12 MiB | 10.25 MiB |
| Alice HTML, 18 px | 16.26 MiB | 16.39 MiB | 17.56 MiB | 17.55 MiB |
| Adobe PDF, first page / fit width | 35.63 MiB | 35.71 MiB | 37.25 MiB | 37.30 MiB |
| Alice EPUB, cover / 18 px | 15.11 MiB | 15.14 MiB | 16.30 MiB | 16.27 MiB |

These are 30 valid root-process collections: three 12-second runs per scenario
per binary, sampled every 250 ms, using final approximately five-second live
windows (4.75–5.00 s). Memory is the median of the three per-run medians. CPU uses
the corresponding cumulative user/kernel delta divided by QPC elapsed time and
one logical core. All 15 new-UI windows recorded zero CPU increase. Previous-UI
scenario CPU medians were also zero, with two individual windows at 0.99% and
2.63%; this is not evidence of a CPU speedup. A previous-UI recent run exited
naturally after about 3.3 seconds: its raw data was retained, excluded from the
12-second comparison and replaced by a complete run. Included duration stops
have exit 124 and zero required live-query failures.

The largest private-working-set median increase was **0.14 MiB**. The portable
folder is **18,951,233 bytes / 18.07 MiB / 345 files**, up **53.5 KiB**; it still
contains notices for 176 shipped Rust dependencies and the same PDFium DLL.
The executable is 9,588,224 bytes. These first-view comparisons are deliberately
not compared with the earlier 20 px prose/chapter and page-100 memory figures.
No active-scrolling frame-latency, peak-memory, long-session, cold-restart or
independent clean-Windows claim is made. Unrelated sessions were not terminated
and Windows was not restarted.

Machine-local, untracked evidence is under
`%TEMP%\simPl-ui-smoke-4a0673d3b49b485280b1b04385367c91\`: `bench-before/` and
`bench-after/` contain startup captures, resource manifests/samples and summaries;
`final-checks.json`, `focused-idle.json`, `entrypoints-checks.json`, saved profiles
and `screenshots/final-*.png` record native checks. The standalone `WM_DROPFILES`
probe in the entrypoint harness was not an OLE drop and is not counted as gesture
verification. A compact comparison and selected previews are retained in
`target/portable/ui-evidence/`.
The baseline executable SHA-256 is
`8b1e4679b2a7491a7880be5bc39694bee18b20c3f6844d3a8ac002edbffc45e2`;
the measured new executable SHA-256 is
`58ffc7c5992fa60f858a7d6d03b91a7d49c061b8b51c45bb52972c012db0c418`.

### Supplied-design native workspace

The supplied `design/code.html`, `screen.png`, and `DESIGN.md` now drive the
normal native UI: the 48-pixel custom window header, Inter/Literata typography,
blue-accent palette, three Continue Reading cards, six-column cover shelf,
responsive stacking, segmented sorting, and footer. The source design files are
unchanged. There is no browser/WebView replacement and no animation timer;
hover treatment is event-driven. The nine bundled font/icon faces total
2,362,368 bytes, with upstream and modification notices in `assets/licenses/`
and the portable folder.

The shelf uses real document authors, source covers, sizes, opening times, and
saved progress rather than installed sample records. A separate atomic library
retains up to 4,096 entries / 4 MiB, without the Recent list's 12-entry eviction.
Legacy entries acquire unknown metadata only when explicitly reopened. Cover
PNGs are bounded to 240 × 360 / 512 KiB, cropped before downsampling to preserve
detail for both card shapes. Only visible/overscan covers become UI handles,
and entering a document releases those handles. Startup reads stored metadata
and cached covers, not the source books or PDFium.

Ctrl+K searches the local library. Settings change reflow size, per document and
for new documents in the session. Library/Ctrl+W saves and returns; unfocused
Space resumes the first continuing document. The custom controls retain actual
Windows minimize, maximize, move, resize, and save-before-exit behavior.

Offline formatting, Clippy with warnings denied, and **212 tests** passed.
New boundary regressions cover full-capacity relocation, an existing destination,
center-cropped thumbnails, and keyboard focus ownership when switching surfaces.
Native verification used isolated profiles, an unrelated working directory,
restricted PATH, and actual client pixels at 125% DPI. The reference's nine
titles/covers were placed into test-only documents with generated exercise text,
then imported through the real native picker, not injected into a library JSON.
The 1280 × 838 and 540 × 360 surfaces, sorting, switcher, settings, copy, typed
resume, rejected open, and native frame operations were exercised.
Quick Switcher Locate restored a moved EPUB's chapter, item, fractional offset,
and size exactly, replacing its old library path. Remove preserved the file
and its saved position. The native process then exited normally with code 0.

Visual/runtime checks exposed cover overflow, modal widget-tree replacement
resetting scroll offsets, and a stale shelf action intercepting Space in a reader.
The fixes preserve clipped renderer layers, stable root slots, and content-scoped
focus. Compact traversal reveals the book's title/author rather than only the top
of an oversized cover. Actual before/after saved anchors and screenshots are
retained under `target/portable/design-evidence/`. This is not an independent
clean-host, cold-restart, long-session, or new OLE gesture claim.

The final portable executable is 12,582,912 bytes (12 MiB), SHA-256
`ac967a6262ac4913768226f2120db181784d8a5bbf80eca1ad5f2b6ec2b19ab0`.
The complete folder contains 350 files and is approximately 21 MiB, including
PDFium and third-party notices.

Five isolated 12-second runs used `process-measure`, 250 ms sampling, the default
1280 × 800 logical window, restricted PATH, and an unrelated working directory.
The populated library profile was copied from the actual native import run.
Each row is the median of 21 valid live samples in the final approximately
five seconds; this is one run per scenario, not a comparative benchmark.

| Surface | Private working set | Private commit | Idle CPU-counter increase |
| --- | ---: | ---: | ---: |
| Empty library | 13.05 MiB | 14.15 MiB | 0 |
| Nine-book library | 19.07 MiB | 20.64 MiB | 0 |
| HTML first view | 15.22 MiB | 16.50 MiB | 0 |
| PDF first view | 44.14 MiB | 45.42 MiB | 0 |
| EPUB first view | 14.33 MiB | 16.03 MiB | 0 |

All five collections were valid. Their target exit code 124 is the measurement
tool's intentional duration-limit termination, not the native smoke's graceful
exit. No startup latency or active-frame measurement is inferred from them.
The prior UI measurements used a different window size and documents, so these
numbers do not isolate font or design overhead.

Final evidence: `target/portable/design-evidence/run-06/outcomes.json` and
`screenshots/`, `window-final/` for modal/frame checks, and
`resources-final/<scenario>/{manifest.json,samples.jsonl}` for raw measurements.

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

- [x] Parse ZIP package/spine, navigate chapters and contents, display local images.
- [x] Load chapters on demand and preserve position across font/viewport changes.
- [x] Clearly reject unsupported DRM/fixed-layout books and bound hostile archives.

**Done when:** a multichapter EPUB can be navigated end to end and resumed
without building a second HTML/layout system.

### 4. Portable MVP

- [x] Make opening by dialog, path, and drag/drop consistent across all three formats.
- [x] Finish recent files/resume, keyboard navigation, and missing/moved-file handling.
- [x] Exercise open/read/close/resume on real HTML, PDF, and EPUB files.
- [ ] Run the portable release on a clean Windows machine without the repo or build tools.
- [x] Measure startup, RAM, idle CPU, and full package size; resolve or explicitly
      account for deviations from the targets above.
