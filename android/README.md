# simPl for Android

The Android app is a Kotlin + Jetpack Compose interface over the shared Rust
core. The plan, decisions and milestones are in
[android-roadmap.md](../android-roadmap.md). The Windows reader is unaffected:
its build, scripts and release workflow do not use anything in this folder.

| Part | Responsibility |
| --- | --- |
| [`crates/reader-ffi`](../crates/reader-ffi) | UniFFI surface of the Rust core; the only Rust API the app sees |
| [`crates/reader-profile`](../crates/reader-profile) | Data/cache roots: `LOCALAPPDATA` on the desktop, `filesDir`/`cacheDir` on Android |
| [`crates/reader-layout`](../crates/reader-layout) | Window-free canonical atlas, page cuts, typography and theme adaptation shared with Windows |
| [`crates/uniffi-bindgen`](../crates/uniffi-bindgen) | Workspace-pinned binding generator (same version as the `uniffi` runtime) |
| `build-logic/` | Gradle plugin `simpl.rust-android`: cargo-ndk build and Kotlin binding generation per variant |
| `app/` | Compose application (`io.github.tikkaaa3.simpl`) |

## Library (M3)

The app opens a responsive card grid with title/author search, Favorites,
Continue and named shelves. Settings selects the device, light or dark theme
and manages shelves. Geist and Literata come from the repository's font files;
Gradle stages the fonts and their license notices as generated resources/assets.
The Reader route opens reflowable pages for EPUB, HTML, TXT and Markdown (M4).
PDF keeps the book-details destination until PDF Document mode in M5.

The import menu uses `ACTION_OPEN_DOCUMENT` for multiple EPUB, PDF, HTML, TXT
and Markdown files, or `ACTION_OPEN_DOCUMENT_TREE` for an HTML book folder.
Provider streams are copied into bounded private staging before Rust validates
and imports them through `reader-document::managed`. Folder imports retain
relative HTML, image, stylesheet and font resources. Use folder import when
HTML depends on sibling assets. No broad storage permission is required.
`ACTION_VIEW`, `ACTION_SEND` and `ACTION_SEND_MULTIPLE` use the same importer.

The UniFFI catalog API uses the desktop `library.json`, `shelves.json`, recent
list and cover cache schemas. It serializes mutations, coalesces identical
content and preserves progress, favorites and shelf membership on reimport or
repair. Rust generates bounded RGBA cover thumbnails; Compose displays them
as bitmaps with a title fallback. Removal requires confirmation and deletes
only the marked managed copy, retaining originals and saved annotations.

`LibraryViewModel` keeps search, filters, pending imports and navigation requests
in `SavedStateHandle`. Navigation Compose and `rememberSaveable` retain the
route, grid position and dialogs. SAF read grants are persisted while an import
is pending, then released. A restored import restarts an incomplete staging
copy; a provider that no longer grants access produces an error requesting a
new selection. App appearance persists separately in private preferences.

`LibraryImportTest` exercises real provider streams, HTML tree assets, corrupt
input, PDF thumbnails and a restored ViewModel finishing its pending import.
`LibraryUiTest` covers search, favorites, shelf creation/membership/rename/delete,
removal confirmation, all three incoming intent actions, themes and navigation
across Activity recreation. The fixture provider/picker exists only in the test
APK. The host/device Rust catalog integration test also checks missing-copy
repair and preservation of source files and state.

## Reflowable reader (M4)

The reader displays one canonical page at a time. Rust supplies its section
fragments, source rows, structural semantics and start/end paragraph cuts.
Compose `TextMeasurer` measures the complete source paragraph at the canonical
paper width, maps the native cut ratio to its own line grid and draws only those
lines. Adjacent pages cover each paragraph exactly once. RTL text retains its
logical order and uses Android's bidirectional shaper and script fallback.
Lists, quotations, code, captions, formula alternatives, table text rows and
footnotes retain the shared document semantics; publisher CSS is not applied.

Use Previous/Next, a horizontal swipe or the outer page edges to turn; tap the
page field to enter an ordinal number or printed label. The label is shown
alongside the ordinal when they differ. Scroll vertically within a tall page.
Pinch to zoom up to 3×, pan the enlarged paper and select Fit width to reset.
Zoom changes display scale only. Tap the center to toggle controls and Android
system bars. Contents and internal links resolve through the native anchors;
Return from link restores the source-row position. Auxiliary EPUB notes appear
in a sheet outside canonical page order, with a return action and backlinks.
Missing and external targets report an error without leaving the document.

Reading options selects Default, Soft, Clear or Compact in light/dark paper,
or follows the app appearance. All Literata, Spectral and Fira Sans reading
faces and their licenses are generated from the shared repository assets.
Per-book font, size, spacing and margin settings use `reader_document::reading`.
Every change runs cancellable Rust `adapt_section_with` work and retains the
canonical page number/count. Theme and paper appearance persist on the device.

`ReaderViewModel` keeps native handles off the saved-state bundle, reopens them
after process death and retains source-row anchors in `SavedStateHandle`.
Position writes use the desktop HTML/EPUB schemas in `position.rs`, with a
debounced write during reading and a completed checkpoint on lifecycle STOP
and reader disposal. Library progress uses the same canonical ordinal/total.
Decoded sections are bounded to three cached entries; exported images are
bounded to 2048 px per dimension, with original dimensions retained for layout.

