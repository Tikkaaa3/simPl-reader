param(
    [string]$ExePath = (Join-Path $PSScriptRoot '..\..\..\target\release\iced-shell.exe'),
    [string]$EvidenceDirectory = (Join-Path $env:TEMP ("iced-bidi-diagnostic-" + [Guid]::NewGuid().ToString('N')))
)

$ErrorActionPreference = 'Stop'
$ExePath = [IO.Path]::GetFullPath($ExePath)
$EvidenceDirectory = [IO.Path]::GetFullPath($EvidenceDirectory)
$RepositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\..'))
if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) { throw "Release executable not found: $ExePath" }
$releaseHash = (Get-FileHash -LiteralPath $ExePath -Algorithm SHA256).Hash.ToLowerInvariant()
$patchSource = Join-Path $RepositoryRoot 'patches\cosmic-text-0.15.0\src\shape.rs'
$patchHash = if (Test-Path -LiteralPath $patchSource) {
    (Get-FileHash -LiteralPath $patchSource -Algorithm SHA256).Hash.ToLowerInvariant()
} else { 'absent' }
$lockHash = (Get-FileHash -LiteralPath (Join-Path $RepositoryRoot 'Cargo.lock') -Algorithm SHA256).Hash.ToLowerInvariant()
if (-not [Environment]::UserInteractive) { throw 'bidi-diagnostic.ps1 requires an interactive Windows desktop' }
if (Test-Path -LiteralPath $EvidenceDirectory) {
    if ((Get-ChildItem -LiteralPath $EvidenceDirectory -Force | Measure-Object).Count -ne 0) {
        throw "Evidence directory must be new or empty: $EvidenceDirectory"
    }
} else {
    New-Item -ItemType Directory -Path $EvidenceDirectory | Out-Null
}

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class IcedBidiNative {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd, ref Rect rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
}
"@

$inheritedOverrides = Get-ChildItem Env: | Where-Object {
    $_.Name -match '^WINIT_' -or $_.Name -match '^ICED_' -or
    $_.Name -in @('GPUI_SHELL_STARTUP_MARKERS', 'GPUI_SHELL_NATIVE_TEST_STATUS')
}
if ($inheritedOverrides.Count -ne 0) {
    throw "Refusing diagnostic evidence with inherited framework/test gates: $($inheritedOverrides.Name -join ', ')"
}

$owned = [Collections.Generic.List[Diagnostics.Process]]::new()
$tracePath = Join-Path $EvidenceDirectory 'iced-cosmic-bidi-trace.txt'
$script:activeShell = $null

function Wait-Title([Diagnostics.Process]$Process, [string]$Expected, [int]$TimeoutSeconds = 60) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        if ($Process.HasExited) { throw "Process exited before status '$Expected' (exit $($Process.ExitCode))" }
        $Process.Refresh()
        if ($Process.MainWindowTitle.Contains($Expected)) { return }
        Start-Sleep -Milliseconds 30
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Expected title status '$Expected', got '$($Process.MainWindowTitle)'"
}

