<#
.SYNOPSIS
Drives the simPl window with native Windows input for live verification lanes.

.DESCRIPTION
One invocation runs the requested steps in this fixed order:
Launch, Resize, Move, Hold, Click, Wheel, Key, IdleCpu, Shot, Stop.
Coordinates and sizes are client-area physical pixels. The window has no OS frame.
This moves the real mouse cursor, so run one driver per desktop at a time.

-Launch <document>  Start simPl on a document. Needs -Profile. Records the PID in target\ui-drive.pid.
-Profile <dir>      Directory used as the child's LOCALAPPDATA. Created when missing.
-Exe <path>         Executable to launch. Default target\release\iced-shell.exe.
-Size WxH           Client size right after -Launch.
-Resize WxH         Resize the client area of the running window.
-Move x,y           Move the cursor to a client point.
-Hold <ms>          Keep the cursor still for this long.
-Click              Left click at the cursor.
-Wheel <delta>      Wheel at the cursor. 120 is one notch; negative scrolls down.
-Key <keys>         SendKeys string, for example '^w' or '{F8}'.
-IdleCpu <seconds>  Print idle_cpu_ms_per_10s=<n> from the process CPU time.
-Shot <png>         Capture the client area and print "shot <path> <w>x<h>".
-Stop               Close the window, wait up to 10 s, then kill.

Later invocations drive the recorded PID, or the single running iced-shell.

.EXAMPLE
scripts\ui-drive.ps1 -Launch "target\lane-fixtures\The Quiet Hours.epub" -Profile target\lane-profile-1 -Size 1400x900 -Shot target\ux0\reading.png
#>
param(
    [string]$Launch,
    [string]$Profile,
    [string]$Exe,
    [string]$Size,
    [string]$Resize,
    [string]$Move,
    [int]$Hold = 0,
    [switch]$Click,
    [int]$Wheel = 0,
    [string]$Key,
    [double]$IdleCpu = 0,
    [string]$Shot,
    [switch]$Stop
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$pidFile = Join-Path $repo 'target\ui-drive.pid'
if (-not $Exe) { $Exe = Join-Path $repo 'target\release\iced-shell.exe' }

Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class UiDrive {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr window, out RECT rect);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out RECT rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr window, ref POINT point);
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr window, int x, int y, int width, int height, bool repaint);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(int flags, int x, int y, int data, IntPtr extra);
    [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, int flags, IntPtr extra);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr hdc, int flags);
}
'@
[UiDrive]::SetProcessDPIAware() | Out-Null

function Get-FullPath([string]$Path) {
    $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Path)
}

function ConvertTo-Pair([string]$Text, [string]$Separator, [string]$Name) {
    # PowerShell binds an unquoted 700,500 as an array and joins it with a space.
    if ($Text -notmatch "^(\d+)(?:$Separator|\s+)(\d+)$") { throw "$Name expects two whole numbers separated by '$Separator', got '$Text'." }
    @([int]$Matches[1], [int]$Matches[2])
}

function Get-Recorded {
    if (-not (Test-Path -LiteralPath $pidFile)) { return $null }
    $process = Get-Process -Id ([int](Get-Content -LiteralPath $pidFile)) -ErrorAction SilentlyContinue
    if ($process -and $process.ProcessName -eq 'iced-shell') { return $process }
    $null
}

function Get-Target {
    $recorded = Get-Recorded
    if ($recorded) { return $recorded }
    $running = @(Get-Process -Name 'iced-shell' -ErrorAction SilentlyContinue)
    if ($running.Count -ne 1) { throw "Expected one running iced-shell, found $($running.Count). Use -Launch or stop the others." }
    $running[0]
}

function Get-ClientRect([IntPtr]$Window) {
    $rect = New-Object UiDrive+RECT
    $origin = New-Object UiDrive+POINT
    [UiDrive]::GetClientRect($Window, [ref]$rect) | Out-Null
    [UiDrive]::ClientToScreen($Window, [ref]$origin) | Out-Null
    [pscustomobject]@{ X = $origin.X; Y = $origin.Y; Width = $rect.Right; Height = $rect.Bottom }
}

function Set-Foreground([IntPtr]$Window) {
    if ([UiDrive]::IsIconic($Window)) { [UiDrive]::ShowWindow($Window, 9) | Out-Null }
    [UiDrive]::SetForegroundWindow($Window) | Out-Null
    if ([UiDrive]::GetForegroundWindow() -ne $Window) {
        # Windows only lets the process that saw the last input take the foreground.
        [UiDrive]::keybd_event(0x12, 0, 0, [IntPtr]::Zero)
        [UiDrive]::keybd_event(0x12, 0, 2, [IntPtr]::Zero)
        [UiDrive]::SetForegroundWindow($Window) | Out-Null
    }
    if ([UiDrive]::GetForegroundWindow() -ne $Window) { throw 'Could not bring the simPl window to the foreground.' }
    Start-Sleep -Milliseconds 100
}

function Set-ClientSize([IntPtr]$Window, [string]$Text, [string]$Name) {
    $width, $height = ConvertTo-Pair $Text 'x' $Name
    $outer = New-Object UiDrive+RECT
    [UiDrive]::GetWindowRect($Window, [ref]$outer) | Out-Null
    $client = Get-ClientRect $Window
    $frameX = ($outer.Right - $outer.Left) - $client.Width
    $frameY = ($outer.Bottom - $outer.Top) - $client.Height
    [UiDrive]::MoveWindow($Window, $outer.Left, $outer.Top, $width + $frameX, $height + $frameY, $true) | Out-Null
    $deadline = (Get-Date).AddSeconds(5)
    do {
        Start-Sleep -Milliseconds 100
        $client = Get-ClientRect $Window
    } until (($client.Width -eq $width -and $client.Height -eq $height) -or (Get-Date) -gt $deadline)
    if ($client.Width -ne $width -or $client.Height -ne $height) {
        throw "$Name asked for ${width}x${height}, the client area is $($client.Width)x$($client.Height)."
    }
    Start-Sleep -Milliseconds 400
    "size ${width}x${height}"
}

