# Windows PowerShell 5.1+. Build and package a standalone Windows x64 release directory.
param(
    [switch]$Offline,
    [string]$OutputDirectory = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$release = Join-Path $root 'target\release'
$targetRoot = [IO.Path]::GetFullPath((Join-Path $root 'target'))
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $targetRoot 'portable\simPl' }
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (-not $output.StartsWith($targetRoot + '\', [StringComparison]::OrdinalIgnoreCase) -or
    $output -eq (Join-Path $targetRoot 'release') -or $output -eq (Join-Path $targetRoot 'debug')) {
    throw 'Package output must be a dedicated subdirectory under target, not the build directory.'
}
$portable = Split-Path -Parent $output
$checkPath = $output
while ($checkPath -and $checkPath.StartsWith($targetRoot, [StringComparison]::OrdinalIgnoreCase)) {
    if ((Test-Path -LiteralPath $checkPath) -and
        ((Get-Item -LiteralPath $checkPath -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "Refusing to replace a package through a directory link: $checkPath"
    }
    $checkPath = Split-Path -Parent $checkPath
}
$staging = Join-Path $portable ('simPl-' + [guid]::NewGuid().ToString('N'))

& (Join-Path $PSScriptRoot 'dev.ps1') -Command build -Offline:$Offline
# The iced-shell package produces iced-shell.exe; the portable product name is simPl.
$exe = Join-Path $release 'iced-shell.exe'
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "Release executable missing: $exe" }
New-Item -ItemType Directory -Path $portable -Force | Out-Null
New-Item -ItemType Directory -Path $staging | Out-Null
try {
    Copy-Item -LiteralPath $exe -Destination (Join-Path $staging 'simPl.exe')
    # Official builds ship under the binary terms; the source license stays in the repository.
    Copy-Item -LiteralPath (Join-Path $root 'LICENSE-BINARY.txt') -Destination (Join-Path $staging 'LICENSE.txt')
    & (Join-Path $PSScriptRoot 'pdfium.ps1') -Offline:$Offline -Destination $staging
    & (Join-Path $PSScriptRoot 'collect-licenses.ps1') -Destination (Join-Path $staging 'third-party') -Offline:$Offline
    # Font binaries are embedded in simPl.exe; ship their notices alongside Rust notices.
    $fontNotices = Join-Path $staging 'third-party\fonts'
    New-Item -ItemType Directory -Path $fontNotices | Out-Null
    foreach ($notice in @('Geist-OFL.txt', 'Inter-OFL.txt', 'Literata-OFL.txt', 'Spectral-OFL.txt', 'FiraSans-OFL.txt', 'Material-Symbols-LICENSE.txt', 'Material-Symbols-CHANGES.txt', 'Typeface-SOURCES.txt')) {
        $source = Join-Path $root "assets\licenses\$notice"
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Bundled font notice missing: $source" }
        Copy-Item -LiteralPath $source -Destination (Join-Path $fontNotices $notice)
    }
    # Dictionary data is embedded; its CC BY-SA rights and provenance are separate.
    $dictionaryNotices = Join-Path $staging 'third-party\dictionaries'
    New-Item -ItemType Directory -Path $dictionaryNotices | Out-Null
    foreach ($notice in @('README.md', 'CC-BY-SA-4.0.txt', 'manifest.json')) {
        Copy-Item -LiteralPath (Join-Path $root "assets\dictionaries\$notice") -Destination (Join-Path $dictionaryNotices $notice)
    }
    if (Test-Path -LiteralPath $output) { Remove-Item -LiteralPath $output -Recurse -Force }
    Move-Item -LiteralPath $staging -Destination $output
} finally {
    if (Test-Path -LiteralPath $staging) { Remove-Item -LiteralPath $staging -Recurse -Force }
}
Write-Host "Portable release: $output"
