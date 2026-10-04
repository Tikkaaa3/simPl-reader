# simPl Android 0.1.0

First Android MVP release, distributed as a signed APK on GitHub.

- Import EPUB, PDF, HTML, TXT and Markdown from a storage provider, Open with
  or Share. HTML folder imports retain sibling resources.
- Browse, search and organize a private library with favorites and shelves.
- Read reflowable documents with the shared desktop canonical page numbers,
  contents, internal links, themes, typography, zoom and saved reading position.
- Read original PDF pages with zoom, text selection and bounded raster caching.
- Copy/share selections and keep bookmarks, four-color highlights and notes.
- Adaptive launcher icon, splash screen, actionable file/storage/save errors,
  large-font layout adjustments and searchable offline license notices.
- R8/resource shrinking and a recorded Baseline Profile for startup and common
  reader/settings journeys.

## Install

Android 8.0 (API 26) or later is required. Most phones use
`simPl-0.1.0-android-arm64.apk`; the optional universal APK also supports x86_64
emulators. Download from the official GitHub release, allow installation from
the browser/file manager when Android asks, then open the APK. Compare its
SHA-256 with the companion `.sha256` file if verifying the download.

Updates must use the same signing certificate. This release has a distribution
certificate distinct from local debug builds. Keep the debug app's private data
before uninstalling it; Android cannot install a differently signed update over it.
The app does not yet offer backup/export.

## Validation and current limits

M7 uses the API 36 x86_64 emulator acceptance agreed for M5/M6. Measurements are
diagnostic emulator results, not physical-phone performance claims. Five-format
opening, canonical layout parity and persistent position/annotations are covered
by shared-core and instrumented UI tests. Windows checks and release compilation
remain required alongside Android lint and signed APK verification.

The 2026-10-04 emulator run passed 33 instrumented app tests, 131 shared Rust
tests, two Baseline Profile generation tests and two release benchmarks. Windows
checks passed 423 tests and the Windows release build passed. With a required
Baseline Profile, five cold starts measured a median 1242.13 ms to initial display.
Five page turns measured a median 0.2501 ms for the native page fetch; frame CPU
duration P50/P90 was 38.15/56.72 ms and deadline overrun P50/P90 was 27.03/63.63 ms.
The emulator missed frame deadlines; native page-fetch time is not end-to-end
page-turn latency. See `android/README.md` for reproduction and measurement scope.

Release lint passed with zero errors (nine warnings). Both signed APKs contain
700 legal/provenance documents and a 10,108-byte compiled Baseline Profile.
Signature, version (`versionCode=100001`), ABI and full-notice validation passed.
The signed universal APK installed and opened the library, settings, notice
index and full license text on the emulator.

| Locally validated artifact | Size |
| --- | --- |
| `simPl-0.1.0-android-arm64.apk` | 27,198,225 bytes |
| `simPl-0.1.0-android-universal.apk` | 49,247,494 bytes |

CI rebuilds emit their own SHA-256 sidecars; compare a downloaded APK with the
sidecar attached to that build.

Distribution signing certificate SHA-256:
`84b96f425c2db7c74001bb898c4e98e7c790b940b22c1624cb08e1051780cb39`.

Read aloud, dictionaries, backup/export and PDF Book conversion view are planned
after the MVP. Books and annotations stay in the app's private storage; removing
the app removes that storage. No network or broad storage permission is required
for reading.
