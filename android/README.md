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

# Cache, verify and stage the pinned Android PDFium (app\build\pdfium).
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\android.ps1 pdfium
```

The staged PDFium is not packaged into the APK yet; the core starts using it
when the document crates are ported (M1).

Start the emulator with `emulator -avd simpl-api36` (create one with
`avdmanager create avd -n simpl-api36 -k "system-images;android-36;google_apis;x86_64" -d pixel_8`).

## How the Rust core is built

For every variant, `simpl.rust-android` registers:

1. `cargoNdkBuild<Variant>`: `cargo ndk -t arm64-v8a -t x86_64 -P <minSdk> build -p reader-ffi --locked`,
   then copies only `libreader_ffi.so` into `app/build/rust/<variant>/jniLibs`
   (`cargo ndk -o` would also copy any dependency cdylibs). Debug uses Cargo's dev
   profile; release uses `android-release` (the desktop release settings, but with
   the symbol table kept). AGP strips the packaged libraries.
2. `uniffiBindgen<Variant>`: `cargo run -p uniffi-bindgen -- generate --library`
   on the x86_64 library into `app/build/rust/<variant>/kotlin`
   (package `io.github.tikkaaa3.simpl.core`, set in `crates/reader-ffi/uniffi.toml`).
UniFFI reads its metadata from the library's symbol table; a stripped library
(the desktop `release` profile) yields "No UniFFI metadata found". The Kotlin
bindings call the library through JNA (`jna@aar`); R8 keep rules are in
`app/proguard-rules.pro`.

Rust changes are picked up by Gradle's up-to-date checks on `crates/`,
`patches/`, `assets/`, `Cargo.toml`, `Cargo.lock` and `rust-toolchain.toml`.
The desktop `scripts\dev.ps1 check` also builds and tests `reader-ffi` on the host.
