# simPl local reader release review and comparison

> Historical release audit. The later [reader comfort follow-up](reader-comfort-review-2026-09-30.md)
> supersedes the gaps below for word highlighting, DPI painting cost, backups,
> note export, Book Fit width, fullscreen and per-book typography. Independent
> clean Windows, multi-monitor and screen-reader qualification remains open.

Reviewed on 30 September 2026, on branch `feature/word-translation`.

simPl has the core features needed for its Windows local-reader MVP. The current
build passes the checks described below and is a reasonable beta candidate.
The remaining release qualification is independent clean-Windows testing,
native multi-monitor/DPI behavior and accessibility. Settings rendering at 200%
DPI remains slower than a 60 FPS target. This review does not establish a
defect-free application or rank competitors by performance.

The [earlier release audit](release-audit-2026-09-30.md) is the baseline for storage
recovery, importer/layout corrections, all thirteen dictionary downloads and
installer retention/deletion checks. This review adds a renderer optimization,
fresh checks on the resulting executable, repeated desktop workflows and current
official-source comparisons. Code signing remains deferred.

The executable/package hashes and performance measurements below describe the
release-review baseline before the subsequent continuous Listen/scroll-follow
change. The feature table incorporates that follow-up; its checks are described
in the shell's [speech QA instructions](crates/iced-shell/README.md).

## What is ready

| Local reader need | Current behavior and important boundary |
| --- | --- |
| Open local documents | PDF, reflowable EPUB 2/3, HTML/XHTML, TXT and Markdown; picker, path argument and drag/drop. Library labels preserve TXT/Markdown despite their private HTML conversion. DRM and fixed-layout EPUB are rejected. |
| Resume and organize | Managed copies, original-file preservation, saved location/mode, Continue Reading, favourites, custom shelves, sorting/filtering and title/author search. Source edits are not automatically refreshed. |
| Read comfortably | Four bundled themes, light/dark, zoom, original PDF Document view and reconstructed PDF Book view, stable global/source page numbering, a collapsible toolbar and keyboard navigation. Book scrolling stays within the current page. EPUB/HTML use our extracted structure, not faithful publisher CSS; tables/MathML have text/source fallbacks. |
| Navigate and find | Contents, chapter/page navigation, internal links/footnotes and return to passage; asynchronous EPUB/PDF search with cancellation, a 64 MiB text-index budget and a 1,000-match cap. |
| Annotate | Bookmarks, coloured highlights, notes, sidebar navigation and per-book sidecars with location recovery. There is no annotation export UI or embedding of our annotations in PDF/EPUB files. |
| Listen offline | Installed Windows SAPI voices; toolbar Listen continues from the current page to the book's end regardless of selection, with spoken-line scroll following and asynchronous chapter transitions. Context-menu Read aloud reads the selection/highlight only. Listen toggle, pause/resume and saved voice/rate. Transient word highlighting is now available; neural voices are not bundled. |
| Look up words offline | Automatic double-click/short-selection card or manual right-click Translate; saved source/target and automatic setting; explicit download, progress, cancellation, retry, removal and verified ZIP import. Selected book text is not uploaded. |
| Keep optional data optional | Thirteen dictionary directions: EN ↔ TR/ES/DE/FR/JA/ZH, and KO → EN. All ZIPs total 19.77 MB; only the 5,175-byte catalog is embedded. Packs work offline after installation. This is dictionary lookup, not general sentence translation. |
| Install locally | Per-user setup, portable folder, native PDFium beside the executable, required notices, upgrade retention and uninstall keep/delete choice. The current artifacts are unsigned and have not been published as an app release. |

The product does not need an account or a background service. Ordinary reading,
TTS and installed dictionary lookup work without a network. Optional dictionary
downloads require a connection; verified package import provides an offline route.

## Code and behavior review

The normal product has one native Iced CPU-rendered UI path. Workload fixtures,
process-measurement tools and startup markers are explicit diagnostics, not
normal-reader prerequisites. Removing them would discard useful regression
coverage without improving ordinary reading.

The reviewed hot paths retain virtualized library/paragraph windows, on-demand
chapter data, bounded PDF raster/conversion work and cancellation/generation
guards for open, search, speech and dictionary replies. Loading animation ticks
exist only during opening/pagination; speech ticks exist only while needed.
The repeated native runs measure idle CPU rather than inferring it from source.

