# Reader comfort follow-up — 2026-09-30

This implements the next product work after the [local reader audit](reader-release-review-2026-09-30.md).
Branch: `feature/reader-release-comfort`, based on `e114930`.
The earlier audit's measurements and feature gaps are historical.

## Listen: visible reading position

Toolbar Listen now outlines and lightly tints the current spoken word. The
existing Windows SAPI word events drive both scroll following and highlighting;
there is no animation timer, polling loop, additional speech engine or download.
UTF-16 speech offsets are mapped back to UTF-8 source text and the actual glyph
boxes. Book paragraphs, segmented text and PDF Document text use their existing
geometry. Selection and saved highlights remain independent.

Pause retains the marker; resume continues it. Stopping Listen, finishing,
closing/changing a book, and entering an asynchronous EPUB chapter load clear
the old marker. Continuous toolbar Listen still crosses pages and chapters,
skipping empty/image-only pages. Context-menu Read aloud remains a bounded
selection/quote operation; its arbitrary text does not drive the continuous
reader-position marker. PDF needs a usable text layer; OCR is not added.

## High-DPI Settings performance

The CPU compositor now omits earlier layers fully covered by an opaque later
background. Bounds are conservative around borders and rounded corners, and
clipping and transparent layers are respected. Integer-aligned solid rectangular
backgrounds use tiny-skia's rectangle path directly. Fractional physical edges
retain the original path: pixel equivalence tests found rounding differences
with a broader shortcut, so that shortcut was restricted.

On this host, the same-turn 200% Settings baseline measured **56.93 ms median /
64.47 ms p95**, before these compositor changes. The final run, including the
new reading and backup controls, measured **14.57 / 21.25 ms** (about 74% less
median CPU frame time). These are production widget build/layout/draw/raster
measurements, excluding the native window's presentation and OS input latency.

| Final Settings scroll case | Median total | p95 total |
| --- | ---: | ---: |
| Book, 100% | 3.90 ms | 4.22 ms |
| Library, 100% | 3.90 ms | 4.11 ms |
| Book, 150% | 8.65 ms | 14.60 ms |
| Book, 200% | 14.57 ms | 21.25 ms |
| Narrow picker, 150% | 6.21 ms | 7.67 ms |
| Dark Book, 100% | 6.65 ms | 8.38 ms |

Each case produced one damage region. Incremental frames were compared against
fresh full redraws at several scroll offsets. Separate renderer tests compare
optimized and original rendering pixel-for-pixel at 100/125/150/200%, including
fractional coordinates, clip masks, transparent overlays and rounded corners.
**This is not a guarantee of 60 FPS at 200%:** the p95 still exceeds 16.7 ms.

## Backups and annotation exports

Settings → Library & data adds ZIP backup/restore, with book files enabled by
default and downloaded dictionaries optional. The size estimate is uncompressed.
Settings, library/history/favourites, custom shelves, positions, covers, per-book
typography, bookmarks, highlights and notes are included. Page/conversion caches
are omitted. Only managed book files inside the profile are bundled; externally
linked sources, or books omitted from the archive, may need Locate on another PC.

Backup streams file contents, records SHA-256/lengths, verifies the completed
archive and atomically writes the destination. Restore checks paths, duplicates,
checksums, bounded sizes and the saved record formats, extracts into a private
staging directory, remaps managed paths and all five position-key formats, then
swaps the profile. The previous profile remains in a `.simPl-before-restore-*`
sibling folder. Omitted book/dictionary directories keep their current local
files. Archives are not encrypted or digitally signed.

The UI reviews the verified archive before **Restore and replace**. Profile
operations require a closed book, completed profile loads and settled save and
dictionary-install queues. Archive/size/export work runs in tasks. Closing the
application waits for active writes. Restore reloads the application state from
the replaced files.

For an open book, Markdown/Text/JSON exports include bookmarks, quotations,
notes and page/chapter labels; JSON also keeps anchors, IDs and timestamps.
The export uses the full book title and does not change the source document.
Limits: 50,000 files, 512 MiB per file, 16 GiB total archive payload.

## Fit width, fullscreen and reading settings

- Select the Book zoom percentage, or press Ctrl+Shift+F, to toggle Fit width.
  It follows window and notes-sidebar changes and restores the preceding zoom
  when toggled off. A 25% minimum accommodates narrow windows with a notes panel.
- F11 toggles fullscreen; Settings also exposes it. Esc dismisses an open panel
  before exiting fullscreen. The preceding maximized/windowed state is restored,
  and hidden header controls are removed from keyboard navigation.
- Settings → Reading independently controls font (theme/Literata/Spectral/Fira
  Sans), size (12–36), line spacing and side margins. Font/spacing/margin choices
  have keyboard-accessible buttons and visible focus; focused controls are
  scrolled into view.
