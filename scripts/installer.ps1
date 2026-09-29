# Windows PowerShell 5.1+. Produce a self-contained Windows x64 web download.
param(
    [ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version = '0.1.1',
    [switch]$Offline,
    [string]$CompilerPath = '',
    [string]$SignToolCommand = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $CompilerPath) {
    $candidates = @(
        $env:ISCC,
        (Join-Path $root 'target\tools\inno-6.7.3\ISCC.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'),
        (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe')
    )
    $CompilerPath = $candidates | Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Leaf) } | Select-Object -First 1
}
if (-not $CompilerPath -or -not (Test-Path -LiteralPath $CompilerPath -PathType Leaf)) {
    throw 'Install Inno Setup 6.7.3 and pass -CompilerPath to ISCC.exe. See installer/README.md.'
}
# The .iss preprocessor checks the compiler version; ISCC.exe itself does not
# carry a useful Windows ProductVersion resource.

$payload = Join-Path $root 'target\installer-payload'
$output = Join-Path $root 'target\installer'
# Keep the running portable copy intact; prepare a separate, fresh payload.
& (Join-Path $PSScriptRoot 'package.ps1') -Offline:$Offline -OutputDirectory $payload
New-Item -ItemType Directory -Path $output -Force | Out-Null
$arguments = @("/DAppVersion=$Version", "/DPayloadDir=$payload", "/DOutputDir=$output")
if ($SignToolCommand) { $arguments += @('/DSignInstaller', "/Srelease=$SignToolCommand") }
$arguments += (Join-Path $root 'installer\simPl.iss')
& $CompilerPath @arguments
if ($LASTEXITCODE -ne 0) { throw "Inno Setup compilation failed ($LASTEXITCODE)." }

$name = "simPl-$Version-windows-x64-setup.exe"
$file = Join-Path $output $name
$hash = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
$signature = (Get-AuthenticodeSignature -LiteralPath $file).Status.ToString()
if ($SignToolCommand -and $signature -ne 'Valid') { throw "Installer signature is not valid: $signature" }
[IO.File]::WriteAllText("$file.sha256", "$hash  $name`n", (New-Object Text.UTF8Encoding($false)))
$manifest = [ordered]@{
    product = 'simPl Reader'
    version = $Version
    platform = 'windows-x64'
    minimumWindows = '10'
    filename = $name
    bytes = (Get-Item -LiteralPath $file).Length
    sha256 = $hash
    authenticode = $signature
}
[IO.File]::WriteAllText((Join-Path $output 'release.json'), ($manifest | ConvertTo-Json) + "`n", (New-Object Text.UTF8Encoding($false)))
Write-Host "Website download: $file"
Write-Host "SHA256: $hash"
Write-Host "Signature: $signature"
