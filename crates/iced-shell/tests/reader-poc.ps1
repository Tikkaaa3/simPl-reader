param(
    [string]$ExePath = (Join-Path $PSScriptRoot '..\..\..\target\release\iced-shell.exe'),
    [string]$EvidenceDirectory = (Join-Path $env:TEMP ("iced-reader-poc-" + [Guid]::NewGuid().ToString('N')))
)

$ErrorActionPreference = 'Stop'
$ExePath = [IO.Path]::GetFullPath($ExePath)
$EvidenceDirectory = [IO.Path]::GetFullPath($EvidenceDirectory)
$RepositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\..'))
$FixtureSource = Join-Path $RepositoryRoot 'fixtures\reader-workload'
if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) { throw "Release executable not found: $ExePath" }
$releaseHash = (Get-FileHash -LiteralPath $ExePath -Algorithm SHA256).Hash.ToLowerInvariant()
$patchSource = Join-Path $RepositoryRoot 'patches\cosmic-text-0.15.0\src\shape.rs'
$patchHash = if (Test-Path -LiteralPath $patchSource) {
    (Get-FileHash -LiteralPath $patchSource -Algorithm SHA256).Hash.ToLowerInvariant()
} else { 'absent' }
$lockHash = (Get-FileHash -LiteralPath (Join-Path $RepositoryRoot 'Cargo.lock') -Algorithm SHA256).Hash.ToLowerInvariant()
if (-not [Environment]::UserInteractive) { throw 'reader-poc.ps1 requires an interactive Windows desktop' }
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
public static class IcedReaderNative {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd, ref Rect rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll", SetLastError=true)] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
    public static void Wheel(int x, int y, int delta) {
        SetCursorPos(x, y);
        mouse_event(0x0800, 0, 0, unchecked((uint)delta), UIntPtr.Zero);
    }
}
"@

$selectionOverrides = Get-ChildItem Env: | Where-Object {
    $_.Name -match '^(WGPU|WINIT)_' -or $_.Name -match '^ICED_'
}
if ($selectionOverrides.Count -ne 0) {
    throw "Refusing reader evidence with inherited Iced/WGPU/winit overrides: $($selectionOverrides.Name -join ', ')"
}

$owned = [Collections.Generic.List[Diagnostics.Process]]::new()
$tempRoots = [Collections.Generic.List[string]]::new()
$script:activeShell = $null
$oldReaderStatus = $env:ICED_SHELL_READER_TEST_STATUS
$oldNativeStatus = $env:ICED_SHELL_NATIVE_TEST_STATUS
$env:ICED_SHELL_READER_TEST_STATUS = '1'
$env:ICED_SHELL_NATIVE_TEST_STATUS = '1'

function Start-Shell([string]$WorkingDirectory, [string[]]$Arguments = @()) {
    if ($Arguments.Count -eq 0) {
        $process = Start-Process -FilePath $ExePath -WorkingDirectory $WorkingDirectory -WindowStyle Normal -PassThru
    } else {
        $process = Start-Process -FilePath $ExePath -ArgumentList $Arguments -WorkingDirectory $WorkingDirectory -WindowStyle Normal -PassThru
    }
    $owned.Add($process)
    if (-not $process.WaitForInputIdle(10000)) { throw 'Iced shell did not become input-idle within 10 seconds' }
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        $process.Refresh()
        if ($process.MainWindowHandle -ne 0 -and [IcedReaderNative]::IsWindowVisible($process.MainWindowHandle)) {
            if ([IcedReaderNative]::SetForegroundWindow($process.MainWindowHandle)) {
                $script:activeShell = $process
                return $process
            }
        }
        Start-Sleep -Milliseconds 25
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Iced shell did not create a visible, foregroundable top-level window'
}

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

function Activate-Shell {
    if ($null -eq $script:activeShell -or $script:activeShell.HasExited) { throw 'No active Iced process is available' }
    $script:activeShell.Refresh()
    if (-not [IcedReaderNative]::SetForegroundWindow($script:activeShell.MainWindowHandle)) { throw 'Could not foreground Iced shell' }
    Start-Sleep -Milliseconds 70
}

