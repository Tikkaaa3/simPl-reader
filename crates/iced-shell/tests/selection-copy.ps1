param(
    [string]$ExePath = (Join-Path $PSScriptRoot '..\..\..\target\release\iced-shell.exe'),
    [string]$EvidenceDirectory = (Join-Path $env:TEMP ("iced-selection-" + [Guid]::NewGuid().ToString('N')))
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ExePath = [IO.Path]::GetFullPath($ExePath)
$EvidenceDirectory = [IO.Path]::GetFullPath($EvidenceDirectory)
$RepositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\..'))
$FixtureRoot = Join-Path $RepositoryRoot 'fixtures\reader-workload'
$GoldenRoot = Join-Path $FixtureRoot 'references\expected-copy'
if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) { throw "Release executable not found: $ExePath" }
if (-not [Environment]::UserInteractive) { throw 'selection-copy.ps1 requires an interactive Windows desktop' }
if ([Threading.Thread]::CurrentThread.GetApartmentState() -ne [Threading.ApartmentState]::STA) {
    throw 'selection-copy.ps1 must run in an STA Windows PowerShell session for OS clipboard access'
}
if (Test-Path -LiteralPath $EvidenceDirectory) {
    if ((Get-ChildItem -LiteralPath $EvidenceDirectory -Force | Measure-Object).Count -ne 0) {
        throw "Evidence directory must be new or empty: $EvidenceDirectory"
    }
} else {
    New-Item -ItemType Directory -Path $EvidenceDirectory | Out-Null
}

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class IcedSelectionNative {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd, ref Rect rect);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, ref Rect rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern short GetAsyncKeyState(int virtualKey);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll", SetLastError=true)] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
}
"@

$overrides = @(Get-ChildItem Env: | Where-Object {
    $_.Name -match '^WINIT_' -or ($_.Name -match '^ICED_' -and $_.Name -notmatch '^ICED_SHELL_(READER_TEST_STATUS|VIRTUAL_TEST_STATUS|SELECTION_TEST_STATUS|NATIVE_TEST_STATUS)$')
})
if ($overrides.Count -ne 0) {
    throw "Refusing selection evidence with inherited renderer/framework overrides: $($overrides.Name -join ', ')"
}