One concrete improvement was made in the patched tiny-skia engine: adjacent
identical clips now reuse their mask within a draw instead of clearing and
filling the viewport mask repeatedly. Each draw resets the cache, and changed
bounds rebuild it. Regression coverage includes fractional clips, repeated
bounds, shrink/expand and replacement masks. Production scroll probes still
compare incremental painting against full redraws at multiple DPI scales.
See [the patch explanation](patches/iced_tiny_skia-0.14.1/SIMPL-PATCH.md).

Previous library/shelf write-failure recovery remains covered: dirty state is
retained, Retry saves the latest state, and failed writes cannot create a retry
loop. Settings and recent-history failures show notices but have less complete
explicit retry UX; they deserve a follow-up. This review does not claim every
filesystem failure has identical recovery behavior.

README installer naming and roadmap summaries were updated to avoid describing
implemented dictionary support as future work or treating all desktop checks as
still pending. Historical milestones and measurements remain historical.

## Fresh verification

| Check | Result / scope |
| --- | --- |
| Formatting and Clippy | Passed, with Clippy warnings denied, all workspace targets. Vendored engine formatting checked separately. |
| Default debug and release suites | 388 passed and 30 opt-in tests ignored in each configuration; no failures. The separate renderer regression also passed. |
| Production format workflows | Passed again on the changed renderer: all five formats, source preservation, source labels, find, favourites, bookmarks, themes, resume, corrupt-open recovery and restricted PDF fallback. Production widgets rendered at 540/900/1280 DIP in light/dark. |
| Settings scroll probes | Three release-profile probe runs (two cached, one uncached); six scenarios each; incremental/full pixels agree and damage stays one region. The uncached run provides a nearby timing comparison. |
| Normal desktop reader | Fresh isolated profile, native file picker, actual word selection → download → Turkish result, five imports, Find, bookmark/sidebar, F8, page jumps, EPUB contents, narrow PDF, Settings wheel scrolling and normal exit. Native desktop scale: 125%. |
| Repeated desktop session | Separate six- and twenty-cycle runs added 30 and 100 open/page-turn/close sequences across five formats. No duplicate library entries; both normal exits passed. Resources are recorded below. |
| Installer lifecycle | Passed again with the current payload: installed-reader launch/running guard, associations and unchanged defaults, shortcuts, upgrade, retain/reinstall, dictionary retention, profile deletion and original/junction protection. QA uses its own AppId/profile and 0.1.0 → 0.1.1 version labels with the same payload; this is not an old-binary migration test. |
| Packages | Release executable, portable executable and installer payload have the same SHA-256. Current portable/setup artifacts rebuilt locally; no app release uploaded. |

New evidence is under `target/release-audit/`: `final-review-check-after.log`,
`final-review-release-tests.log`, `clip-mask-regression-final.log`,
`clip-cache-{probe,baseline,final}.log`, `final-review-workflows.log` and its
rendered previews, `final-review-native/result.json`, and
`final-review-repeat/result.json`; `final-review-installer-qa.log` links the
installer evidence directory. These disposable profiles contain authored fixtures, not the
user's library. Ignored external-book tests and multi-hour endurance are not
included in the pass count.

## Performance and UI findings

Settings measurements use 70 retained production-widget frames per scenario,
excluding ten warmup frames. They include widget construction/layout, draw
commands and CPU rasterization, but exclude OS input/display latency. The nearby
uncached/cached comparison is one pair on this machine, not a universal speedup.

| Settings scenario | Uncached median / p95 | Cached median / p95 |
| --- | ---: | ---: |
| Book, 100% DPI | 11.28 / 14.94 ms | 10.83 / 11.75 ms |
| Library, 100% DPI | 6.64 / 7.69 ms | 6.45 / 6.79 ms |
| Book, 150% DPI | 23.25 / 32.77 ms | 21.79 / 24.77 ms |
| Book, 200% DPI | 41.23 / 52.11 ms | 42.20 / 52.28 ms |
| Narrow picker, 150% DPI | 11.51 / 14.02 ms | 11.00 / 11.86 ms |
| Dark Book, 100% DPI | 11.52 / 13.98 ms | 11.14 / 13.20 ms |