- Changes with a reflowable book open become that book's override, keyed by
  content fingerprint. Library changes set defaults. Use reading defaults
  removes the open book's override. Invalid saved records are preserved and
  reported; save queues coalesce changes and reject stale completions.

The canonical page map and page numbers remain unchanged. Existing cuts are
adapted to the new typography; larger type can make a sheet taller, with vertical
scrolling. This preserves position/bookmark semantics rather than repaginating
on every preference change. PDF typography comes from the document, so those
controls are disabled for PDF Document and PDF Book. Fit and fullscreen still
work there. A zoom event during pending pagination no longer invalidates the
page-map reply and leaves pagination stuck.

Visual review also fixed Settings/Find close symbols that rendered as squares
in the CPU previews, and the narrow Library-back symbol. No font, dictionary or
runtime dependency was added.

## Validation and artifacts

The final debug workspace check passed formatting, Clippy with `-D warnings`,
and **404 tests across 19 suites, zero failures, 36 opt-in tests ignored**.
The final full release workspace run also passed **404 / 0 / 36** across the
same 19 suites. Logs: `reader-comfort-final-check.log` and
`reader-comfort-final-release-tests.log`.
Opt-in checks were selected and run separately: six native speech tests, the
real application portability workflow, three renderer pixel-equivalence tests,
three visual preview generators and the Settings scrolling profile. The patched
renderer also passed its own Clippy check with `-D warnings`.
Evidence logs live in `target/release-audit/reader-comfort-*.log`; generated PNGs
are in `target/tts-previews`, `target/reader-comfort-previews` and
`target/reader-comfort-dpi-previews`. These generated artifacts are not tracked.

Native muted Windows SAPI/PDFium checks passed all **six** continuous playback,
selection, pause/race, document-close and asynchronous EPUB tests. They included
six Book sheets, four EPUB chapters including empty chapters, and PDF text pages
with an intervening blank page. Assertions verify that a PDF spoken marker is
observed and cleared on completion. The Book geometry test verifies the precise
word, retained pause marker, unchanged selection and stop cleanup.

Backup tests cover relocation to a different profile root, all five position
keys, Unicode annotations/shelves, favourites, format metadata, covers, local
resources and per-book typography. Rejected path traversal, reserved names,
case duplicates, corrupt hashes, invalid records and overflowing sizes leave
the existing profile untouched. The opt-in application portability workflow
also exercises real import/save/export/reviewed restore/reload/reopen.

To reproduce the standard checks in PowerShell:

```powershell
$env:PSModulePath = (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\Modules') + ';' + $env:PSModulePath
.\scripts\dev.ps1 -Command check -Offline
cargo test --release --workspace --all-targets --offline --locked
.\scripts\dev.ps1 -Command build -Offline
```

Run opt-in visual/profile tests with their specific filter (not a blanket
`--ignored`, which also selects legacy tests requiring authored fixture paths):
`render_read_aloud_previews`, `render_reading_comfort_previews`,
`render_backup_previews`, `profile_settings_scroll`. The portability test needs
a fresh `SIMPL_PREVIEW_STORE` equal to `LOCALAPPDATA`, with
`SIMPL_PREVIEW_OUTPUT` outside that profile. Native EPUB speech tests additionally
need `SIMPL_TTS_EPUB` pointing to the authored empty/prose/empty/prose fixture.

## Independent Windows qualification remains open

This session uses Windows 11 Pro 10.0.26200 x64, with development tools and the
native speech/PDF runtime. An independent Windows machine or VM was unavailable;
Windows Sandbox was not installed. Native desktop automation was also unavailable.
Production widget renders, update/task workflows and native engine tests were
used; they do not establish clean-machine deployment or real monitor behavior.

Before calling the release independently qualified, check on clean Windows
10/11 without build tools: portable/setup launch and PDF runtime loading;
physical 100/125/150/200% display scaling and movement between mixed-DPI monitors;
Settings scrolling and popup geometry under those transitions; F11/Esc from
both maximized and normal windows; keyboard traversal with Narrator/NVDA;
and a longer mixed-document Listen/read/export/restore session. These are
explicit remaining checks, not completed test claims.

The final offline release build succeeded (`reader-comfort-final-build.log`).
`target/release/iced-shell.exe` is **18,878,976 bytes (18.00 MiB)**, an increase
of **357,888 bytes (0.34 MiB)** over the `e114930` executable. SHA-256:
`794e8971869866ce75688ca508f4e9e3d10eff25a9cff81c84423e90e1d1588f`.
The existing pinned PDFium DLL is staged alongside it. No dependency, bundled
dictionary or translation-model payload was added.

Run the updated local executable with `.\target\release\iced-shell.exe`.
Portable ZIP/setup installers from earlier releases have not been regenerated.
