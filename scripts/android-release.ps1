# Produce and verify signed APKs. Secrets are read from the process or local DPAPI storage.
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [switch]$Offline,
    [switch]$Universal,
    # Prepare a signed Play upload artifact without publishing it.
    [switch]$Bundle
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if ($Version -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') { throw 'Version must be x.y.z.' }
$parts = @($Version.Split('.') | ForEach-Object { [int]$_ })
if ($parts[0] -gt 209 -or $parts[1] -gt 99 -or $parts[2] -gt 99) { throw 'Version exceeds the Android versionCode range.' }
$notes = Join-Path $root "docs\releases\android-$Version.md"
if (-not (Test-Path -LiteralPath $notes -PathType Leaf)) { throw "Missing release notes: $notes" }
& (Join-Path $PSScriptRoot 'android-signing.ps1') load
$sdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { Join-Path $env:LOCALAPPDATA 'Android\Sdk' }
$buildTools = Get-ChildItem -LiteralPath (Join-Path $sdk 'build-tools') -Directory |
    Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'apksigner.bat') } |
    Sort-Object { [version]($_.Name -replace '-.*$', '') } -Descending | Select-Object -First 1
if (-not $buildTools) { throw 'Android build-tools with apksigner are required.' }
$gradleArguments = @('-p', (Join-Path $root 'android'), ':app:lintRelease', ':app:assembleRelease', "-PsimplVersion=$Version", '-PsimplSplitApks=true')
if ($Offline) { $gradleArguments += '--offline' }
& (Join-Path $root 'android\gradlew.bat') @gradleArguments
if ($LASTEXITCODE -ne 0) { throw 'Android release build failed.' }
$destination = Join-Path $root "target\android-release\$Version"
New-Item -ItemType Directory -Path $destination -Force | Out-Null
$architectures = @(@{ Name = 'arm64'; Split = 'arm64-v8a' })
if ($Universal) { $architectures += @{ Name = 'universal'; Split = 'universal' } }
$metadata = Get-Content -LiteralPath (Join-Path $root 'android\app\build\outputs\apk\release\output-metadata.json') -Raw | ConvertFrom-Json
$code = $parts[0] * 10000000 + $parts[1] * 100000 + $parts[2] * 1000 + 1
Add-Type -AssemblyName System.IO.Compression.FileSystem
foreach ($architecture in $architectures) {
    $apk = Join-Path $root ("android\app\build\outputs\apk\release\app-{0}-release.apk" -f $architecture.Split)
    if (-not (Test-Path -LiteralPath $apk -PathType Leaf)) { throw "Missing signed APK: $apk" }
    $entry = @($metadata.elements | Where-Object { $_.outputFile -eq (Split-Path -Leaf $apk) })
    if ($entry.Count -ne 1 -or $entry[0].versionName -ne $Version -or $entry[0].versionCode -ne $code) { throw 'APK version metadata does not match the requested release.' }
    & (Join-Path $buildTools.FullName 'apksigner.bat') verify --verbose --print-certs $apk
    if ($LASTEXITCODE -ne 0) { throw 'APK signature verification failed.' }
    $zip = [IO.Compression.ZipFile]::OpenRead($apk)
    try {
        $files = @($zip.Entries | ForEach-Object { $_.FullName })
        foreach ($required in @('assets/licenses/index.json', 'assets/dexopt/baseline.prof', 'assets/dexopt/baseline.profm',
            'lib/arm64-v8a/libreader_ffi.so', 'lib/arm64-v8a/libpdfium.so')) {
            if ($files -notcontains $required) { throw "Release APK is missing $required" }
        }
        $reader = New-Object IO.StreamReader($zip.GetEntry('assets/licenses/index.json').Open())
        try { $notices = ConvertFrom-Json -InputObject $reader.ReadToEnd() } finally { $reader.Dispose() }
        if ($notices.Count -eq 0) { throw 'Release APK has an empty license index.' }
        foreach ($notice in $notices) {
            $text = $zip.GetEntry("assets/$($notice.path)")
            if (-not $text -or $text.Length -eq 0) { throw "Release APK is missing full notice text: $($notice.path)" }
        }
        if ($zip.GetEntry('assets/dexopt/baseline.prof').Length -ge 1500000) { throw 'Compiled Baseline Profile exceeds the size limit.' }
        if ($architecture.Name -eq 'arm64' -and @($files | Where-Object { $_ -match '^lib/(?!arm64-v8a/)' }).Count -gt 0) { throw 'The arm64 artifact contains another ABI.' }
        if ($architecture.Name -eq 'universal') {
            foreach ($library in @('libreader_ffi.so', 'libpdfium.so')) {
                if ($files -notcontains "lib/x86_64/$library") { throw "Universal APK is missing x86_64/$library." }
            }
        }
    } finally { $zip.Dispose() }
    $name = "simPl-$Version-android-$($architecture.Name).apk"
    $output = Join-Path $destination $name
    Copy-Item -LiteralPath $apk -Destination $output -Force
    $hash = (Get-FileHash -LiteralPath $output -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText("$output.sha256", "$hash  $name`n", (New-Object Text.UTF8Encoding($false)))
}
Copy-Item -LiteralPath $notes -Destination (Join-Path $destination 'release-notes.md') -Force
if ($Bundle) {
    # AGP cannot shrink ABI-split APK resources and an App Bundle in one build.
    # Copy/verify APKs first, then rebuild the bundle with APK splitting disabled.
    $bundleArguments = @('-p', (Join-Path $root 'android'), ':app:bundleRelease', "-PsimplVersion=$Version", '-PsimplSplitApks=false')
    if ($Offline) { $bundleArguments += '--offline' }
    & (Join-Path $root 'android\gradlew.bat') @bundleArguments
    if ($LASTEXITCODE -ne 0) { throw 'Android App Bundle build failed.' }
    $bundleSource = Join-Path $root 'android\app\build\outputs\bundle\release\app-release.aab'
    if (-not (Test-Path -LiteralPath $bundleSource -PathType Leaf)) { throw 'Missing release App Bundle.' }
    $jarsigner = Join-Path $env:JAVA_HOME 'bin\jarsigner.exe'
    $verification = & $jarsigner -verify $bundleSource 2>&1
    if ($LASTEXITCODE -ne 0 -or ($verification -join ' ') -notmatch 'jar verified') { throw 'App Bundle signature verification failed.' }
    $zip = [IO.Compression.ZipFile]::OpenRead($bundleSource)
    try {
        foreach ($required in @('base/manifest/AndroidManifest.xml', 'base/assets/licenses/index.json',
            'base/lib/arm64-v8a/libreader_ffi.so', 'base/lib/arm64-v8a/libpdfium.so',
            'base/lib/x86_64/libreader_ffi.so', 'base/lib/x86_64/libpdfium.so')) {
            if (-not $zip.GetEntry($required)) { throw "App Bundle is missing $required" }
        }
    } finally { $zip.Dispose() }
    $name = "simPl-$Version-android.aab"
    $output = Join-Path $destination $name
    Copy-Item -LiteralPath $bundleSource -Destination $output -Force
    $hash = (Get-FileHash -LiteralPath $output -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText("$output.sha256", "$hash  $name`n", (New-Object Text.UTF8Encoding($false)))
}
Write-Host "Verified signed Android artifacts: $destination"
