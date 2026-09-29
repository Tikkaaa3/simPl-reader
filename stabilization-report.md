# simPl 0.1.1 consolidation — 2026-09-29

Work continues on `feature/tts`. The audit covered the normal Windows reader,
search/index jobs, pagination, PDF rendering/cache requests, speech progression,
highlight recovery/painting, local persistence, responsive controls and packaging.
The baseline is the release executable copied before this audit, with the existing
TTS work already present; it is not a comparison with the earlier 0.1.0 installer.

## Corrections

- Page access borrows the cached page slice instead of cloning every page on
  frequent input, view, title and read-aloud paths. Current-page lookup uses binary
  search, with edge/gap behavior checked against the previous linear lookup.
- EPUB contents are borrowed and HTML headings are cached once per loaded book.
  Titles no longer repeatedly clone the table of contents or recompute the anchor
  for every heading. PDF Book illustration presence is precomputed per source page.
- Search folds text incrementally, uses linear pattern matching and stops at
  1,000 results. Grapheme snapping advances through each paragraph once. Unicode
  case expansion, whitespace folding, source byte ranges and PDF glyph indices
  remain covered by tests.
- Queries covering at least 64 KiB run on the background executor. New queries,
  closing find and changing books cancel obsolete jobs. Generation checks reject
  stale index/search results before they can clear a newer job or replace matches.
  Retained EPUB/PDF index text is capped at 64 MiB, excluding item/index metadata;
  the UI explains partial coverage. Native chapter parsing already in progress may
  finish before cancellation is observed.
- A PDF text-layer error no longer suppresses raster rendering of the same page.
  Failed text requests are remembered separately, preventing repeated requests
  while the reader stays on that page.
- Recovering a short saved quote retains only the necessary candidate information.
  Connected highlight merging and equal-color painting ranges use sorted intervals.
  Retained annotation IDs, note counts, transitive overlaps and color layer order
  are checked against the previous rules on 300 generated overlap scenarios. Annotation
  storage indexes IDs once and clones only the retained record, avoiding repeated
  full-list searches and copies when applying a large merge.
- Book toolbars use two rows below 900 pixels. Toolbar control bounds are checked
  at 540, 780, 899, 900, 1050 and 1280 pixels. Settings reserve scrollbar space so
  controls and explanatory text do not run under it.
- Read-aloud uses Listen to start/stop, with pause/resume retained. The selection
  menu reads the selection; late PDF replies cannot replace a restarted session;
  pending selection extraction preserves pause; empty/failed EPUB chapters finish
  or advance correctly. COM initialization is balanced after releasing interfaces.
- Measurement manifest writes tolerate brief Windows locks and clean up their own
  failed temporary file, allowing finalization/retry to proceed. Windows PowerShell
  checks restore their built-in module path when launched from PowerShell 7.
- The library import hint includes TXT/Markdown. Roadmaps distinguish implemented
  themes/shelves/speech from per-book appearance, free-form tags and dictionaries.
  Installer defaults and documentation now identify the patch release as 0.1.1.

## Focused timings

Debug test build, same host; times measure the algorithms, not GUI response or
release frame latency. Search reports the median of three runs; the highlight
cases have one before/after timing each.

| Authored stress case | Before | After |
| --- | ---: | ---: |
| Search a 1 MiB paragraph for `a`, bounded to 1,000 matches | 278 ms | 1.11 ms |
| Merge a backwards-connected chain of 500 highlights | 285 ms | 0.58 ms |
| Prepare 5,000 disjoint same-color painting ranges | 276 ms | 0.72 ms |

Raw logs stay under `target/stabilization/`: `search-before.log`,
`search-after.log`, `highlights-before.log` and `highlights-after.log`.

## Release resource samples

One 10-second run per case/build, sampled every 250 ms on Windows build
26200.9457, x64, 16 logical processors. Each run uses its own local profile.
The after build is the final portable reader shipped with the 0.1.1 installer. Builds and installer work were
not running during sampling. Values below are median private commit in the
5–10 second window; CPU uses cumulative root-process time over that window.

| Authored input | Before private commit | After private commit | After CPU, one core |
| --- | ---: | ---: | ---: |
| Empty library | 17.25 MiB | 16.86 MiB | 0.000% |
| Structured HTML | 18.92 MiB | 18.83 MiB | 0.000% |
| 3.16 MB HTML: 5,000 paragraphs, 50 headings | 26.10 MiB | 26.71 MiB | 0.000% |
| Four-chapter EPUB | 19.00 MiB | 18.65 MiB | 0.000% |
| Five-page prose/illustration PDF | 45.32 MiB | 45.76 MiB | 0.000% |

All ten collections completed successfully. The small memory differences are
single-run variation; these measurements do not establish a memory improvement.
No root CPU-time increase was measured in the after runs' late sampling windows.
Helper processes are not aggregated. This is a short late-window resource check,
not startup, active speech, gesture latency or sustained frame-time measurement.
Manifests and samples stay in `target/stabilization/measurements`; the summary
is `target/stabilization/resource-summary.json`.

## Validation

- `scripts/dev.ps1 -Command check`: formatting, workspace Clippy with warnings
  denied and all targets' default parallel tests passed: **363 passed, 21 ignored**.
- Four opt-in native speech tests passed with SAPI volume zero, using Microsoft
  Tolga (OneCore, Turkish) and Zira Desktop (classic, English). They cover selection,
  page reading, pause/resume, closing, pending PDF selection and EPUB transitions.
- Production widgets were rendered at 1280 and 540 pixels; player and settings
  previews were inspected. This uses tiny-skia directly and injects no OS input.
- The pre-audit native HTML window exercised Ctrl+F, text entry and exact selection
  highlighting. Computer Use was stopped with physical Escape; further UI input
  stopped. A complete live interaction pass of the final build remains open.
- Isolated installer QA passed installation, 0.1.0→0.1.1 upgrade, shortcuts,
  PDF/HTML/EPUB/TXT/MD associations and cleanup, unchanged user defaults, keep-data
  uninstall, reinstall, explicit purge and original-file/junction protection.
  Installed-reader launch and the running guard were explicitly skipped.
- The portable package is rebuilt from release sources, with PDFium and notices
  for 189 shipped Rust dependencies plus the bundled font notices.

## Release qualification still open

- Independent clean Windows 10/11 installation, cross-DPI interaction and sustained
  scroll/selection/zoom frame measurements. Short resource samples do not qualify
  these behaviors or establish cold-start-to-readable time.
- Screen-reader support: the sampled native window exposed only a window-level
  accessibility node. Keyboard traversal alone does not establish accessibility.
- The interactive installer wizard and the installed-reader running guard need
  a desktop pass when UI control is available again. The isolated installer QA
  run in this session uses `-SkipReaderLaunch` and must not be described as covering
  those two checks.
- Public release signing and publication are separate from this local build.
  The locally generated installer is unsigned.

Follow-up product work: profile backup/restore and annotation export, an offline
user-supplied dictionary with clear licensing, then per-book reading appearance.
