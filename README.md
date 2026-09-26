# simPl

**Open a file. Read. Return to where you stopped.**

simPl aims to be a small, responsive, offline native reader for local books.
The first target is Windows x64. The focused MVP reads **HTML, then PDF, then EPUB**;
it is not a library platform, cloud service, or browser engine.

## Current state

The Windows reader opens **local UTF-8 HTML/XHTML files**, displays text and
local images, supports selection/copy and font-size changes, and resumes a
document after an explicit reopen. The single UI path is **Iced with the
tiny-skia CPU renderer**; WGPU is no longer the active backend.

The normal application needs no repository fixtures or bundled test fonts.
The 1,000/10,000-paragraph fixture modes remain explicit diagnostics for
virtualization, mixed-script layout, and native selection regressions.

PDF, EPUB, recent files, and a verified portable distribution are not implemented.
The [roadmap](roadmap.md) records the boundaries, measurements, and next PDF slice.

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
# Release build and normal reader; use Open HTML to choose a local file.
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 run

# Optional diagnostics, not the product's document-loading path.
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 run -Fixture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 run -Large

# Format check, workspace Clippy, and workspace tests; no release build.
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 check

# Release build of the active UI crate only.
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 build
```

The script locates and initializes an x64 MSVC environment for its process and
children; it does not install tools or change the global PATH. If detection fails,
use an x64 Native Tools prompt. You can invoke the script by absolute path from
another directory. Initial dependency downloads may need a network connection;
the reader's local reading path does not. Add `-Offline` to any command once the
Cargo cache is populated. The script passes `--locked` to build, run, Clippy,
and tests.

From an initialized x64 Native Tools prompt, direct Cargo commands also work:

```powershell
cargo run --release --locked -- "C:\Books\book.html"
cargo test --workspace --all-targets --locked
```

Without arguments, `target\release\iced-shell.exe` opens the welcome screen.
You can also run the executable with one HTML path, or drop a local file onto
its window. The executable and a document with its relative image directory can
be placed outside the repository; the working directory is not a fixture search
root in normal use. Only `--reader-poc` / `--reader-poc-large` need repository
fixtures; `--shell-poc` opens the old empty diagnostic shell.

## Reading HTML

- **Open HTML / Ctrl+O:** native file picker. **Ctrl+W / Close:** close the document.
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

## Repository layout

| Path | Purpose |
|---|---|
| `crates/iced-shell/` | Windows reader UI and explicit fixture diagnostics |
| `crates/reader-document/` | Small reflow model, HTML extraction, and per-file position storage |
| `crates/reader-workload/` | Fixture generation and reference checks using the shared item types |
| `fixtures/reader-workload/` | Local test fonts, image, text, and licenses |
| `patches/cosmic-text-0.15.0/` | Narrow RTL layout correction, with source and licenses |
| `crates/shell-startup-markers/` | Opt-in startup markers for diagnostic modes |
| `crates/process-measure/` | Optional Windows process RAM/CPU tool, not an app dependency |
| `scripts/dev.ps1` | Run/check/build entry point |

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

### CPU/HTML verification

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
