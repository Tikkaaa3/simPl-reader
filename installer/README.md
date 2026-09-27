# Windows installer

`scripts/installer.ps1` builds a single, self-contained Windows x64 setup executable
for website downloads. The default installer release version is **0.1.0**; this is
independent of the internal Cargo workspace's development version.

## User experience

- English and Turkish wizard, selected from the Windows language with a language
  chooser. The wizard follows the Windows light/dark appearance.
- Per-user installation in `%LOCALAPPDATA%\Programs\simPl`; no administrator prompt.
  A different application folder can be selected.
- Optional desktop shortcut (off initially) and Start menu shortcuts (on initially).
- Optional launch at the end. Silent installation never launches the reader.
- A **simPl Reader** entry in Windows Settings → Apps → Installed apps, plus an
  uninstall shortcut when Start menu shortcuts are selected.
- Stable application identity across versions. Reinstalling/upgrading replaces
  program files and retains the library, preferences and saved reading positions.
- The reader holds `Local\simPl.Reader.Running` for its lifetime. Setup and Uninstall
  ask users to close running copies normally before proceeding.

Uninstall asks whether to **keep the library**, **delete the library**, or cancel.
Keeping it is the default. The delete option removes the fixed
`%LOCALAPPDATA%\simPl` profile, including imported copies, favourites, positions,
preferences and caches. It never follows paths from `library.json` to delete source
documents. Directory junction targets are not traversed. A failed deletion reports
the remaining profile folder instead of claiming the library was fully removed.

Installed and portable copies share that profile. Keeping it preserves data for a
future reinstall; deleting it also removes the library used by portable copies.
No default file associations or startup entries are changed.

## Build

Prerequisites are the normal reader build tools and **Inno Setup 6.7.3** from the
[official download page](https://jrsoftware.org/isdl.php). The compiler is pinned
in `simPl.iss`. Observe Inno Setup's licensing terms for commercial builds.

The official compiler installer is available at:
`https://github.com/jrsoftware/issrc/releases/download/is-6_7_3/innosetup-6.7.3.exe`

Its SHA-256 is
`9c73c3bae7ed48d44112a0f48e66742c00090bdb5bef71d9d3c056c66e97b732`;
the publisher signature was verified as Pyrsys B.V. during setup.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\installer.ps1 -Version 0.1.0

# With cached dependencies and an explicit compiler location:
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\installer.ps1 `
  -Version 0.1.0 -Offline -CompilerPath 'C:\Tools\Inno Setup 6\ISCC.exe'
```

The script creates a fresh payload in `target\installer-payload`, leaving the
portable folder intact. Only the reader, PDFium, icon and third-party notices are
included. Test books, the compiler, user data and other portable executables are
not included.

Outputs in `target\installer`:

| File | Purpose |
| --- | --- |
| `simPl-0.1.0-windows-x64-setup.exe` | Website download; runs without downloading runtime dependencies |
| `simPl-0.1.0-windows-x64-setup.exe.sha256` | SHA-256 checksum for the exact executable |
| `release.json` | Version, platform, filename, byte count, checksum and signature status |

Host the executable unchanged over HTTPS and point the website's Windows download
button to it. The checksum and manifest can be published alongside it. Building
these files does not upload them or publish a GitHub release.

### Signing

The default local build is **unsigned**. For a signed public release, supply your
own certificate-backed signing command through `-SignToolCommand`. This enables
Inno's `release` SignTool and signs both Setup and its generated uninstaller.
The build fails if the resulting setup signature is not valid.

In an initialized signing environment, an example command string is:

```powershell
$signingCommand = 'signtool.exe sign /sha1 CERTIFICATE_THUMBPRINT /fd SHA256 /tr HTTPS_TIMESTAMP_SERVICE /td SHA256 $f'
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\installer.ps1 `
  -Version 0.1.0 -Offline -SignToolCommand $signingCommand
```

Replace the placeholders with your certificate and timestamp service. Keep private
keys and credentials outside the repository. Inno substitutes `$f` with the file
being signed. See its [SignTool documentation](https://jrsoftware.org/ishelp/topic_setup_signtool.htm).
An unsigned package can display an unknown-publisher or SmartScreen prompt; signing
does not itself guarantee SmartScreen reputation.

## Unattended use

```powershell
# No launch; select both shortcuts. Standard Inno /DIR and /LANG are supported.
.\simPl-0.1.0-windows-x64-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /TASKS="desktopicon,startmenuicon"
```

Use the registered `QuietUninstallString` to remove the app silently. Silent
uninstall **keeps user data** by default. Administrators explicitly opting to delete
the whole simPl profile may pass `/PURGEUSERDATA` to the registered uninstaller.
Do not guess its filename: Inno may use `unins000.exe`, `unins001.exe`, etc.

## Verification

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\test-installer.ps1
```

Run after building the payload. Override `-CompilerPath` or `-PayloadDirectory` as
needed. The test compiles the same installer with a distinct **Installer QA** AppId,
temporary installation folder and a compile-time test profile rooted under `target`.
Only that QA build accepts the test profile definition; the shipped script's
production path is fixed and has no runtime data-directory override.

The test performs real installs, launches the installed reader, verifies the running
app guard, checks optional shortcuts and Windows registration, upgrades 0.1.0 to
0.1.1, uninstalls while retaining data, reinstalls, then uninstalls with explicit
deletion. A synthetic original file outside the profile and a junction to it must
survive. Temporary QA shortcuts and registration are removed; logs remain under
`target\installer-tests`. Tests refuse to overwrite an existing QA installation.

Interactive wizard appearance and independent clean-Windows qualification still
require a desktop/host pass. The native Computer Use helper was unavailable in the
build session; automated installation tests do not claim interactive visual QA.

The project-authored icon is in `simPl.svg` with a multi-resolution `simPl.ico` for
Setup, shortcuts and the Installed apps entry. The installer uses no downloaded
branding assets.
