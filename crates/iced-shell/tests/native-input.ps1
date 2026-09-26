param(
    [string]$ExePath = (Join-Path $PSScriptRoot '..\..\..\target\release\iced-shell.exe')
)

$ErrorActionPreference = 'Stop'
$ExePath = [IO.Path]::GetFullPath($ExePath)
if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) {
    throw "Release executable not found: $ExePath"
}
if (-not [Environment]::UserInteractive) {
    throw 'native-input.ps1 requires an interactive Windows desktop'
}

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class IcedShellNativeInput {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd, int command);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hwnd, uint message, UIntPtr wParam, IntPtr lParam);
}
"@
Add-Type -AssemblyName System.Drawing

$owned = [Collections.Generic.List[Diagnostics.Process]]::new()
$oldDiagnostic = $env:ICED_SHELL_NATIVE_TEST_STATUS
$oldAdapterDiagnostic = $env:ICED_SHELL_ADAPTER_DIAGNOSTICS
Remove-Item Env:ICED_SHELL_ADAPTER_DIAGNOSTICS -ErrorAction SilentlyContinue
$env:ICED_SHELL_NATIVE_TEST_STATUS = '1'

function Wait-Status([Diagnostics.Process]$Process, [string]$Expected) {
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        if ($Process.HasExited) {
            throw "Process exited before status '$Expected' (exit $($Process.ExitCode))"
        }
        $Process.Refresh()
        if ($Process.MainWindowTitle.Contains($Expected)) { return }
        Start-Sleep -Milliseconds 25
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Expected status '$Expected', got '$($Process.MainWindowTitle)'"
}

