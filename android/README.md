# simPl for Android

The Android app is a Kotlin + Jetpack Compose interface over the shared Rust
core. The plan, decisions and milestones are in
[android-roadmap.md](../android-roadmap.md). The Windows reader is unaffected:
its build, scripts and release workflow do not use anything in this folder.

| Part | Responsibility |
| --- | --- |
| [`crates/reader-ffi`](../crates/reader-ffi) | UniFFI surface of the Rust core; the only Rust API the app sees |
| [`crates/reader-profile`](../crates/reader-profile) | Data/cache roots: `LOCALAPPDATA` on the desktop, `filesDir`/`cacheDir` on Android |
| [`crates/uniffi-bindgen`](../crates/uniffi-bindgen) | Workspace-pinned binding generator (same version as the `uniffi` runtime) |
| `build-logic/` | Gradle plugin `simpl.rust-android`: cargo-ndk build and Kotlin binding generation per variant |
| `app/` | Compose application (`io.github.tikkaaa3.simpl`) |

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

`test` runs the `reader-profile`, `reader-document`, `reader-pdf` and `reader-ffi`
tests through `cargo ndk test` with `scripts/android-test-runner.ps1` as Cargo's
runner: it pushes each test executable to `/data/local/tmp/simpl-test` (beside
`libpdfium.so`) and runs it there with `TMPDIR` in that folder. Tests that read
repository files via `CARGO_MANIFEST_DIR` are skipped on the device; the list is in
`scripts/android.ps1`. cargo-ndk's own runner does not handle Windows paths.

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
