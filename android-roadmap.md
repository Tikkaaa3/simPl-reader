# simPl Android roadmap

> Working document. Decisions: 2026-10-04. For desktop (Windows) product behavior
> see the [README](README.md); for the desktop plan see [roadmap.md](roadmap.md).

## Decisions

| Topic | Decision |
| --- | --- |
| Architecture | **A:** Kotlin + Jetpack Compose interface; the Rust core is shared through UniFFI. |
| Repository | Same repository. The Android project lives in `android/`, new Rust crates under `crates/`. |
| Windows | **Must not break.** Every step ends with `scripts\dev.ps1 check` green and desktop behavior unchanged. |
| Page numbers | **Same as the desktop** (canonical page map). No repagination per screen. |
| Outside the MVP | Read aloud and dictionary → **after the MVP** (P1, P2). |
| Distribution | Signed APK on GitHub Releases. No Play Store for now. |
| Target | minSdk 26, compileSdk/targetSdk 37, `arm64-v8a` (+ `x86_64` for the emulator). |

## Technical core: page number parity

The shared canonical page map (`reader-layout/src/atlas.rs`, re-exported by
`iced-shell/src/book_map.rs`) is built like this:

1. Every block (`Item`) is measured with the default theme and `DEFAULT_FONT_SIZE`
   on a 720 px paper (`TEXT` = 624 px) using the **Iced widget layout**
   (`reader-layout::measure::measure_book_with` → item layout → cosmic-text).
2. `book_pages::reflow` cuts pages from these heights; long paragraphs are split
   on the line grid.
3. When the theme/font/margins change, `adapt_section_with` moves every canonical
   cut in the new layout to **the same line / the matching line inside the
   paragraph**; the page number and the passage a page starts with do not change.
4. EPUBs with a source page list and PDFs take their numbers from the source.

**Consequence:** for the same numbers on Android, steps 1 and 2 must run **the same
Rust code with the same embedded fonts**. Display (step 3) can happen in Compose;
Rust provides `(row range, in-paragraph cut row)` for every page, and Compose
applies the cut at the matching line in its own layout — the same logic as the
desktop theme adaptation.

**Implemented in M2:** the measurement/pagination code lives in the window-free
`reader-layout` crate (Iced's `core`/`widget` layout + the cosmic-text patch; no
winit). Desktop and Android use the same crate. Its paragraph layout rule is also
used by the desktop's selectable widget.

**Acceptance criterion:** the atlas JSON (`sections`, `pages`, `total`) of the
fixture books is **byte-for-byte identical** on Windows before and after the move,
and on the Android emulator/device. Known limit: scripts not covered by the
embedded fonts (e.g. CJK) fall back to a system font; those books may diverge and
are reported separately.

**Plan B** (if the Iced layout does not compile on Android or is too heavy):
`reader-layout` measures directly on cosmic-text; the desktop keeps the old path
until golden tests prove it produces the same atlas.

**On a phone:** a page is a "paper" as on the desktop. If it does not fit the
screen with the reader's typography, it scrolls vertically inside the page; a
horizontal swipe/tap goes to the next page. A "Fit width" option scales the paper
to the screen width.

## Rules that protect Windows

- At the end of every PR/stage: `scripts\dev.ps1 check` (fmt, clippy `-D warnings`, all tests).
- Desktop refactors are **behavior-preserving** moves only; they are compared
  before/after with golden tests (atlas JSON, `book_preview` PNGs).
- Windows-only dependencies move under `[target.'cfg(windows)'.dependencies]`;
  versions resolved for the desktop build do not change (review the `Cargo.lock` diff).
- Atlas/cache `VERSION` values are not changed (existing user caches stay valid).
- `release.yml` (tag `v*.*.*`) is not touched; Android uses its own workflow and
  the `android-v*.*.*` tag.
- `rust-toolchain.toml` only changes by adding `targets`; the channel stays the same.

## Target layout

