param(
    [string]$ExePath = (Join-Path $PSScriptRoot '..\..\..\target\release\iced-shell.exe'),
    [string]$EvidenceDirectory = (Join-Path $env:TEMP ("iced-virtual-reader-" + [Guid]::NewGuid().ToString('N')))
)
$ErrorActionPreference = 'Stop'
$ExePath = [IO.Path]::GetFullPath($ExePath)
$EvidenceDirectory = [IO.Path]::GetFullPath($EvidenceDirectory)
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\..'))
if (Test-Path -LiteralPath $EvidenceDirectory) { throw "Evidence directory must not exist: $EvidenceDirectory" }
if (-not [Environment]::UserInteractive) { throw 'Interactive desktop required' }
$overrides = Get-ChildItem Env: | Where-Object { $_.Name -match '^(ICED_|WINIT_)' }
if ($overrides) { throw "Refusing inherited framework/evidence overrides: $($overrides.Name -join ',')" }
if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) { throw "Missing release executable: $ExePath" }
New-Item -ItemType Directory -Path $EvidenceDirectory | Out-Null
$hash = (Get-FileHash $ExePath -Algorithm SHA256).Hash.ToLowerInvariant()
$patch = (Get-FileHash (Join-Path $root 'patches\cosmic-text-0.15.0\src\shape.rs') -Algorithm SHA256).Hash.ToLowerInvariant()
$lock = (Get-FileHash (Join-Path $root 'Cargo.lock') -Algorithm SHA256).Hash.ToLowerInvariant()
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class VirtualNative {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, ref Rect r);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref Point p);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint x, uint y, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int hgt, uint flags);
}
"@
$previous = $env:ICED_SHELL_READER_TEST_STATUS
$previousVirtual = $env:ICED_SHELL_VIRTUAL_TEST_STATUS
$env:ICED_SHELL_READER_TEST_STATUS = '1'
$env:ICED_SHELL_VIRTUAL_TEST_STATUS = '1'
$owned = [Collections.Generic.List[Diagnostics.Process]]::new()
$records = [Collections.Generic.List[string]]::new()
function Activate([Diagnostics.Process]$p) {
    $p.Refresh()
    if ($p.HasExited -or $p.MainWindowHandle -eq 0 -or -not [VirtualNative]::SetForegroundWindow($p.MainWindowHandle)) { throw "Cannot foreground owned process $($p.Id)" }
    Start-Sleep -Milliseconds 100
}
function Assert-Owned([Diagnostics.Process]$p) {
    $p.Refresh()
    [uint32]$owner = 0
    $h = $p.MainWindowHandle
    if ($p.HasExited -or $h -eq 0 -or -not [VirtualNative]::IsWindowVisible($h) -or [VirtualNative]::GetForegroundWindow() -ne $h -or
        [VirtualNative]::GetWindowThreadProcessId($h, [ref]$owner) -eq 0 -or $owner -ne [uint32]$p.Id) { throw 'Capture target not visible foreground owned HWND' }
}
function Start-Reader([string]$mode) {
    $p = Start-Process -FilePath $ExePath -ArgumentList @($mode) -WorkingDirectory $root -PassThru
    $owned.Add($p)
    if (-not $p.WaitForInputIdle(15000)) { throw 'No input-idle reader window' }
    Activate $p
    return $p
}
function Wait-Ready([Diagnostics.Process]$p, [string]$fragment) {
    $until = [DateTime]::UtcNow.AddSeconds(90)
    do {
        $p.Refresh()
        if ($p.HasExited) { throw "Reader exited $($p.ExitCode)" }
        if ($p.MainWindowTitle.Contains($fragment)) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $until)
    throw "Expected $fragment, got $($p.MainWindowTitle)"
}
function Key([Diagnostics.Process]$p, [byte]$code) {
    Activate $p
    [VirtualNative]::keybd_event($code,0,0,[UIntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    [VirtualNative]::keybd_event($code,0,2,[UIntPtr]::Zero)
    Start-Sleep -Milliseconds 220
}
function Point-In([Diagnostics.Process]$p, [int]$x, [int]$y) {
    $point = New-Object VirtualNative+Point
    $point.X = $x; $point.Y = $y
    if (-not [VirtualNative]::ClientToScreen($p.MainWindowHandle,[ref]$point)) { throw 'ClientToScreen failed' }
    return $point
}
function Click([Diagnostics.Process]$p, [int]$x, [int]$y) {
    Activate $p
    $point = Point-In $p $x $y
    [void][VirtualNative]::SetCursorPos($point.X,$point.Y)
    Start-Sleep -Milliseconds 130
    [VirtualNative]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    [VirtualNative]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
    Start-Sleep -Milliseconds 350
}
function Wheel([Diagnostics.Process]$p, [int]$delta) {
    Activate $p
    $point = Point-In $p 500 400
    [void][VirtualNative]::SetCursorPos($point.X,$point.Y)
    $wheelData = [BitConverter]::ToUInt32([BitConverter]::GetBytes([int32]$delta),0)
    [VirtualNative]::mouse_event(0x0800,0,0,$wheelData,[UIntPtr]::Zero)
    Start-Sleep -Milliseconds 600
}
function Scroll-AtTop([Diagnostics.Process]$p) {
    Activate $p
    $p.Refresh()
    if ($p.MainWindowTitle -match 'scroll=(\d+)/(\d+)' -and [int]$Matches[1] -le 2) { return }
    $bottom = Point-In $p 970 685
    $top = Point-In $p 970 140
    [void][VirtualNative]::SetCursorPos($bottom.X,$bottom.Y)
    Start-Sleep -Milliseconds 140
    [VirtualNative]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
    Start-Sleep -Milliseconds 100
    [void][VirtualNative]::SetCursorPos($top.X,$top.Y)
    Start-Sleep -Milliseconds 260
    [VirtualNative]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
    Start-Sleep -Milliseconds 450
    for ($attempt = 0; $attempt -lt 40; $attempt++) {
        $p.Refresh()
        if ($p.MainWindowTitle -match 'scroll=(\d+)/(\d+)' -and [int]$Matches[1] -le 2) { break }
        Wheel $p 1200
    }
    $p.Refresh()
    if ($p.MainWindowTitle -notmatch 'scroll=(\d+)/(\d+)' -or [int]$Matches[1] -gt 2) { throw "Native thumb did not restore top: $($p.MainWindowTitle)" }
}
function Scroll-AtEnd([Diagnostics.Process]$p) {
    Activate $p
    # Start from a known top scrollbar thumb; move the owned native thumb to
    # the physical bottom, rather than assuming a huge wheel event reaches it.
    for ($i = 0; $i -lt 60; $i++) {
        Wheel $p 1200
        $p.Refresh()
        if ($p.MainWindowTitle -match 'scroll=(\d+)/(\d+)' -and [int]$Matches[1] -le 2) { break }
    }
    $p.Refresh()
    if ($p.MainWindowTitle -notmatch 'scroll=(\d+)/(\d+)' -or [int]$Matches[1] -gt 2) { throw 'Could not restore scrollbar top before drag' }
    $top = Point-In $p 970 140
    $bottom = Point-In $p 970 710
    [void][VirtualNative]::SetCursorPos($top.X,$top.Y)
    Start-Sleep -Milliseconds 130
    [VirtualNative]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
    Start-Sleep -Milliseconds 100
    [void][VirtualNative]::SetCursorPos($bottom.X,$bottom.Y)
    Start-Sleep -Milliseconds 250
    [VirtualNative]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
    Start-Sleep -Milliseconds 600
    $p.Refresh()
    if ($p.MainWindowTitle -notmatch 'scroll=(\d+)/(\d+)' -or [int]$Matches[2] -lt 50000 -or ([int]$Matches[2] - [int]$Matches[1]) -gt 10) {
        throw "Native scrollbar did not reach end: $($p.MainWindowTitle)"
    }
}
function Reader-PixelFingerprint([Drawing.Bitmap]$bmp) {
    # Physical client pixels on the pinned 120-DPI, 1000x720 initial viewport.
    # Heading, original RTL text and source image occupy distinct regions.
    # Fail closed when DPI/size differs: this is an evidence probe, not a renderer oracle.
    if ($bmp.Width -ne 1250 -or $bmp.Height -ne 900) { throw "Top content pixel probe needs 1250x900 pixels, got $($bmp.Width)x$($bmp.Height)" }
    $counts = @(0,0,0)
    $regions = @(@(325,950,175,220), @(325,950,320,465), @(480,770,790,865))
    foreach ($regionIndex in 0..2) {
        $region = $regions[$regionIndex]
        # Count high-contrast text/image pixels rather than trusting title/layout.
        for ($y = $region[2]; $y -lt $region[3]; $y += 6) {
            for ($x = $region[0]; $x -lt $region[1]; $x += 6) {
                $color = $bmp.GetPixel($x,$y)
                if ($color.R -gt 110 -and $color.G -gt 90 -and $color.B -gt 80) { $counts[$regionIndex]++ }
            }
        }
    }
    if ($counts[0] -lt 30 -or $counts[1] -lt 40 -or $counts[2] -lt 50) { return $null }
    $crop = $bmp.Clone((New-Object Drawing.Rectangle(325,175,625,690)), [Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $stream = New-Object IO.MemoryStream
    try {
        $crop.Save($stream,[Drawing.Imaging.ImageFormat]::Png)
        $sha = [Security.Cryptography.SHA256]::Create()
        try { return ([BitConverter]::ToString($sha.ComputeHash($stream.ToArray()))).Replace('-','').ToLowerInvariant() }
        finally { $sha.Dispose() }
    } finally { $crop.Dispose(); $stream.Dispose() }
}
function Capture([Diagnostics.Process]$p, [string]$name, [switch]$RequireReaderPixels) {
    Activate $p
    Assert-Owned $p
    $rect = New-Object VirtualNative+Rect
    if (-not [VirtualNative]::GetClientRect($p.MainWindowHandle,[ref]$rect)) { throw 'GetClientRect failed' }
    $point = Point-In $p 0 0
    $scale = [VirtualNative]::GetDpiForWindow($p.MainWindowHandle) / 96.0
    $bmp = New-Object System.Drawing.Bitmap([int][Math]::Round($rect.Right*$scale),[int][Math]::Round($rect.Bottom*$scale))
    $graphics = [System.Drawing.Graphics]::FromImage($bmp)
    try {
        $fingerprint = $null
        $attempts = if ($RequireReaderPixels) { 12 } else { 1 }
        for ($attempt = 0; $attempt -lt $attempts; $attempt++) {
            Assert-Owned $p
            $graphics.CopyFromScreen([int][Math]::Round($point.X*$scale),[int][Math]::Round($point.Y*$scale),0,0,$bmp.Size)
            Assert-Owned $p
            if (-not $RequireReaderPixels) { break }
            $candidate = Reader-PixelFingerprint $bmp
            if ($candidate -and $candidate -eq $fingerprint) { break }
            $fingerprint = $candidate
            Start-Sleep -Milliseconds 250
        }
        if ($RequireReaderPixels -and (-not $fingerprint -or $attempt -ge $attempts)) { throw "Top reader pixels never stabilized after $attempts guarded captures: $name" }
        if ($RequireReaderPixels -and $name.EndsWith('reentered-rtl-image') -and $fingerprint -ne $script:topFingerprint) {
            throw "Re-entered heading/RTL/image pixels differ from the same-process initial narrow top: $name"
        }
        if ($RequireReaderPixels -and $name.EndsWith('narrow-top')) { $script:topFingerprint = $fingerprint }
        $bmp.Save((Join-Path $EvidenceDirectory "$name.png"),[System.Drawing.Imaging.ImageFormat]::Png)
        $p.Refresh()
        $title = $p.MainWindowTitle
        if ($title -match 'built=(\d+);layout=(\d+);peak=(\d+)/(\d+);index=(\d+);range=(\d+)\.\.(\d+);ids=([^;]+)') {
            $built = [int]$Matches[1]; $layout = [int]$Matches[2]
            $index = [int]$Matches[5]; $first = [int]$Matches[6]; $end = [int]$Matches[7]
            if ($built -gt 128 -or $layout -gt 128 -or [int]$Matches[3] -gt 128 -or [int]$Matches[4] -gt 128) { throw "Eager widget/layout path detected: $title" }
            if ($name.EndsWith('-closed')) {
                if ($built -ne 0 -or $layout -ne 0 -or $index -ne 0 -or $end -ne 0) { throw "Closed content retains app rows/index: $title" }
            } elseif ($built -ne ($end - $first) -or $index -ne $script:expectedItems -or $layout -ne $built) {
                throw "Inconsistent active row/index/layout counters: $title"
            }
        } else { throw "Missing native layout counters: $title" }
        $records.Add("$name pid=$($p.Id) dpi=$([VirtualNative]::GetDpiForWindow($p.MainWindowHandle)) pixel_sha256=$fingerprint stable_attempts=$($attempt+1) title=$title")
    } finally { $graphics.Dispose(); $bmp.Dispose() }
}
try {
    $blank = New-Object Drawing.Bitmap(1250,900)
    $blankGraphics = [Drawing.Graphics]::FromImage($blank)
    try {
        $blankGraphics.Clear([Drawing.Color]::FromArgb(22,27,34))
        if (Reader-PixelFingerprint $blank) { throw 'Blank-reader pixel negative control was incorrectly accepted' }
    } finally { $blankGraphics.Dispose(); $blank.Dispose() }
    @("exe=$ExePath", "release_sha256=$hash", "release_bytes=$((Get-Item $ExePath).Length)", "patch_shape_sha256=$patch", "cargo_lock_sha256=$lock", 'fixture_revision=reader-workload-fx-3', "os=$([Environment]::OSVersion.VersionString)", 'native release tiny-skia (CPU); opt-in reader/virtual status only; no startup/BiDi diagnostics') | Set-Content (Join-Path $EvidenceDirectory 'context.txt')
    foreach ($case in @(@('--reader-poc','1000','1051'), @('--reader-poc-large','10000','10501'))) {
        $script:expectedItems = [int]$case[2]
        $p = Start-Reader $case[0]
        Wait-Ready $p "reader=ready;width=800;body=$($case[1]);items=$($case[2])"
        Capture $p "$($case[1])-wide-top"
        Click $p 930 115
        Wait-Ready $p "reader=ready;width=480;body=$($case[1]);items=$($case[2])"
        Capture $p "$($case[1])-narrow-top" -RequireReaderPixels
        $foundMid = $false
        for ($step = 0; $step -lt 100; $step++) {
            $p.Refresh()
            if ($p.MainWindowTitle -match 'anchor=p-00010;within=([0-9.]+)' -and [double]$Matches[1] -gt 0) { $foundMid = $true; break }
            Wheel $p -120
        }
        if (-not $foundMid) { throw "Could not reach mid-paragraph p-00010 anchor: $($p.MainWindowTitle)" }
        Capture $p "$($case[1])-narrow-mid-paragraph"
        Click $p 930 115
        Wait-Ready $p "reader=ready;width=800;body=$($case[1]);items=$($case[2])"
        if ($p.MainWindowTitle -notmatch 'anchor=p-00010;within=([0-9.]+)') { throw "Narrow-to-wide lost p-00010 mid-paragraph anchor: $($p.MainWindowTitle)" }
        Capture $p "$($case[1])-wide-mid-paragraph"
        Click $p 930 115
        Wait-Ready $p "reader=ready;width=480;body=$($case[1]);items=$($case[2])"
        if ($p.MainWindowTitle -notmatch 'anchor=p-00010;within=([0-9.]+)') { throw "Wide-to-narrow lost p-00010 anchor: $($p.MainWindowTitle)" }
        for ($step = 0; $step -lt 30; $step++) {
            Wheel $p 1200
            $p.Refresh()
            if ($p.MainWindowTitle -match 'scroll=(\d+)/(\d+)' -and [int]$Matches[1] -eq 0) { break }
        }
        if ($p.MainWindowTitle -notmatch 'scroll=(\d+)/(\d+)' -or [int]$Matches[1] -ne 0) { throw "Return to narrow top failed: $($p.MainWindowTitle)" }
        for ($attempt = 0; $attempt -lt 4; $attempt++) {
            Wheel $p -1200
            $p.Refresh()
            if ($p.MainWindowTitle -match 'scroll=(\d+)/(\d+)' -and [int]$Matches[1] -gt 100) { break }
        }
        $p.Refresh()
        if ($p.MainWindowTitle -notmatch 'scroll=(\d+)/(\d+)' -or [int]$Matches[1] -le 100) { throw "Short scroll did not move: $($p.MainWindowTitle)" }
        Capture $p "$($case[1])-narrow-forward"
        Wheel $p -120000
        Capture $p "$($case[1])-narrow-far"
        Scroll-AtEnd $p
        Capture $p "$($case[1])-narrow-end"
        $p.Refresh()
        if (-not $p.MainWindowTitle.Contains("..p-$($case[1].PadLeft(5,'0'));anchor=")) { throw "Final paragraph ID is not at the end: $($p.MainWindowTitle)" }
        $p.Refresh()
        if ($p.MainWindowTitle -notmatch 'scroll=(\d+)/(\d+)') { throw 'Missing end offset' }
        $endOffset = [int]$Matches[1]
        for ($attempt = 0; $attempt -lt 5; $attempt++) {
            Wheel $p 1200
            $p.Refresh()
            if ($p.MainWindowTitle -match 'scroll=(\d+)/(\d+)' -and [int]$Matches[1] -lt ($endOffset - 100)) { break }
        }
        if ($p.MainWindowTitle -notmatch 'scroll=(\d+)/(\d+)' -or [int]$Matches[1] -ge ($endOffset - 100)) { throw "Reverse scroll did not move: $($p.MainWindowTitle)" }
        Capture $p "$($case[1])-narrow-reverse"
        Scroll-AtTop $p
        Capture $p "$($case[1])-narrow-reentered-rtl-image" -RequireReaderPixels
        Scroll-AtEnd $p
        Click $p 930 115
        Wait-Ready $p "reader=ready;width=800;body=$($case[1]);items=$($case[2])"
        Capture $p "$($case[1])-wide-anchor"
        if (-not [VirtualNative]::SetWindowPos($p.MainWindowHandle,[IntPtr]::Zero,0,0,900,610,0x0016)) { throw 'Short resize failed' }
        Start-Sleep -Milliseconds 300
        Capture $p "$($case[1])-short-resized"
        $p.Refresh()
        if ($p.MainWindowTitle -notmatch 'scroll=(\d+)/(\d+)') { throw 'Missing short viewport maximum' }
        $shortMax = [int]$Matches[2]
        if (-not [VirtualNative]::SetWindowPos($p.MainWindowHandle,[IntPtr]::Zero,0,0,900,1100,0x0016)) { throw 'Tall resize failed' }
        Start-Sleep -Milliseconds 300
        Capture $p "$($case[1])-tall-capped"
        $p.Refresh()
        if ($p.MainWindowTitle -notmatch 'scroll=(\d+)/(\d+)' -or $shortMax -le ([int]$Matches[2] + 30)) { throw "Viewport cap/resize did not change scroll range: $($p.MainWindowTitle)" }
        if (-not [VirtualNative]::SetWindowPos($p.MainWindowHandle,[IntPtr]::Zero,0,0,900,820,0x0016)) { throw 'Restore resize failed' }
        Start-Sleep -Milliseconds 300
        Capture $p "$($case[1])-resized"
        Key $p 0x74 # F5: release all reader content and active rows, not global font registration
        Wait-Ready $p 'reader=closed;'
        Capture $p "$($case[1])-closed"
        Key $p 0x74
        Wait-Ready $p "reader=ready;width=800;body=$($case[1]);items=$($case[2])"
        Capture $p "$($case[1])-reopened"
        Wheel $p 1200000
        Capture $p "$($case[1])-reopened-top"
        Activate $p
        [VirtualNative]::keybd_event(0x12,0,0,[UIntPtr]::Zero)
        [VirtualNative]::keybd_event(0x73,0,0,[UIntPtr]::Zero)
        [VirtualNative]::keybd_event(0x73,0,2,[UIntPtr]::Zero)
        [VirtualNative]::keybd_event(0x12,0,2,[UIntPtr]::Zero)
        if (-not $p.WaitForExit(10000) -or $p.ExitCode -ne 0) { throw "Reader did not close cleanly: $($p.Id)" }
    }
    $records | Set-Content (Join-Path $EvidenceDirectory 'counters.txt')
    if ((Get-FileHash $ExePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $hash) { throw 'Executable changed during capture' }
    Write-Host "virtual-reader: captures and bounded counters recorded at $EvidenceDirectory; manually inspect original pixels and scroll anchors"
} finally {
    foreach ($p in $owned) { if (-not $p.HasExited) { $p.Kill(); $p.WaitForExit(5000) | Out-Null }; $p.Dispose() }
    $env:ICED_SHELL_READER_TEST_STATUS = $previous
    $env:ICED_SHELL_VIRTUAL_TEST_STATUS = $previousVirtual
}
