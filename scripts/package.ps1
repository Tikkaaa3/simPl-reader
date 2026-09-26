# Windows PowerShell 5.1+. Build and package a standalone Windows x64 release directory.
param([switch]$Offline)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$release = Join-Path $root 'target\release'
$portable = Join-Path $root 'target\portable'
$output = Join-Path $portable 'simPl'
$staging = Join-Path $portable ('simPl-' + [guid]::NewGuid().ToString('N'))

& (Join-Path $PSScriptRoot 'dev.ps1') -Command build -Offline:$Offline
# The iced-shell package produces iced-shell.exe; the portable product name is simPl.
$exe = Join-Path $release 'iced-shell.exe'
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "Release executable missing: $exe" }
New-Item -ItemType Directory -Path $portable -Force | Out-Null
New-Item -ItemType Directory -Path $staging | Out-Null
try {
    Copy-Item -LiteralPath $exe -Destination (Join-Path $staging 'simPl.exe')
    & (Join-Path $PSScriptRoot 'pdfium.ps1') -Offline:$Offline -Destination $staging
    & (Join-Path $PSScriptRoot 'collect-licenses.ps1') -Destination (Join-Path $staging 'third-party') -Offline:$Offline
    if (Test-Path -LiteralPath $output) { Remove-Item -LiteralPath $output -Recurse -Force }
    Move-Item -LiteralPath $staging -Destination $output
} finally {
    if (Test-Path -LiteralPath $staging) { Remove-Item -LiteralPath $staging -Recurse -Force }
}
Write-Host "Portable release: $output"
