# Windows PowerShell 5.1+. Build an x64 MSIX package for the Microsoft Store.
param(
    [ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version = '0.1.7',
    [string]$IdentityName = 'Tikka.simplreader',
    [string]$Publisher = 'CN=ED890B43-87C5-4E6D-9686-F1422B3F58C9',
    [string]$PublisherDisplayName = 'Tikka',
    [string]$PackageDisplayName = 'simPl Reader',
    [string]$AppDisplayName = 'simPl Reader',
    [switch]$Offline,
    [switch]$SkipBuild,
    [switch]$SignForLocalTest,
    [string]$PayloadDirectory = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$msixVersion = "$Version.0"

function Find-SdkTool([string]$ToolName) {
    $kitsRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    if (Test-Path -LiteralPath $kitsRoot) {
        $found = Get-ChildItem -Path $kitsRoot -Directory -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match '^\d+\.\d+\.\d+\.\d+$' } |
            Sort-Object { [version]$_.Name } -Descending |
            ForEach-Object { Join-Path $_.FullName "x64\$ToolName" } |
            Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
            Select-Object -First 1
        if ($found) { return $found }
    }
    $cmd = Get-Command $ToolName -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    throw "Required Windows SDK tool not found: $ToolName. Install the Windows 10/11 SDK."
}

function Escape-XmlValue([string]$Value) {
    return [System.Security.SecurityElement]::Escape($Value)
}

$makeappx = Find-SdkTool 'makeappx.exe'
$makepri = Find-SdkTool 'makepri.exe'
$signtool = Find-SdkTool 'signtool.exe'

$payload = if ($PayloadDirectory) {
    [IO.Path]::GetFullPath($PayloadDirectory)
} else {
    Join-Path $root 'target\msix-payload'
}

if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot 'package.ps1') -Offline:$Offline -OutputDirectory $payload
}
if (-not (Test-Path -LiteralPath (Join-Path $payload 'simPl.exe') -PathType Leaf)) {
    throw "Packaged simPl.exe not found in $payload. Run without -SkipBuild."
}

$outputDir = Join-Path $root 'target\msix'
$stageDir = Join-Path $outputDir 'stage'
if (Test-Path -LiteralPath $stageDir) { Remove-Item -LiteralPath $stageDir -Recurse -Force }
New-Item -ItemType Directory -Path $stageDir -Force | Out-Null

# Copy the complete portable payload (simPl.exe, pdfium.dll, LICENSE.txt, third-party\).
Copy-Item -Path (Join-Path $payload '*') -Destination $stageDir -Recurse -Force

# Render MSIX visual assets from installer/simPl.svg.
$assetsDir = Join-Path $stageDir 'Assets'
& python (Join-Path $PSScriptRoot 'msix-assets.py') package $assetsDir
if ($LASTEXITCODE -ne 0) { throw "msix-assets.py failed ($LASTEXITCODE)." }

# Render AppxManifest.xml from template.
$templatePath = Join-Path $root 'installer\msix\AppxManifest.xml.template'
$manifestText = [IO.File]::ReadAllText($templatePath, [Text.UTF8Encoding]::new($false))
$replacements = @{
    '__IDENTITY_NAME__'          = Escape-XmlValue $IdentityName
    '__PUBLISHER__'              = Escape-XmlValue $Publisher
    '__VERSION__'                = Escape-XmlValue $msixVersion
    '__PACKAGE_DISPLAY_NAME__'   = Escape-XmlValue $PackageDisplayName
    '__PUBLISHER_DISPLAY_NAME__' = Escape-XmlValue $PublisherDisplayName
    '__APP_DISPLAY_NAME__'       = Escape-XmlValue $AppDisplayName
}
foreach ($key in $replacements.Keys) {
    $manifestText = $manifestText.Replace($key, $replacements[$key])
}
$manifestPath = Join-Path $stageDir 'AppxManifest.xml'
[IO.File]::WriteAllText($manifestPath, $manifestText, [Text.UTF8Encoding]::new($false))