function Start-Shell {
    $process = Start-Process -FilePath $ExePath -ArgumentList '--bidi-diagnostic' `
        -WorkingDirectory $RepositoryRoot -WindowStyle Normal -PassThru
    $owned.Add($process)
    if (-not $process.WaitForInputIdle(10000)) { throw 'Iced diagnostic did not become input-idle within 10 seconds' }
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $process.Refresh()
        if ($process.MainWindowHandle -ne 0 -and [IcedBidiNative]::IsWindowVisible($process.MainWindowHandle)) {
            if ([IcedBidiNative]::SetForegroundWindow($process.MainWindowHandle)) {
                $script:activeShell = $process
                return $process
            }
        }
        Start-Sleep -Milliseconds 25
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Iced diagnostic did not create a visible, foregroundable window'
}

function Activate-Shell {
    if ($null -eq $script:activeShell -or $script:activeShell.HasExited) { throw 'No active Iced diagnostic process' }
    $script:activeShell.Refresh()
    if (-not [IcedBidiNative]::SetForegroundWindow($script:activeShell.MainWindowHandle)) {
        throw 'Could not foreground Iced diagnostic'
    }
    Start-Sleep -Milliseconds 80
}

function Send-Key([byte]$VirtualKey) {
    Activate-Shell
    [IcedBidiNative]::keybd_event($VirtualKey, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    [IcedBidiNative]::keybd_event($VirtualKey, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 150
}

function Assert-CaptureOwnership([Diagnostics.Process]$Process) {
    if ($Process.HasExited) { throw "Capture target process $($Process.Id) has exited" }
    $Process.Refresh()
    $handle = $Process.MainWindowHandle
    $foreground = [IcedBidiNative]::GetForegroundWindow()
    [uint32]$ownerId = 0
    if ($handle -eq [IntPtr]::Zero -or -not [IcedBidiNative]::IsWindowVisible($handle) -or
        $foreground -ne $handle -or
        [IcedBidiNative]::GetWindowThreadProcessId($foreground, [ref]$ownerId) -eq 0 -or
        $ownerId -ne [uint32]$Process.Id) {
        throw "Capture target is not visible foreground window owned by process $($Process.Id)"
    }
}

function Capture-Client([Diagnostics.Process]$Process, [string]$Name) {
    Assert-CaptureOwnership $Process
    $Process.Refresh()
    $rect = New-Object IcedBidiNative+Rect
    if (-not [IcedBidiNative]::GetClientRect($Process.MainWindowHandle, [ref]$rect)) { throw 'GetClientRect failed' }
    $origin = New-Object IcedBidiNative+Point
    $origin.X = 0; $origin.Y = 0
    if (-not [IcedBidiNative]::ClientToScreen($Process.MainWindowHandle, [ref]$origin)) { throw 'ClientToScreen failed' }
    $scale = [IcedBidiNative]::GetDpiForWindow($Process.MainWindowHandle) / 96.0
    $width = [int][Math]::Round($rect.Right * $scale)
    $height = [int][Math]::Round($rect.Bottom * $scale)
    $x = [int][Math]::Round($origin.X * $scale)
    $y = [int][Math]::Round($origin.Y * $scale)
    $bitmap = New-Object System.Drawing.Bitmap($width, $height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        Start-Sleep -Milliseconds 250
        Assert-CaptureOwnership $Process
        $graphics.CopyFromScreen($x, $y, 0, 0, $bitmap.Size, [System.Drawing.CopyPixelOperation]::SourceCopy)
        Assert-CaptureOwnership $Process
        $bitmap.Save((Join-Path $EvidenceDirectory $Name), [System.Drawing.Imaging.ImageFormat]::Png)
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
}

function Close-Cleanly([Diagnostics.Process]$Process) {
    Activate-Shell
    [IcedBidiNative]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
    [IcedBidiNative]::keybd_event(0x73, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    [IcedBidiNative]::keybd_event(0x73, 0, 2, [UIntPtr]::Zero)
    [IcedBidiNative]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
    if (-not $Process.WaitForExit(6000)) { throw 'Iced diagnostic did not close within six seconds' }
    if ($Process.ExitCode -ne 0) { throw "Iced diagnostic exited $($Process.ExitCode), expected zero" }
}

$oldGate = $env:ICED_SHELL_BIDI_DIAGNOSTICS
$oldTrace = $env:ICED_SHELL_BIDI_TRACE_PATH
try {
    'negative gate: explicit mode without its process gate exits before creating a window or trace'
    Remove-Item Env:ICED_SHELL_BIDI_DIAGNOSTICS -ErrorAction SilentlyContinue
    Remove-Item Env:ICED_SHELL_BIDI_TRACE_PATH -ErrorAction SilentlyContinue
    $stderr = Join-Path $EvidenceDirectory 'ungated.stderr.txt'
    $negativeStart = New-Object Diagnostics.ProcessStartInfo
    $negativeStart.FileName = $ExePath
    $negativeStart.Arguments = '--bidi-diagnostic'
    $negativeStart.WorkingDirectory = $RepositoryRoot
    $negativeStart.UseShellExecute = $false
    $negativeStart.CreateNoWindow = $true
    $negativeStart.RedirectStandardError = $true
    $ungated = New-Object Diagnostics.Process
    $ungated.StartInfo = $negativeStart
    if (-not $ungated.Start()) { throw 'Could not start the ungated diagnostic negative control' }
    $owned.Add($ungated)
    $negativeDeadline = [DateTime]::UtcNow.AddSeconds(5)
    $negativeExited = $false
    do {
        $ungated.Refresh()
        $ungatedWindow = $ungated.MainWindowHandle
        if ($null -ne $ungatedWindow -and $ungatedWindow.ToInt64() -ne 0) {
            $visible = [IcedBidiNative]::IsWindowVisible($ungatedWindow)
            throw "Ungated diagnostic unexpectedly created a main window (visible=$visible)"
        }
        if ($ungated.WaitForExit(25)) {
            $negativeExited = $true
            break
        }
    } while ([DateTime]::UtcNow -lt $negativeDeadline)
    if (-not $negativeExited) {
        throw 'Ungated diagnostic did not exit within the five-second bound'
    }
    $ungated.WaitForExit()
    $negativeStderr = $ungated.StandardError.ReadToEnd()
    [IO.File]::WriteAllText($stderr, $negativeStderr)
    if ($ungated.ExitCode -ne 2) { throw "Ungated diagnostic exited $($ungated.ExitCode), expected 2" }
    if (-not (Select-String -LiteralPath $stderr -SimpleMatch 'requires ICED_SHELL_BIDI_DIAGNOSTICS=1' -Quiet)) {
        throw 'Ungated diagnostic did not report the required exact process gate'
    }
    if (Test-Path -LiteralPath $tracePath) { throw 'Ungated diagnostic unexpectedly created trace output' }

    @(
        "os=$([Environment]::OSVersion.VersionString)"
        "powershell=$($PSVersionTable.PSVersion)"
        'fixture_revision=reader-workload-fx-3'
        'renderer=Iced 0.14.0 / tiny-skia (CPU); startup markers disabled'
        'mode=--bidi-diagnostic; exact process gate enabled; matched native rich_text display plus public Iced Graphics paragraph reconstruction trace'
        "exe=$ExePath"
        "release_sha256=$releaseHash"
        "release_bytes=$((Get-Item -LiteralPath $ExePath).Length)"
        "patch_shape_sha256=$patchHash"
        "cargo_lock_sha256=$lockHash"
    ) | Set-Content -LiteralPath (Join-Path $EvidenceDirectory 'run-context.txt') -Encoding UTF8

    $env:ICED_SHELL_BIDI_DIAGNOSTICS = '1'
    $env:ICED_SHELL_BIDI_TRACE_PATH = $tracePath
    'diagnostic loads fixture immediately; wait for first ready condition'
    $reader = Start-Shell
    Wait-Title $reader 'bidi=ready;case=p-00004;variant=styled-rlm;width=800' 90
    $dpi = [IcedBidiNative]::GetDpiForWindow($reader.MainWindowHandle)
    "host_dpi=$dpi" | Add-Content -LiteralPath (Join-Path $EvidenceDirectory 'run-context.txt')

    $cases = @('p-00004', 'p-00003', 'p-00001')
    $variants = @('styled-rlm', 'uniform-rlm', 'uniform-no-rlm', 'styled-no-rlm')
    foreach ($case in $cases) {
        $caseVariants = if ($case -eq 'p-00004') { $variants } else { @('styled-rlm') }
        foreach ($variant in $caseVariants) {
            foreach ($width in @(800, 480)) {
                $expectedTitle = "bidi=ready;case=$case;variant=$variant;width=$width"
                Wait-Title $reader $expectedTitle
                $safeCase = $case.Replace('-', '')
                $safeVariant = $variant.Replace('-', '_')
                Capture-Client $reader "$safeCase-$safeVariant-$width.png"
                if ($width -eq 800) {
                    Send-Key 0x73 # F4 toggles to 480 DIP
                }
            }
            if ($case -eq 'p-00004' -and $variant -ne 'styled-no-rlm') {
                Send-Key 0x71 # F2 advances one Gray-code condition; resets to 800 DIP
            }
        }
        if ($case -ne 'p-00001') {
            Send-Key 0x72 # F3 advances case; resets condition and width
        }
    }
    Close-Cleanly $reader

    $traceInfo = Get-Item -LiteralPath $tracePath
    $conditionCount = (Select-String -LiteralPath $tracePath -Pattern '^condition ' | Measure-Object).Count
    $dispositionCount = (Select-String -LiteralPath $tracePath -Pattern '^disposition ' | Measure-Object).Count
    if ($traceInfo.Length -gt 262144) { throw "Trace exceeded 256 KiB: $($traceInfo.Length) bytes" }
    if ($conditionCount -ne 12 -or $dispositionCount -ne 12) {
        throw "Expected 12 bounded trace conditions/dispositions, found $conditionCount/$dispositionCount"
    }
    if (Get-ChildItem Env: | Where-Object { $_.Name -eq 'ICED_SHELL_STARTUP_MARKERS' }) {
        throw 'A startup-marker gate leaked into the diagnostic process environment'
    }
    "trace_bytes=$($traceInfo.Length)" | Add-Content -LiteralPath (Join-Path $EvidenceDirectory 'run-context.txt')
    "conditions=$conditionCount" | Add-Content -LiteralPath (Join-Path $EvidenceDirectory 'run-context.txt')

    'foreground guard negative: two owned shell diagnostics; background capture must throw'
    $background = Start-Process -FilePath $ExePath -ArgumentList '--shell-poc' -WorkingDirectory $env:TEMP -WindowStyle Normal -PassThru
    $owned.Add($background)
    if (-not $background.WaitForInputIdle(10000)) { throw 'Background control was not input-idle' }
    Wait-Title $background 'Iced Shell PoC'
    $cover = Start-Process -FilePath $ExePath -ArgumentList '--shell-poc' -WorkingDirectory $env:TEMP -WindowStyle Normal -PassThru
    $owned.Add($cover)
    if (-not $cover.WaitForInputIdle(10000)) { throw 'Cover control was not input-idle' }
    Wait-Title $cover 'Iced Shell PoC'
    if (-not [IcedBidiNative]::SetForegroundWindow($cover.MainWindowHandle)) { throw 'Could not foreground cover control' }
    Start-Sleep -Milliseconds 100
    if ($background.HasExited -or -not [IcedBidiNative]::IsWindowVisible($background.MainWindowHandle) -or
        [IcedBidiNative]::GetForegroundWindow() -ne $cover.MainWindowHandle) {
        throw 'Foreground guard control setup did not establish a visible background window and owned cover'
    }
    try {
        Capture-Client $background 'background-must-not-exist.png'
        throw 'Negative control unexpectedly succeeded'
    } catch {
        if ($_.Exception.Message -notlike 'Capture target is not visible foreground window owned by process *') { throw }
    }
    if (Test-Path -LiteralPath (Join-Path $EvidenceDirectory 'background-must-not-exist.png')) {
        throw 'Foreground guard wrote a background image'
    }
    foreach ($control in @($cover, $background)) {
        [void]$control.CloseMainWindow()
        if (-not $control.WaitForExit(6000) -or $control.ExitCode -ne 0) {
            throw 'Foreground guard control did not close cleanly'
        }
    }
    if ((Get-FileHash -LiteralPath $ExePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $releaseHash) {
        throw 'Release executable changed during diagnostic evidence capture'
    }
    'bidi-diagnostic: interactive matrix, actual client captures, and bounded trace completed'
    $EvidenceDirectory
} finally {
    if ($null -eq $oldGate) { Remove-Item Env:ICED_SHELL_BIDI_DIAGNOSTICS -ErrorAction SilentlyContinue }
    else { $env:ICED_SHELL_BIDI_DIAGNOSTICS = $oldGate }
    if ($null -eq $oldTrace) { Remove-Item Env:ICED_SHELL_BIDI_TRACE_PATH -ErrorAction SilentlyContinue }
    else { $env:ICED_SHELL_BIDI_TRACE_PATH = $oldTrace }
    foreach ($process in $owned) {
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
    }
}
