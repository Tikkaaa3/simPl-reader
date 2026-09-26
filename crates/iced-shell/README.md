# Iced reader

The only active UI path in simPl: a Windows local HTML reader using Iced's
tiny-skia CPU renderer. PDF and EPUB are not implemented. See
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
.\target\release\iced-shell.exe                    # welcome screen / native picker
.\target\release\iced-shell.exe "C:\Books\book.html"
.\target\release\iced-shell.exe --shell-poc        # empty diagnostic shell
.\target\release\iced-shell.exe --reader-poc       # fixture diagnostic
.\target\release\iced-shell.exe --reader-poc-large
```

## Local HTML reading

Normal use needs no repository assets. Open one local file through **Open HTML**,
Ctrl+O, a path argument, or file drop. A canceled picker leaves the current book
alone. Loading/parsing and position I/O run off the UI thread; obsolete open
results cannot replace a newer document. An open failure leaves an existing
document readable.

- HTML5 parsing with html5ever; XHTML is read with HTML5 tree construction, not
  XML validation. Input must be UTF-8.
- Headings, paragraphs, bold/italic/nested bold-italic, lists, link text,
  preformatted whitespace, basic table text, and local PNG/JPEG/GIF/WebP images.
  Missing/blocked assets produce warnings and available alt text.
- System fonts with OS fallback, a fluid viewport, 12–36 px body size, and
  viewport-plus-overscan native layout. Full source items and the height index
  remain O(N); only visible/overscan rows become native text/image widgets.
- Wheel, Page Up/Down, and Space scroll; Ctrl+Home/End jumps to the ends.
  A−/A+ or Ctrl+−/+ changes size; Ctrl+0 resets it. Drag selects; Ctrl+A selects
  all source text; Ctrl+C or Copy copies in logical source order; Escape clears.
- Close/Ctrl+W closes the document; native window close exits. Normal close,
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
and they do not instrument the normal HTML path. Other `ICED_SHELL_*`
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
from current CPU/HTML measurements. Timing diagnostics do not establish
presented-frame percentiles, dropped frames, or input-to-display latency.
