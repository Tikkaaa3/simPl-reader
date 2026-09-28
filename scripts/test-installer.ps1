# Real install/upgrade/uninstall checks with a separate AppId and disposable profile.
param([string]$CompilerPath = '', [string]$PayloadDirectory = '')
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $CompilerPath) { $CompilerPath = Join-Path $root 'target\tools\inno-6.7.3\ISCC.exe' }
if (-not $PayloadDirectory) { $PayloadDirectory = Join-Path $root 'target\installer-payload' }
$sandbox = Join-Path $root ('target\installer-tests\' + [guid]::NewGuid().ToString('N'))
$testProfile = Join-Path $sandbox 'userdata\simPl'
$app = Join-Path $sandbox 'app'
$original = Join-Path $sandbox 'originals'
$registry = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\simPl.Reader.InstallerQA_is1'
$desktop = Join-Path ([Environment]::GetFolderPath('Desktop')) 'simPl Reader Installer QA.lnk'
$startMenu = Join-Path ([Environment]::GetFolderPath('Programs')) 'simPl Reader Installer QA'
if ((Test-Path $registry) -or (Test-Path -LiteralPath $desktop) -or (Test-Path -LiteralPath $startMenu)) {
    throw 'A previous Installer QA installation exists. Remove that test installation before retrying.'
}
New-Item -ItemType Directory -Path $sandbox,$testProfile,$original -Force | Out-Null

$associationBase = 'simPl.Reader.InstallerQA'
$capabilityKey = 'HKCU:\Software\simPl\InstallerQA\Capabilities'
function Existing-Associations {
    $snapshot = [ordered]@{}
    foreach ($extension in @('.pdf', '.html', '.epub')) {
        foreach ($kind in @('default', 'choice', 'candidates')) {
            $path = switch ($kind) {
                'default' { "HKCU:\Software\Classes\$extension" }
                'choice' { "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$extension\UserChoice" }
                'candidates' { "HKCU:\Software\Classes\$extension\OpenWithProgids" }
            }
            $values = [ordered]@{}
            if (Test-Path $path) {
                $key = Get-Item $path
                foreach ($name in ($key.GetValueNames() | Sort-Object)) {
                    if (($kind -ne 'default' -or $name -eq '') -and -not $name.StartsWith($associationBase + '.')) {
                        $values[$name] = $key.GetValue($name)
                    }
                }
            }
            $snapshot["$extension/$kind"] = $values
        }
    }
    return $snapshot | ConvertTo-Json -Depth 8 -Compress
}
$existingAssociations = Existing-Associations
Add-Type -Path (Join-Path $PSScriptRoot 'installer-associations.cs')

function Assert-That([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
function Invoke-Setup([string]$File, [string[]]$Arguments, [bool]$ExpectSuccess = $true) {
    # Inno's uninstaller relaunches from a temporary copy. -Wait observes the
    # whole process tree; waiting only for the initial process races cleanup.
    $process = Start-Process -FilePath $File -ArgumentList $Arguments -WindowStyle Hidden -PassThru -Wait
    $process.Refresh()
    if ($ExpectSuccess) { Assert-That ($process.ExitCode -eq 0) "Setup exited $($process.ExitCode): $File" }
    return $process.ExitCode
}
function Build-QA([string]$Version) {
    $compilerArgs = @("/DAppVersion=$Version", "/DPayloadDir=$PayloadDirectory", "/DOutputDir=$sandbox", "/DTestProfileRoot=$testProfile", (Join-Path $root 'installer\simPl.iss'))
    & $CompilerPath @compilerArgs *> (Join-Path $sandbox "compile-$Version.log")
    if ($LASTEXITCODE -ne 0) { throw "QA compiler failed; inspect $sandbox." }
    return Join-Path $sandbox 'simPl-installer-qa.exe'
}
function Get-QAUninstaller {
    $file = ((Get-ItemProperty $registry).UninstallString).Trim('"')
    Assert-That ($file.StartsWith($app + '\', [StringComparison]::OrdinalIgnoreCase)) 'Uninstaller is outside the QA app folder.'
    Assert-That ($file -match '\\unins\d+\.exe$') 'Unexpected uninstall command.'
    return $file
}
function Install-QA([string]$Setup, [string]$Tasks, [string]$Log) {
    $null = Invoke-Setup $Setup @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', ('/DIR="' + $app + '"'), ('/TASKS="' + $Tasks + '"'), ('/LOG="' + (Join-Path $sandbox $Log) + '"'))
    Assert-That (Test-Path $registry) 'Windows uninstall registration is missing.'
    $entry = Get-ItemProperty $registry
    Assert-That ($entry.DisplayName -eq 'simPl Reader Installer QA') 'Wrong uninstall entry.'
    $null = Get-QAUninstaller
    foreach ($file in @('simPl.exe', 'pdfium.dll', 'LICENSE.txt')) {
        Assert-That ((Get-FileHash (Join-Path $app $file)).Hash -eq (Get-FileHash (Join-Path $PayloadDirectory $file)).Hash) "Installed $file differs from payload."
    }
    Assert-That (Test-Path -LiteralPath (Join-Path $app 'third-party\fonts\Geist-OFL.txt')) 'Font notices missing.'
    Assert-That ((Get-Item $registry).GetValue('Inno Setup: Language') -eq 'english') 'Installer language is not English.'
    Assert-That ((Get-Item 'HKCU:\Software\RegisteredApplications').GetValue('simPl Reader Installer QA') -eq 'Software\simPl\InstallerQA\Capabilities') 'Default-app capabilities registration missing.'
    foreach ($extension in @('pdf', 'html', 'epub')) {
        $progId = $associationBase + '.' + $extension.ToUpperInvariant()
        $candidates = Get-Item "HKCU:\Software\Classes\.$extension\OpenWithProgids"
        Assert-That ($candidates.GetValueNames() -contains $progId) "Open with candidate missing for $extension."
        Assert-That ((Get-Item "$capabilityKey\FileAssociations").GetValue(".$extension") -eq $progId) "Default-app capability missing for $extension."
        $command = (Get-Item "HKCU:\Software\Classes\$progId\shell\open\command").GetValue('')
        Assert-That ($command -eq ('"' + (Join-Path $app 'simPl.exe') + '" "%1"')) "Incorrect or unquoted open command for $extension."
        $expectedHandler = 'simPl Reader Installer QA|' + (Join-Path $app 'simPl.exe')
        Assert-That ([InstallerAssociations]::Recommended(".$extension") -contains $expectedHandler) "Windows does not recommend simPl with its friendly name for $extension."
    }
    Assert-That ((Existing-Associations) -eq $existingAssociations) 'Install changed existing defaults or another application registration.'
}
function Assert-AssociationsRemoved {
    Assert-That (-not (Test-Path $capabilityKey)) 'Capabilities remain after uninstall.'
    foreach ($extension in @('pdf', 'html', 'epub')) {
        $progId = $associationBase + '.' + $extension.ToUpperInvariant()
        Assert-That (-not (Test-Path "HKCU:\Software\Classes\$progId")) "ProgID remains after uninstall: $progId"
        $path = "HKCU:\Software\Classes\.$extension\OpenWithProgids"
        if (Test-Path $path) {
            Assert-That (-not ((Get-Item $path).GetValueNames() -contains $progId)) "Open with entry remains for $extension."
        }
    }
    Assert-That (-not ((Get-Item 'HKCU:\Software\RegisteredApplications').GetValueNames() -contains 'simPl Reader Installer QA')) 'Registered application entry remains.'
    Assert-That ((Existing-Associations) -eq $existingAssociations) 'Uninstall changed existing defaults or another application registration.'
}

# Synthetic managed copy and reading-state sentinel. Never use the actual profile.
New-Item -ItemType Directory -Path (Join-Path $testProfile 'documents'),(Join-Path $testProfile 'positions') -Force | Out-Null
Set-Content (Join-Path $testProfile 'documents\test-book.html') '<p>Installer QA book</p>'
Set-Content (Join-Path $testProfile 'positions\sentinel.txt') 'saved-page-22'
Set-Content (Join-Path $original 'original-book.html') '<p>Original must survive</p>'
$originalHash = (Get-FileHash (Join-Path $original 'original-book.html')).Hash

$setup = Build-QA '0.1.0'
Install-QA $setup 'startmenuicon' 'install-default.log'
Assert-That (-not (Test-Path -LiteralPath $desktop)) 'Desktop shortcut should be optional.'
Assert-That (Test-Path -LiteralPath (Join-Path $startMenu 'simPl Reader Installer QA.lnk')) 'Start menu shortcut missing.'

# The installed executable advertises its lifetime; uninstall must not remove live data.
$previousLocal = $env:LOCALAPPDATA
$reader = $null
try {
    $env:LOCALAPPDATA = Split-Path -Parent $testProfile
    $reader = Start-Process -FilePath (Join-Path $app 'simPl.exe') -WindowStyle Hidden -PassThru
    Start-Sleep -Seconds 3
    $reader.Refresh()
    Assert-That (-not $reader.HasExited) 'Installed reader did not start.'
    $marker = [Threading.Mutex]::OpenExisting('Local\simPl.Reader.Running')
    $marker.Dispose()
    $blocked = Invoke-Setup (Get-QAUninstaller) @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/PURGEUSERDATA', ('/LOG="' + (Join-Path $sandbox 'blocked-running.log') + '"')) $false
    Assert-That ($blocked -ne 0) 'Uninstall should refuse while the reader is running.'
    Assert-That (Test-Path -LiteralPath (Join-Path $testProfile 'documents\test-book.html')) 'Running reader profile was touched.'
} finally {
    $env:LOCALAPPDATA = $previousLocal
    if ($reader -and -not $reader.HasExited) {
        $null = $reader.CloseMainWindow()
        if (-not $reader.WaitForExit(10000)) { throw 'QA reader did not close normally; no forced termination performed.' }
    }
}

$setup = Build-QA '0.1.1'
Install-QA $setup 'desktopicon,startmenuicon' 'upgrade.log'
Assert-That ((Get-ItemProperty $registry).DisplayVersion -eq '0.1.1') 'Upgrade did not update version.'
Assert-That (Test-Path -LiteralPath $desktop) 'Selected desktop shortcut missing.'
$shortcut = (New-Object -ComObject WScript.Shell).CreateShortcut($desktop)
Assert-That ($shortcut.TargetPath -eq (Join-Path $app 'simPl.exe')) 'Shortcut points to the wrong application.'
Assert-That ((Get-Content (Join-Path $testProfile 'positions\sentinel.txt') -Raw).Trim() -eq 'saved-page-22') 'Upgrade changed reading data.'

$null = Invoke-Setup (Get-QAUninstaller) @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', ('/LOG="' + (Join-Path $sandbox 'keep-library.log') + '"'))
Assert-That (-not (Test-Path $registry)) 'Uninstall registration remained.'
Assert-AssociationsRemoved
Assert-That (-not (Test-Path -LiteralPath $desktop)) 'Desktop shortcut remained.'
Assert-That (-not (Test-Path -LiteralPath $startMenu)) 'Start menu shortcuts remained.'
Assert-That (-not (Test-Path -LiteralPath (Join-Path $app 'simPl.exe'))) 'Reader executable remained.'
Assert-That (Test-Path -LiteralPath (Join-Path $testProfile 'documents\test-book.html')) 'Default uninstall deleted the library.'

Install-QA $setup '' 'reinstall.log'
Assert-That (Test-Path -LiteralPath (Join-Path $testProfile 'positions\sentinel.txt')) 'Reinstall lost saved data.'
# DelTree must remove the link itself, never follow it into an original folder.
New-Item -ItemType Junction -Path (Join-Path $testProfile 'documents\external-link') -Target $original | Out-Null
$null = Invoke-Setup (Get-QAUninstaller) @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/PURGEUSERDATA', ('/LOG="' + (Join-Path $sandbox 'delete-library.log') + '"'))
Assert-That (-not (Test-Path -LiteralPath $testProfile)) 'Opt-in deletion did not remove the test library.'
Assert-That ((Get-FileHash (Join-Path $original 'original-book.html')).Hash -eq $originalHash) 'Original document was changed or removed.'
Assert-That (-not (Test-Path $registry)) 'Final uninstall entry remained.'
Assert-AssociationsRemoved
Write-Host "PASS: English setup, Open with/default-app registration and cleanup, unchanged user defaults, shortcuts, startup, running-app guard, upgrade, retain, reinstall, delete and original/junction protection. Evidence: $sandbox"
