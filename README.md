# simPl

**Open a file. Read. Return to where you stopped.**

simPl aims to be a small, responsive, offline native reader for local books.
The first target is Windows x64. The focused MVP reads **HTML, then PDF, then EPUB**;
it is not a library platform, cloud service, or browser engine.

## Current state

The Windows reader opens **local UTF-8 HTML/XHTML, PDF, and reflowable EPUB 2/3
files**, supports selection/copy, and resumes an unchanged document after an
explicit reopen. HTML and EPUB reflow with font and viewport changes; PDFs retain
their page layout and use zoom or fit-width. The single UI path is **Iced with
the tiny-skia CPU renderer**; WGPU is not the active backend.

The normal application needs no repository fixtures or bundled test fonts.
The 1,000/10,000-paragraph fixture modes remain explicit diagnostics for
virtualization, mixed-script layout, and native selection regressions.

Recent files are not implemented. A portable-folder build is available;
clean-machine qualification is still pending. The [roadmap](roadmap.md) records
the boundaries, measurements, and remaining work.

## Run the reader

Requirements: Windows x64, Windows PowerShell 5.1+, [rustup](https://rustup.rs/),
Visual Studio C++ Build Tools, and the Windows SDK. `rust-toolchain.toml` pins
the project's Rust version without changing your global default. For a first
installation:

```powershell
rustup toolchain install 1.97.1-x86_64-pc-windows-msvc --profile minimal --component rustfmt,clippy
```

From the repository root:

```powershell
# Release build and normal reader; Open chooses a local HTML, PDF or EPUB file.
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 run

# Optional diagnostics, not the product's document-loading path.
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 run -Fixture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 run -Large

# Format check, workspace Clippy, and workspace tests; no release build.
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 check

# Release build of the active UI crate and staging of the native PDF runtime.
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 build

# Complete portable folder: target\portable\simPl\simPl.exe and its dependencies.
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\package.ps1
```

The script locates and initializes an x64 MSVC environment for its process and
children; it does not install tools or change the global PATH. If detection fails,
use an x64 Native Tools prompt. You can invoke the script by absolute path from
another directory. Initial Cargo, pinned PDFium, and license downloads need a
network connection; the reader's local reading path does not. Add `-Offline`
after those caches are populated. Offline setup/packaging fails explicitly on
missing cached inputs rather than downloading or omitting notices. The script
passes `--locked` to build, run, Clippy, and tests.

From an initialized x64 Native Tools prompt, direct Cargo commands also work:

```powershell
cargo run --release --locked -- "C:\Books\book.html"
# Direct Cargo does not fetch PDFium; this stages it beside the release binary.
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\pdfium.ps1
.\target\release\iced-shell.exe "C:\Books\book.pdf"
.\target\release\iced-shell.exe "C:\Books\book.epub"
cargo test --workspace --all-targets --locked
```

Without arguments, `target\release\iced-shell.exe` opens the welcome screen.
You can also run the executable with one HTML, PDF or EPUB path, or drop a local
file onto its window. Keep `pdfium.dll` beside the executable for PDF support;
HTML, EPUB and the welcome screen do not load it. The portable folder works from
another working directory without repository fixtures. Only `--reader-poc` /
`--reader-poc-large` need repository fixtures; `--shell-poc` opens the old empty
diagnostic shell.

## Reading HTML

- **Open / Ctrl+O:** native file picker. **Ctrl+W / Close:** close the document.
- Scroll with the wheel, Page Up/Down, or Space; Ctrl+Home/End jumps to the ends.
- **A− / A+** or Ctrl+−/+ changes font size; Ctrl+0 resets it. Window resizing reflows text.
- Drag to select, Ctrl+A selects document text, and Ctrl+C copies in source order.
  Escape clears selection.
- Normal close, exit, or file replacement saves the content item, intra-item
  fraction, and font size under `%LOCALAPPDATA%\simPl\positions\`. Reopening the
  same unchanged file restores that location. Changed source bytes invalidate
  the old position; files are not silently reopened at startup.

This is a reading view, not a browser. It extracts headings, paragraphs, lists,
bold/italic text, link text, preformatted whitespace, and basic table text.
PNG/JPEG/GIF/WebP images must be relative local files inside the document's
directory. Missing/blocked images show warnings and available alt text. JavaScript,
remote resources, CSS layout, interactive forms, and link navigation are not
implemented. Only UTF-8 input is supported. See the
[reader and diagnostics README](crates/iced-shell/README.md) for limits and controls.

## Reading PDF

- Continuous original-layout pages, **Prev/Next**, an editable page number
  (Enter to jump), wheel/Page Up/Page Down/Space, and Ctrl+Home/End.
- **− / +** or Ctrl+−/+ zooms; **100% / Ctrl+0** resets zoom; **Fit width**
  fits each page to the reading area.
- Drag over actual PDF text to select it. Ctrl+A selects document text;
  **Copy / Ctrl+C** copies it. Selection remains source-relative through
  scrolling, page-cache eviction, and zoom. Focused page-number input keeps
  its own text-editing shortcuts.
- Scanned/image-only pages are displayed, but no text is invented and no OCR
  is performed. Copy restrictions are respected. PDFium supplies extraction
  order; arbitrary multi-column/BiDi reading order is not guaranteed.
- Close, exit, or replacement saves page, page-relative offset, horizontal
  position, and zoom in separate `*.pdf.json` records in the same position
  directory. Reopening the unchanged source restores them.

PDFium is loaded lazily from the executable's directory and runs on a serial
background worker. The application page-raster/text cache is capped at 32 MiB;
PDFium, renderer, and transient allocations are additional. Source/page/raster
and copy limits are documented in the [reader README](crates/iced-shell/README.md).
Password-protected files requiring a password are rejected. Interactive forms,
annotation rendering, JavaScript, external actions, and OCR are not implemented.
The native engine runs **in-process, not in a sandbox**.

`scripts/package.ps1` assembles the executable, pinned non-V8 Windows x64
PDFium DLL, and third-party notices. Windows builds statically link the MSVC
runtime instead of requiring a separate VC++ redistributable. PDFium's archive
and SHA-256 are pinned in `scripts/pdfium.ps1`; Cargo versions are locked.
Notices retain the binary distributor's license, the PDFium/native notices,
Rust dependency licenses, and standard-library notices. The application's own
license has not been chosen; no open-source license for simPl is asserted.
Upstream sources: [pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)
and [pdfium-render](https://github.com/ajrcarey/pdfium-render).

## Reading EPUB

- Open DRM-free reflowable EPUB 2/3 books through the same picker, path argument,
  or file-drop path. ZIP members stay in the archive; nothing is extracted to disk.
- **Prev chapter / Next chapter** or **Ctrl+Page Up / Ctrl+Page Down** follows
  the linear spine order. Chapters load on demand into the existing HTML view.
- **Contents / Ctrl+T** opens the nested EPUB3 navigation or EPUB2 NCX contents.
  Entries can target a specific heading/paragraph inside a chapter. Escape
  closes contents. Books without a contents document use the spine list.
- Font controls, scrolling and mouse selection work as for HTML.
  **Ctrl+A and Ctrl+C apply to the current chapter**, not the entire book.
- Close, exit, replacement and chapter changes save the chapter href, content
  item, intra-item fraction and font size in a separate `*.epub.json` record.
  Reopening unchanged source restores that location after viewport changes.

Publisher stylesheets/custom fonts, general link/footnote navigation, non-linear
auxiliary sections, DRM and fixed-layout EPUB are not supported. The shared HTML
extractor displays local raster images, including raster references inside SVG
cover wrappers; it does not render SVG vector artwork. Scripts, remote resources
and external XML entities are not loaded. UTF-8 XML/XHTML is required.
See the [reader README](crates/iced-shell/README.md) for archive and decoding bounds.

## Repository layout

| Path | Purpose |
|---|---|
| `crates/iced-shell/` | Windows reader UI and explicit fixture diagnostics |
| `crates/reader-document/` | Shared HTML extraction, bounded EPUB packages, and per-file position storage |
| `crates/reader-pdf/` | Lazy, serial PDFium worker, bounded rasters, text geometry, and copy |
| `crates/reader-workload/` | Fixture generation and reference checks using the shared item types |
| `fixtures/reader-workload/` | Local test fonts, image, text, and licenses |
| `patches/cosmic-text-0.15.0/` | Narrow RTL layout correction, with source and licenses |
| `crates/shell-startup-markers/` | Opt-in startup markers for diagnostic modes |
| `crates/process-measure/` | Optional Windows process RAM/CPU tool, not an app dependency |
| `scripts/dev.ps1` | Run/check/build entry point |
| `scripts/pdfium.ps1`, `scripts/package.ps1` | Verified native-runtime setup and portable assembly |

The default Cargo workspace member is `iced-shell`; `--workspace` explicitly
includes the other packages. UI text and repository prose are English. Unicode
correctness tests may deliberately contain Arabic, Hebrew, CJK, or combining
characters; they are test data, not translated UI or Turkish fixture samples.

## Verification and measured limitations

After a release build, a focused interactive fixture check is available:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File crates\iced-shell\tests\reader-poc.ps1
```

It needs an unobscured, interactive desktop and controls focus and the pointer.
The separate [process-measure](crates/process-measure/README.md) tool samples
root-process RAM and CPU; it does not measure GPU memory or first displayed frame.

The CPU cutover removes the old WGPU path and the artificial demo loading delay.
Current and historical resource observations are distinguished in the
[roadmap](roadmap.md). Executable size is not distributable package size;
process RAM does not include all OS/compositor memory.

### Earlier CPU/HTML verification

Workspace formatting, Clippy, 176 Rust tests, and the release build passed.
A copied executable was exercised from a temporary directory outside the repo:
HTML and local image display, native mouse selection with exact clipboard text,
file-picker cancellation/reopening, font and viewport reflow, wheel scrolling
after reflow, close/reopen at the same content location, missing/remote image
warnings, and corrupt-input errors. Mixed Arabic/Hebrew/Japanese text was
displayed and copied in source order. The native CPU selection and virtualization
drivers also passed for 1,000/10,000 paragraphs; screenshots were inspected,
including RTL text after row eviction and re-entry.

Screenshots, smoke records, and measurements under `target/` are untracked
machine-local evidence, not committed or downloadable artifacts. This is not
clean-machine packaging, broad font/Unicode conformance, or cold-start proof.

### PDF slice verification

Workspace formatting, Clippy with warnings denied, **178 Rust tests**, and the
static-CRT release build passed. Offline packaging produced a **17.49 MiB**
folder (18,335,781 bytes): executable, PDFium DLL, and complete dependency notices;
no fixture fonts or books. PE imports require no separate VC++ redistributable.

The complete folder was copied outside the repository and launched from an
unrelated working directory at 125% Windows display scaling. Native desktop
checks covered lazy loading, a Unicode picker path, visible forward/reverse
selection with exact clipboard text, full-document copy against an independent
PDFium extraction oracle, focused page-input shortcuts, zoom/fit-width and
position restoration after resizing, crop/rotation, image-only pages, corrupt
input retaining the current document, HTML/PDF switching, 5,000-page navigation,
and a real 756-page Adobe PDF. Selection survived page-cache eviction and zoom;
stationary-pointer wheel dragging selected across pages, and closing canceled
pending copy without a late clipboard overwrite. Screenshots were inspected.
The real Adobe PDF also resumed page 100 at 125% zoom after closing and resizing.
A missing adjacent DLL left HTML usable and did not load a DLL placed in the
working directory.

Eighteen resource collections (six scenarios, three runs each) observed zero
idle CPU-time increase in their final approximately five-second windows.
The real PDF's first reading view used **35.71 MiB private working set /
37.32 MiB private commit**. See the [roadmap](roadmap.md) for every scenario,
method, executable hash, and evidence locations. These results support keeping
PDFium; they do not establish cold/warm startup budgets, long-session memory
behavior, arbitrary PDF fidelity, or clean-machine compatibility.

### EPUB slice verification

The final workspace format/Clippy check and **190 Rust tests** passed. One earlier
run encountered a PID-publication timeout in the existing native sampler fault
test; its isolated run and the final full workspace run passed without changing
the sampler. Offline packaging included notices for **176 shipped Rust
dependencies**, PDFium/native components, and the Rust standard library.

Native desktop verification ran the copied complete folder outside the repo at
125% display scaling. It covered a Unicode picker path, local sibling images,
visible mouse selection and exact chapter clipboard text, spine order differing
from manifest order, EPUB3 nested contents and EPUB2 NCX fragment targets,
font/viewport changes, wheel scrolling, chapter/item/fraction resume, and
corrupt/DRM/fixed-layout/traversal errors retaining the current chapter and location.
HTML/PDF switching also passed. PDFium stayed unloaded during EPUB-only use.
The failed-open smoke caught a native scroll-widget reset while the document model
was retained; restoring the widget offset fixed it. The final run checked the
saved item/fraction after each rejected file and visually retained the same text.

The real [Project Gutenberg Alice EPUB](https://www.gutenberg.org/ebooks/11)
displayed its SVG-wrapped raster cover, traversed all 15 reading-order sections,
copied chapter III text, and restored its location and 20 px font after resizing.
Screenshots of selection, exact contents targets, cover and restored text were
inspected. This is not general EPUB/CSS/SVG conformance, accessibility validation,
or clean-machine/startup-latency proof. Measurements and evidence are recorded in
the [roadmap](roadmap.md).