```
simPl-reader/
├─ crates/
│  ├─ reader-document/   (existing; Windows dependencies split by cfg)
│  ├─ reader-pdf/        (existing)
│  ├─ reader-layout/     NEW  measurement, canonical atlas, page cuts, themes/styles
│  ├─ reader-core/       NEW  UI-independent app logic (Book model, in-book search,
│  │                          note export; later: read-aloud text chunking,
│  │                          dictionary lookup)
│  ├─ reader-profile/    NEW  data and cache roots (desktop LOCALAPPDATA / Android)
│  ├─ reader-ffi/        NEW  UniFFI surface; the only API Android sees
│  ├─ uniffi-bindgen/    NEW  version-pinned Kotlin binding generator
│  └─ iced-shell/        (existing; uses reader-layout/core)
├─ android/              NEW  Gradle project
│  ├─ app/               Compose UI, Activity, SAF, services
│  └─ build-logic/       cargo-ndk + uniffi-bindgen Gradle tasks
└─ scripts/android.ps1   NEW  environment check, PDFium (Android) download, build
   scripts/pdfium-android.ps1
```

---

## MVP

### M0 — Environment and skeleton ✅ (2026-10-04)

- [x] JDK 21 (Temurin, `%LOCALAPPDATA%\Programs\Temurin`), Android SDK
      (`%LOCALAPPDATA%\Android\Sdk`): Platform 36 + 37, Build-Tools 36.1, NDK
      `29.0.14206865`, Platform-Tools, Emulator. User environment variables:
      `JAVA_HOME`, `ANDROID_HOME`, `Path`. Android Studio is optional (uses the same SDK).
- [x] WHPX is usable; `simpl-api36` emulator (API 36, Google APIs, x86_64, Pixel 8).
- [x] `rustup target add aarch64-linux-android x86_64-linux-android`;
      `rust-toolchain.toml` → `targets = [...]` (same channel).
- [x] `cargo-ndk` 4.1.2; UniFFI `=0.32.2` (the `reader-ffi` runtime and the
      `crates/uniffi-bindgen` generator use the same version).
- [x] `scripts/android.ps1 doctor | build [-Release] | run | pdfium`.
- [x] `scripts/pdfium-android.ps1` (a separate script; the desktop `pdfium.ps1` is
      untouched): chromium/8066 `android-arm64` and `android-x64`, length + SHA-256
      verification, `app/build/pdfium/jniLibs/<abi>/` and `third-party/pdfium`.
      Added to the APK in M1.
- [x] `android/` Gradle skeleton: Gradle 9.8.0 wrapper (pinned SHA-256), AGP 9.4.1,
      Kotlin 2.4.20 (AGP built-in Kotlin), Compose BOM 2026.09.00, version catalog,
      compileSdk/targetSdk 37, minSdk 26; `.gitignore` entries.
- [x] `reader-ffi`: `build_info()`; `build-logic` plugin `simpl.rust-android`
      (per variant `cargoNdkBuild*` → jniLibs, `uniffiBindgen*` → Kotlin sources).
      Release uses `[profile.android-release]` (release + symbol table; needed for
      UniFFI metadata, AGP strips it when packaging).
- [x] **Result:** the emulator shows the Compose screen `Rust core 0.0.0 · android/x86_64 · debug`;
      the R8-minified release APK (1.9 MB) shows the same screen as `release`.
      The desktop `iced-shell` dependency tree is unchanged; `Cargo.lock` only gains entries.
      Windows `scripts\dev.ps1 check` is green (fmt, clippy `-D warnings`, all tests).

Note: `selection::tests::stable_selection_rebuilds_native_highlight_after_visible_row_reentry`
in `iced-shell` failed once during a concurrent heavy build (different glyph
widths); it passed on reruns alone, with `-p iced-shell` and with `--workspace`.
It looks flaky under load; to be investigated separately on the desktop side.

