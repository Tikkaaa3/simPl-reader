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

On the desktop the canonical page map (`iced-shell/src/book_map.rs`) is built like this:

1. Every block (`Item`) is measured with the default theme and `DEFAULT_FONT_SIZE`
   on a 720 px paper (`TEXT` = 624 px) using the **Iced widget layout**
   (`app.rs::measure_book_with` → `render_item` → cosmic-text).
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

**Plan:** the measurement/pagination code moves, behavior-preserving, out of
`iced-shell` into a window-free `reader-layout` crate (Iced's `core`/`widget`
layout + the cosmic-text patch; no winit). Desktop and Android use the same crate.

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

### M1 — Core builds on Android

- [ ] `reader-document`: `windows-sys` only as a `cfg(windows)` target dependency;
      check the `cfg(not(windows))` paths (atomic `rename`, path comparison, path key).
  - `text.rs` ANSI fallback: outside Windows, use the Windows code page of the app language.
- [ ] Profile root: `LOCALAPPDATA` stays on the desktop; on Android the app's
      `filesDir`/`cacheDir` are set once at startup. Caches go to `cacheDir`.
- [ ] `reader-pdf`: load the packaged `libpdfium.so` on Android (desktop: `pdfium.dll`
      beside the exe, unchanged). Same worker thread model.
- [ ] `reader-ffi`: initialization, document import and a summary of an opened
      document (format, title, author, SHA-256, chapter and source page count).
- [ ] Gradle: package the verified PDFium into the APK `jniLibs`.
- [ ] Tests: Windows `scripts\dev.ps1 check` green; the core crates' Rust tests and an
      app smoke test (EPUB, PDF, TXT, error path) on the emulator.

### M2 — `reader-layout`: page number parity (critical risk)

- [ ] Golden data: the atlas JSON of the fixture books (book-structure, pdf-book,
      reader-workload, EPUB with and without source pages, HTML, TXT, MD) is
      recorded **before the move**.
- [ ] Spike (1–3 days): does the Iced layout (`iced_core`/`iced_widget` + tiny-skia
      renderer, without winit) compile for `aarch64-linux-android` and measure? → plan A or B.
- [ ] Move: `Book`, the `render_item` measurement path, `book_style`, `themes`,
      `book_pages`, `book_map` (building, cache, `adapt_section_with`) → `reader-layout`.
      `iced-shell` re-exports them; desktop code behaves the same.
- [ ] Fonts are loaded from a single source in `reader-layout` (the same `include_bytes!` set as the desktop).
- [ ] Golden tests: on Windows before = after; on Android (on-device test) = Windows.
- [ ] FFI: `open_book`, `atlas(fingerprint)`, `page(n) → { rows, cuts, label }`,
      `adapt(theme, options)`; cancellable background work.
- [ ] **Result:** the same book has the same total page count and the same page starts on Windows and Android.

### M3 — Library

- [ ] Compose theme: `design/DESIGN.md` colors and typography, light/dark, Geist/Literata.
- [ ] Navigation: Library ↔ Reader ↔ Settings (Navigation Compose).
- [ ] Import: SAF `ACTION_OPEN_DOCUMENT` (multi-select), `content://` stream →
      in-app copy → `reader-document::managed`. `OPEN_DOCUMENT_TREE` for an HTML folder.
- [ ] "Open with" / share target: intent filters for `.epub`, `.pdf`, `.html`, `.txt`, `.md`.
- [ ] Card grid, cover thumbnails (generated in Rust, shown with Coil/bitmaps),
      Continue, Favorites, shelves, title/author search, delete confirmation.
- [ ] Process death and recreation: state in `ViewModel` + `SavedStateHandle`.

### M4 — Reader: reflowable books (EPUB/HTML/TXT/MD)

- [ ] Page view: a "paper" with the row range and cut row from Rust; blocks
      (heading, paragraph, list, quote, table, code, image, footnote) are drawn in Compose.
