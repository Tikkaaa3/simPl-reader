# Windows PowerShell 5.1+. Android app workflow: environment check, APK build,
# install-and-launch, and PDFium staging.
param(
    [ValidateSet('doctor', 'build', 'run', 'pdfium')]
    [string]$Command = 'doctor',
    [switch]$Release,
    [switch]$Offline
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$android = Join-Path $root 'android'
$appBuild = Join-Path $android 'app\build.gradle.kts'
$applicationId = 'io.github.tikkaaa3.simpl'

# The app build script is the single source of the SDK/NDK versions it needs.
function Get-BuildSetting([string]$Name) {
    $text = Get-Content -LiteralPath $appBuild -Raw
    if ($text -notmatch ($Name + '\s*=\s*"?([0-9.]+)"?')) { throw "Could not read $Name from $appBuild" }
    return $Matches[1]
}

function Resolve-Sdk {
    foreach ($candidate in @($env:ANDROID_HOME, $env:ANDROID_SDK_ROOT, (Join-Path $env:LOCALAPPDATA 'Android\Sdk'))) {
        if ($candidate -and (Test-Path -LiteralPath $candidate -PathType Container)) { return $candidate }
    }
    return $null
}

function Resolve-Java {
    if ($env:JAVA_HOME -and (Test-Path -LiteralPath (Join-Path $env:JAVA_HOME 'bin\java.exe'))) { return $env:JAVA_HOME }
    return $null
}

function Invoke-Native([string]$Exe, [string[]]$Arguments, [string]$WorkingDirectory = $root) {
    Write-Host ('=> ' + (Split-Path -Leaf $Exe) + ' ' + ($Arguments -join ' '))
    # Gradle and Cargo write progress to stderr; it must not become a terminating error.
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = $null
    Push-Location -LiteralPath $WorkingDirectory
    try { & $Exe @Arguments } finally { Pop-Location }
    $code = $global:LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($null -eq $code -or $code -ne 0) { throw "$Exe failed (exit $code)." }
}

function Invoke-Doctor {
    $problems = New-Object 'System.Collections.Generic.List[string]'
    function Report([bool]$Ok, [string]$What, [string]$Fix) {
        if ($Ok) { Write-Host "  ok   $What" } else { Write-Host "  FAIL $What" -ForegroundColor Red; $problems.Add("$What -> $Fix") }
    }

    $java = Resolve-Java
    $javaVersion = $null
    if ($java) {
        $ErrorActionPreference = 'Continue'
        $line = (& (Join-Path $java 'bin\java.exe') -version 2>&1 | Select-Object -First 1) -as [string]
        $ErrorActionPreference = 'Stop'
        if ($line -match 'version "(\d+)') { $javaVersion = [int]$Matches[1] }
    }
    Report ($null -ne $javaVersion -and $javaVersion -ge 17) "JDK 17+ (JAVA_HOME=$java, major $javaVersion)" 'Install JDK 21 and set JAVA_HOME.'

    $sdk = Resolve-Sdk
    Report ($null -ne $sdk) "Android SDK ($sdk)" 'Install the SDK (Android Studio or cmdline-tools) and set ANDROID_HOME.'
    if ($sdk) {
        $compileSdk = Get-BuildSetting 'compileSdk'
        $ndk = Get-BuildSetting 'ndkVersion'
        $platform = @(Get-ChildItem -LiteralPath (Join-Path $sdk 'platforms') -Directory -Filter "android-$compileSdk*" -ErrorAction SilentlyContinue)
        Report ($platform.Count -gt 0) "SDK Platform $compileSdk" "android sdk install platforms/android-$compileSdk.0"
        Report (Test-Path -LiteralPath (Join-Path $sdk "ndk\$ndk") -PathType Container) "NDK $ndk" "android sdk install ndk/$ndk"
        Report (Test-Path -LiteralPath (Join-Path $sdk 'platform-tools\adb.exe') -PathType Leaf) 'Platform-Tools (adb)' 'android sdk install platform-tools'
        $emulator = Join-Path $sdk 'emulator\emulator.exe'
        if (Test-Path -LiteralPath $emulator -PathType Leaf) {
            $ErrorActionPreference = 'Continue'
            $accel = (& $emulator -accel-check 2>&1) -join ' '
            $ErrorActionPreference = 'Stop'
            Report ($accel -match 'is installed and usable') 'Emulator acceleration (WHPX)' 'Enable "Windows Hypervisor Platform" in Windows Features (admin, reboot).'
        } else {
            Write-Host '  info Emulator not installed (optional; a USB device also works).'
        }
    }

    $ErrorActionPreference = 'Continue'
    Push-Location -LiteralPath $root
    try { $targets = @(& rustup target list --installed 2>$null) } finally { Pop-Location }
    $ndkTool = Get-Command cargo-ndk -ErrorAction SilentlyContinue
    $ErrorActionPreference = 'Stop'
    foreach ($target in @('aarch64-linux-android', 'x86_64-linux-android')) {
        Report ($targets -contains $target) "Rust target $target" 'rustup target add (from the repository root)'
    }
    Report ($null -ne $ndkTool) 'cargo-ndk' 'cargo install cargo-ndk --locked'

    if ($problems.Count -gt 0) { throw ("Android environment incomplete:`n  " + ($problems -join "`n  ")) }
    Write-Host 'Android environment ready.'
}

function Initialize-Environment {
    $java = Resolve-Java
    if (-not $java) { throw 'JAVA_HOME is not set to a JDK. Run: scripts\android.ps1 doctor' }
    $sdk = Resolve-Sdk
    if (-not $sdk) { throw 'Android SDK not found. Run: scripts\android.ps1 doctor' }
    $env:ANDROID_HOME = $sdk
    return $sdk
}

function Invoke-Gradle([string[]]$Tasks) {
    $arguments = @($Tasks)
    if ($Offline) { $arguments += '--offline' }
    Invoke-Native (Join-Path $android 'gradlew.bat') $arguments $android
}

switch ($Command) {
    'doctor' { Invoke-Doctor }
    'pdfium' { & (Join-Path $PSScriptRoot 'pdfium-android.ps1') -Offline:$Offline }
    'build' {
        Initialize-Environment | Out-Null
        if ($Release) { Invoke-Gradle @('assembleRelease') } else { Invoke-Gradle @('assembleDebug') }
    }
    'run' {
        if ($Release) { throw 'run installs the debug build; release APKs are unsigned until signing is configured.' }
        $sdk = Initialize-Environment
        Invoke-Gradle @('installDebug')
        $adb = Join-Path $sdk 'platform-tools\adb.exe'
        Invoke-Native $adb @('shell', 'am', 'start', '-n', "$applicationId/.MainActivity")
    }
}