To decide: `applicationId` = `io.github.tikkaaa3.simpl` (must be final before the
first public APK; changing it later forces users to reinstall).

### M1 — Core builds on Android ✅ (2026-10-04)

- [x] `reader-document`: `windows-sys` is only a `cfg(windows)` target dependency;
      `encoding_rs` only `cfg(not(windows))`. The existing `cfg(not(windows))` paths
      (atomic `rename`, case-sensitive path comparison, Unix path key) were already
      correct and did not change.
  - `text.rs` ANSI fallback: outside Windows, the Windows code page of the app
    language (`tr` → 1254, `ru` → 1251, `zh-Hant` → Big5 …; default 1252).
    `reader_document::set_legacy_text_language(tag)`.
- [x] Profile root: new `reader-profile` crate (not in `reader-document` because
      `reader-pdf` uses it too). `storage_base()` and `cache_base()`; on the desktop
      `LOCALAPPDATA` is read on every call (same behavior), on Android
      `configure(files, cache)` sets them once. The `pdf-books` cache goes to `cacheDir`.
      `page-maps` is still in `iced-shell` (desktop); it moves to `reader-layout` in M2
      and uses `cache_base()` there.
- [x] `reader-pdf`: on Android loads `libpdfium.so` by name (desktop: `pdfium.dll`
      beside the exe, unchanged). Same worker thread model.
- [x] `reader-ffi`: `initialize(dataDir, cacheDir, language)`, `importDocument`,
      `inspectDocument` (format, title, author, SHA-256, chapter and source page count),
      `CoreException.Failed(reason)`. The app initializes it in `SimplApplication.onCreate`.
- [x] Gradle: `stagePdfium` (verified PDFium → APK `jniLibs`); `cargoNdkBuild*`
      copies only `libreader_ffi.so` (`cargo ndk -o` also copied dependency cdylibs).
- [x] Tests:
  - Windows `scripts\dev.ps1 check` green; the only addition to the `iced-shell`
    dependency tree is the project's own `reader-profile` crate.
  - `scripts\android.ps1 test`: Rust tests on the emulator (`reader-document` 74,
    `reader-pdf` 31 + PDFium `page_text`, `reader-ffi` 3, `reader-profile` 1) and
    `CoreSmokeTest` (5): EPUB chapters, PDF pages (packaged PDFium), TXT import,
    error path. 3 dictionary tests that read repository files run on the host only.
  - `cargo-ndk-runner` does not work with Windows paths, so
    `scripts/android-test-runner.ps1` (adb push + run) is used via
    `--config target.<triple>.runner`.

Notes:
- The release APK (two ABIs, uncompressed `.so`) is 28 MB; `libreader_ffi.so` ≈ 6.8 MB,
  `libpdfium.so` ≈ 6.5 MB per ABI. M7 will consider an `arm64-v8a`-only APK (~14 MB)
  and size settings (e.g. `opt-level`, unused `image` formats).

### M2 — `reader-layout`: page number parity ✅ (2026-10-04)

- [x] Golden data: the atlas JSON of the fixture books (book-structure, pdf-book,
      reader-workload, EPUB with and without source pages, HTML, TXT, MD) is
      recorded **before the move**.
- [x] Spike: the Iced layout (`iced_core`/`iced_widget` + tiny-skia renderer,
      without winit) compiles for `aarch64-linux-android` and measures on the
      x86_64 emulator. **Plan A.**
- [x] Move: `Book`, the `render_item` measurement path, `book_style`, `themes`,
      `book_pages`, `book_map` (building, cache, `adapt_section_with`) → `reader-layout`.
      `iced-shell` re-exports them; desktop code behaves the same.
- [x] Fonts are loaded from a single source in `reader-layout` (the same `include_bytes!` set as the desktop).
- [x] Golden tests: on Windows before = after; on Android (on-device test) = Windows.
- [x] FFI: `open_book`, `atlas(fingerprint)`, `page(n) → { rows, cuts, label }`,
      `adapt(theme, options)`; cancellable background work.
