# simPl

**Open a file. Read. Return to where you stopped.**

simPl aims to be a small, responsive, offline native reader for local books.
The first target is Windows x64. The focused MVP reads **HTML, then PDF, then EPUB**;
it is not a library platform, cloud service, or browser engine.

## Current state

**The app cannot open a real book yet.** The working Iced prototype displays a
local fixture with 1,000 or 10,000 paragraphs. It exercises scrolling, layout
limited to the visible area and overscan, and text selection/copy across mixed
writing systems. There is no HTML, PDF, or EPUB file opening, parser, or saved
reading position. The fixture demo is not the MVP.

The [roadmap](roadmap.md) sets out the scope and performance targets. The next
usable slice is opening a real local HTML file, reading it, and returning to the
same position after restarting. Reducing the current renderer's high empty-window
memory use is part of that work.

## Run the prototype

Requirements: Windows x64, Windows PowerShell 5.1+, [rustup](https://rustup.rs/),
Visual Studio C++ Build Tools, and the Windows SDK. `rust-toolchain.toml` pins
the project's Rust version without changing your global default. For a first
installation:

```powershell
rustup toolchain install 1.97.1-x86_64-pc-windows-msvc --profile minimal --component rustfmt,clippy
```

From the repository root:

```powershell
# Release build and 1,000-paragraph fixture demo; does not open a book.
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 run

# The same demo with 10,000 paragraphs.
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
the prototype's reading path does not. Add `-Offline` to any command once the
Cargo cache is populated. The script passes `--locked` to build, run, Clippy,
and tests.

From an initialized x64 Native Tools prompt, direct Cargo commands also work:

```powershell
cargo run --release --locked -- --reader-poc
cargo test -p iced-shell --lib --locked
```

Without arguments, `iced-shell.exe` opens an empty window. In reader mode it
looks for fixture assets in the working directory or its ancestors;
`dev.ps1 run` supplies the repository root. See the
[Iced prototype README](crates/iced-shell/README.md) for controls and limitations.

## Repository layout

| Path | Purpose |
|---|---|
| `crates/iced-shell/` | Active UI prototype; not yet a file reader |
| `crates/reader-workload/` | Fixture/test workload, not the production document model |
| `fixtures/reader-workload/` | Local test fonts, image, text, and licenses |
| `patches/cosmic-text-0.15.0/` | Narrow RTL layout correction, with source and licenses |
| `crates/shell-startup-markers/` | Opt-in startup markers for the prototype |
| `crates/process-measure/` | Optional Windows process RAM/CPU tool, not an app dependency |
| `scripts/dev.ps1` | Run/check/build entry point |

The default Cargo workspace member is `iced-shell`; `--workspace` explicitly
includes the other packages. UI text and repository prose are English. Unicode
correctness tests may deliberately contain Arabic, Hebrew, CJK, or combining
characters; they are test data, not translated UI or Turkish fixture samples.

## Verification and measured limitations

After a release build, a focused interactive prototype check is available:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File crates\iced-shell\tests\reader-poc.ps1
```

It needs an unobscured, interactive desktop and controls focus and the pointer.
The separate [process-measure](crates/process-measure/README.md) tool samples
root-process RAM and CPU; it does not measure GPU memory or first displayed frame.

**The current prototype does not meet the lightweight memory target.** On
2026-09-26, single short runs on one Windows machine measured approximately
158.28 MiB empty-window and 173.29 MiB 1,000-paragraph private working set
before cleanup. The old release executable was 13.56 MiB; a subsequent release
executable was 11.23 MiB, while another single empty-window run still measured
158.46 MiB. Executable size is not distributable package size. These are dated
observations of a fixture prototype, not a reader benchmark or evidence that the
RAM issue was fixed. More conditions and targets are in the roadmap. The original
measurement outputs under `target/` were untracked machine-local artifacts, not
committed or downloadable evidence.

### English snapshot verification (2026-09-26)

The workspace format check, Clippy, Rust tests, release build, and Python
interaction-analyzer regressions passed. The real-window reader driver checked
the English UI, scrolling, resizing, and missing/corrupt assets. The native
selection driver matched clipboard goldens, including selection across the
translated opening paragraph, row eviction/re-entry, and the 10,000-paragraph
mode. Generated screenshots and run logs remain local under `target/`.
