# simPl Android 0.1.1

This signed APK update includes P1 through P5, following the first Android MVP.

## Changes

- Read aloud with installed Android speech engines, source highlights, playback
  controls and screen-off listening. Offline voices are preferred; network voices
  require consent.
- Download dictionary packages explicitly, then look up selected words offline.
- Export annotations as Markdown, plain text or JSON; create and restore portable
  backups compatible with supported Windows reading state.
- Switch PDFs between faithful Document pages and a text-oriented Book view.
  Background Book preparation supports cancellation and reusable conversion caches.
- Search inside books and PDF text, navigate exact matches and retain the search
  across rotation. Quickly switch books by title or author while saving your place.
- Use tablet side panels, layouts that avoid separating fold hinges, desktop-style
  keyboard shortcuts, optional volume-key page turns and a keep-screen-on setting.
- Find keyboard help and the privacy policy in Settings. Store listing materials
  are prepared; distribution continues through GitHub APK releases.

## Install or update

Android 8.0 (API 26) or later is required. Most phones should use
`simPl-0.1.1-android-arm64.apk`. The universal APK also supports x86_64 emulators.
Verify downloads with the matching `.sha256` files if desired.

This update uses the same distribution signing certificate as Android 0.1.0 and
raises `versionCode` from 100001 to 101001. Install it over the existing release
to keep app-private books, reading positions and annotations. Local debug builds
use a different signing certificate and cannot receive this signed update.

Distribution certificate SHA-256:
`84b96f425c2db7c74001bb898c4e98e7c790b940b22c1624cb08e1051780cb39`.

## Validation and limits

The P5 source passed 432 Windows tests, 150 shared Android Rust tests and all 65
app instrumentation tests on the accepted API 36 x86_64 emulator. Debug/release
lint reported zero errors and ten existing warnings. Release APK verification
checks signatures, version metadata, ARM64/x86_64 libraries, full license texts,
the compiled Baseline Profile and 16 KB alignment.
The startup and reader/settings Baseline Profile generation tests also passed
after refreshing the profiles for P1-P5. Their page-label helper now tolerates
nodes replaced during Open-with navigation.

Emulator validation is not a physical-phone or foldable performance claim.
Scanned PDFs need an existing text layer for search and Book conversion;
copy-restricted PDFs disable these features. Some scripts use device font
fallbacks. Portable ZIP backups are unencrypted; protect exported files yourself.
Optional speech engines, download hosts, document providers and sharing apps
have their own privacy practices. See the
[privacy policy](https://github.com/Tikkaaa3/simPl-reader/blob/main/docs/privacy.md).
