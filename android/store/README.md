# Play Store preparation

P5 prepares distribution materials; it does not enroll an account, upload an
artifact, change signing identity or publish a Play release. GitHub APK
distribution remains the current channel. A store launch is a separate decision.

## Prepared materials

- English title, short description and full description live beside this file.
- [Privacy policy](../../docs/privacy.md); after pushing, its public HTML URL is
  `https://github.com/Tikkaaa3/simPl-reader/blob/main/docs/privacy.md`.
- Store icon and feature graphic are reproducible from repository artwork with
  `python scripts/android-store-assets.py`. Pillow is needed for raster output.
- `scripts/android-release.ps1 -Version <x.y.z> -Bundle` builds and verifies a
  signed APK and App Bundle using the existing local signing workflow. Nothing
  is uploaded. Artifacts and SHA-256 files live under `target/android-release`.

## Launch decisions still required

Choose the publisher's verified Play Console account, support email, countries,
pricing and testing track. Review the store copy and privacy URL under the final
publisher identity. Use an unused versionCode and decide the Play App Signing
enrollment/upload-key arrangement before the first upload; preserve compatibility
with the existing GitHub signing certificate if cross-channel upgrades are wanted.
Complete any testing requirements shown for that account. Actual submission and
publication need a separate authorization.

The app category is Books & Reference; it has no ads or login. Reviewer access
instructions: import a DRM-free file through the picker, open it, and use Settings
for optional speech/dictionary features. Supply an authored sample book if asked.
Content rating, target audience and countries are publisher decisions; do not
invent an age rating before the Play questionnaire is completed.

## Data safety review

The first-party application has no telemetry SDK or backend and processes library,
notes, search and dictionary lookups on device. Local processing alone is not a
claim that every optional provider does the same: explicitly requested downloads
expose connection metadata to their package host; selected network speech engines
can receive book text with consent; cloud document providers and sharing apps can
process chosen content. Review these flows, the shipped dependency graph and the
current Play definitions before submitting the Data safety form. Do not submit a
blanket “no data collected” response without that review. Backups are unencrypted.

The mediaPlayback foreground service implements user-started read aloud with a
visible notification and stop control. Include that user-visible flow in any
foreground-service declaration requested by Play Console. The app uses the system
document picker, not broad storage permissions.

## Screenshots and validation

Capture the final release build with authored books and no personal data. At least
two phone screenshots are required; prepare four tablet screenshots in each
chosen tablet category when targeting large-screen promotion. Show library,
reading, annotations/search and PDF Document/Book behavior. Use the current Play
asset requirements for dimensions/aspect ratios and provide short alt text.
P5 emulator screenshots are test evidence under `target/p5-visual`, not automatic
store publication. Review them and recapture store screenshots after selecting
the final release version.

Before upload, run the Windows workspace check, shared Android tests, the full
instrumentation suite, debug/release lint, R8 release assembly and bundle build.
Check both ARM64/x86_64 native libraries, licenses and the existing 16 KB alignment
check. Play's pre-launch report, real tablet/foldable and physical-phone validation
remain checks of the actual store candidate.

Official references checked on 2026-10-04:
[listing assets](https://support.google.com/googleplay/android-developer/answer/9866151),
[Data safety](https://support.google.com/googleplay/android-developer/answer/10787469),
[prepare for review](https://support.google.com/googleplay/android-developer/answer/9859455).