- [x] **Result:** the same portable fixture has the same total page count and
      page starts on Windows and Android.

Validation and API details:
- Eight pre-move Windows golden JSON files, including canonical heights/cuts and
  six theme/font/size/margin/spacing adaptations, remain byte-for-byte identical.
  A separate test compares the shared measurements to real desktop selectable
  widgets for all eight books and layouts.
- Android `scripts\android.ps1 test`: six portable goldens match Windows
  byte-for-byte, including PDF Book. The two structured fixtures use system
  monospace/script fallback and are reported separately. Fixture ZIP creator
  metadata is fixed to Windows so source fingerprints are identical too.
- `reader-layout` unit tests (11), parity tests (2), cache test (1) and
  `reader-ffi` tests (6) pass on the emulator. `CoreSmokeTest` (7) verifies the
  packaged Kotlin bindings, long-paragraph cuts, atlas lookup, theme adaptation,
  cache root and PDF Book source pages. Both Android ABIs build in Gradle.
- Windows `scripts\dev.ps1 check` is green. Thirty `book_preview` PNGs (HTML,
  EPUB, all themes, light/dark and responsive sizes) are byte-for-byte identical
  to the M1 commit `14f048c`. No dependency versions, atlas schema, PDF conversion
  version or Windows release workflow changed.
- `open_book` returns an `OpenBookTask`; `adapt` returns an `AdaptBookTask`.
  Both expose `status`, `result` and `cancel`, and closing a pending handle also
  requests cancellation. PDF conversion finishes before cancellation is checked.
  Keep `OpenBook` alive for `atlas(fingerprint)`. FFI page numbers are one-based;
  a source page spanning chapters returns every section fragment. See
  [`reader-layout/README.md`](crates/reader-layout/README.md).
- The page-map cache keeps `v1-<fingerprint>.json`; Windows paths are unchanged,
  Android uses `cacheDir/simPl/page-maps`. Tests explicitly disable persistence
  or use isolated roots.

### M3 — Library (completed)

- [x] Compose theme: `design/DESIGN.md` colors and typography, light/dark, Geist/Literata.
- [x] Navigation: Library ↔ Reader ↔ Settings (Navigation Compose).
- [x] Import: SAF `ACTION_OPEN_DOCUMENT` (multi-select), `content://` stream →
      in-app copy → `reader-document::managed`. `OPEN_DOCUMENT_TREE` for an HTML folder.
- [x] "Open with" / share target: intent filters for `.epub`, `.pdf`, `.html`, `.txt`, `.md`.
- [x] Card grid, cover thumbnails (generated in Rust, shown with Coil/bitmaps),
      Continue, Favorites, shelves, title/author search, delete confirmation.
- [x] Process death and recreation: state in `ViewModel` + `SavedStateHandle`.

The UniFFI catalog uses the existing desktop library, shelf, recent and cover
storage schemas. Imports are validated before being listed, duplicate content
keeps its state, missing private copies can be repaired, and deletion retains
original files and annotations. Provider streams and HTML trees have bounded
private staging; pending SAF imports retain read grants until completion.
Fonts and licenses are generated Android resources from the shared assets.

The Reader destination is a book-details screen in M3. It records recently
opened books without inventing reading progress; page rendering follows in M4
and PDF Document mode in M5.

Validation: shared Rust tests on Android, the host catalog integration test,
13 app instrumentation tests, the desktop `dev.ps1 check`, Android lint, and
debug/minified-release builds for both ABIs. UI tests cover recreation, all
incoming intent actions, shelf operations and deletion confirmation. A typing
regression test keeps search focused when the Continue card disappears.
Host-driven emulator QA also restores navigation, theme, query and Favorites
after stopping the activity and killing its process; screenshots and PID
records are in `target/m3-visual`.

### M4 — Reader: reflowable books (EPUB/HTML/TXT/MD) (completed)

