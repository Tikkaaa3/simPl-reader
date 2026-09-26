param(
    [string]$ExePath = (Join-Path $PSScriptRoot '..\..\..\target\release\iced-shell.exe')
)

$ErrorActionPreference = 'Stop'
$ExePath = [IO.Path]::GetFullPath($ExePath)
if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) {
    throw "Release executable not found: $ExePath"
}
if (-not [Environment]::UserInteractive) {
    throw 'runtime-evidence.ps1 requires an interactive Windows desktop'
}

function Assert-ZeroExit([int]$ExitCode) {
    if ($ExitCode -ne 0) { throw "Diagnostic shell exited $ExitCode, expected zero" }
}

$selectionOverrides = Get-ChildItem Env: | Where-Object { $_.Name -match '^(ICED_|WINIT_)' }
if ($selectionOverrides.Count -ne 0) {
    throw "Inherited Iced/winit override(s) are not allowed: $(($selectionOverrides.Name | Sort-Object) -join ',')"
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class IcedShellEvidenceNative {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hwnd, uint message, UIntPtr wParam, IntPtr lParam);
}
"@

function Assert-Color([Drawing.Color]$Actual, [int]$Red, [int]$Green, [int]$Blue, [string]$Name) {
    $distance = [Math]::Abs($Actual.R - $Red) + [Math]::Abs($Actual.G - $Green) + [Math]::Abs($Actual.B - $Blue)
    if ($distance -gt 9) {
        throw "$Name pixel was #$($Actual.R.ToString('x2'))$($Actual.G.ToString('x2'))$($Actual.B.ToString('x2')), expected near #$($Red.ToString('x2'))$($Green.ToString('x2'))$($Blue.ToString('x2'))"
    }
}

