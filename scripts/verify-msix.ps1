# Run in an elevated (Administrator) PowerShell window to run WACK and optionally
# sideload the self-signed local test MSIX for manual testing.
param(
    [ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version = '0.1.7',
    [switch]$Install,
    [switch]$Cleanup
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).
    IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    throw 'Run this script in an Administrator PowerShell window (WACK and local certificate trust require elevation).'
}

$root = Split-Path -Parent $PSScriptRoot
$msixDir = Join-Path $root 'target\msix'
$localMsix = Join-Path $msixDir "simPl-$Version-windows-x64-local-test.msix"
$cerPath = Join-Path $msixDir 'simPl-local-test.cer'
$reportPath = Join-Path $msixDir 'wack-report.xml'
$appcert = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\App Certification Kit\appcert.exe'

if ($Cleanup) {
    Get-AppxPackage -Name 'Tikka.simplreader' -ErrorAction SilentlyContinue | Remove-AppxPackage
    if (Test-Path -LiteralPath $cerPath -PathType Leaf) {
        $certObj = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2($cerPath)
        Get-ChildItem Cert:\LocalMachine\TrustedPeople | Where-Object { $_.Thumbprint -eq $certObj.Thumbprint } | Remove-Item -Force
    }
    Write-Host 'Removed local test MSIX install and local test certificate from TrustedPeople.'
    exit 0
}

if (-not (Test-Path -LiteralPath $localMsix -PathType Leaf)) {
    throw "Local test MSIX not found: $localMsix. Run scripts\msix.ps1 -Version $Version -SignForLocalTest first."
}
if (-not (Test-Path -LiteralPath $cerPath -PathType Leaf)) {
    throw "Local test certificate not found: $cerPath."
}

# Trust the self-signed test certificate for sideloading and WACK.
Import-Certificate -FilePath $cerPath -CertStoreLocation 'Cert:\LocalMachine\TrustedPeople' | Out-Null
Write-Host "Trusted local test certificate in Cert:\LocalMachine\TrustedPeople"

if ($Install) {
    Get-AppxPackage -Name 'Tikka.simplreader' -ErrorAction SilentlyContinue | Remove-AppxPackage
    Add-AppxPackage -Path $localMsix -ForceApplicationShutdown
    $pkg = Get-AppxPackage -Name 'Tikka.simplreader'
    Write-Host "Installed local test package: $($pkg.PackageFullName)"
    Write-Host 'Launch "simPl Reader" from the Start menu or right-click a PDF/EPUB -> Open with.'
    Write-Host 'When finished testing, run: powershell -NoProfile -ExecutionPolicy Bypass -File scripts\verify-msix.ps1 -Cleanup'
}

if (-not (Test-Path -LiteralPath $appcert -PathType Leaf)) {
    throw "Windows App Certification Kit not found at $appcert."
}

if (Test-Path -LiteralPath $reportPath) { Remove-Item -LiteralPath $reportPath -Force }
Write-Host 'Running Windows App Certification Kit (this takes 2-5 minutes)...'
& $appcert test -appxpackagepath $localMsix -reportoutputpath $reportPath
$code = $LASTEXITCODE

if (Test-Path -LiteralPath $reportPath -PathType Leaf) {
    [xml]$report = Get-Content -LiteralPath $reportPath -Raw
    $overall = $report.REPORT.OVERALL_RESULT
    Write-Host "WACK Overall Result: $overall"
    Write-Host "Full XML report: $reportPath"
    foreach ($test in $report.SelectNodes('//TEST')) {
        $res = if ($test.RESULT -is [string]) { $test.RESULT.Trim() } elseif ($test.RESULT) { $test.RESULT.InnerText.Trim() } else { '' }
        if ($res -and $res -ne 'PASS') {
            $opt = if ($test.OPTIONAL -eq 'TRUE') { ' (optional)' } else { ' (required)' }
            Write-Host "  [$res$opt] $($test.NAME)"
            foreach ($msg in $test.SelectNodes('.//MESSAGE')) {
                if ($msg.TEXT) { Write-Host "    - $($msg.TEXT)" }
            }
        }
    }
}
if ($code -ne 0) {
    throw "Windows App Certification Kit exited with code $code. Check $reportPath."
}