function Set-OwnedForeground([Diagnostics.Process]$Process) {
    if (-not [IcedShellNativeInput]::SetForegroundWindow($Process.MainWindowHandle)) {
        $activated = (New-Object -ComObject WScript.Shell).AppActivate($Process.Id)
        if (-not $activated) { throw "Could not request foreground for process $($Process.Id)" }
    }
    $deadline = [DateTime]::UtcNow.AddSeconds(3)
    do {
        if ([IcedShellNativeInput]::GetForegroundWindow() -eq $Process.MainWindowHandle) { return }
        Start-Sleep -Milliseconds 25
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Process $($Process.Id) did not become foreground"
}

function Start-Shell {
    $process = Start-Process -FilePath $ExePath -WorkingDirectory $env:TEMP -WindowStyle Normal -PassThru
    $owned.Add($process)
    if (-not $process.WaitForInputIdle(15000)) {
        throw 'Iced shell did not become input-idle within 15 seconds'
    }
    Start-Sleep -Milliseconds 500
    $process.Refresh()
    if ($process.MainWindowHandle -eq 0 -or -not [IcedShellNativeInput]::IsWindowVisible($process.MainWindowHandle)) {
        throw 'Iced shell did not create a visible top-level window'
    }
    Set-OwnedForeground $process
    Wait-Status $process 'panel=hidden;focus=info'
    return $process
}

function Send-Key([byte]$VirtualKey, [switch]$Shift, [switch]$Alt, [switch]$Repeat) {
    if ($Shift) { [IcedShellNativeInput]::keybd_event(0x10, 0, 0, [UIntPtr]::Zero) }
    if ($Alt) { [IcedShellNativeInput]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero) }
    [IcedShellNativeInput]::keybd_event($VirtualKey, 0, 0, [UIntPtr]::Zero)
    if ($Repeat) {
        Start-Sleep -Milliseconds 60
        [IcedShellNativeInput]::keybd_event($VirtualKey, 0, 0, [UIntPtr]::Zero)
    }
    Start-Sleep -Milliseconds 80
    [IcedShellNativeInput]::keybd_event($VirtualKey, 0, 2, [UIntPtr]::Zero)
    if ($Alt) { [IcedShellNativeInput]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero) }
    if ($Shift) { [IcedShellNativeInput]::keybd_event(0x10, 0, 2, [UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 150
}

function Mouse-At([Diagnostics.Process]$Process, [int]$LogicalX, [int]$LogicalY, [ValidateSet('move','down','up','click')] [string]$Action) {
    $point = New-Object IcedShellNativeInput+Point
    $point.X = $LogicalX
    $point.Y = $LogicalY
    if (-not [IcedShellNativeInput]::ClientToScreen($Process.MainWindowHandle, [ref]$point)) {
        throw 'ClientToScreen failed'
    }
    if (-not [IcedShellNativeInput]::SetCursorPos($point.X, $point.Y)) { throw 'SetCursorPos failed' }
    # Let winit observe the native cursor move before the button transition.
    Start-Sleep -Milliseconds 100
    if ($Action -in @('down','click')) { [IcedShellNativeInput]::mouse_event(2,0,0,0,[UIntPtr]::Zero) }
    if ($Action -in @('up','click')) { [IcedShellNativeInput]::mouse_event(4,0,0,0,[UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 150
}

function Physical-Mouse-At([Diagnostics.Process]$Process, [int]$PhysicalX, [int]$PhysicalY, [ValidateSet('down','up')] [string]$Action) {
    $previous = [IcedShellNativeInput]::SetThreadDpiAwarenessContext([IntPtr](-4))
    if ($previous -eq [IntPtr]::Zero) { throw 'SetThreadDpiAwarenessContext failed' }
    try {
        $point = New-Object IcedShellNativeInput+Point
        if (-not [IcedShellNativeInput]::ClientToScreen($Process.MainWindowHandle, [ref]$point)) {
            throw 'DPI-aware ClientToScreen origin lookup failed'
        }
        $point.X += $PhysicalX
        $point.Y += $PhysicalY
    } finally {
        [void][IcedShellNativeInput]::SetThreadDpiAwarenessContext($previous)
    }
    if (-not [IcedShellNativeInput]::SetCursorPos($point.X, $point.Y)) { throw 'SetCursorPos failed' }
    Start-Sleep -Milliseconds 250
    if ($Action -eq 'down') { [IcedShellNativeInput]::mouse_event(2,0,0,0,[UIntPtr]::Zero) }
    if ($Action -eq 'up') { [IcedShellNativeInput]::mouse_event(4,0,0,0,[UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 250
}

function Resize-To-Minimum([Diagnostics.Process]$Process) {
    $previous = [IcedShellNativeInput]::SetThreadDpiAwarenessContext([IntPtr](-4))
    if ($previous -eq [IntPtr]::Zero) { throw 'SetThreadDpiAwarenessContext failed' }
    try {
        $outer = New-Object IcedShellNativeInput+Rect
        if (-not [IcedShellNativeInput]::GetWindowRect($Process.MainWindowHandle, [ref]$outer)) {
            throw 'GetWindowRect failed'
        }
        [void][IcedShellNativeInput]::SetCursorPos($outer.Right - 2, $outer.Bottom - 2)
        Start-Sleep -Milliseconds 150
        [IcedShellNativeInput]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
        Start-Sleep -Milliseconds 100
        [void][IcedShellNativeInput]::SetCursorPos($outer.Left + 100, $outer.Top + 100)
        Start-Sleep -Milliseconds 300
        [IcedShellNativeInput]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
        Start-Sleep -Milliseconds 300

        $client = New-Object IcedShellNativeInput+Rect
        if (-not [IcedShellNativeInput]::GetClientRect($Process.MainWindowHandle, [ref]$client)) {
            throw 'minimum GetClientRect failed'
        }
        $width = $client.Right - $client.Left
        $height = $client.Bottom - $client.Top
        if ($width -ne 800 -or $height -ne 600) {
            throw "Expected 800x600 physical minimum client, got ${width}x${height}"
        }
    } finally {
        [void][IcedShellNativeInput]::SetThreadDpiAwarenessContext($previous)
    }
}

function Assert-NormalPalette([Diagnostics.Process]$Process) {
    $previous = [IcedShellNativeInput]::SetThreadDpiAwarenessContext([IntPtr](-4))
    if ($previous -eq [IntPtr]::Zero) { throw 'SetThreadDpiAwarenessContext failed' }
    try {
        $client = New-Object IcedShellNativeInput+Rect
        $origin = New-Object IcedShellNativeInput+Point
        if (-not [IcedShellNativeInput]::GetClientRect($Process.MainWindowHandle, [ref]$client) -or
            -not [IcedShellNativeInput]::ClientToScreen($Process.MainWindowHandle, [ref]$origin)) {
            throw 'DPI-aware palette geometry lookup failed'
        }
    } finally {
        [void][IcedShellNativeInput]::SetThreadDpiAwarenessContext($previous)
    }
    $bitmap = New-Object Drawing.Bitmap ($client.Right - $client.Left), ($client.Bottom - $client.Top)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($origin.X, $origin.Y, 0, 0, $bitmap.Size)
        $toolbar = $bitmap.GetPixel(600, 30)
        $body = $bitmap.GetPixel(1000, 700)
        if ([Math]::Abs($toolbar.R - 0x1a) + [Math]::Abs($toolbar.G - 0x20) + [Math]::Abs($toolbar.B - 0x29) -gt 9 -or
            [Math]::Abs($body.R - 0x12) + [Math]::Abs($body.G - 0x16) + [Math]::Abs($body.B - 0x1d) -gt 9) {
            throw 'Diagnostics-disabled shell palette pixels did not match the known client'
        }
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

function Assert-PanelVisible([Diagnostics.Process]$Process) {
    $previous = [IcedShellNativeInput]::SetThreadDpiAwarenessContext([IntPtr](-4))
    if ($previous -eq [IntPtr]::Zero) { throw 'SetThreadDpiAwarenessContext failed' }
    try {
        $client = New-Object IcedShellNativeInput+Rect
        $origin = New-Object IcedShellNativeInput+Point
        if (-not [IcedShellNativeInput]::GetClientRect($Process.MainWindowHandle, [ref]$client) -or
            -not [IcedShellNativeInput]::ClientToScreen($Process.MainWindowHandle, [ref]$origin)) {
            throw 'DPI-aware panel geometry lookup failed'
        }
    } finally {
        [void][IcedShellNativeInput]::SetThreadDpiAwarenessContext($previous)
    }
    $bitmap = New-Object Drawing.Bitmap ($client.Right - $client.Left), ($client.Bottom - $client.Top)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($origin.X, $origin.Y, 0, 0, $bitmap.Size)
        $panel = $bitmap.GetPixel(40, 220)
        if ([Math]::Abs($panel.R - 0x20) + [Math]::Abs($panel.G - 0x27) + [Math]::Abs($panel.B - 0x33) -gt 9) {
            throw "Diagnostics-disabled sibling click did not expose the panel palette: $($panel.R),$($panel.G),$($panel.B)"
        }
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

function Assert-CleanExit([Diagnostics.Process]$Process, [string]$Case) {
    if (-not $Process.WaitForExit(5000)) { throw "$Case did not exit within five seconds" }
    if ($Process.ExitCode -ne 0) { throw "$Case exited $($Process.ExitCode), expected zero" }
}

try {
    $shell = Start-Shell

    $rect = New-Object IcedShellNativeInput+Rect
    if (-not [IcedShellNativeInput]::GetClientRect($shell.MainWindowHandle, [ref]$rect)) {
        throw 'GetClientRect failed'
    }
    $dpi = [IcedShellNativeInput]::GetDpiForWindow($shell.MainWindowHandle)
    "observed: dpi=$dpi client=$($rect.Right - $rect.Left)x$($rect.Bottom - $rect.Top)"

    'case: Enter/Space and repeat suppression on Info'
    Send-Key 0x0D
    Wait-Status $shell 'panel=visible;focus=info'
    Send-Key 0x0D
    Wait-Status $shell 'panel=hidden;focus=info'
    Send-Key 0x20 -Repeat
    Wait-Status $shell 'panel=visible;focus=info'
    Send-Key 0x20
    Wait-Status $shell 'panel=hidden;focus=info'

    'case: canceled press establishes visible focus but does not activate'
    Send-Key 0x09
    Wait-Status $shell 'panel=hidden;focus=exit'
    Mouse-At $shell 50 31 down
    Wait-Status $shell 'panel=hidden;focus=info'
    Mouse-At $shell 900 600 up
    Wait-Status $shell 'panel=hidden;focus=info'
    Mouse-At $shell 135 31 down
    Wait-Status $shell 'panel=hidden;focus=exit'
    Mouse-At $shell 900 600 up
    Wait-Status $shell 'panel=hidden;focus=exit'

    'case: physical near-edge release agrees with visible Info geometry'
    Mouse-At $shell 50 31 down
    Physical-Mouse-At $shell 63 62 up
    Wait-Status $shell 'panel=hidden;focus=info'
    Mouse-At $shell 50 49 click
    Wait-Status $shell 'panel=visible;focus=info'
    Send-Key 0x1B
    Wait-Status $shell 'panel=hidden;focus=info'

    'case: physical near-edge canceled release on Exit stays alive'
    Mouse-At $shell 135 31 down
    Physical-Mouse-At $shell 170 62 up
    Wait-Status $shell 'panel=hidden;focus=exit'

    'case: direct Exit-to-Info sibling move preserves hover and activation'
    Mouse-At $shell 400 200 move
    Wait-Status $shell 'hover=none'
    Mouse-At $shell 135 31 move
    Wait-Status $shell 'hover=exit'
    Mouse-At $shell 50 31 move
    Wait-Status $shell 'hover=info'
    Mouse-At $shell 50 31 click
    Wait-Status $shell 'panel=visible;focus=info'
    Send-Key 0x1B
    Wait-Status $shell 'panel=hidden;focus=info'

    'case: forward/reverse traversal wraps'
    Send-Key 0x09
    Wait-Status $shell 'panel=hidden;focus=exit'
    Send-Key 0x09 -Shift
    Wait-Status $shell 'panel=hidden;focus=info'
    Send-Key 0x09 -Shift
    Wait-Status $shell 'panel=hidden;focus=exit'

    'case: click, F1, Escape, resize/minimize/restore and focus return preserve state'
    Mouse-At $shell 50 31 click
    Wait-Status $shell 'panel=visible;focus=info'
    Send-Key 0x1B
    Wait-Status $shell 'panel=hidden;focus=info'
    Send-Key 0x70
    Wait-Status $shell 'panel=visible;focus=info'
    Resize-To-Minimum $shell
    Wait-Status $shell 'panel=visible;focus=info'
    [void][IcedShellNativeInput]::ShowWindow($shell.MainWindowHandle, 3)
    Start-Sleep -Milliseconds 300
    Wait-Status $shell 'panel=visible;focus=info'
    [void][IcedShellNativeInput]::ShowWindow($shell.MainWindowHandle, 6)
    Start-Sleep -Milliseconds 300
    [void][IcedShellNativeInput]::ShowWindow($shell.MainWindowHandle, 9)
    Start-Sleep -Milliseconds 300
    Set-OwnedForeground $shell
    Wait-Status $shell 'panel=visible;focus=info'
    Wait-Status $shell 'active=yes'

    $focusShell = Start-Shell
    Wait-Status $shell 'panel=visible;focus=info;hover=none;active=no'
    [void][IcedShellNativeInput]::SendMessage($focusShell.MainWindowHandle, 0x0010, [UIntPtr]::Zero, [IntPtr]::Zero)
    Assert-CleanExit $focusShell 'owned focus-loss helper'
    Set-OwnedForeground $shell
    Wait-Status $shell 'panel=visible;focus=info'
    Wait-Status $shell 'active=yes'

    'case: Enter, Space, mouse, Alt+F4 and native close terminate fresh processes'
    Send-Key 0x09
    Send-Key 0x0D
    Assert-CleanExit $shell 'Enter on Exit'

    $shell = Start-Shell
    Send-Key 0x09
    Send-Key 0x20
    Assert-CleanExit $shell 'Space on Exit'

    $shell = Start-Shell
    Mouse-At $shell 135 31 click
    Assert-CleanExit $shell 'mouse click on Exit'

    $shell = Start-Shell
    Mouse-At $shell 400 200 move
    Wait-Status $shell 'hover=none'
    Mouse-At $shell 50 31 move
    Wait-Status $shell 'hover=info'
    Mouse-At $shell 135 31 move
    Wait-Status $shell 'hover=exit'
    Mouse-At $shell 135 31 click
    Assert-CleanExit $shell 'direct Info-to-Exit sibling move and click'

    $shell = Start-Shell
    Mouse-At $shell 135 49 click
    Assert-CleanExit $shell 'inside-edge release on Exit'

    $shell = Start-Shell
    Send-Key 0x73 -Alt
    Assert-CleanExit $shell 'Alt+F4'

    $shell = Start-Shell
    [void][IcedShellNativeInput]::SendMessage($shell.MainWindowHandle, 0x0010, [UIntPtr]::Zero, [IntPtr]::Zero)
    Assert-CleanExit $shell 'native close request'

    'case: ordinary outside-repository launch has no diagnostic status'
    Remove-Item Env:ICED_SHELL_NATIVE_TEST_STATUS -ErrorAction SilentlyContinue
    $normal = Start-Process -FilePath $ExePath -WorkingDirectory $env:TEMP -WindowStyle Normal -PassThru
    $owned.Add($normal)
    if (-not $normal.WaitForInputIdle(15000)) { throw 'normal launch did not become input-idle' }
    Start-Sleep -Milliseconds 500
    $normal.Refresh()
    if ($normal.MainWindowTitle -ne 'Iced Shell PoC') {
        throw "Normal title included unexpected diagnostics: '$($normal.MainWindowTitle)'"
    }
    Assert-NormalPalette $normal
    Set-OwnedForeground $normal
    Mouse-At $normal 400 200 move
    Mouse-At $normal 135 31 move
    Mouse-At $normal 50 31 move
    Mouse-At $normal 50 31 click
    Assert-PanelVisible $normal
    [void][IcedShellNativeInput]::SendMessage($normal.MainWindowHandle, 0x0010, [UIntPtr]::Zero, [IntPtr]::Zero)
    Assert-CleanExit $normal 'ordinary launch native close'

    'native-input: all real-input regressions passed'
} finally {
    if ($null -eq $oldDiagnostic) {
        Remove-Item Env:ICED_SHELL_NATIVE_TEST_STATUS -ErrorAction SilentlyContinue
    } else {
        $env:ICED_SHELL_NATIVE_TEST_STATUS = $oldDiagnostic
    }
    if ($null -eq $oldAdapterDiagnostic) {
        Remove-Item Env:ICED_SHELL_ADAPTER_DIAGNOSTICS -ErrorAction SilentlyContinue
    } else {
        $env:ICED_SHELL_ADAPTER_DIAGNOSTICS = $oldAdapterDiagnostic
    }
    foreach ($process in $owned) {
        if (-not $process.HasExited) {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        }
    }
}