- [x] Page view: a "paper" with the row range and cut row from Rust; blocks
      (heading, paragraph, list, quote, table, code, image, footnote) are drawn in Compose.
- [x] In-paragraph cut: split at the line of the Compose `TextMeasurer` layout that
      corresponds to the canonical cut row.
- [x] Navigation: previous/next page by swipe and edge tap zones, jump by typing a
      number/label in the page field, vertical scrolling inside a page.
- [x] Contents, internal links, footnotes, returning from a link.
- [x] Zoom (pinch) and "Fit width"; zoom does not change page numbers.
- [x] Reading themes (Default/Soft/Clear/Compact × light/dark); per-book font,
      size, line spacing and margins (`adapt_section_with`).
- [x] Immersive full screen; tap to show/hide the toolbar.
- [x] Save/restore position (`position.rs`); forced save in `onStop`.
- [x] Visual check of right-to-left and bidirectional text (fixtures).

The Compose reader measures full source paragraphs and clips their matching
line ranges, retaining Unicode styles and accessible link actions. Rust exports
resolved block typography/palettes, bounded images, contents and navigation
targets. Auxiliary EPUB notes stay outside next/previous page order. Printed
labels appear alongside canonical ordinals; missing/external links report an
error. Theme adaptation, pinch and fit never change canonical page identity.
Typography and HTML/EPUB positions use the existing desktop storage schemas.

Validation: Windows `dev.ps1 check`, shared Rust unit/integration/golden tests on
the API 36 emulator, all 17 app instrumentation tests, Android lint and the
minified release APK for arm64-v8a/x86_64. The new Rust integration test checks
all seven reflowable golden fixtures and restores every split-page boundary.
Compose line coverage is exact under four contrasting typography settings;
UI tests cover gestures, zoom/fit, immersion, contents/links/notes, printed
labels, STOP checkpoints, recreation and per-book options. Eight theme/paper
screenshots include lists, quotes, code, table text, images and mixed Arabic,
Hebrew and Latin text. A real stopped-process kill changes PID 17525 → 18214
and restores page 4/19, font size 26 and the exact source-row fraction
0.15744351. Screenshots and checkpoint records are in `target/m4-visual`.
PDF Document mode is implemented in M5.

### M5 — Reader: PDF Document view (completed)

- [x] Page rendering with PDFium: tiles/bitmaps for visible pages, re-rendering by
      scale, LRU bitmap cache (memory cap).
- [x] Page-by-page navigation, page field, pinch zoom, fit width.
- [x] Selection and copy via the text layer (permissions respected).
- [x] Memory and first-open time measurement for large PDFs (emulator accepted
      for M5 on 2026-10-04; physical-device measurement deferred).