if ($Launch) {
    if (-not $Profile) { throw '-Launch needs -Profile, so the real library stays untouched.' }
    $previous = Get-Recorded
    if ($previous) { throw "iced-shell $($previous.Id) from an earlier -Launch is still running. Use -Stop first." }
    $document = (Resolve-Path -LiteralPath $Launch).Path
    $profileDir = (New-Item -ItemType Directory -Force -Path (Get-FullPath $Profile)).FullName
    $savedLocalAppData = $env:LOCALAPPDATA
    try {
        $env:LOCALAPPDATA = $profileDir
        $process = Start-Process -FilePath (Get-FullPath $Exe) -ArgumentList ('"' + $document + '"') -WorkingDirectory $repo -PassThru
    } finally {
        $env:LOCALAPPDATA = $savedLocalAppData
    }
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $pidFile) | Out-Null
    Set-Content -LiteralPath $pidFile -Value $process.Id
    # The title gains the book name once the document is open.
    $openTitle = ' ' + [char]0x2014 + ' simPl'
    $deadline = (Get-Date).AddSeconds(30)
    do {
        Start-Sleep -Milliseconds 200
        if ($process.HasExited) { throw "iced-shell exited during launch with code $($process.ExitCode)." }
        $process.Refresh()
    } until (($process.MainWindowHandle -ne 0 -and $process.MainWindowTitle.EndsWith($openTitle)) -or (Get-Date) -gt $deadline)
    if ($process.MainWindowHandle -eq 0) { throw 'No simPl window appeared within 30 s.' }
    if (-not $process.MainWindowTitle.EndsWith($openTitle)) { throw "The document did not open within 30 s; the title is '$($process.MainWindowTitle)'." }
    Start-Sleep -Milliseconds 500
    "launched $($process.Id) '$($process.MainWindowTitle)' profile $profileDir"
    if ($Size) { Set-ClientSize $process.MainWindowHandle $Size '-Size' }
} elseif ($Size) {
    throw '-Size applies at -Launch; use -Resize for a running window.'
}

$needsWindow = $Resize -or $Move -or $Hold -gt 0 -or $Click -or $Wheel -ne 0 -or $Key -or $IdleCpu -gt 0 -or $Shot -or $Stop
if (-not $needsWindow) { return }
$target = Get-Target
$window = $target.MainWindowHandle
if ($window -eq 0) { throw "iced-shell $($target.Id) has no main window." }

if ($Resize -or $Move -or $Click -or $Wheel -ne 0 -or $Key -or $Shot) { Set-Foreground $window }

if ($Resize) { Set-ClientSize $window $Resize '-Resize' }

if ($Move) {
    $x, $y = ConvertTo-Pair $Move ',' '-Move'
    $client = Get-ClientRect $window
    # A move to the cursor's current spot sends no WM_MOUSEMOVE, so step off it first.
    [UiDrive]::SetCursorPos($client.X + $x + 1, $client.Y + $y) | Out-Null
    Start-Sleep -Milliseconds 50
    [UiDrive]::SetCursorPos($client.X + $x, $client.Y + $y) | Out-Null
    Start-Sleep -Milliseconds 250
    "move $x,$y"
}

if ($Hold -gt 0) {
    Start-Sleep -Milliseconds $Hold
    "hold $Hold ms"
}

if ($Click) {
    [UiDrive]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 50
    [UiDrive]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 500
    'click'
}

if ($Wheel -ne 0) {
    [UiDrive]::mouse_event(0x0800, 0, 0, $Wheel, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 500
    "wheel $Wheel"
}

if ($Key) {
    [System.Windows.Forms.SendKeys]::SendWait($Key)
    Start-Sleep -Milliseconds 500
    "key $Key"
}

if ($IdleCpu -gt 0) {
    $target.Refresh()
    $before = $target.TotalProcessorTime
    $clock = [Diagnostics.Stopwatch]::StartNew()
    Start-Sleep -Milliseconds ([int]($IdleCpu * 1000))
    $target.Refresh()
    $used = ($target.TotalProcessorTime - $before).TotalMilliseconds
    "idle_cpu_ms_per_10s=$([math]::Round($used * 10 / $clock.Elapsed.TotalSeconds))"
}

if ($Shot) {
    $path = Get-FullPath $Shot
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
    $client = Get-ClientRect $window
    $bitmap = New-Object System.Drawing.Bitmap $client.Width, $client.Height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $hdc = $graphics.GetHdc()
    try {
        # PW_CLIENTONLY | PW_RENDERFULLCONTENT reads the window itself, so toasts and overlapping windows stay out.
        if (-not [UiDrive]::PrintWindow($window, $hdc, 3)) { throw 'PrintWindow failed.' }
    } finally {
        $graphics.ReleaseHdc($hdc)
        $graphics.Dispose()
    }
    try {
        $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $bitmap.Dispose()
    }
    "shot $path $($client.Width)x$($client.Height)"
}

if ($Stop) {
    $target.CloseMainWindow() | Out-Null
    if ($target.WaitForExit(10000)) {
        "stopped $($target.Id)"
    } else {
        $target.Kill()
        $target.WaitForExit()
        "killed $($target.Id) after 10 s"
    }
    Remove-Item -LiteralPath $pidFile -ErrorAction SilentlyContinue
}