# Build a single unified resources.pri (no split .scale-*.pri files, which are only for bundles).
$priConfig = Join-Path $outputDir 'priconfig.xml'
$priOut = Join-Path $stageDir 'resources.pri'
& $makepri createconfig /cf $priConfig /dq en-US /o | Out-Host
if ($LASTEXITCODE -ne 0) { throw "makepri createconfig failed ($LASTEXITCODE)." }
[xml]$priXml = Get-Content -LiteralPath $priConfig -Raw
foreach ($node in @($priXml.SelectNodes('//packaging'))) {
    [void]$node.ParentNode.RemoveChild($node)
}
$priXml.Save($priConfig)
& $makepri new /pr $stageDir /cf $priConfig /mn $manifestPath /of $priOut /o | Out-Host
if ($LASTEXITCODE -ne 0) { throw "makepri new failed ($LASTEXITCODE)." }
Remove-Item -LiteralPath $priConfig -Force -ErrorAction SilentlyContinue
$splitPris = @(Get-ChildItem -LiteralPath $stageDir -Filter 'resources.*.pri' -ErrorAction SilentlyContinue)
if ($splitPris.Count -gt 0) {
    throw "Unexpected split PRI files generated: $($splitPris.Name -join ', ')"
}

# Pack the Microsoft Store upload MSIX (unsigned; Microsoft signs on Store upload).
$msixName = "simPl-$Version-windows-x64.msix"
$msixPath = Join-Path $outputDir $msixName
& $makeappx pack /d $stageDir /p $msixPath /o | Out-Host
if ($LASTEXITCODE -ne 0) { throw "makeappx pack failed ($LASTEXITCODE)." }

$hash = (Get-FileHash -LiteralPath $msixPath -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText("$msixPath.sha256", "$hash  $msixName`n", [Text.UTF8Encoding]::new($false))

$releaseMeta = [ordered]@{
    product              = $AppDisplayName
    packageDisplayName   = $PackageDisplayName
    identityName         = $IdentityName
    publisher            = $Publisher
    publisherDisplayName = $PublisherDisplayName
    version              = $msixVersion
    architecture         = 'x64'
    filename             = $msixName
    bytes                = (Get-Item -LiteralPath $msixPath).Length
    sha256               = $hash
}
[IO.File]::WriteAllText((Join-Path $outputDir 'msix-release.json'), ($releaseMeta | ConvertTo-Json) + "`n", [Text.UTF8Encoding]::new($false))

Write-Host "Store upload MSIX: $msixPath"
Write-Host "SHA256: $hash"

if ($SignForLocalTest) {
    $friendly = "simPl Reader Local MSIX Test ($IdentityName)"
    $cert = Get-ChildItem Cert:\CurrentUser\My |
        Where-Object { $_.Subject -eq $Publisher -and $_.FriendlyName -eq $friendly -and $_.HasPrivateKey -and $_.NotAfter -gt (Get-Date).AddDays(7) } |
        Sort-Object NotAfter -Descending |
        Select-Object -First 1
    if (-not $cert) {
        $cert = New-SelfSignedCertificate `
            -Type Custom `
            -Subject $Publisher `
            -FriendlyName $friendly `
            -KeyUsage DigitalSignature `
            -CertStoreLocation 'Cert:\CurrentUser\My' `
            -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}') `
            -NotAfter (Get-Date).AddYears(1)
    }
    $cerPath = Join-Path $outputDir 'simPl-local-test.cer'
    Export-Certificate -Cert $cert -FilePath $cerPath -Force | Out-Null

    $localMsixPath = Join-Path $outputDir "simPl-$Version-windows-x64-local-test.msix"
    Copy-Item -LiteralPath $msixPath -Destination $localMsixPath -Force
    & $signtool sign /fd SHA256 /sha1 $cert.Thumbprint $localMsixPath | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "signtool sign failed ($LASTEXITCODE)." }

    Write-Host "Local test MSIX (self-signed): $localMsixPath"
    Write-Host "Local test certificate (.cer): $cerPath"
    Write-Host "Certificate thumbprint: $($cert.Thumbprint)"
}
