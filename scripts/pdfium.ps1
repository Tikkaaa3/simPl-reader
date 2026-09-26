# Windows PowerShell 5.1+. Cache the exact Windows x64 PDFium release and stage its DLL and notices.
param(
    [switch]$Offline,
    [string]$Destination = (Join-Path (Split-Path -Parent $PSScriptRoot) 'target\release')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not [Environment]::Is64BitOperatingSystem -or -not [Environment]::Is64BitProcess) {
    throw 'The pinned PDFium binary requires 64-bit Windows and 64-bit PowerShell.'
}
$root = Split-Path -Parent $PSScriptRoot
$cache = Join-Path $root 'target\native'
$archive = Join-Path $cache 'pdfium-win-x64-8066.tgz'
$url = 'https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/8066/pdfium-win-x64.tgz'
$expectedHash = '739a57d597d864297909cc40a2411eba728490c76a0fa25e3ea299c7f6b07020'
$expectedLength = 3823498

function Test-PinnedArchive([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    if ((Get-Item -LiteralPath $Path).Length -ne $expectedLength) { return $false }
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -eq $expectedHash
}

New-Item -ItemType Directory -Path $cache -Force | Out-Null
if (-not (Test-PinnedArchive $archive)) {
    if ($Offline) { throw "Pinned PDFium archive missing or corrupt in offline mode: $archive" }
    $download = Join-Path $cache ('pdfium-' + [guid]::NewGuid().ToString('N') + '.tmp')
    try {
        # TLS 1.2 is required by the release host on Windows PowerShell 5.1.
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -Uri $url -OutFile $download -UseBasicParsing
        if (-not (Test-PinnedArchive $download)) { throw "Downloaded PDFium archive did not match pinned length/SHA256: $url" }
        Move-Item -LiteralPath $download -Destination $archive -Force
    } finally {
        if (Test-Path -LiteralPath $download) { Remove-Item -LiteralPath $download -Force }
    }
}

# Extract only the runtime and every license supplied by the verified archive.
# A fresh extraction also prevents stale/corrupted extracted caches being packaged.
$extract = Join-Path $cache ('extract-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $extract | Out-Null
try {
    & tar.exe -xzf $archive -C $extract bin/pdfium.dll LICENSE licenses
    if ($LASTEXITCODE -ne 0) { throw 'Could not extract the pinned PDFium archive.' }
    $dll = Join-Path $extract 'bin\pdfium.dll'
    $license = Join-Path $extract 'LICENSE'
    $notices = Join-Path $extract 'licenses'
    if (-not (Test-Path -LiteralPath $dll -PathType Leaf) -or
        -not (Test-Path -LiteralPath $license -PathType Leaf) -or
        -not (Test-Path -LiteralPath (Join-Path $notices 'pdfium.txt') -PathType Leaf)) {
        throw 'Pinned PDFium archive is missing its DLL or legal notices.'
    }
    New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    $targetDll = Join-Path $Destination 'pdfium.dll'
    # A running reader may have this DLL mapped: leave an already identical binary alone.
    if (-not (Test-Path -LiteralPath $targetDll -PathType Leaf) -or
        (Get-FileHash -LiteralPath $targetDll -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $dll -Algorithm SHA256).Hash) {
        Copy-Item -LiteralPath $dll -Destination $targetDll -Force
    }
    $legal = Join-Path $Destination 'third-party\pdfium'
    if (Test-Path -LiteralPath $legal) { Remove-Item -LiteralPath $legal -Recurse -Force }
    New-Item -ItemType Directory -Path $legal | Out-Null
    Copy-Item -LiteralPath $license -Destination (Join-Path $legal 'LICENSE')
    Copy-Item -LiteralPath $notices -Destination (Join-Path $legal 'licenses') -Recurse
    Write-Host "PDFium 156.0.8066.0 staged: $Destination"
} finally {
    Remove-Item -LiteralPath $extract -Recurse -Force
}
