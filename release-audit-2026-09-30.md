# Windows release candidate audit — 2026-09-30

The later [local reader review](reader-release-review-2026-09-30.md) records a
subsequent renderer improvement, rebuilt artifact hashes and fresh repetition
checks. This audit retains the baseline build and its evidence.

The current candidate passes the local automated, native-desktop and installer
checks below. The Add Document caption is aligned, library write failures retain
pending changes with an explicit retry, and dictionary indexing does less work.
This is local release evidence; independent-machine, accessibility and sustained
high-DPI frame qualification remain open.

The baseline is the release executable saved before this audit, after the
optional-dictionary implementation on `feature/word-translation` (`c9e5ffd`).
No application release was published during this audit. The setup version
`0.1.5` labels the locally built candidate.

## Corrections

| Finding | Result |
| --- | --- |
| TXT/MD extended the Add Document subtitle beyond its available width; wrapping shifted the fixed-height content. | The button is 280 DIP wide and its adjacent introduction uses the remaining width. Empty/populated libraries were rendered at 540, 640, 767, 768, 900, 1024 and 1280 DIP in both palettes. |
| The empty-library hint omitted TXT/Markdown and the footer implied UTF-8-only input. | Both use accurate, shorter wording. Existing TXT decoding support is unchanged. |
| A failed library/shelves write consumed the dirty snapshot, so pending edits could be forgotten on exit. | Failure retains dirty state, pauses automatic writes and offers Retry save. Retry writes the latest state, including subsequent edits. A failed window close offers retry or explicit Close without saving. Regression tests cover both stores and the close path; there is no automatic failure/retry loop. |
| The title-bar Settings tooltip appeared above the opened modal. | Background chrome tooltips are suppressed during settings, search, removal confirmation and note editing. The native settings screenshot confirms the tooltip is absent. |
| Quick Switcher said Remove only forgot the library entry. | The text now explains that simPl's managed copy is deleted while the original file is kept. |
| Dictionary indexing allocated a temporary field list for every TSV line. | Three fields are read directly from the iterator. Empty/extra fields, ordering and size checks remain in place; dictionary package bytes are unchanged. |

The existing Settings damage and cached-glyph clipping patches were retained.
The production scroll probe confirms incremental painting matches full redraws
and damage remains one region. Their performance limits are quantified below.

## Verification

All profiles, managed copies, dictionaries and screenshots used for QA are in
owned test directories below `target/`. The user's normal library was not used.

| Check | Evidence and result |
| --- | --- |
| Formatting, static analysis, default tests | `scripts/dev.ps1 -Command check -Offline`: formatting and Clippy with warnings denied passed. Both the default build and `cargo test --workspace --all-targets --release --offline --locked` passed **388 tests, 0 failed, 30 opt-in tests skipped per build**. `target/release-audit/check-final.log`, `check-release-final.log`. |
| Additional opt-in tests | **24 distinct opt-in tests passed**: 21 tests selected by 17 focused filters, two additional selection/page tests, and dictionary lookup profiling. Native speech, real PDFium, published downloads and widget renders are included. Filter totals are distinct from workspace defaults. |
| Production state/storage workflows | Real `update` and async tasks import/open TXT, Markdown, HTML, EPUB and PDF; verify source labels, favourites, search, persisted bookmarks, themes, reopen/resume and original bytes; reject a broken EPUB without replacing the active book; test PDF Book/Document switching and extraction restrictions. Final source rerun: `final-workflows.log` and PNGs. Native window and clipboard actions are excluded from this probe. |
| Normal release reader on Windows desktop | `tests/release-smoke.ps1`: native Ctrl+O picker for five source formats, double-click word selection, keyboard-operated dictionary download, downloaded SHA verification, displayed Turkish word meanings, Find, bookmark/sidebar, toolbar hide/restore, next page, Quick Switcher, EPUB contents, PDF page field, narrow-window fit, Settings scroll, return to library and normal exit code 0. Actual desktop DPI: **125%**. All 18 screenshots were reviewed. `native-smoke-3/result.json`. |
| UI layout | Production renders cover empty/populated libraries around the 768-DIP breakpoint, five document formats at 540/900/1280 DIP in light/dark, four reading themes, annotations, shelves, speech controls and dictionary missing/download/progress/error/installed/manager states. Native narrow PDF fits the window; no modal tooltip leaks. |
| Selection and page stress | Authored EPUB multiline whitespace selection at 400/720 DIP passed. Single-page/toolbar/zoom/navigation checks passed on a 16-page authored EPUB and a **2,500-page** generated HTML document. `extra-*.log`. |
| Windows speech | All installed voice smoke checks and three application tests passed at volume zero: selection/page reading, pause/resume/close, paused late PDF extraction, and empty/prose EPUB section transitions. `focused-native_read_aloud.log`, `focused-installed_voices_speak.log`. |
| PDF engine | Authored fixtures passed prose/heading style, columns, links, page-label and fallback reconstruction checks. Production workflows test five source pages and permission-restricted original PDF preservation. `focused-book_preserves_layout_evidence_from_real_pdf.log`. |
| Dictionaries/network | All 13 published ZIPs downloaded over real WinHTTP, matched the pinned catalog and installed with real offline word lookups. UI worker progress and retry of the open card passed. A real 404 and midstream cancellation return no installable bytes; cancellation before connecting remains covered by the default test. `focused-published_.log`. |
| Async/error behavior | Default tests cover stale open/search/PDF/translation replies, bounded caches/search, cancelled conversions and downloads, corrupt state, missing files, source-format preservation, UTF-8 word boundaries, automatic/manual lookup and persistence recovery. |
| Installer | Real isolated install, installed-reader startup, running-reader uninstall refusal, upgrade, retain-data uninstall, reinstall, opt-in data purge, shortcut handling, five-format Open with registration, unchanged existing defaults, registry cleanup and original/junction protection passed. A verified EN→TR ZIP keeps its hash through upgrade/retention/reinstall. The QA 0.1.0→0.1.1 labels use the **same current payload**; this is not an old-binary migration claim. `installer-qa.log`, `target/installer-tests/ba24d9a885dc411998509cee45c28c7f/`. |
| Final packaging | Offline release build, PDFium staging and notices for 190 shipped Rust dependencies passed. Portable ZIP CRC passed; no dictionary TSVs/packs are bundled. Release, portable and installer-payload executable SHA values match the executable exercised on the native desktop. |

