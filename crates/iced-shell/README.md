# Iced reader prototype

The only active UI path in simPl. **This is still a fixture demo, not a file
reader.** It does not open PDF, EPUB or HTML files. Product direction and next
work are in [the roadmap](../../roadmap.md); this document explains the retained
prototype and its diagnostics, not a framework approval process.

## Run

From the repository root (Windows PowerShell 5.1+):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 run
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 run -Large
```

The helper initializes a local x64 MSVC environment, builds release, and runs
`--reader-poc` (1,000 body paragraphs) or `--reader-poc-large` (10,000). The
fixture has additional headings and an image. Direct commands from an x64
Native Tools prompt:

```powershell
cargo build -p iced-shell --release --locked
.\target\release\iced-shell.exe                    # empty shell
.\target\release\iced-shell.exe --reader-poc       # fixture demo
.\target\release\iced-shell.exe --reader-poc-large
```

Empty mode needs no adjacent files. Reader mode resolves
`fixtures/reader-workload/manifest.txt` from the current directory or an ancestor,
checks the local font/image assets, and reports missing/corrupt assets in the
window. It deliberately delays fixture loading to expose the empty shell first;
this is experimental staging, **not** the intended product startup behavior.
No network, telemetry, persistence or application-owned continuous render loop.

## Existing behavior worth reusing

- Dark, resizable native window; `Info` toggles an information panel, F1 opens
  it, Escape hides it, Tab/Shift+Tab traverse controls. `Exit` or native close exits.
- Reader width switches between 800 and 480 DIP; viewport height is capped at
  600 DIP by the old shared fixture recipe, not a future reader UX requirement.
- Viewport-plus-overscan row construction and layout for 1k/10k workloads.
  Full source content and a compact height index are still O(N). Counters do
  not prove that private Iced/Cosmic/GPU caches are bounded.
- Native-hit-tested, document-spanning text selection and Ctrl+C; selection
  survives visible-row eviction and width/resize changes in the retained cases.
- Reader-only F5 unloads/reloads the fixture. Font registration may survive;
  this does not prove full memory reclamation after a real document closes.

## Focused verification

```powershell
cargo test -p iced-shell --lib --locked
powershell -NoProfile -ExecutionPolicy Bypass -File crates\iced-shell\tests\reader-poc.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File crates\iced-shell\tests\virtual-reader.ps1 -EvidenceDirectory target\virtual-fresh
powershell -NoProfile -ExecutionPolicy Bypass -File crates\iced-shell\tests\selection-copy.ps1 -EvidenceDirectory target\selection-fresh
```

Build release first. Interactive drivers require an idle, unobscured Windows
desktop, move the pointer/focus and own the processes they launch. Selection
verification overwrites the clipboard. Use fresh output directories. Run the
scenario relevant to the change; these are not mandatory sequential gates.

Additional tools: `tests/native-input.ps1` for shell input/lifecycle;
`tests/runtime-evidence.ps1` for adapter/DPI/UI Automation observations;
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

`ICED_SHELL_STARTUP_MARKERS=<path>` emits bounded QPC markers;
`ICED_SHELL_INTERACTION_TRACE=<new-path>` records optional reader callbacks.
These are **not** presented-frame timestamps or startup-budget proof. Other
`ICED_SHELL_*` test-status/adapter/BiDi variables are explicit diagnostics; leave
them absent in ordinary use and resource measurements. Adapter logging itself
causes extra enumeration and cannot stand in for a timed process's GPU identity.

Current features use Iced 0.14/WGPU only, PNG decoding, advanced widgets and the
thread-pool executor; no tiny-skia or unconditional-rendering feature is enabled.
Framework-owned worker/GPU/font costs still exist. `Cargo.toml` and `Cargo.lock`
are authoritative; `dependency-inventory.txt` is a historical license snapshot,
not an automatically refreshed current graph.

The existing UI Automation observation found no client control descendants.
Keyboard operation is not screen-reader support. Cross-DPI/multi-monitor and
other-host behavior are unverified. The root roadmap records the reset's short
resource baseline: the current WGPU shell is **over the product RAM target**.
The timing diagnostics do not establish presented-frame percentiles, dropped
frames or input-to-display latency.
