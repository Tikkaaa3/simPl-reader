# Windows PowerShell 5.1+. Initializes MSVC only in this process and its children.
param(
    [ValidateSet('run', 'check', 'build')]
    [string]$Command = 'run',
    [switch]$Offline,
    [switch]$Large,
    [switch]$Fixture
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (($Large -or $Fixture) -and $Command -ne 'run') { throw '-Large and -Fixture are only valid with run.' }
$root = Split-Path -Parent $PSScriptRoot

# A PowerShell 7 parent can leave Windows PowerShell without its built-in
# module directory. Child checks use Get-FileHash and must load that module.
if ($PSVersionTable.PSVersion.Major -eq 5) {
    $builtinModules = Join-Path $PSHOME 'Modules'
    if (($env:PSModulePath -split ';') -notcontains $builtinModules) {
        $env:PSModulePath = $builtinModules + ';' + $env:PSModulePath
    }
}

# Git also ships a link.exe; finding any command named link is not sufficient.
if ($env:VSCMD_ARG_TGT_ARCH -ne 'x64') {
    $candidates = @()
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (Test-Path -LiteralPath $vswhere) {
        $installations = & $vswhere -all -products '*' -property installationPath
        $candidates += @($installations | ForEach-Object {
            Join-Path $_ 'VC\Auxiliary\Build\vcvars64.bat'
        })
    }
    # A partially registered Build Tools install may not appear in vswhere.
    foreach ($base in @(${env:ProgramFiles(x86)}, $env:ProgramFiles)) {
        $candidates += @(Get-ChildItem -Path "$base\Microsoft Visual Studio\*\*\VC\Auxiliary\Build\vcvars64.bat" -File -ErrorAction SilentlyContinue |
            Sort-Object FullName -Descending | Select-Object -ExpandProperty FullName)
    }
    $vcvars = $candidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
    if (-not $vcvars) {
        throw 'Install Visual Studio C++ Build Tools and a Windows SDK, or use an x64 Native Tools prompt.'
    }
    $environment = & $env:ComSpec /d /c "call `"$vcvars`" && set"
    if ($LASTEXITCODE -ne 0) { throw "MSVC environment initialization failed ($LASTEXITCODE)." }
    foreach ($line in $environment) {
        if ($line -match '^([^=]+)=(.*)$') {
            [Environment]::SetEnvironmentVariable($Matches[1], $Matches[2], 'Process')
        }
    }
    if ($env:VSCMD_ARG_TGT_ARCH -ne 'x64') { throw 'MSVC did not initialize an x64 environment.' }
}

function Invoke-Cargo([string[]]$CargoArgs) {
    Write-Host ('=> cargo ' + ($CargoArgs -join ' '))
    # Cargo writes ordinary progress to stderr; Windows PowerShell must not
    # turn redirected native stderr into a terminating PowerShell exception.
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = $null
    & cargo @CargoArgs
    $code = $global:LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($null -eq $code -or $code -ne 0) { throw "Cargo failed: $($CargoArgs -join ' ') (exit $code)." }
}

Push-Location -LiteralPath $root
try {
    $locked = @('--locked')
    if ($Offline) { $locked += '--offline' }
    switch ($Command) {
        'run' {
            & (Join-Path $PSScriptRoot 'pdfium.ps1') -Offline:$Offline
            $cargoArgs = @('run', '-p', 'iced-shell', '--release') + $locked
            if ($Large) { $cargoArgs += @('--', '--reader-poc-large') }
            elseif ($Fixture) { $cargoArgs += @('--', '--reader-poc') }
            Invoke-Cargo $cargoArgs
        }
        'build' {
            & (Join-Path $PSScriptRoot 'pdfium.ps1') -Offline:$Offline
            Invoke-Cargo (@('build', '--release') + $locked)
        }
        'check' {
            Invoke-Cargo @('fmt', '--all', '--', '--check')
            Invoke-Cargo (@('clippy', '--workspace', '--all-targets') + $locked + @('--', '-D', 'warnings'))
            Invoke-Cargo (@('test', '--workspace', '--all-targets') + $locked)
        }
    }
} finally {
    Pop-Location
}