The first cached probe measured 36.37 / 42.26 ms at 200%; the subsequent run above
was slower. This variability reinforces the qualification boundary. The cache
reduces redundant work, but **does not solve high-DPI raster cost**. Layout in the
latest run was below 0.24 ms at p95; CPU painting dominates. High-DPI work should
focus on reducing painted area and repaint work, with equivalent-pixel checks,
before changing the rendering backend.

In the first repeated native session, private commit ranged from 33.26 to
38.24 MiB and working set from 94.53 to 100.33 MiB. All seven two-second idle CPU
samples recorded zero processor time. Handles varied from 900 to 948 and threads
from 43 to 38; these observations alone do not prove absence of leaks. The earlier
audit's cold, per-format private-memory figures of roughly 17–46 MiB are a
different workload and should not be substituted for the mixed-session figures.

The twenty-cycle run added **100** open/page-turn/close sequences. Private commit
ranged from 33.16 to 40.40 MiB and finished at 40.34 MiB; working set ranged from
93.27 to 101.87 MiB. All 21 idle samples recorded zero CPU time. Handles ranged
from 889 to 968, ending at 950; thread counts fell from 43 to 34. In cycles 12–20,
private commit stayed between 40.12 and 40.40 MiB instead of continuing its early
growth. No crash, stuck operation, duplicate import or sustained growth appeared
in this short workload. This is useful repetition evidence, not a multi-hour
endurance test or proof for arbitrary large books.

The inspected desktop views show the corrected Add Document caption, intact
Settings clipping, readable translation card, structured Markdown and wrapped
narrow PDF toolbar. Light/dark production previews provide wider coverage.
Four themes and keyboard access are usable, but detailed typography controls,
true fullscreen, touch gestures and independently verified screen-reader access
remain gaps. Stable pages are an intentional trade-off: zoom keeps references
stable but does not offer freely repaginated font/margin layouts. At 100% zoom a
narrow Book viewport can require horizontal scrolling; a Book Fit-width action
would make this trade-off easier to discover without altering the page map.

## Comparison with other readers

Features below were checked against official documentation on the review date.
No competing applications were installed or benchmarked. Linux/device readers
are UX references, not native Windows alternatives. Listed extras are features
we currently lack, not an exhaustive inventory or claims that other readers lack
our shared basics.