`ReaderCutTest` checks complete, nonoverlapping Compose line coverage for long
Unicode paragraphs under four contrasting typography configurations.
`ReaderUiTest` exercises page gestures, pinch/fit, immersion, contents, links,
auxiliary notes, printed labels, settings, STOP persistence and recreation.
The host/device Rust reader integration test checks all seven reflowable golden
fixtures, including position roundtrips at every canonical page boundary.
On API 29+ UI screenshots survive test APK removal in `Pictures/simPl-M4`:

```powershell
adb pull /sdcard/Pictures/simPl-M4 target/m4-visual
```

## Environment (Windows)

| Tool | Version | Notes |
| --- | --- | --- |
| JDK | 21 (Temurin) | `JAVA_HOME` |
| Android SDK | Platform 37, Platform-Tools, Emulator | `ANDROID_HOME`; Android Studio's default `%LOCALAPPDATA%\Android\Sdk` |
| NDK | `29.0.14206865` | Pinned as `ndkVersion` in `app/build.gradle.kts` |
| Rust | toolchain from `rust-toolchain.toml` | Includes `aarch64-linux-android` and `x86_64-linux-android` |
| cargo-ndk | 4.x | `cargo install cargo-ndk --locked` |
| Gradle | wrapper (`gradlew.bat`) | Downloads and verifies the pinned distribution |

The command-line tools install SDK packages with `android sdk install
<path>/<version>` (for example `ndk/29.0.14206865`); `sdkmanager` is deprecated.
The emulator needs **Windows Hypervisor Platform**; `emulator -accel-check`
reports whether WHPX is usable. Android Studio is optional: open `android/` and
it uses the same SDK and wrapper.

Check everything from the repository root:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\android.ps1 doctor
```

## Build and run

```powershell
# Debug APK (arm64-v8a + x86_64): app\build\outputs\apk\debug\app-debug.apk
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\android.ps1 build

# Build, install and launch on the connected device or running emulator.
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\android.ps1 run

# Minified release APK (unsigned until release signing is configured).
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\android.ps1 build -Release

# Core Rust tests on the running emulator (-Abi arm64-v8a for a phone), then the
# instrumented app tests (connectedDebugAndroidTest).
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\android.ps1 test

# Cache, verify and stage the pinned Android PDFium (app\build\pdfium).
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\android.ps1 pdfium
```

`test` runs the `reader-profile`, `reader-document`, `reader-pdf`, `reader-layout` and `reader-ffi`
tests through `cargo ndk test` with `scripts/android-test-runner.ps1` as Cargo's
runner: it pushes each test executable to `/data/local/tmp/simpl-test` (beside
`libpdfium.so`) and runs it there with `TMPDIR` in that folder. Tests that read
repository files via `CARGO_MANIFEST_DIR` are skipped on the device; the list is in
`scripts/android.ps1`. cargo-ndk's own runner does not handle Windows paths.
The layout parity test embeds the pre-move Windows atlas JSON and requires
identical canonical pages and theme adaptations for the portable fixtures.
`CoreSmokeTest` also exercises the generated layout object bindings, page cuts,
adaptation and PDF Book physical pages inside the app process.

Start the emulator with `emulator -avd simpl-api36` (create one with
`avdmanager create avd -n simpl-api36 -k "system-images;android-36;google_apis;x86_64" -d pixel_8`).

## How the Rust core is built

For every variant, `simpl.rust-android` registers:

1. `cargoNdkBuild<Variant>`: `cargo ndk -t arm64-v8a -t x86_64 -P <minSdk> build -p reader-ffi --locked`,
   then copies only `libreader_ffi.so` into `app/build/rust/<variant>/jniLibs`
   (`cargo ndk -o` would also copy dependency cdylibs). Debug uses Cargo's dev
   profile; release uses `android-release` (the desktop release settings, but with
   the symbol table kept). AGP strips the packaged libraries.
2. `uniffiBindgen<Variant>`: `cargo run -p uniffi-bindgen -- generate --library`
   on the x86_64 library into `app/build/rust/<variant>/kotlin`
   (package `io.github.tikkaaa3.simpl.core`, set in `crates/reader-ffi/uniffi.toml`).
3. `stagePdfium` (with `pdfium = true`): runs `scripts/pdfium-android.ps1`, which
   verifies the pinned archives, and packages `jniLibs/<abi>/libpdfium.so`. The core
   loads it by name; notices are staged in `app/build/pdfium/third-party`.

At startup `SimplApplication` calls `initialize(filesDir, cacheDir, language)`: user
data goes to `filesDir/simPl/…` (the desktop profile layout), disposable caches to
`cacheDir/simPl/…`, and the locale picks the code page for legacy non-Unicode text.

UniFFI reads its metadata from the library's symbol table; a stripped library
(the desktop `release` profile) yields "No UniFFI metadata found". The Kotlin
bindings call the library through JNA (`jna@aar`); R8 keep rules are in
`app/proguard-rules.pro`.

Rust changes are picked up by Gradle's up-to-date checks on `crates/`,
`patches/`, `assets/`, `Cargo.toml`, `Cargo.lock` and `rust-toolchain.toml`.
The desktop `scripts\dev.ps1 check` also builds and tests `reader-ffi` on the host.