$script:owned = [Collections.Generic.List[Diagnostics.Process]]::new()
$script:records = [Collections.Generic.List[string]]::new()
$script:heldMouseButton = $false
$script:heldKeys = @{}
$script:inputDispatches = [Collections.Generic.List[string]]::new()
$statusNames = @(
    'ICED_SHELL_READER_TEST_STATUS',
    'ICED_SHELL_VIRTUAL_TEST_STATUS',
    'ICED_SHELL_SELECTION_TEST_STATUS',
    'ICED_SHELL_NATIVE_TEST_STATUS'
)
$oldStatus = @{}
foreach ($name in $statusNames) {
    $oldStatus[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
$env:ICED_SHELL_READER_TEST_STATUS = '1'
$env:ICED_SHELL_VIRTUAL_TEST_STATUS = '1'
$env:ICED_SHELL_SELECTION_TEST_STATUS = '1'
$env:ICED_SHELL_NATIVE_TEST_STATUS = '1'
$releaseHash = (Get-FileHash -LiteralPath $ExePath -Algorithm SHA256).Hash.ToLowerInvariant()
$lockHash = (Get-FileHash -LiteralPath (Join-Path $RepositoryRoot 'Cargo.lock') -Algorithm SHA256).Hash.ToLowerInvariant()
$patchPath = Join-Path $RepositoryRoot 'patches\cosmic-text-0.15.0\src\shape.rs'
$patchHash = if (Test-Path -LiteralPath $patchPath) { (Get-FileHash -LiteralPath $patchPath -Algorithm SHA256).Hash.ToLowerInvariant() } else { 'absent' }
$manifestHash = (Get-FileHash -LiteralPath (Join-Path $FixtureRoot 'manifest.txt') -Algorithm SHA256).Hash.ToLowerInvariant()

function Start-Shell([string]$WorkingDirectory, [string[]]$Arguments = @()) {
    if ($Arguments.Count -eq 0) {
        $process = Start-Process -FilePath $ExePath -WorkingDirectory $WorkingDirectory -WindowStyle Normal -PassThru
    } else {
        $process = Start-Process -FilePath $ExePath -ArgumentList $Arguments -WorkingDirectory $WorkingDirectory -WindowStyle Normal -PassThru
    }
    $script:owned.Add($process)
    if (-not $process.WaitForInputIdle(10000)) { throw 'Iced shell did not become input-idle within 10 seconds' }
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $process.Refresh()
        if ($process.MainWindowHandle -ne 0 -and [IcedSelectionNative]::IsWindowVisible($process.MainWindowHandle)) {
            if ([IcedSelectionNative]::SetForegroundWindow($process.MainWindowHandle)) {
                Assert-InputTarget $process
                return $process
            }
        }
        Start-Sleep -Milliseconds 25
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Iced shell did not create a visible foreground top-level window'
}

function Get-Title([Diagnostics.Process]$Process) {
    $Process.Refresh()
    if ($Process.HasExited) { throw "Iced process exited with code $($Process.ExitCode)" }
    return $Process.MainWindowTitle
}

function Wait-Title([Diagnostics.Process]$Process, [string]$Expected, [int]$TimeoutSeconds = 90) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $title = Get-Title $Process
        if ($title.Contains($Expected)) { return $title }
        Start-Sleep -Milliseconds 25
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for '$Expected'; title was '$((Get-Title $Process))'"
}

function Get-ClientScreenPoint([Diagnostics.Process]$Process, [int]$X, [int]$Y) {
    $point = New-Object IcedSelectionNative+Point
    $point.X = $X; $point.Y = $Y
    if (-not [IcedSelectionNative]::ClientToScreen($Process.MainWindowHandle, [ref]$point)) { throw 'ClientToScreen failed' }
    return $point
}

function Assert-InputTarget([Diagnostics.Process]$Process) {
    if ($null -eq $Process) { throw 'Refusing global input without an owned process target' }
    $Process.Refresh()
    if ($Process.HasExited) { throw "Refusing global input: owned process $($Process.Id) has exited" }
    $handle = $Process.MainWindowHandle
    if ($handle -eq [IntPtr]::Zero -or -not [IcedSelectionNative]::IsWindowVisible($handle) -or [IcedSelectionNative]::IsIconic($handle)) {
        throw "Refusing global input: process $($Process.Id) has no visible target window"
    }
    $foreground = [IcedSelectionNative]::GetForegroundWindow()
    [uint32]$ownerId = 0
    $threadId = [IcedSelectionNative]::GetWindowThreadProcessId($foreground, [ref]$ownerId)
    if ($foreground -ne $handle -or $threadId -eq 0 -or $ownerId -ne [uint32]$Process.Id) {
        throw "Refusing global input: process $($Process.Id) is not the visible foreground window"
    }
}

function Move-OwnedCursor([Diagnostics.Process]$Process, [int]$X, [int]$Y) {
    $point = Get-ClientScreenPoint $Process $X $Y
    Assert-InputTarget $Process
    if (-not [IcedSelectionNative]::SetCursorPos($point.X, $point.Y)) { throw 'SetCursorPos failed' }
    $script:inputDispatches.Add('mouse-move')
}

function Send-Wheel([Diagnostics.Process]$Process, [int]$X, [int]$Y, [int]$Delta) {
    $wheelData = [BitConverter]::ToUInt32([BitConverter]::GetBytes($Delta), 0)
    Move-OwnedCursor $Process $X $Y
    Assert-InputTarget $Process
    $script:inputDispatches.Add("wheel:$Delta")
    [IcedSelectionNative]::mouse_event(0x0800, 0, 0, $wheelData, [UIntPtr]::Zero)
}

function Read-Hit([Diagnostics.Process]$Process, [int]$X, [int]$Y) {
    $deadline = [DateTime]::UtcNow.AddMilliseconds(100)
    do {
        $title = Get-Title $Process
        if ($title -match 'cursor=(-?[0-9.]+),(-?[0-9.]+)(?:;|\])') {
            $cursorX = [double]::Parse($Matches[1], [Globalization.CultureInfo]::InvariantCulture)
            $cursorY = [double]::Parse($Matches[2], [Globalization.CultureInfo]::InvariantCulture)
            if ([Math]::Abs($cursorX - $X) -le 0.5 -and [Math]::Abs($cursorY - $Y) -le 0.5) {
                if ($title -match 'hit=([^;]+);point=(-?[0-9.]+),(-?[0-9.]+);cursor=') {
                    $hitText = $Matches[1]
                    $hitX = [double]::Parse($Matches[2], [Globalization.CultureInfo]::InvariantCulture)
                    $hitY = [double]::Parse($Matches[3], [Globalization.CultureInfo]::InvariantCulture)
                    if ($hitText -ne 'none' -and [Math]::Abs($hitX - $X) -le 0.5 -and [Math]::Abs($hitY - $Y) -le 0.5 -and
                        $hitText -match '^(?<id>[^@]+)@(?<offset>\d+)$') {
                        return [pscustomobject]@{ Id = $Matches.id; Offset = [int]$Matches.offset; X = $X; Y = $Y }
                    }
                }
                Start-Sleep -Milliseconds 8
                return $null
            }
        }
        Start-Sleep -Milliseconds 3
    } while ([DateTime]::UtcNow -lt $deadline)
    return $null
}

function Move-Hit([Diagnostics.Process]$Process, [int]$X, [int]$Y) {
    Move-OwnedCursor $Process $X $Y
    return Read-Hit $Process $X $Y
}

function Scroll-Window([Diagnostics.Process]$Process, [int]$Delta, [int]$Count = 1) {
    for ($index = 0; $index -lt $Count; $index++) {
        Send-Wheel $Process 500 430 $Delta
        Start-Sleep -Milliseconds 45
    }
    Start-Sleep -Milliseconds 120
}

function Restore-Top([Diagnostics.Process]$Process) {
    for ($round = 0; $round -lt 30; $round++) {
        $title = Get-Title $Process
        if ($title -match 'scroll=(\d+)/') {
            if ([int]$Matches[1] -le 2) { return }
        }
        Scroll-Window $Process 1200 1
    }
    throw "Could not restore reader top: $((Get-Title $Process))"
}

function Find-Endpoint([Diagnostics.Process]$Process, [string]$ItemId, [int]$Offset, [int]$Width) {
    if ($Width -ne 480 -and $Width -ne 800) { throw "Unsupported content width: $Width" }
    $left = [int][Math]::Floor((1000 - $Width) / 2) + 2
    $right = [int][Math]::Ceiling((1000 + $Width) / 2) - 2
    $center = 500
    $lastOffsets = @()
    for ($scrollRound = 0; $scrollRound -lt 24; $scrollRound++) {
        $samples = @()
        for ($y = 100; $y -le 690; $y += 6) {
            $hit = Move-Hit $Process $center $y
            if ($null -ne $hit -and $hit.Id -eq $ItemId) {
                $samples += [pscustomobject]@{ Y = $y; Offset = $hit.Offset }
            }
        }
        if ($samples.Count -eq 0) {
            if ($lastOffsets.Count -gt 0) { break }
            Scroll-Window $Process -1200 1
            continue
        }

        # The center-column native hits identify the actual visible row/line.
        # Search only those native text bounds; fine horizontal movement then
        # resolves the requested endpoint through the live Paragraph::hit_test.
        $candidateYs = @($samples | Sort-Object @{ Expression = { [Math]::Abs($_.Offset - $Offset) } }, Y | Select-Object -ExpandProperty Y -Unique)
        $lastOffsets = @($samples | ForEach-Object { $_.Offset })
        foreach ($y in $candidateYs) {
            $best = $null
            for ($x = $left; $x -le $right; $x += 4) {
                $hit = Move-Hit $Process $x $y
                if ($null -eq $hit -or $hit.Id -ne $ItemId) { continue }
                $distance = [Math]::Abs($hit.Offset - $Offset)
                if ($distance -eq 0) { return $hit }
                if ($null -eq $best -or $distance -lt $best.Distance) {
                    $best = [pscustomobject]@{ Hit = $hit; Distance = $distance }
                }
            }
            if ($null -ne $best) {
                $minX = [Math]::Max($left, $best.Hit.X - 8)
                $maxX = [Math]::Min($right, $best.Hit.X + 8)
                for ($x = $minX; $x -le $maxX; $x++) {
                    $hit = Move-Hit $Process $x $y
                    if ($null -ne $hit -and $hit.Id -eq $ItemId -and $hit.Offset -eq $Offset) { return $hit }
                }
            }
        }

        # A drag may encounter a partially visible wrapped row. Wheel farther
        # while the actual left button remains down, then hit-test its newly
        # exposed lines; an idle search never scrolls past an endpoint.
        $dragStatus = Get-Title $Process
        if ($dragStatus.Contains(';drag=true;') -and $scrollRound -lt 23) {
            Scroll-Window $Process -120 1
            continue
        }
        break
    }
    if ($lastOffsets.Count -eq 0) {
        throw "Native hit testing did not expose $ItemId in the current/downward reader view: $((Get-Title $Process))"
    }
    throw "No real mouse coordinate mapped to $ItemId@$Offset; native visible hits: $($lastOffsets -join ','); status=$((Get-Title $Process))"
}

function Wait-Selection([Diagnostics.Process]$Process, [string]$Expected, [bool]$Dragging) {
    $needle = ";selection=$Expected;drag=$($Dragging.ToString().ToLowerInvariant())"
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        $title = Get-Title $Process
        if ($title.Contains($needle)) { return $title }
        Start-Sleep -Milliseconds 5
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Expected '$needle', got '$((Get-Title $Process))'"
}

function Set-MouseButton([Diagnostics.Process]$Process, [bool]$Down) {
    if ($Down) {
        Assert-InputTarget $Process
        if ($script:heldMouseButton) { throw 'The selection driver already holds the left mouse button' }
        $script:heldMouseButton = $true
        $script:inputDispatches.Add('mouse-down')
        [IcedSelectionNative]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
    } elseif ($script:heldMouseButton) {
        Assert-InputTarget $Process
        [IcedSelectionNative]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
        $script:inputDispatches.Add('mouse-up')
        $script:heldMouseButton = $false
    }
}

function Release-HeldInput {
    if ($script:heldMouseButton) {
        # Cleanup can follow focus loss, so do not move the global pointer or
        # synthesize a fresh press. Send only button-up to release our hold.
        try {
            [IcedSelectionNative]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
            $script:inputDispatches.Add('mouse-up-cleanup')
        } catch { }
        finally { $script:heldMouseButton = $false }
    }
    $keys = @($script:heldKeys.Keys)
    $modifiers = @(0x10, 0x11, 0x12)
    $orderedKeys = @($keys | Where-Object { $modifiers -notcontains [int]$_ }) + @($keys | Where-Object { $modifiers -contains [int]$_ })
    foreach ($key in $orderedKeys) {
        try {
            [IcedSelectionNative]::keybd_event([byte]$key, 0, 2, [UIntPtr]::Zero)
            $script:inputDispatches.Add("key-up:$key")
        } catch { }
        finally { $script:heldKeys.Remove([int]$key) }
    }
}

function Press-Key([Diagnostics.Process]$Process, [byte]$VirtualKey) {
    Assert-InputTarget $Process
    $key = [int]$VirtualKey
    if ($script:heldKeys.ContainsKey($key)) { throw "Virtual key $key is already held by the driver" }
    $script:heldKeys[$key] = $Process
    $script:inputDispatches.Add("key-down:$key")
    [IcedSelectionNative]::keybd_event($VirtualKey, 0, 0, [UIntPtr]::Zero)
}

function Toggle-ReaderWidth([Diagnostics.Process]$Process, [int]$ExpectedWidth) {
    $rect = New-Object IcedSelectionNative+Rect
    if (-not [IcedSelectionNative]::GetClientRect($Process.MainWindowHandle, [ref]$rect)) { throw 'GetClientRect failed for width toggle' }
    $xCandidates = @(($rect.Right - 35), ($rect.Right - 70), ($rect.Right - 105), ($rect.Right - 140), ($rect.Right - 175), ($rect.Right - 210), ($rect.Right - 245), ($rect.Right - 280))
    foreach ($y in @(105, 115, 125)) {
        foreach ($x in $xCandidates) {
            if ($x -lt 100) { continue }
            Move-OwnedCursor $Process $x $y
            try {
                Start-Sleep -Milliseconds 60
                Set-MouseButton $Process $true
                Start-Sleep -Milliseconds 35
            } finally { Release-HeldInput }
            Start-Sleep -Milliseconds 80
            if ((Get-Title $Process).Contains("reader=ready;width=$ExpectedWidth;")) { return }
        }
    }
    throw "Native width-control clicks did not switch to ${ExpectedWidth} DIP: $((Get-Title $Process))"
}

function Send-Key([Diagnostics.Process]$Process, [byte]$VirtualKey, [switch]$Control, [switch]$InjectFailureAfterControlDown) {
    try {
        if ($Control) {
            Press-Key $Process 0x11
            if ($InjectFailureAfterControlDown) { throw 'Injected failure after modifier press' }
        }
        Press-Key $Process $VirtualKey
        Start-Sleep -Milliseconds 40
    } finally { Release-HeldInput }
    Start-Sleep -Milliseconds 100
}

function Normalize-ClipboardTransport([string]$Text) { return $Text.Replace("`r`n", "`n") }

function Test-ClipboardTransportNormalization {
    $probe = "one`r`ntwo`rthree`nfour"
    $expected = "one`ntwo`rthree`nfour"
    if ((Normalize-ClipboardTransport $probe) -cne $expected) {
        throw 'Clipboard transport normalization must replace CRLF only and preserve lone CR characters'
    }
}

function Read-ClipboardText([Diagnostics.Process]$Process) {
    Assert-InputTarget $Process
    for ($attempt = 0; $attempt -lt 8; $attempt++) {
        try { return [System.Windows.Forms.Clipboard]::GetText() }
        catch { Start-Sleep -Milliseconds 25; Assert-InputTarget $Process }
    }
    throw 'Could not read the actual Windows text clipboard'
}

function Write-ClipboardText([Diagnostics.Process]$Process, [string]$Text) {
    Assert-InputTarget $Process
    for ($attempt = 0; $attempt -lt 8; $attempt++) {
        try { [System.Windows.Forms.Clipboard]::SetText($Text); return }
        catch { Start-Sleep -Milliseconds 25; Assert-InputTarget $Process }
    }
    throw 'Could not set the Windows clipboard sentinel'
}

function Assert-Golden([Diagnostics.Process]$Process, [string]$Name, [string]$ExpectedSelection, [string]$AnchorLabel, [string]$FocusLabel, [string]$ScreenshotName) {
    $title = Wait-Selection $Process $ExpectedSelection $false
    if ($title -match 'peak=(\d+)/(\d+)') {
        if ([int]$Matches[1] -ge 128 -or [int]$Matches[2] -ge 128) { throw "Active rows exceeded the generous 128-row guard: $title" }
    }
    if ($ScreenshotName) { Capture-Client $Process $ScreenshotName }
    Write-ClipboardText $Process '__ICED_SELECTION_SENTINEL__'
    Send-Key $Process 0x43 -Control
    $actual = Normalize-ClipboardTransport (Read-ClipboardText $Process)
    $expectedBytes = [IO.File]::ReadAllBytes((Join-Path $GoldenRoot ($Name + '.txt')))
    $expected = [Text.UTF8Encoding]::new($false, $true).GetString($expectedBytes)
    if ($expected.Contains("`r")) { throw "Fixture copy golden $Name is not LF-only" }
    if ($actual.Contains("`r")) { throw "Clipboard copy for $Name contains a bare CR not attributable to CRLF transport" }
    if (-not [string]::Equals($actual, $expected, [StringComparison]::Ordinal)) {
        throw "Clipboard mismatch for $Name`nexpected=[$expected]`nactual=[$actual]"
    }
    $script:records.Add(("case={0};anchor={1};focus={2};selection={3};clipboard_bytes={4};golden_sha256={5};screenshot={6}" -f
        $Name, $AnchorLabel, $FocusLabel, $ExpectedSelection, [Text.Encoding]::UTF8.GetByteCount($actual),
        (Get-FileHash -LiteralPath (Join-Path $GoldenRoot ($Name + '.txt')) -Algorithm SHA256).Hash.ToLowerInvariant(), $ScreenshotName))
    Write-Host ("clipboard golden PASS: {0} ({1} bytes)" -f $Name, [Text.Encoding]::UTF8.GetByteCount($actual))
}

function Capture-Client([Diagnostics.Process]$Process, [string]$Name) {
    $Process.Refresh()
    $handle = $Process.MainWindowHandle
    [uint32]$ownerId = 0
    $foreground = [IcedSelectionNative]::GetForegroundWindow()
    if ($handle -eq [IntPtr]::Zero -or -not [IcedSelectionNative]::IsWindowVisible($handle) -or [IcedSelectionNative]::IsIconic($handle) -or $foreground -ne $handle -or
        [IcedSelectionNative]::GetWindowThreadProcessId($foreground, [ref]$ownerId) -eq 0 -or $ownerId -ne [uint32]$Process.Id) {
        throw "Capture target is not visible foreground window owned by process $($Process.Id)"
    }
    $rect = New-Object IcedSelectionNative+Rect
    if (-not [IcedSelectionNative]::GetClientRect($handle, [ref]$rect)) { throw 'GetClientRect failed' }
    $origin = Get-ClientScreenPoint $Process 0 0
    $scale = [IcedSelectionNative]::GetDpiForWindow($handle) / 96.0
    $bitmap = New-Object System.Drawing.Bitmap([int][Math]::Round($rect.Right * $scale), [int][Math]::Round($rect.Bottom * $scale))
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        Start-Sleep -Milliseconds 200
        $graphics.CopyFromScreen([int][Math]::Round($origin.X * $scale), [int][Math]::Round($origin.Y * $scale), 0, 0, $bitmap.Size, [System.Drawing.CopyPixelOperation]::SourceCopy)
        $afterForeground = [IcedSelectionNative]::GetForegroundWindow()
        [uint32]$afterOwner = 0
        if ($afterForeground -ne $handle -or
            [IcedSelectionNative]::GetWindowThreadProcessId($afterForeground, [ref]$afterOwner) -eq 0 -or
            $afterOwner -ne [uint32]$Process.Id) {
            throw "Foreground ownership changed while capturing $Name"
        }
        $bitmap.Save((Join-Path $EvidenceDirectory $Name), [System.Drawing.Imaging.ImageFormat]::Png)
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
}

function Select-Case([Diagnostics.Process]$Process, [string]$Name, [int]$Width, [string]$AnchorId, [int]$AnchorOffset, [string]$FocusId, [int]$FocusOffset, [string]$ScreenshotName, [switch]$ScrollWhileDragging, [switch]$InjectFailureAfterPress) {
    try {
    Restore-Top $Process
    $anchor = Find-Endpoint $Process $AnchorId $AnchorOffset $Width
    if ($ScrollWhileDragging) {
        # The anchor is an actual native hit. Keep the left button held while
        # the reader scrolls; locate the focus only with subsequent OS pointer
        # movements, allowing the original row to be evicted before release.
        Move-OwnedCursor $Process $anchor.X $anchor.Y
        $primedAnchor = Read-Hit $Process $anchor.X $anchor.Y
        if ($null -eq $primedAnchor -or $primedAnchor.Id -ne $AnchorId -or $primedAnchor.Offset -ne $AnchorOffset) {
            throw "Anchor hover did not settle before press: expected $AnchorId@$AnchorOffset at $($anchor.X),$($anchor.Y), got $primedAnchor"
        }
        Set-MouseButton $Process $true
        if ($InjectFailureAfterPress) { throw 'Injected failure after native mouse press' }
        Start-Sleep -Milliseconds 80
        $startTitle = Wait-Selection $Process "$AnchorId@$AnchorOffset->$AnchorId@$AnchorOffset" $true
        $focus = Find-Endpoint $Process $FocusId $FocusOffset $Width
        if ($Name -eq 'across-viewport') {
            for ($refine = 0; $refine -lt 8; $refine++) {
                $live = Get-Title $Process
                if ($live -match 'range=(\d+)\.\.' -and [int]$Matches[1] -gt 1) { break }
                Scroll-Window $Process -120 1
                $focus = Find-Endpoint $Process $FocusId $FocusOffset $Width
            }
        }
    } else {
        $focus = Find-Endpoint $Process $FocusId $FocusOffset $Width
        Move-OwnedCursor $Process $anchor.X $anchor.Y
        $primedAnchor = Read-Hit $Process $anchor.X $anchor.Y
        if ($null -eq $primedAnchor -or $primedAnchor.Id -ne $AnchorId -or $primedAnchor.Offset -ne $AnchorOffset) {
            throw "Anchor hover did not settle before press: expected $AnchorId@$AnchorOffset at $($anchor.X),$($anchor.Y), got $primedAnchor"
        }
        Set-MouseButton $Process $true
        if ($InjectFailureAfterPress) { throw 'Injected failure after native mouse press' }
        Start-Sleep -Milliseconds 70
        $startTitle = Wait-Selection $Process "$AnchorId@$AnchorOffset->$AnchorId@$AnchorOffset" $true
        Move-OwnedCursor $Process $focus.X $focus.Y
        $primedFocus = Read-Hit $Process $focus.X $focus.Y
        if ($null -eq $primedFocus -or $primedFocus.Id -ne $FocusId -or $primedFocus.Offset -ne $FocusOffset) {
            throw "Focus hover did not settle while dragging: expected $FocusId@$FocusOffset at $($focus.X),$($focus.Y), got $primedFocus"
        }
    }
    if ($Name -eq 'across-viewport') {
        $live = Get-Title $Process
        if ($live -notmatch 'range=(\d+)\.\.'){ throw "Active virtual range is missing during drag: $live" }
        $activeFirst = [int]$Matches[1]
        if ($activeFirst -le 1) { throw "Anchor row was not demonstrably evicted during drag: $live" }
    }
    $expectedSelection = "$AnchorId@$AnchorOffset->$FocusId@$FocusOffset"
    $duringDrag = Wait-Selection $Process $expectedSelection $true
    Set-MouseButton $Process $false
    Start-Sleep -Milliseconds 100
    $completed = Wait-Selection $Process $expectedSelection $false
    $script:records.Add(("gesture={0};width={1};anchor_point={2},{3};focus_point={4},{5};during_drag={6}" -f
        $Name, $Width, $anchor.X, $anchor.Y, $focus.X, $focus.Y, $duringDrag))
    Assert-Golden $Process $Name $expectedSelection "$AnchorId@$AnchorOffset" "$FocusId@$FocusOffset" $ScreenshotName
    } finally { Release-HeldInput }
}

function Assert-PhysicalInputReleased {
    if ($script:heldMouseButton -or $script:heldKeys.Count -ne 0) { throw 'Driver still tracks a held input after cleanup' }
    foreach ($virtualKey in @(0x01, 0x11, 0x43)) {
        $state = [IcedSelectionNative]::GetAsyncKeyState($virtualKey)
        if (($state -band 0x8000) -ne 0) { throw "OS still reports injected mouse/key state for virtual key $virtualKey" }
    }
}

function Expect-InputRefusal([scriptblock]$Action, [string]$Label) {
    try {
        & $Action
        throw "Input guard accepted non-foreground target: $Label"
    } catch {
        if ($_.Exception.Message -notlike 'Refusing global input:*') { throw }
    }
}

function Test-InputTargetGuards([Diagnostics.Process]$ForegroundProcess, [Diagnostics.Process]$BackgroundProcess) {
    Assert-InputTarget $ForegroundProcess
    $beforeDispatches = $script:inputDispatches.Count
    $beforeMouseHeld = $script:heldMouseButton
    $beforeKeyCount = $script:heldKeys.Count
    Expect-InputRefusal { Move-OwnedCursor $BackgroundProcess 500 430 } 'cursor'
    Expect-InputRefusal { Set-MouseButton $BackgroundProcess $true } 'mouse-down'
    Expect-InputRefusal { Send-Wheel $BackgroundProcess 500 430 -120 } 'wheel'
    Expect-InputRefusal { Press-Key $BackgroundProcess 0x43 } 'key-down'
    if ($script:inputDispatches.Count -ne $beforeDispatches -or $script:heldMouseButton -ne $beforeMouseHeld -or $script:heldKeys.Count -ne $beforeKeyCount) {
        throw 'A refused background-target operation dispatched input or changed held-input bookkeeping'
    }
    Assert-InputTarget $ForegroundProcess
    Assert-PhysicalInputReleased
    $script:records.Add('safety_test=nonforeground-cursor-mouse-wheel-key-rejected;no_input_dispatched=true')
}

function Test-ExceptionInputCleanup([Diagnostics.Process]$Process, [int]$Width) {
    $beforeMouse = $script:inputDispatches.Count
    $mouseFailedAsExpected = $false
    try { Select-Case $Process 'cleanup-mouse-injection' $Width 'p-00009' 22 'p-00009' 56 '' -InjectFailureAfterPress }
    catch { $mouseFailedAsExpected = $_.Exception.Message.Contains('Injected failure after native mouse press') }
    if (-not $mouseFailedAsExpected) { throw 'Native mouse-press cleanup injection did not fail at the intended point' }
    Assert-PhysicalInputReleased
    $mouseEvents = @($script:inputDispatches | Select-Object -Skip $beforeMouse)
    if ($mouseEvents -notcontains 'mouse-down' -or $mouseEvents -notcontains 'mouse-up-cleanup') {
        throw "Mouse exception test did not record a balanced press/release: $($mouseEvents -join ',')"
    }

    $beforeKeys = $script:inputDispatches.Count
    $keyFailedAsExpected = $false
    try { Send-Key $Process 0x43 -Control -InjectFailureAfterControlDown }
    catch { $keyFailedAsExpected = $_.Exception.Message.Contains('Injected failure after modifier press') }
    if (-not $keyFailedAsExpected) { throw 'Modifier cleanup injection did not fail at the intended point' }
    Assert-PhysicalInputReleased
    $keyEvents = @($script:inputDispatches | Select-Object -Skip $beforeKeys)
    if ($keyEvents -notcontains 'key-down:17' -or $keyEvents -notcontains 'key-up:17' -or $keyEvents -contains 'key-down:67') {
        throw "Modifier exception test did not release Ctrl without pressing C: $($keyEvents -join ',')"
    }
    $script:records.Add('safety_test=exception-after-mouse-down-and-ctrl-down;button_and_modifier_released=true')
}

try {
    Write-Warning 'This opt-in native driver overwrites the Windows text clipboard and does not restore its prior contents.'
    @(
        "os=$([Environment]::OSVersion.VersionString)"
        "powershell=$($PSVersionTable.PSVersion)"
        "apartment=$([Threading.Thread]::CurrentThread.GetApartmentState())"
        "fixture_revision=reader-workload-fx-3"
        'renderer=Iced 0.14.0 / tiny-skia (CPU)'
        "fixture_manifest_sha256=$manifestHash"
        "exe=$ExePath"
        "release_sha256=$releaseHash"
        "release_bytes=$((Get-Item -LiteralPath $ExePath).Length)"
        "cargo_lock_sha256=$lockHash"
        "patch_shape_sha256=$patchHash"
        'input_provenance=cursor movement, wheel, button/key presses and normal mouse release are foreground HWND/PID/visibility guarded; failure cleanup injects only release-only mouse/key-up (no cursor move or new press); no fixture endpoint is installed into app state'
        'clipboard_provenance=System.Windows.Forms.Clipboard.GetText from the Windows OS clipboard; actual CRLF transport only normalized to LF; bare CR rejected; strict UTF-8 fixture goldens compared unchanged; driver warns that clipboard contents are not restored'
    ) | Set-Content -LiteralPath (Join-Path $EvidenceDirectory 'run-context.txt') -Encoding UTF8
    Test-ClipboardTransportNormalization
    $script:records.Add('clipboard_transport_test=CRLF_only;bare_CR_preserved=true')

    $default = Start-Shell $env:TEMP @('--shell-poc')
    Wait-Title $default 'panel=hidden;focus=info'
    Write-ClipboardText $default '__ICED_SELECTION_SENTINEL__'
    Send-Key $default 0x43 -Control
    if ((Read-ClipboardText $default) -cne '__ICED_SELECTION_SENTINEL__') { throw 'Diagnostic shell Ctrl+C changed the OS clipboard sentinel' }
    $script:records.Add('negative_control=shell-poc-ctrl-c;clipboard_sentinel_unchanged=true')
    $reader = Start-Shell $RepositoryRoot @('--reader-poc')
    Wait-Title $reader 'reader=ready;width=800;body=1000;items=1051;error=none'
    $dpi = [IcedSelectionNative]::GetDpiForWindow($reader.MainWindowHandle)
    "small_dpi=$dpi" | Add-Content -LiteralPath (Join-Path $EvidenceDirectory 'run-context.txt')
    $idleBefore = Get-Title $reader
    Start-Sleep -Milliseconds 700
    $idleAfter = Get-Title $reader
    if ($idleBefore -cne $idleAfter) { throw "Opt-in reader status changed while idle: before=[$idleBefore] after=[$idleAfter]" }
    Test-InputTargetGuards $reader $default
    [void]$default.CloseMainWindow()
    if (-not $default.WaitForExit(6000) -or $default.ExitCode -ne 0) { throw 'Shell diagnostic did not close cleanly' }
    Assert-InputTarget $reader

    Test-ExceptionInputCleanup $reader 800
    Send-Key $reader 0x74
    Wait-Title $reader 'reader=closed;width=800' | Out-Null
    Send-Key $reader 0x74
    Wait-Title $reader 'reader=ready;width=800;body=1000;items=1051;error=none' 90 | Out-Null
    Wait-Selection $reader 'none' $false | Out-Null

    Write-ClipboardText $reader '__ICED_SELECTION_SENTINEL__'
    Send-Key $reader 0x43 -Control
    if ((Read-ClipboardText $reader) -cne '__ICED_SELECTION_SENTINEL__') { throw 'Reader Ctrl+C without selection changed the clipboard sentinel' }
    $script:records.Add('negative_control=reader-no-selection-ctrl-c;clipboard_sentinel_unchanged=true')

    # Switch to the shared 480-DIP width using the actual reader control.
    Toggle-ReaderWidth $reader 480
    Wait-Title $reader 'reader=ready;width=480;body=1000;items=1051;error=none'

    # Collapsed/click-only range is a native input, and must preserve the OS
    # clipboard rather than writing the empty reference string.
    Restore-Top $reader
    $collapsed = Find-Endpoint $reader 'p-00010' 217 480
    Move-OwnedCursor $reader $collapsed.X $collapsed.Y
    try {
        Set-MouseButton $reader $true
        Start-Sleep -Milliseconds 70
        Wait-Selection $reader 'p-00010@217->p-00010@217' $true | Out-Null
        Set-MouseButton $reader $false
    } finally { Release-HeldInput }
    Wait-Selection $reader 'p-00010@217->p-00010@217' $false | Out-Null
    Write-ClipboardText $reader '__ICED_SELECTION_SENTINEL__'
    Send-Key $reader 0x43 -Control
    if ((Read-ClipboardText $reader) -cne '__ICED_SELECTION_SENTINEL__') { throw 'Collapsed reader selection overwrote the clipboard sentinel' }
    $script:records.Add('negative_control=collapsed-native-click-ctrl-c;endpoint=p-00010@217;clipboard_sentinel_unchanged=true')

    # Wrapped/styled text, RTL order, paragraph separation, and the image plus
    # heading between p-00008 and p-00009 are exercised with real pointer input.
    Select-Case $reader 'rtl-mixed' 480 'p-00002' 0 'p-00002' 150 'selection-rtl-mixed.png'
    Select-Case $reader 'cross-paragraph-partial' 480 'p-00005' 32 'p-00006' 77 'selection-cross-paragraph.png'
    Select-Case $reader 'reversed' 480 'p-00006' 152 'p-00005' 0 'selection-reversed.png'
    Select-Case $reader 'cross-image-heading' 480 'p-00008' 0 'p-00009' 85 'selection-image-heading.png' -ScrollWhileDragging
    Select-Case $reader 'style-boundary' 480 'p-00009' 22 'p-00009' 56 'selection-styled.png'

    $styleSelection = 'p-00009@22->p-00009@56'
    Toggle-ReaderWidth $reader 800
    $wideTitle = Wait-Title $reader 'reader=ready;width=800;body=1000;items=1051;error=none'
    if ($wideTitle -notmatch 'marks=.*p-00009@22-56') { throw "Selection marks did not survive 480-to-800 width toggle: $wideTitle" }
    Assert-Golden $reader 'style-boundary' $styleSelection 'p-00009@22' 'p-00009@56' 'selection-width-toggle.png'

    $originalWindowRect = New-Object IcedSelectionNative+Rect
    if (-not [IcedSelectionNative]::GetWindowRect($reader.MainWindowHandle, [ref]$originalWindowRect)) {
        throw 'Could not record original reader window bounds'
    }
    $restoreWidth = $originalWindowRect.Right - $originalWindowRect.Left
    $restoreHeight = $originalWindowRect.Bottom - $originalWindowRect.Top
    if (-not [IcedSelectionNative]::SetWindowPos($reader.MainWindowHandle, [IntPtr]::Zero, 0, 0, 900, 820, 0x0016)) {
        throw 'Resize during persistent selection failed'
    }
    Start-Sleep -Milliseconds 350
    $resizedTitle = Wait-Title $reader 'reader=ready;width=800;body=1000;items=1051;error=none'
    if ($resizedTitle -notmatch 'marks=.*p-00009@22-56') { throw "Selection marks did not survive window resize: $resizedTitle" }
    Assert-Golden $reader 'style-boundary' $styleSelection 'p-00009@22' 'p-00009@56' 'selection-resized.png'
    if (-not [IcedSelectionNative]::SetWindowPos($reader.MainWindowHandle, [IntPtr]::Zero, 0, 0, $restoreWidth, $restoreHeight, 0x0016)) {
        throw 'Restore reader window size failed'
    }
    Start-Sleep -Milliseconds 250
    Toggle-ReaderWidth $reader 480
    Wait-Title $reader 'reader=ready;width=480;body=1000;items=1051;error=none' | Out-Null
    Wait-Selection $reader $styleSelection $false | Out-Null
    Restore-Top $reader

    Select-Case $reader 'long-within-paragraph' 480 'p-00010' 99 'p-00010' 392 'selection-wrapped.png'
    Select-Case $reader 'across-viewport' 480 'p-00001' 0 'p-00012' 67 'selection-across-viewport.png' -ScrollWhileDragging
    Restore-Top $reader
    $returnedAnchor = Find-Endpoint $reader 'p-00001' 0 480
    $returnedTitle = Wait-Selection $reader 'p-00001@0->p-00012@67' $false
    if ($returnedTitle -notmatch 'marks=[^;]*p-00001@0-') {
        throw "Original p-00001 selection highlight did not reappear after row re-entry: $returnedTitle"
    }
    $script:records.Add(("reentry=native-return-to-anchor;hit={0}@{1};selection={2};marks={3}" -f
        $returnedAnchor.Id, $returnedAnchor.Offset, 'p-00001@0->p-00012@67', ($returnedTitle -replace '^.*;marks=', '')))
    Assert-Golden $reader 'across-viewport' 'p-00001@0->p-00012@67' 'p-00001@0' 'p-00012@67' 'selection-across-viewport-anchor-return.png'

    Select-Case $reader 'style-boundary' 480 'p-00009' 22 'p-00009' 56 'selection-before-reload.png'
    Send-Key $reader 0x74
    $closedTitle = Wait-Title $reader 'reader=closed;width=480'
    if ($closedTitle -notmatch 'selection=none;drag=false') { throw "F5 close retained selection or drag state: $closedTitle" }
    Send-Key $reader 0x74
    Wait-Title $reader 'reader=ready;width=480;body=1000;items=1051;error=none' 90 | Out-Null
    Wait-Selection $reader 'none' $false | Out-Null
    Write-ClipboardText $reader '__ICED_SELECTION_SENTINEL__'
    Send-Key $reader 0x43 -Control
    if ((Read-ClipboardText $reader) -cne '__ICED_SELECTION_SENTINEL__') { throw 'F5 reload restored stale clipboard selection' }
    $script:records.Add('lifecycle=f5-close-reopen;selection_cleared=true;drag_stopped=true;clipboard_sentinel_unchanged=true')

    # A later native selection in the 10k mode proves the common curated prefix
    # still maps to the same independent golden while active row counts remain
    # bounded. It does not characterize performance or hidden framework caches.
    [void]$reader.CloseMainWindow()
    if (-not $reader.WaitForExit(6000) -or $reader.ExitCode -ne 0) { throw 'Small reader did not close cleanly' }
    $large = Start-Shell $RepositoryRoot @('--reader-poc-large')
    Wait-Title $large 'reader=ready;width=800;body=10000;items=10501;error=none' 120
    "large_dpi=$([IcedSelectionNative]::GetDpiForWindow($large.MainWindowHandle))" | Add-Content -LiteralPath (Join-Path $EvidenceDirectory 'run-context.txt')
    Select-Case $large 'style-boundary' 800 'p-00009' 22 'p-00009' 56 'selection-large-style-boundary.png'
    [void]$large.CloseMainWindow()
    if (-not $large.WaitForExit(6000) -or $large.ExitCode -ne 0) { throw 'Large reader did not close cleanly' }

    if ((Get-FileHash -LiteralPath $ExePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $releaseHash) {
        throw 'Release executable changed during selection evidence run'
    }
    $script:records | Set-Content -LiteralPath (Join-Path $EvidenceDirectory 'selection-cases.txt') -Encoding UTF8
    Write-Host 'selection-copy: interactive mouse/scroll/clipboard checks passed; inspect the captured highlight pixels'
    Write-Output $EvidenceDirectory
} finally {
    Release-HeldInput
    foreach ($name in $statusNames) {
        if ($null -eq $oldStatus[$name]) { Remove-Item "Env:$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name, $oldStatus[$name], 'Process') }
    }
    foreach ($process in $script:owned) {
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
    }
}