function Send-Key([byte]$VirtualKey, [switch]$Shift) {
    Activate-Shell
    if ($Shift) { [IcedReaderNative]::keybd_event(0x10, 0, 0, [UIntPtr]::Zero) }
    [IcedReaderNative]::keybd_event($VirtualKey, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 65
    [IcedReaderNative]::keybd_event($VirtualKey, 0, 2, [UIntPtr]::Zero)
    if ($Shift) { [IcedReaderNative]::keybd_event(0x10, 0, 2, [UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 100
}

function Send-AltF4 {
    Activate-Shell
    [IcedReaderNative]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
    [IcedReaderNative]::keybd_event(0x73, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    [IcedReaderNative]::keybd_event(0x73, 0, 2, [UIntPtr]::Zero)
    [IcedReaderNative]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
}

function Mouse-At([Diagnostics.Process]$Process, [int]$LogicalX, [int]$LogicalY) {
    Activate-Shell
    $point = New-Object IcedReaderNative+Point
    $point.X = $LogicalX; $point.Y = $LogicalY
    if (-not [IcedReaderNative]::ClientToScreen($Process.MainWindowHandle, [ref]$point)) { throw 'ClientToScreen failed' }
    [void][IcedReaderNative]::SetCursorPos($point.X, $point.Y)
    Start-Sleep -Milliseconds 120
    [IcedReaderNative]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 50
    [IcedReaderNative]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 180
}

function Wheel-Window([Diagnostics.Process]$Process, [int]$LogicalX, [int]$LogicalY, [int]$Delta, [int]$Count = 1) {
    Activate-Shell
    $point = New-Object IcedReaderNative+Point
    $point.X = $LogicalX; $point.Y = $LogicalY
    if (-not [IcedReaderNative]::ClientToScreen($Process.MainWindowHandle, [ref]$point)) { throw 'ClientToScreen failed' }
    for ($index = 0; $index -lt $Count; $index++) {
        [IcedReaderNative]::Wheel($point.X, $point.Y, $Delta)
        Start-Sleep -Milliseconds 45
    }
    Start-Sleep -Milliseconds 180
}

function Assert-CaptureOwnership([Diagnostics.Process]$Process) {
    if ($Process.HasExited) { throw "Capture target process $($Process.Id) has exited" }
    $Process.Refresh()
    $handle = $Process.MainWindowHandle
    $foreground = [IcedReaderNative]::GetForegroundWindow()
    [uint32]$ownerId = 0
    if ($handle -eq [IntPtr]::Zero -or -not [IcedReaderNative]::IsWindowVisible($handle) -or
        $foreground -ne $handle -or
        [IcedReaderNative]::GetWindowThreadProcessId($foreground, [ref]$ownerId) -eq 0 -or
        $ownerId -ne [uint32]$Process.Id) {
        throw "Capture target is not visible foreground window owned by process $($Process.Id)"
    }
}

function Capture-Client([Diagnostics.Process]$Process, [string]$Name) {
    Assert-CaptureOwnership $Process
    $Process.Refresh()
    $rect = New-Object IcedReaderNative+Rect
    if (-not [IcedReaderNative]::GetClientRect($Process.MainWindowHandle, [ref]$rect)) { throw 'GetClientRect failed' }
    $origin = New-Object IcedReaderNative+Point
    $origin.X = 0; $origin.Y = 0
    if (-not [IcedReaderNative]::ClientToScreen($Process.MainWindowHandle, [ref]$origin)) { throw 'ClientToScreen failed' }
    $scale = [IcedReaderNative]::GetDpiForWindow($Process.MainWindowHandle) / 96.0
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
        Write-Host "screenshot: $Name"
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
}

function Get-Scroll([Diagnostics.Process]$Process) {
    $Process.Refresh()
    if ($Process.MainWindowTitle -match 'scroll=(\d+)/(\d+)') { return @([int]$Matches[1], [int]$Matches[2]) }
    return @(0, 0)
}

function Measure-ScrollMaximum([Diagnostics.Process]$Process) {
    Wait-AtBottom $Process
    $atBottom = Get-Scroll $Process
    for ($round = 0; $round -lt 24; $round++) {
        Wheel-Window $Process 500 400 1200 24
        $restored = Get-Scroll $Process
        if ($restored[0] -le 3) { return $atBottom[1] }
    }
    throw "Scroll did not return to the top after viewport measurement (offset=$($restored[0]))"
}

function Wait-AtBottom([Diagnostics.Process]$Process) {
    for ($round = 0; $round -lt 24; $round++) {
        Wheel-Window $Process 500 400 -1200 12
        $scroll = Get-Scroll $Process
        if ($scroll[1] -gt 0 -and ($scroll[1] - $scroll[0]) -le 3) { return }
    }
    $scroll = Get-Scroll $Process
    throw "Could not reach scroll bottom (offset=$($scroll[0]), max=$($scroll[1]))"
}

function Close-Cleanly([Diagnostics.Process]$Process, [string]$Case) {
    Send-AltF4
    if (-not $Process.WaitForExit(6000)) { throw "$Case did not close within six seconds" }
    if ($Process.ExitCode -ne 0) { throw "$Case exited $($Process.ExitCode), expected zero" }
}

function New-TempRoot([string]$Name) {
    $path = Join-Path $env:TEMP ("iced-reader-$Name-" + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $path | Out-Null
    $tempRoots.Add($path)
    return $path
}

function Copy-Fixture([string]$Destination) {
    $fixtures = Join-Path $Destination 'fixtures'
    New-Item -ItemType Directory -Path $fixtures | Out-Null
    Copy-Item -LiteralPath $FixtureSource -Destination $fixtures -Recurse
    return (Join-Path $fixtures 'reader-workload')
}

try {
    @(
        "os=$([Environment]::OSVersion.VersionString)"
        "powershell=$($PSVersionTable.PSVersion)"
        'fixture_revision=reader-workload-fx-3'
        'renderer=Iced 0.14.0 / WGPU; adapter diagnostic disabled for normal-mode evidence'
        'mode=--reader-poc; bounded reader/native status gates enabled only for this interactive driver; external screen captures are evidence, not app markers'
        "exe=$ExePath"
        "release_sha256=$releaseHash"
        "release_bytes=$((Get-Item -LiteralPath $ExePath).Length)"
        "patch_shape_sha256=$patchHash"
        "cargo_lock_sha256=$lockHash"
    ) | Set-Content -LiteralPath (Join-Path $EvidenceDirectory 'run-context.txt') -Encoding UTF8

    'default negative control: empty mode launched from OS TEMP does not read fixtures'
    $empty = Start-Shell $env:TEMP
    Wait-Title $empty 'panel=hidden;focus=info'
    if ($empty.MainWindowTitle.Contains('reader=')) { throw "Default launch entered the opt-in reader path: $($empty.MainWindowTitle)" }
    Capture-Client $empty 'default-empty-shell.png'
    Start-Sleep -Milliseconds 500
    if ($empty.MainWindowTitle.Contains('reader=') -or -not $empty.MainWindowTitle.Contains('panel=hidden;focus=info')) {
        throw "Default shell changed unexpectedly: $($empty.MainWindowTitle)"
    }
    Mouse-At $empty 50 31
    Wait-Title $empty 'panel=visible;focus=info'
    Send-Key 0x1b
    Wait-Title $empty 'panel=hidden;focus=info'
    Send-Key 0x70
    Wait-Title $empty 'panel=visible;focus=info'
    Send-Key 0x1b
    Close-Cleanly $empty 'default empty shell'

    'reader shell-first: capture initial empty shell before deferred fixture access'
    $reader = Start-Shell $RepositoryRoot @('--reader-poc')
    Wait-Title $reader 'reader=shell;width=800;body=0;items=0;error=none' 15
    $dpi = [IcedReaderNative]::GetDpiForWindow($reader.MainWindowHandle)
    Capture-Client $reader 'reader-shell-only.png'
    "host_dpi=$dpi" | Add-Content -LiteralPath (Join-Path $EvidenceDirectory 'run-context.txt')
    "shell_only_title=$($reader.MainWindowTitle)" | Add-Content -LiteralPath (Join-Path $EvidenceDirectory 'run-context.txt')
    Wait-Title $reader 'reader=ready;width=800;body=1000;items=1051;error=none' 90
    Capture-Client $reader 'reader-wide-800-top.png'
    "reader_ready_title=$($reader.MainWindowTitle)" | Add-Content -LiteralPath (Join-Path $EvidenceDirectory 'run-context.txt')

    'toolbar/focus: reader mode keeps Info, F1/Escape, Tab traversal, and Exit focus'
    Mouse-At $reader 50 31
    Wait-Title $reader 'panel=visible;focus=info'
    Send-Key 0x1b
    Wait-Title $reader 'panel=hidden;focus=info'
    Send-Key 0x70
    Wait-Title $reader 'panel=visible;focus=info'
    Send-Key 0x1b
    Send-Key 0x09
    Wait-Title $reader 'panel=hidden;focus=exit'
    Send-Key 0x09 -Shift
    Wait-Title $reader 'panel=hidden;focus=info'

    'narrow 480 DIP scenario and resize'
    Mouse-At $reader 930 115
    Wait-Title $reader 'reader=ready;width=480;body=1000;items=1051;error=none'
    Capture-Client $reader 'reader-narrow-480-top.png'
    $smallViewportMaximum = Measure-ScrollMaximum $reader
    if (-not [IcedReaderNative]::SetWindowPos($reader.MainWindowHandle, [IntPtr]::Zero, 0, 0, 900, 820, 0x0016)) { throw 'SetWindowPos resize failed' }
    Start-Sleep -Milliseconds 300
    Capture-Client $reader 'reader-narrow-resized.png'
    "narrow_resized_title=$($reader.MainWindowTitle)" | Add-Content -LiteralPath (Join-Path $EvidenceDirectory 'run-context.txt')

    'tall-window resize keeps the shared 600-DIP scroll viewport capped'
    if (-not [IcedReaderNative]::SetWindowPos($reader.MainWindowHandle, [IntPtr]::Zero, 0, 0, 900, 1200, 0x0016)) { throw 'First tall SetWindowPos resize failed' }
    Start-Sleep -Milliseconds 350
    $firstTallMaximum = Measure-ScrollMaximum $reader
    if (-not [IcedReaderNative]::SetWindowPos($reader.MainWindowHandle, [IntPtr]::Zero, 0, 0, 900, 1500, 0x0016)) { throw 'Second tall SetWindowPos resize failed' }
    Start-Sleep -Milliseconds 350
    $secondTallMaximum = Measure-ScrollMaximum $reader
    if (($smallViewportMaximum - $firstTallMaximum) -le 3) {
        throw "Tall reader viewport did not grow beyond the smaller-window viewport (small max=$smallViewportMaximum, tall max=$firstTallMaximum)"
    }
    if ([Math]::Abs($firstTallMaximum - $secondTallMaximum) -gt 3) {
        throw "Reader scroll viewport kept growing past the recipe cap (tall maxima=$firstTallMaximum,$secondTallMaximum)"
    }
    "viewport_small_scroll_max=$smallViewportMaximum" | Add-Content -LiteralPath (Join-Path $EvidenceDirectory 'run-context.txt')
    "viewport_tall_scroll_max=$firstTallMaximum,$secondTallMaximum" | Add-Content -LiteralPath (Join-Path $EvidenceDirectory 'run-context.txt')
    if (-not [IcedReaderNative]::SetWindowPos($reader.MainWindowHandle, [IntPtr]::Zero, 0, 0, 900, 820, 0x0016)) { throw 'Restore reader window resize failed' }
    Start-Sleep -Milliseconds 350

    'scroll through curated cases and image, then capture later generated paragraphs'
    Wheel-Window $reader 500 430 -1200 2
    Capture-Client $reader 'reader-narrow-curated-cases.png'
    "curated_scroll_title=$($reader.MainWindowTitle)" | Add-Content -LiteralPath (Join-Path $EvidenceDirectory 'run-context.txt')
    Wheel-Window $reader 500 430 -1200 3
    Capture-Client $reader 'reader-narrow-probes-and-image.png'
    "image_scroll_title=$($reader.MainWindowTitle)" | Add-Content -LiteralPath (Join-Path $EvidenceDirectory 'run-context.txt')
    Wait-AtBottom $reader
    Capture-Client $reader 'reader-narrow-later-items-bottom.png'
    "bottom_scroll_title=$($reader.MainWindowTitle)" | Add-Content -LiteralPath (Join-Path $EvidenceDirectory 'run-context.txt')
    Wheel-Window $reader 500 430 1200 24
    Capture-Client $reader 'reader-narrow-scroll-restored.png'

    Close-Cleanly $reader 'reader mode'

    'missing root: explicit mode reports an actionable in-window error'
    $missingRoot = New-TempRoot 'missing-root'
    $rootFailure = Start-Shell $missingRoot @('--reader-poc')
    Wait-Title $rootFailure 'reader=failed;width=800;body=0;items=0;error=root'
    Capture-Client $rootFailure 'error-missing-root.png'
    Close-Cleanly $rootFailure 'missing fixture root'

    'missing font and checksum-corrupt font are visible failures, never system substitutes'
    $missingFontRoot = New-TempRoot 'missing-font'
    $fixture = Copy-Fixture $missingFontRoot
    Remove-Item -LiteralPath (Join-Path $fixture 'assets\fonts\NotoSans-Regular.ttf')
    $missingFont = Start-Shell $missingFontRoot @('--reader-poc')
    Wait-Title $missingFont 'reader=failed;width=800;body=0;items=0;error=missing-asset'
    Capture-Client $missingFont 'error-missing-font.png'
    Close-Cleanly $missingFont 'missing font'

    $corruptFontRoot = New-TempRoot 'corrupt-font'
    $fixture = Copy-Fixture $corruptFontRoot
    $fontPath = Join-Path $fixture 'assets\fonts\NotoSans-Regular.ttf'
    $fontBytes = [IO.File]::ReadAllBytes($fontPath); $fontBytes[0] = $fontBytes[0] -bxor 0xff
    [IO.File]::WriteAllBytes($fontPath, $fontBytes)
    $corruptFont = Start-Shell $corruptFontRoot @('--reader-poc')
    Wait-Title $corruptFont 'reader=failed;width=800;body=0;items=0;error=checksum'
    Capture-Client $corruptFont 'error-corrupt-font.png'
    Close-Cleanly $corruptFont 'corrupt font'

    'missing and checksum-corrupt PNG inputs are visible failures'
    $missingImageRoot = New-TempRoot 'missing-image'
    $fixture = Copy-Fixture $missingImageRoot
    Remove-Item -LiteralPath (Join-Path $fixture 'assets\images\reader-sample.png')
    $missingImage = Start-Shell $missingImageRoot @('--reader-poc')
    Wait-Title $missingImage 'reader=failed;width=800;body=0;items=0;error=missing-asset'
    Capture-Client $missingImage 'error-missing-image.png'
    Close-Cleanly $missingImage 'missing image'

    $corruptImageRoot = New-TempRoot 'corrupt-image'
    $fixture = Copy-Fixture $corruptImageRoot
    $imagePath = Join-Path $fixture 'assets\images\reader-sample.png'
    $imageBytes = [IO.File]::ReadAllBytes($imagePath); $middle = [int]($imageBytes.Length / 2)
    $imageBytes[$middle] = $imageBytes[$middle] -bxor 0xff
    [IO.File]::WriteAllBytes($imagePath, $imageBytes)
    $corruptImage = Start-Shell $corruptImageRoot @('--reader-poc')
    Wait-Title $corruptImage 'reader=failed;width=800;body=0;items=0;error=checksum'
    Capture-Client $corruptImage 'error-corrupt-image.png'
    Close-Cleanly $corruptImage 'corrupt image'

    'foreground guard negative: another owned shell covers a still-visible target'
    $background = Start-Shell $env:TEMP
    Wait-Title $background 'panel=hidden;focus=info'
    $cover = Start-Shell $env:TEMP
    Wait-Title $cover 'panel=hidden;focus=info'
    Start-Sleep -Milliseconds 100
    if ($background.HasExited -or -not [IcedReaderNative]::IsWindowVisible($background.MainWindowHandle) -or
        [IcedReaderNative]::GetForegroundWindow() -ne $cover.MainWindowHandle) {
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
    Close-Cleanly $cover 'foreground guard cover'
    [void]$background.CloseMainWindow()
    if (-not $background.WaitForExit(6000) -or $background.ExitCode -ne 0) {
        throw 'Foreground guard background shell did not close cleanly'
    }

    if ((Get-FileHash -LiteralPath $ExePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $releaseHash) {
        throw 'Release executable changed during reader evidence capture'
    }
    'reader-poc: interactive checks passed; inspect captured pixels against fixtures/reader-workload/references/bidi-reference.md'
    $EvidenceDirectory
} finally {
    if ($null -eq $oldReaderStatus) { Remove-Item Env:ICED_SHELL_READER_TEST_STATUS -ErrorAction SilentlyContinue }
    else { $env:ICED_SHELL_READER_TEST_STATUS = $oldReaderStatus }
    if ($null -eq $oldNativeStatus) { Remove-Item Env:ICED_SHELL_NATIVE_TEST_STATUS -ErrorAction SilentlyContinue }
    else { $env:ICED_SHELL_NATIVE_TEST_STATUS = $oldNativeStatus }
    foreach ($process in $owned) {
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
    }
    foreach ($path in $tempRoots) {
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force -ErrorAction SilentlyContinue }
    }
}