- [ ] In-paragraph cut: split at the line of the Compose `TextMeasurer` layout that
      corresponds to the canonical cut row.
- [ ] Navigation: previous/next page by swipe and edge tap zones, jump by typing a
      number/label in the page field, vertical scrolling inside a page.
- [ ] Contents, internal links, footnotes, returning from a link.
- [ ] Zoom (pinch) and "Fit width"; zoom does not change page numbers.
- [ ] Reading themes (Default/Soft/Clear/Compact × light/dark); per-book font,
      size, line spacing and margins (`adapt_section_with`).
- [ ] Immersive full screen; tap to show/hide the toolbar.
- [ ] Save/restore position (`position.rs`); forced save in `onStop`.
- [ ] Visual check of right-to-left and bidirectional text (fixtures).

### M5 — Reader: PDF Document view

- [ ] Page rendering with PDFium: tiles/bitmaps for visible pages, re-rendering by
      scale, LRU bitmap cache (memory cap).
- [ ] Page-by-page navigation, page field, pinch zoom, fit width.
- [ ] Selection and copy via the text layer (permissions respected).
- [ ] Memory and first-open time measurement for large PDFs (real device).

### M6 — Bookmarks, highlights, notes

- [ ] Custom selection layer: long press, handles, selection across page edges;
      selection ↔ `annotations::Place` (`item_id` + byte / PDF glyph) conversion in Rust.
- [ ] Selection menu: Copy, 4 highlight colors, Add note (plus Share).
- [ ] Drawing highlights: same colors merge, different colors may overlap (desktop rules).
- [ ] Bookmark: page menu or toolbar; `BookmarkPlace` in the same format as the desktop.
- [ ] A bottom sheet instead of the side panel: bookmarks, highlights, notes; tap to go.
- [ ] Edit/delete a highlight, edit a note.

### M7 — MVP release (GitHub APK)

- [ ] Icon (adaptive icon from `installer/simPl.svg`), splash screen, app name.
- [ ] Error states: corrupt file, unsupported format, disk full, save failure.
- [ ] Basic accessibility check: content descriptions, touch targets, font scale.
- [ ] Baseline Profile; R8 shrinking; startup and page turn measurements.
- [ ] License screen: PDFium, Rust dependencies (`collect-licenses.ps1` output), fonts, AndroidX.
- [ ] Signing: release keystore (outside the repository; base64 in GitHub Secrets).
- [ ] `.github/workflows/android-release.yml`: `android-v*.*.*` tag → `windows-latest`
      (so the toolchain file stays unchanged) → `check` + APK → draft GitHub Release.
      `versionCode` derived from the tag.
- [ ] Artifacts: `simPl-<version>-android-arm64.apk` (+ SHA-256); optional universal APK.
- [ ] `docs/releases/android-<version>.md` and an Android section in the README.
- [ ] **MVP acceptance:** all 5 formats open on a real device; page numbers match
      the desktop; position/highlights/notes persist; Windows `check` and the
      Windows release build are green.

---

## After the MVP

### P1 — Read aloud

- [ ] `read_aloud` text chunking, language guessing and page/section follow logic → `reader-core`
      (the desktop SAPI path keeps working as is).
- [ ] Android `TextToSpeech`: voice list, rate, word highlight with `onRangeStart`.
- [ ] Listening with the screen off: `MediaSessionService` (foreground, `mediaPlayback`),
      notification controls, headset buttons, audio focus.
- [ ] The view follows the spoken line, automatic page/section turns; reading the selection/highlight.

### P2 — Offline dictionary

- [ ] `word_translation` lookup logic → `reader-core`.
- [ ] Package downloads: WorkManager (progress, cancel, retry), verification and
      installation with the existing Rust code; import from ZIP via SAF.
- [ ] Dictionary card on double tap/selection; language pair and automatic lookup settings.
- [ ] `INTERNET` permission only for this feature; document text is never sent.

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