The Compose Document reader renders only the active physical page through the
shared PDFium worker, with quantized/coalesced scale requests and a 24 MiB LRU.
The worker's four-megapixel/8192 px limits remain unchanged; Android targets
approximately two megapixels before width quantization. Text selection uses a
spatial glyph index, source glyph ordinals and a native permission check on copy.
Restricted documents render without exposing text. Page/scroll/zoom positions
use the existing desktop PDF schema and completed lifecycle STOP checkpoints.
See [Android implementation and measurement instructions](android/README.md#pdf-document-reader-m5).

The self-authored 512-page PDF measurement and screenshot tests run on the API 36
x86_64 emulator. The user accepted emulator validation for M5 on 2026-10-04;
physical-phone measurements remain follow-up work. Reported PSS is sampled
debug instrumentation process memory,
including fixture setup; it is not a cache-only or transient peak measurement.

Validation: Windows `dev.ps1 check`, shared Rust tests (including the new PDF FFI
integration test) on API 36, all 20 app instrumentation tests, Android lint and
minified arm64-v8a/x86_64 release builds. The three PDF UI tests pass again after
the gesture fix; a long press no longer also hides controls. Visual checks cover
fit, zoom, glyph selection and a restricted PDF. No dependency versions, desktop
PDF code or position/cache schema versions changed.

The final debug measurement uses a 13,474,555-byte, 512-page PDF: metadata open
463 ms, first published raster 1124 ms, uncached high-resolution renders
480–618 ms, cached render 1 ms, cache 18,923,520 bytes, one cache hit and two
evictions. Sampled process PSS rises from 135,850 KiB to 245,676 KiB. These are
warm-library instrumentation measurements, not cold startup or phone benchmarks.
JSON records and screenshots are retained in `target/m5-performance` and
`target/m5-visual`.

### M6 — Bookmarks, highlights, notes

- [x] Custom selection layer: long press, handles, selection across page edges;
      selection ↔ `annotations::Place` (`item_id` + byte / PDF glyph) conversion in Rust.
- [x] Selection menu: Copy, 4 highlight colors, Add note (plus Share).
- [x] Drawing highlights: same colors merge, different colors may overlap (desktop rules).
- [x] Bookmark: page menu or toolbar; `BookmarkPlace` in the same format as the desktop.
- [x] A bottom sheet instead of the side panel: bookmarks, highlights, notes; tap to go.
- [x] Edit/delete a highlight, edit a note.

M6 shares `reader-document::annotation_logic` with the desktop and exposes source
selection, annotation mutation, paint marks and navigation through UniFFI. The
version 1 JSON schema is unchanged. Reflowable ends are validated UTF-8 grapheme
boundaries; PDF endpoints use inclusive source glyph ordinals. Cross-chapter EPUB
selection splits into chapter-local records, while a PDF record can span pages.
Copy, highlight/note creation, edits and bookmark toggles run off the UI thread;
mutations reload the file and save atomically after validation. Corrupt or failed
writes preserve the prior file. Restricted PDFs still permit bookmarks.

The viewport owns pointer capture, including both 48 dp selection handles, so a
held edge scrolls and replaces a page without dropping selection. The floating
selection menu preserves the paper's position during a gesture. Tapping text
after Previous/Next also extends a selection. Highlight painting uses desktop
colors and component ordering; separate notes survive same-color merges.
Canonical shaping locates saved selections inside long paragraph cuts.

Validated on the accepted API 36 x86_64 emulator on 2026-10-04: all 26 Android
instrumentation tests passed, including six M6 selection/annotation tests and the
existing library, reflowable reader and PDF checks. Shared Rust tests pass on the
host and emulator, including annotation schema compatibility, Unicode, chapter
splitting, overlapping colors, permissions and atomic failure handling. Windows
`scripts\dev.ps1 check` remains green. Screenshots are retained in
`target/m6-visual`; see `android/README.md` for gestures, limits and test commands.

### M7 — MVP release (GitHub APK)

- [x] Icon (adaptive icon from `installer/simPl.svg`), splash screen, app name.
- [x] Error states: corrupt file, unsupported format, disk full, save failure.
- [x] Basic accessibility check: content descriptions, touch targets, font scale.
- [x] Baseline Profile; R8 shrinking; startup and page turn measurements.
- [x] License screen: PDFium, Rust dependencies (`collect-licenses.ps1` output), fonts, AndroidX.
- [x] Signing: release keystore (outside the repository; base64 in GitHub Secrets).
- [x] `.github/workflows/android-release.yml`: `android-v*.*.*` tag → `windows-latest`
      (so the toolchain file stays unchanged) → `check` + APK → draft GitHub Release.
      `versionCode` derived from the tag.
- [x] Artifacts: `simPl-<version>-android-arm64.apk` (+ SHA-256); optional universal APK.
- [x] `docs/releases/android-<version>.md` and an Android section in the README.
- [x] **MVP acceptance:** all 5 formats open on the accepted API 36 emulator; page numbers match
      the desktop; position/highlights/notes persist; Windows `check` and the
       Windows release build are green.

**Acceptance scope:** emulator validation was explicitly accepted by the user
for M5 and remains the M7 device requirement. Physical-phone measurements remain
follow-up work. On 2026-10-04, 33 Android instrumented tests passed, including seven
M7 checks for five-format opening, complete notice assets, 1.8x font scale,
corrupt-import cleanup, disk-full recovery and actual annotation-save failure
followed by a successful retry. Shared Rust checks passed on Windows (423 tests)
and the emulator (131 tests); the Windows release build also passed.

The license index packages 700 offline legal/provenance documents. Both Android
Rust target graphs are collected (208 arm64 dependencies, 209 x86_64 dependencies)
so architecture-specific notices are retained. A distribution identity lives
outside the checkout and the four signing secrets are configured in GitHub.
The workflow is validated with actionlint; it will create a draft when triggered.
No tag or GitHub release has been published as part of this implementation.

Two Baseline Profile generation tests and two minified release benchmarks passed.
The committed startup/reader profiles are compiled into the release APK. Five
iterations measured cold-start initial display at 1242.13 ms median and native
page fetch at 0.2501 ms median. Page-turn frame CPU P50/P90 was 38.15/56.72 ms;
deadline overrun P50/P90 was 27.03/63.63 ms. The emulator missed frame deadlines;
these are diagnostic measurements, and native fetch excludes drawing/gestures.
See `android/README.md` for the complete scope and reproduction commands.

The signed 0.1.0 artifacts and SHA-256 sidecars are in
`target/android-release/0.1.0/`: arm64 (27.2 MB) and universal (49.2 MB),
`versionCode=100001`. Release lint passed with zero errors (nine warnings).
Both APK signatures, complete notice assets, ABI contents and the compiled
10,108-byte Baseline Profile were verified. The signed universal APK installed
and launched on the emulator; library, settings, the notice index and full
license text opened successfully. See `docs/releases/android-0.1.0.md`.

---

## After the MVP

### P1 — Read aloud

- [x] `read_aloud` text chunking, language guessing and page/section follow logic → `reader-core`
      (the desktop SAPI path keeps working as is).
- [x] Android `TextToSpeech`: voice list, rate, word highlight with `onRangeStart`.
- [x] Listening with the screen off: `MediaSessionService` (foreground, `mediaPlayback`),
      notification controls, headset buttons, audio focus.
- [x] The view follows the spoken line, automatic page/section turns; reading the selection/highlight.

**Implementation:** the platform-independent `reader-core` shares source offsets,
sentence boundaries, language hints and page-follow rules with the existing
Windows SAPI adapter. The UniFFI speech plan retains its document and streams
bounded utterances across canonical sections or physical PDF pages. Android
synthesizes one temporary WAV at a time and plays it in simPl's own audio session;
this makes system media keys target the reader rather than the external TTS
engine. Word/frame markers follow the playback clock. Voice/rate preferences,
foreground notification controls, audio focus and selection/highlight passages
are documented in [android/README.md](android/README.md#read-aloud-p1).

**Acceptance (2026-10-04):** Windows `scripts/dev.ps1 check` passed: 426 tests
and 37 existing ignored tests. Two additional native SAPI selection/pause and
EPUB chapter-transition tests passed. On the accepted API 36 x86_64 emulator,
134 Rust tests passed (two ignored, three host-fixture skips); the 33 existing
instrumented UI tests and all eight `ReadAloudTest` cases passed. The speech
cases cover Unicode word overlays (including a visible-pixel check), pause and
recreation, actual global media keys, screen-off playback, audio focus, canonical
sheets/EPUB sections, PDF physical pages and selected/highlighted passages. A
separate case uses the installed production TTS engine. ARM64/x86_64 R8 release
assembly and debug/release lint passed with zero lint errors. Physical-device
performance and individual voice quality remain follow-up checks, as in M5/M7.

### P2 — Offline dictionary

- [x] `word_translation` lookup logic → `reader-core`.
- [x] Package downloads: WorkManager (progress, cancel, retry), verification and
      installation with the existing Rust code; import from ZIP via SAF.
- [x] Dictionary card on double tap/selection; language pair and automatic lookup settings.
- [x] `INTERNET` permission only for this feature; document text is never sent.

**Implementation:** `reader-core::word_translation` owns Unicode query/word
boundaries, languages, settings validation, TSV lookup and labeled English base
forms. The desktop reuses it without changing its saved settings or package
format. Android's shared UniFFI store retains the existing Rust catalog,
SHA-256/ZIP checks, bounded index and atomic installer. Compose offers a card on
double tap, a selection-menu action, automatic selection lookup and persistent
language/automatic preferences. Settings manages all 13 catalog directions and
their full license notices. WorkManager owns bounded HTTPS downloads, progress,
active/queued cancellation, transient retries and durable SAF ZIP imports. A
startup barrier configures the native roots before a restored worker opens its
store; terminal jobs release provider grants and discard staging. Only package
IDs/ZIP URIs enter work requests; local queries never enter the downloader.
Details are in [android/README.md](android/README.md#offline-dictionary-p2).

**Acceptance (2026-10-04):** The Windows workspace check passed with 429 tests
and 37 existing ignored cases. On the accepted API 36 x86_64 emulator, 137 Rust
tests passed (two ignored, three host-fixture skips), and all 49 instrumented UI
tests passed without failures or skips. The eight dictionary cases cover all
13 catalog directions, Unicode queries and base forms, verified license notices,
corrupt/cancelled installs, cache invalidation, durable SAF import and retry,
real GitHub download progress through Activity recreation, active/queued
cancellation, persistent language/automatic settings and reflow/PDF source
selection. The dictionary cards and package screen were visually reviewed.
ARM64/x86_64 R8 release assembly and debug/release lint passed with zero errors
and the ten existing warnings; P2 introduced no lint warnings.

### P3 — Backup and export

- [ ] Create/restore backups (`backup.rs`), SAF `CREATE_DOCUMENT`/`OPEN_DOCUMENT`.
- [ ] **Windows ↔ Android backup compatibility:** path remapping, a "Locate" flow for missing books.
- [ ] Export notes as Markdown/Text/JSON + share sheet.

### P4 — PDF Book view

- [ ] `reader-pdf::book` conversion, progress indicator and cache; switching
      Document ↔ Book keeps the page.
- [ ] Time/memory measurement on a phone CPU; background preparation if needed.

### P5 — Polish

- [ ] Tablet/foldable layout (two-column library, persistent side panel), hardware
      keyboard shortcuts (the desktop shortcuts).
- [ ] In-book search, Ctrl+K-style quick book switching.
- [ ] Page turns with the volume keys (optional), keep-screen-on setting.
- [ ] Play Store preparation (separate decision).

## Risks and mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Iced layout does not compile on Android / is heavy | Page parity delayed | Early M2 spike; plan B (direct cosmic-text + golden tests) |
| Atlas drift for text drawn with fallback fonts (CJK etc.) | Different numbers in these books | Report as a known limit; an extra embedded fallback font if needed |
| Custom selection over paginated text | M6 takes longer | Selection model in Rust, Compose only draws and handles gestures; early prototype |
| First atlas time for a large book (phone CPU) | Slow first open | Cancellable per-section background work, persistent cache, progress indicator |
| Large PDF memory use | Crash (OOM) | Tiled rendering, bitmap pool, resolution cap by scale |
| Regression in the desktop refactor | Windows breaks | Behavior-preserving move, golden atlas/PNG tests, `check` at every step |

## Desktop parts that are not ported

`process-measure`, `shell-startup-markers`, the Inno Setup installer and
Windows-specific window behavior are not ported to Android. Their Android
counterparts are the Android Studio Profiler, Macrobenchmark and APK signing.