| Reader | Documented strengths / extras beyond simPl | Where simPl offers a different experience |
| --- | --- | --- |
| **SumatraPDF, Windows** | Portable native reader, more formats including DjVu/XPS/CHM/comics, tabs/session restoration and PDF annotations saved into the document. [Product](https://www.sumatrapdfreader.org/free-pdf-reader), [tabs](https://www.sumatrapdfreader.org/docs/Tabs-and-windows), [annotations](https://www.sumatrapdfreader.org/docs/Editing-annotations). | Our managed library/shelves, reader typography, two PDF views and integrated downloadable offline bilingual word packs are the main product combination. Lightweight native reading by itself is not unique to us. |
| **calibre viewer and suite** | Viewer: paged/flow layouts, columns/margins/CSS, regex/proximity search, precise reference links, OS/Piper speech with spoken-word/sentence highlighting. Suite: metadata/tag management, format conversion, ebook editing, devices, content server and plugins. [Viewer manual](https://manual.calibre-ebook.com/viewer.html), [suite](https://calibre-ebook.com/about). | simPl keeps reading, local organization and optional dictionaries together in one focused interface. This is a scope/design distinction, not a measured performance victory. Suite features should not be attributed solely to the viewer. |
| **Thorium, Windows/macOS/Linux** | Fixed-layout EPUB, DAISY/audiobooks/media overlays, Readium LCP, OPDS catalogs, annotation sharing, detailed typography and documented JAWS/NVDA/VoiceOver support. [EDRLab](https://www.edrlab.org/software/thorium-reader/). | simPl emphasizes native Windows local files, PDF prose reconstruction and its bilingual offline card. Both offer local reading; privacy/offline use should not be portrayed as unique to simPl. |
| **Foliate, Linux** | Paged/continuous reading, font/spacing/margin controls, touchpad/touch gestures, vertical/RTL/fixed-layout books and JSON annotation portability; Speech Dispatcher TTS and Wiktionary/Wikipedia/Google lookup services. [Official site](https://johnfactotum.github.io/foliate/). | simPl is available as a native Windows application and provides its own downloadable word packs with local lookup. Foliate is a useful model for reading-layout freedom and annotation portability. |
| **KOReader, devices/Android/Linux** | PDF reflow/cropping, optional Tesseract OCR, per-book appearance, local TXT/Markdown/HTML/JSON annotation export, reading statistics and plugins. [Guide](https://koreader.rocks/user_guide/). StarDict dictionaries and a vocabulary builder extend word lookup. [Dictionary support](https://github.com/koreader/koreader/wiki/Dictionary-support), [vocabulary](https://github.com/koreader/koreader/wiki/Vocabulary-builder). | simPl offers a desktop Windows UI with a smaller settings scope. Offline dictionaries are not a unique invention: KOReader is already more extensible here. Its richer controls are useful follow-up references rather than requirements to copy wholesale. |

**Version caution for SumatraPDF:** the official site currently labels 3.6.1
stable and 3.7 prerelease. Formatted Markdown and newer speech features appear
under the prerelease section of its [version history](https://www.sumatrapdfreader.org/docs/Version-history).
Its [Read Aloud documentation](https://www.sumatrapdfreader.org/docs/Read-Aloud)
explicitly targets prerelease 3.7+, including voice selection and spoken-text
following. Our TTS should not be advertised as a lasting exclusive advantage.

calibre's dictionary panel also supports custom sources, including a local HTTP
server. Its [viewer documentation](https://manual.calibre-ebook.com/viewer.html)
therefore does not support calling calibre dictionary lookup online-only. Our
advantage is the integrated package/install/local-card flow, not the impossibility
of offline lookup elsewhere.

## Recommended next priorities

| Priority | Work | Why it matters |
| --- | --- | --- |
| Release qualification | Clean Windows 10/11 without build tools; actual 100/150/200% DPI and multi-monitor transitions; keyboard and NVDA/Narrator checks; longer mixed-document endurance. | This local audit cannot certify other machines or accessibility. High-DPI Settings smoothness remains a release-quality limitation. |
| First product follow-up | Profile backup/restore and annotation export, then a clear source-file refresh/reimport action. | Users need to carry their reading work elsewhere and understand why an edited original differs from the managed copy. |
| Reading UX | Book Fit-width and fullscreen, independent font/spacing/margin controls, per-book preferences; carefully specified continuous/two-page modes and touchpad/touch support. | The larger gap against mature readers is control over reading comfort. These changes must define stable-page and annotation semantics first. |
| Language learning | Save looked-up words, reusable/exportable vocabulary; expand dictionary coverage and inflection handling; consider external dictionary adapters later. | Existing lookup is useful but not yet a vocabulary-learning workflow. Packs can miss words and inflected forms. |
| Optional later work | Neural TTS and spoken-word highlighting; PDF printing/rotation; richer library metadata and tabs if users need them. | Useful extras, with separate UI/storage/testing costs; they are not required to finish this local-reading MVP. |
| Separate roadmap | Argos sentence translation/plugin runtime, OCR, OPDS/cloud sync, DRM/fixed-layout expansion. | These substantially widen scope. They should remain deliberate later decisions; no Argos or plugin system was implemented in this review. |

My recommendation is to keep the current product scope for a beta, disclose the
remaining qualification and high-DPI limit, and put backup/export and reading
comfort ahead of another large feature. simPl's strongest proposition is the
combined, simple local-reading workflow; feature-count parity is not necessary.

## Current local build

- Run: `.\target\release\iced-shell.exe` (from the repository workspace).
- Executable: 18,449,920 bytes; SHA-256
  `454928ab0f19e0caa7821ce1273db9f5c942da33853a19dc5166ed07c938aef2`.
- Portable folder: `target/portable/simPl`, 28,038,866 bytes in files; retain the
  whole folder, including PDFium and notices.
- Setup: `target/installer/simPl-0.1.5-windows-x64-setup.exe`, 9,794,130 bytes;
  locally built, unsigned. The filename's candidate version is separate from the
  Cargo workspace's development version.

The previous audit's executable and setup hashes describe the baseline artifacts,
not these rebuilt files. The optional dictionary data release and pinned hashes
were not changed.