$process = $null
try {
    $startInfo = New-Object Diagnostics.ProcessStartInfo
    $startInfo.FileName = $ExePath
    $startInfo.WorkingDirectory = $env:TEMP
    $startInfo.Arguments = '--shell-poc'
    $startInfo.UseShellExecute = $false
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $startInfo
    if (-not $process.Start()) { throw 'Could not start Iced shell' }
    if (-not $process.WaitForInputIdle(15000)) { throw 'Iced shell did not become input-idle' }
    Start-Sleep -Milliseconds 750
    $process.Refresh()
    if ($process.MainWindowHandle -eq 0) { throw 'Iced shell has no top-level window' }
    'renderer=Iced 0.14.0 / tiny-skia (CPU); mode=--shell-poc'

    $virtualRect = New-Object IcedShellEvidenceNative+Rect
    if (-not [IcedShellEvidenceNative]::GetClientRect($process.MainWindowHandle, [ref]$virtualRect)) {
        throw 'virtualized GetClientRect failed'
    }
    $previousDpiContext = [IcedShellEvidenceNative]::SetThreadDpiAwarenessContext([IntPtr](-4))
    if ($previousDpiContext -eq [IntPtr]::Zero) { throw 'SetThreadDpiAwarenessContext failed' }
    try {
        $physicalRect = New-Object IcedShellEvidenceNative+Rect
        if (-not [IcedShellEvidenceNative]::GetClientRect($process.MainWindowHandle, [ref]$physicalRect)) {
            throw 'DPI-aware GetClientRect failed'
        }
        $origin = New-Object IcedShellEvidenceNative+Point
        if (-not [IcedShellEvidenceNative]::ClientToScreen($process.MainWindowHandle, [ref]$origin)) {
            throw 'DPI-aware ClientToScreen failed'
        }
    } finally {
        [void][IcedShellEvidenceNative]::SetThreadDpiAwarenessContext($previousDpiContext)
    }
    $physicalWidth = $physicalRect.Right - $physicalRect.Left
    $physicalHeight = $physicalRect.Bottom - $physicalRect.Top
    $dpi = [IcedShellEvidenceNative]::GetDpiForWindow($process.MainWindowHandle)
    "window: title=$($process.MainWindowTitle) dpi=$dpi virtualized-client=$($virtualRect.Right - $virtualRect.Left)x$($virtualRect.Bottom - $virtualRect.Top) physical-client=${physicalWidth}x${physicalHeight}"

    $bitmap = New-Object Drawing.Bitmap $physicalWidth, $physicalHeight
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($origin.X, $origin.Y, 0, 0, $bitmap.Size)
        Assert-Color $bitmap.GetPixel(600, 30) 0x1a 0x20 0x29 'toolbar'
        Assert-Color $bitmap.GetPixel(1000, 700) 0x12 0x16 0x1d 'body'
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
    'render-check: toolbar and body palette pixels matched the known shell'

    $previousDpiContext = [IcedShellEvidenceNative]::SetThreadDpiAwarenessContext([IntPtr](-4))
    try {
        $windowRect = New-Object IcedShellEvidenceNative+Rect
        if (-not [IcedShellEvidenceNative]::GetWindowRect($process.MainWindowHandle, [ref]$windowRect)) {
            throw 'DPI-aware GetWindowRect failed'
        }
        [void][IcedShellEvidenceNative]::SetForegroundWindow($process.MainWindowHandle)
        [void][IcedShellEvidenceNative]::SetCursorPos($windowRect.Right - 2, $windowRect.Bottom - 2)
        Start-Sleep -Milliseconds 150
        [IcedShellEvidenceNative]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 100
        [void][IcedShellEvidenceNative]::SetCursorPos($windowRect.Left + 100, $windowRect.Top + 100)
        Start-Sleep -Milliseconds 300
        [IcedShellEvidenceNative]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 300

        $minimumRect = New-Object IcedShellEvidenceNative+Rect
        if (-not [IcedShellEvidenceNative]::GetClientRect($process.MainWindowHandle, [ref]$minimumRect)) {
            throw 'minimum DPI-aware GetClientRect failed'
        }
    } finally {
        [void][IcedShellEvidenceNative]::SetThreadDpiAwarenessContext($previousDpiContext)
    }
    $minimumWidth = $minimumRect.Right - $minimumRect.Left
    $minimumHeight = $minimumRect.Bottom - $minimumRect.Top
    if ($minimumWidth -ne 800 -or $minimumHeight -ne 600) {
        throw "Physical minimum client was ${minimumWidth}x${minimumHeight}, expected 800x600"
    }
    "minimum-client: physical=${minimumWidth}x${minimumHeight}"

    $root = [Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    if ($null -eq $root) { throw 'UI Automation could not open the top-level window' }
    $descendants = $root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition)
    "uia-root: name=$($root.Current.Name) type=$($root.Current.ControlType.ProgrammaticName) focus=$($root.Current.HasKeyboardFocus) descendants=$($descendants.Count)"
    for ($index = 0; $index -lt $descendants.Count; $index++) {
        $element = $descendants.Item($index)
        $invoke = $false
        try { $invoke = $null -ne $element.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern) } catch {}
        "uia-descendant: name=$($element.Current.Name) type=$($element.Current.ControlType.ProgrammaticName) focus=$($element.Current.HasKeyboardFocus) invoke=$invoke"
    }

    [void][IcedShellEvidenceNative]::SendMessage($process.MainWindowHandle, 0x0010, [UIntPtr]::Zero, [IntPtr]::Zero)
    if (-not $process.WaitForExit(5000)) { throw 'Diagnostic shell did not close within five seconds' }
    Assert-ZeroExit $process.ExitCode
} finally {
    if ($null -ne $process -and -not $process.HasExited) {
        [void][IcedShellEvidenceNative]::SendMessage($process.MainWindowHandle, 0x0010, [UIntPtr]::Zero, [IntPtr]::Zero)
        if (-not $process.WaitForExit(5000)) { Stop-Process -Id $process.Id -Force }
    }
}