The six remaining opt-in tests require particular externally supplied books or
desktop inputs: global-page cross-chapter 400-page checks, hybrid PDF/HTML-folder
checks, PDF blank-page/cache checks, publisher-label checks, a specific long
EPUB chapter and the Alice PDF visual checks. They were not substituted with
unrelated fixtures or reported as passed. Generic equivalents are covered only
to the extent stated above.

The initial direct PowerShell 7 release-test invocation failed three fixture
checksum tests because its Windows PowerShell child could not find the built-in
`Get-FileHash` module. Adding
`$env:SystemRoot\System32\WindowsPowerShell\v1.0\Modules` to the child module
path resolved the harness failure; the complete release suite then passed.
`scripts/dev.ps1` already initializes this path for its normal checks. The first
attempt is retained in `check-release.log`; no product workaround was added.

## Focused performance measurements

These are release-build CPU measurements on this machine, excluding OS
presentation, input-to-display latency and network time unless explicitly stated.

Dictionary profiling installs all 13 source packages in an owned store. Each
direction uses five Store-reopen first lookups and reports their median, then
1,000 warm lookups. Removing per-line temporary allocations reduced first-load
times by a **13.1% median across the 13 directions** in this before/after run.
EN→TR changed from 9.839 to 8.350 ms; Chinese→English changed from 75.870 to
67.341 ms. Current first loads range from 4.839–67.341 ms; warm means are
0.60–0.86 microseconds. This measures core lookup/index work, not the card's
display latency. See `dictionary-timing-{before,after}.log` and
`dictionary-timing-summary.json`.

The bounded search probe over one MiB with many matches took 202/130/102
microseconds (median 0.130 ms). Merging 500 connected highlights took 101
microseconds; deriving paint ranges for 5,000 highlights took 330 microseconds.
These algorithm probes do not measure PDF extraction or full-book UI search.

Settings scroll uses the retained production widget tree and tiny-skia, with
70 frames per scenario, the first ten excluded from timing. The probe compares
incremental pixels to a full redraw at frames 0/20/50. Both runs passed all pixel
and clipping assertions; damage median/max stayed one region.

| Settings scenario | Final CPU frame median | Final p95 |
| --- | ---: | ---: |
| Book background, 100% | 10.98 ms | 13.00 ms |
| Library background, 100% | 6.53 ms | 8.08 ms |
| Book background, 150% | 22.38 ms | 37.29 ms |
| Book background, 200% | 59.99 ms | 72.65 ms |
| Narrow language picker/manager, 150% | 17.43 ms | 21.67 ms |
| Dark book background, 100% | 21.05 ms | 30.63 ms |

