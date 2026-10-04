# Windows PowerShell 5.1+. Cache the exact Android PDFium releases (same Chromium
# build as the desktop pdfium.ps1) and stage libpdfium.so per ABI with its notices.
param(
    [switch]$Offline,
    [string]$Destination = (Join-Path (Split-Path -Parent $PSScriptRoot) 'android\app\build\pdfium')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$cache = Join-Path $root 'target\native'
# Android ABI -> pinned bblanchon/pdfium-binaries archive for chromium/8066.
$pins = @(
    @{ Abi = 'arm64-v8a'; Name = 'pdfium-android-arm64'; Length = 3385698
       Hash = 'A665E3A9D40FB0024E3959261A400A722A13F7AA6602C59A82C93D44A362C055' },
    @{ Abi = 'x86_64'; Name = 'pdfium-android-x64'; Length = 3527656
       Hash = '44B9444D58F055AB892019AEA27423BAB74D671A37B5BAD84647A91ABEC82A1D' }
)

function Test-PinnedArchive([string]$Path, $Pin) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    if ((Get-Item -LiteralPath $Path).Length -ne $Pin.Length) { return $false }
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -eq $Pin.Hash
}

New-Item -ItemType Directory -Path $cache -Force | Out-Null
$legalStaged = $false
foreach ($pin in $pins) {
    $archive = Join-Path $cache ($pin.Name + '-8066.tgz')
    $url = 'https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/8066/' + $pin.Name + '.tgz'
    if (-not (Test-PinnedArchive $archive $pin)) {
        if ($Offline) { throw "Pinned PDFium archive missing or corrupt in offline mode: $archive" }
        $download = Join-Path $cache ('pdfium-' + [guid]::NewGuid().ToString('N') + '.tmp')
        try {
            [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
            Invoke-WebRequest -Uri $url -OutFile $download -UseBasicParsing
            if (-not (Test-PinnedArchive $download $pin)) { throw "Downloaded PDFium archive did not match pinned length/SHA256: $url" }
            Move-Item -LiteralPath $download -Destination $archive -Force
        } finally {
            if (Test-Path -LiteralPath $download) { Remove-Item -LiteralPath $download -Force }
        }
    }

    # Fresh extraction of only the runtime and its legal notices.
    $extract = Join-Path $cache ('extract-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $extract | Out-Null
    try {
        # Windows' bsdtar by full path: a Git Bash PATH would otherwise pick GNU tar,
        # which reads "C:" in the archive path as a remote host.
        & (Join-Path $env:SystemRoot 'System32\tar.exe') -xzf $archive -C $extract lib/libpdfium.so LICENSE licenses
        if ($LASTEXITCODE -ne 0) { throw "Could not extract the pinned PDFium archive: $archive" }
        $library = Join-Path $extract 'lib\libpdfium.so'
        $license = Join-Path $extract 'LICENSE'
        $notices = Join-Path $extract 'licenses'
        if (-not (Test-Path -LiteralPath $library -PathType Leaf) -or
            -not (Test-Path -LiteralPath $license -PathType Leaf) -or
            -not (Test-Path -LiteralPath (Join-Path $notices 'pdfium.txt') -PathType Leaf)) {
            throw "Pinned PDFium archive is missing its library or legal notices: $archive"
        }
        $abiDir = Join-Path $Destination ('jniLibs\' + $pin.Abi)
        New-Item -ItemType Directory -Path $abiDir -Force | Out-Null
        Copy-Item -LiteralPath $library -Destination (Join-Path $abiDir 'libpdfium.so') -Force
        # Both ABIs come from one PDFium revision; stage its notices once.
        if (-not $legalStaged) {
            $legal = Join-Path $Destination 'third-party\pdfium'
            if (Test-Path -LiteralPath $legal) { Remove-Item -LiteralPath $legal -Recurse -Force }
            New-Item -ItemType Directory -Path $legal | Out-Null
            Copy-Item -LiteralPath $license -Destination (Join-Path $legal 'LICENSE')
            Copy-Item -LiteralPath $notices -Destination (Join-Path $legal 'licenses') -Recurse
            $legalStaged = $true
        }
    } finally {
        Remove-Item -LiteralPath $extract -Recurse -Force
    }
}
Write-Host "PDFium 156.0.8066.0 (Android) staged: $Destination"