The earlier focused run measured 39.12 ms median at 200%, 11.01 ms for the
narrow case and 11.03 ms for dark 100%. That variation is retained rather than
discarding the slower results. The final run was sequential with no other audit
build or desktop driver running. Raster work dominates: layout is below 0.3 ms
p95. **High-DPI sustained 60 FPS is not established and 200% CPU scrolling needs
further optimization.** These offscreen scale tests do not qualify native monitor
transitions. Logs: `focused-profile_settings_scroll.log`,
`final-settings-profile.log`.

## Root-process idle resources

Baseline/current pairs each ran for ten seconds with a separate fresh profile,
250-ms samples and the same authored input. Memory is the median of valid samples
in the 5–10-second window. CPU is the cumulative root-process CPU delta over
that window, expressed as a percentage of one logical core. Measurements use
Windows build 26200.9457, x64, 16 logical processors.

| Authored input | Baseline private commit | Current private commit | Current private working set | Current late CPU |
| --- | ---: | ---: | ---: | ---: |
| Empty library | 16.99 MiB | 16.93 MiB | 15.80 MiB | 0.000% |
| TXT | 18.05 MiB | 18.02 MiB | 16.82 MiB | 0.000% |
| Markdown | 17.25 MiB | 17.14 MiB | 16.01 MiB | 0.000% |
| Structured HTML | 19.02 MiB | 19.32 MiB | 17.40 MiB | 0.000% |
| 3,164,546-byte / 5,000-paragraph HTML | 26.00 MiB | 26.47 MiB | 24.48 MiB | 0.000% |
| Structured EPUB | 19.55 MiB | 19.52 MiB | 17.63 MiB | 0.000% |
| Five-page PDF | 46.03 MiB | 45.67 MiB | 44.21 MiB | 0.000% |

No late root CPU-time increase was observed in either build's runs, and there is
no material memory regression in these samples. These short paired runs do not
prove absence of long-session leaks, real-time loop freedom, helper-process
resource costs or identical results on other machines. Dictionaries were not
indexed during these idle cases. The sampler terminates only its owned process
at the duration limit (recorded exit 124); normal product close is tested by the
separate desktop driver. Every resource manifest reports valid collection and
zero required live query failures. Evidence: `resources/*/{manifest.json,samples.jsonl}`,
`resource-summary.json`.

## Candidate artifacts

Sizes below are bytes; MB is decimal. All files are local, below `target/`.

| Artifact | Bytes | MB |
| --- | ---: | ---: |
| `target/release/iced-shell.exe` | 18,448,384 | 18.45 |
| `target/portable/simPl/` including PDFium/notices | 28,037,330 | 28.04 |
| `target/portable/simPl-release-audit-portable.zip` | 11,861,836 | 11.86 |
| `target/installer/simPl-0.1.5-windows-x64-setup.exe` | 9,792,945 | 9.79 |
| All optional dictionary downloads together | 19,769,702 | 19.77 |

Current executable SHA-256:
`653c2207bcddde453560d93af7ed68daf67ea8e19532ed419247936b3c15f000`.
Baseline SHA-256:
`ede391aa2985f0bd9aaf9aac94edd1dbbf4e6f921146b706919bc2236e9edcee`.
Setup SHA-256:
`7b1da0f07b1606ec4aaff65eb426ef6ec436dad022ed577210ca4d4129b3b6a4`.
Portable ZIP SHA-256:
`2612cc1225bbedd2177f4363b54dcfccd3d8a470c48af3ad627d7d132e9fea5a`.

The setup is unsigned. Signing and public application release publication are
separate from this local candidate. Optional dictionary data already uses the
immutable `dictionaries-v1-2026-09-30` release; the audit does not change its bytes
or catalog hashes. The application embeds the 5,175-byte catalog only, and the
current executable grew by 4,096 bytes during this audit.

## Qualification still open

- Run portable/setup on independent clean Windows 10/11 machines without the
  repository, build tools or cached components; the current installer QA shares
  this development machine.
- Optimize and measure sustained Settings scrolling at high DPI. Verify real
  native 100/150/200% monitors, monitor transitions, displayed frames and input
  latency; offscreen CPU timings and native 125% screenshots cover different things.
- Exercise long reading sessions and repeated document/PDF mode switches with
  memory sampling. Ten-second idle samples and bounded state tests do not prove
  leak-free sessions.
- Validate screen-reader/UI Automation exposure and contrast/accessibility on
  actual assistive tools. Keyboard navigation passed for the workflows stated
  above; full accessibility is not qualified.

Run the tested executable with PowerShell: `.\target\release\iced-shell.exe`.
The repeatable native QA driver and opt-in
state/storage probe are documented in [the reader guide](crates/iced-shell/README.md).
