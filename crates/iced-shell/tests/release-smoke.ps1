# Interactive QA of the normal reader, with an isolated profile and owned window.
param(
    [string]$ExePath = (Join-Path $PSScriptRoot '..\..\..\target\release\iced-shell.exe'),
    [string]$EvidenceDirectory = (Join-Path $PSScriptRoot '..\..\..\target\release-audit\native-smoke')
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\..'))
$ExePath = [IO.Path]::GetFullPath($ExePath)
$EvidenceDirectory = [IO.Path]::GetFullPath($EvidenceDirectory)
if (Test-Path -LiteralPath $EvidenceDirectory) { throw 'Use a fresh evidence directory.' }
if (-not [Environment]::UserInteractive) { throw 'An interactive Windows desktop is required.' }
if (-not (Test-Path -LiteralPath $ExePath)) { throw 'Build the release executable first.' }
New-Item -ItemType Directory -Path $EvidenceDirectory -Force | Out-Null
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class ReleaseSmokeNative {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X,Y; }
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct KeyboardInput { public ushort Key,Scan; public uint Flags,Time; public UIntPtr Extra; }
    [StructLayout(LayoutKind.Explicit, Size=32)] public struct InputUnion { [FieldOffset(0)] public KeyboardInput Keyboard; }
    [StructLayout(LayoutKind.Sequential)] public struct Input { public uint Type; public InputUnion Data; }
    [DllImport("user32.dll")] public static extern uint SendInput(uint count, Input[] input, int size);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr window, ref Point point);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x,int y,int w,int h,uint flags);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
    [DllImport("user32.dll")] public static extern void keybd_event(byte key,byte scan,uint flags,UIntPtr extra);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags,uint x,uint y,uint data,UIntPtr extra);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window,StringBuilder value,int count);
    public static string Title(IntPtr window) { var value=new StringBuilder(1024); GetWindowText(window,value,value.Capacity); return value.ToString(); }
    public static void Unicode(string value) {
        foreach (char c in value) {
            var down=new Input {Type=1, Data=new InputUnion {Keyboard=new KeyboardInput {Scan=c, Flags=4}}};
            var up=down; up.Data.Keyboard.Flags=6;
            if(SendInput(2,new[]{down,up},Marshal.SizeOf(typeof(Input)))!=2) throw new Exception("Unicode SendInput failed");
        }
    }
    public static void Wheel(int delta) { mouse_event(0x0800,0,0,unchecked((uint)delta),UIntPtr.Zero); }
}
"@
$process = $null
$previousLocal = $env:LOCALAPPDATA
$previousForeground = [ReleaseSmokeNative]::GetForegroundWindow()
$previousDpi = [ReleaseSmokeNative]::SetThreadDpiAwarenessContext([IntPtr](-4))
$env:LOCALAPPDATA = Join-Path $EvidenceDirectory 'profile'
$profile = $env:LOCALAPPDATA
$window = [IntPtr]::Zero
function Assert-Target {
    if ($null -eq $process -or $process.HasExited) { throw 'Owned reader is not running.' }
    $active = [ReleaseSmokeNative]::GetForegroundWindow()
    $activeProcess = [uint32]0
    [void][ReleaseSmokeNative]::GetWindowThreadProcessId($active, [ref]$activeProcess)
    if ($activeProcess -ne $process.Id) { throw 'Foreground changed; stopped input to protect other apps.' }
}
function Key([byte]$Key, [switch]$Ctrl, [switch]$Shift, [switch]$Alt) {
    Assert-Target
    try {
        if ($Ctrl) { [ReleaseSmokeNative]::keybd_event(0x11,0,0,[UIntPtr]::Zero) }
        if ($Shift) { [ReleaseSmokeNative]::keybd_event(0x10,0,0,[UIntPtr]::Zero) }
        if ($Alt) { [ReleaseSmokeNative]::keybd_event(0x12,0,0,[UIntPtr]::Zero) }
        [ReleaseSmokeNative]::keybd_event($Key,0,0,[UIntPtr]::Zero)
        Start-Sleep -Milliseconds 50
        [ReleaseSmokeNative]::keybd_event($Key,0,2,[UIntPtr]::Zero)
    } finally {
        if ($Alt) { [ReleaseSmokeNative]::keybd_event(0x12,0,2,[UIntPtr]::Zero) }
        if ($Shift) { [ReleaseSmokeNative]::keybd_event(0x10,0,2,[UIntPtr]::Zero) }
        if ($Ctrl) { [ReleaseSmokeNative]::keybd_event(0x11,0,2,[UIntPtr]::Zero) }
    }
    Start-Sleep -Milliseconds 250
}
function Text([string]$Value) { Assert-Target; [ReleaseSmokeNative]::Unicode($Value); Start-Sleep -Milliseconds 250 }
function Capture([string]$Name) {
    Assert-Target
    $bounds = New-Object ReleaseSmokeNative+Rect
    $point = New-Object ReleaseSmokeNative+Point
    if (-not [ReleaseSmokeNative]::GetClientRect($window,[ref]$bounds) -or
        -not [ReleaseSmokeNative]::ClientToScreen($window,[ref]$point)) { throw 'Capture geometry failed.' }
    $bitmap = New-Object Drawing.Bitmap ($bounds.Right-$bounds.Left),($bounds.Bottom-$bounds.Top)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try { $graphics.CopyFromScreen($point.X,$point.Y,0,0,$bitmap.Size); $bitmap.Save((Join-Path $EvidenceDirectory ($Name+'.png'))) }
    finally { $graphics.Dispose(); $bitmap.Dispose() }
    Write-Host "Captured $Name"
}
function Size([int]$Width,[int]$Height) {
    $scale = [ReleaseSmokeNative]::GetDpiForWindow($window)/96.0
    $client = New-Object ReleaseSmokeNative+Rect
    $outer = New-Object ReleaseSmokeNative+Rect
    [void][ReleaseSmokeNative]::GetClientRect($window,[ref]$client)
    [void][ReleaseSmokeNative]::GetWindowRect($window,[ref]$outer)
    $work = [Windows.Forms.Screen]::PrimaryScreen.WorkingArea
    $w = [Math]::Min([int]($Width*$scale + ($outer.Right-$outer.Left)-$client.Right),$work.Width-32)
    $h = [Math]::Min([int]($Height*$scale + ($outer.Bottom-$outer.Top)-$client.Bottom),$work.Height-32)
    if (-not [ReleaseSmokeNative]::SetWindowPos($window,[IntPtr]::Zero,$work.Left+16,$work.Top+16,$w,$h,0x0040)) { throw 'Resize failed.' }
    Start-Sleep -Milliseconds 500
}
function Click([int]$X,[int]$Y,[int]$Count=1) {
    Assert-Target
    $scale = [ReleaseSmokeNative]::GetDpiForWindow($window)/96.0
    $point = New-Object ReleaseSmokeNative+Point
    $point.X = [int]($X*$scale); $point.Y = [int]($Y*$scale)
    if (-not [ReleaseSmokeNative]::ClientToScreen($window,[ref]$point)) { throw 'Click geometry failed.' }
    [void][ReleaseSmokeNative]::SetCursorPos($point.X,$point.Y)
    Start-Sleep -Milliseconds 80
    for ($i=0;$i -lt $Count;$i++) {
        Assert-Target
        [ReleaseSmokeNative]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
        [ReleaseSmokeNative]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
        Start-Sleep -Milliseconds 90
    }
    Start-Sleep -Milliseconds 400
}
function Wait-Title([string]$Expected) {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        if ($process.HasExited) { throw 'Reader exited while opening a document.' }
        if ([ReleaseSmokeNative]::Title($window).Contains($Expected)) { Start-Sleep -Milliseconds 700; return }
        Start-Sleep -Milliseconds 30
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Expected '$Expected'; title is '$([ReleaseSmokeNative]::Title($window))'."
}
function Open([string]$Path,[string]$Title) {
    Key 0x4f -Ctrl
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while ([ReleaseSmokeNative]::GetForegroundWindow() -eq $window -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 30 }
    Assert-Target
    if ([ReleaseSmokeNative]::GetForegroundWindow() -eq $window) { throw 'Ctrl+O did not open the native file picker.' }
    Text ([IO.Path]::GetFullPath($Path))
    Key 0x0d
    Wait-Title $Title
    Assert-Target
    if ([ReleaseSmokeNative]::GetForegroundWindow() -ne $window) { throw 'File picker did not return to the reader.' }
}
try {
    $source = Join-Path $EvidenceDirectory 'sources'
    New-Item -ItemType Directory -Path $source -Force | Out-Null
    $txt = Join-Path $source 'Native reading notes.txt'
    $md = Join-Path $source 'Native reading checklist.md'
    [IO.File]::WriteAllText($txt, ('A book gives its reader room to think.'+[Environment]::NewLine+[Environment]::NewLine)*100)
    [IO.File]::WriteAllText($md,"# A reading checklist`r`n`r`nA **book** with _emphasis_.`r`n`r`n- One idea`r`n- Another idea`r`n")
    $process = Start-Process -FilePath $ExePath -WorkingDirectory $env:TEMP -WindowStyle Normal -PassThru
    if (-not $process.WaitForInputIdle(15000)) { throw 'Reader did not become input idle.' }
    Start-Sleep -Milliseconds 700
    $process.Refresh(); $window = $process.MainWindowHandle
    if ($window -eq [IntPtr]::Zero) { throw 'Reader did not create a window.' }
    [void][ReleaseSmokeNative]::SetForegroundWindow($window)
    Start-Sleep -Milliseconds 200
    Size 900 640
    Capture '01-empty-library'
    Open $txt 'Native reading notes'
    Click 160 151 2; Capture '02a-missing-dictionary'
    Key 0x09; Key 0x09; Key 0x0d
    $catalog = Get-Content -Raw -LiteralPath (Join-Path $root 'assets/dictionaries/catalog.json') | ConvertFrom-Json
    $package = $catalog.packages | Where-Object { $_.source -eq 'english' -and $_.target -eq 'turkish' }
    $installed = Join-Path $profile ('simPl/dictionaries/'+$package.file)
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    while (-not (Test-Path -LiteralPath $installed) -and [DateTime]::UtcNow -lt $deadline) { Assert-Target; Start-Sleep -Milliseconds 50 }
    if (-not (Test-Path -LiteralPath $installed)) { Capture 'dictionary-download-failed'; throw 'Native word-card download did not install its package.' }
    if ((Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash.ToLowerInvariant() -ne $package.sha256) { throw 'Installed package differs from the catalog.' }
    Start-Sleep -Milliseconds 500; Capture '02b-offline-word-result'; Key 0x1b
    Key 0x46 -Ctrl; Text 'book'; Capture '02-txt-find'; Key 0x1b
    Key 0x44 -Ctrl; Key 0x42 -Ctrl; Capture '03-txt-bookmark'; Key 0x42 -Ctrl
    Key 0x77; Capture '04-toolbar-hidden'; Key 0x77
    Key 0x27; Capture '05-next-page'
    Key 0x57 -Ctrl; Wait-Title 'simPl'; Capture '06-library-after-txt'
    Open $md 'A reading checklist'; Capture '07-markdown'
    Key 0x4b -Ctrl; Text 'notes'; Capture '08-quick-switcher'; Key 0x1b
    Key 0x57 -Ctrl
    Open (Join-Path $root 'fixtures/book-structure/structured.html') 'A quieter page'; Capture '09-html'
    Key 0x57 -Ctrl
    Open (Join-Path $root 'target/book-milestone2/structured.epub') 'A quieter page'
    Key 0x54 -Ctrl; Capture '10-epub-contents'; Key 0x1b
    Key 0x57 -Ctrl
    Open (Join-Path $root 'target/book-milestone3/fixtures/prose.pdf') 'A quiet reading journey'
    Capture '11-pdf-document'
    Key 0x4c -Ctrl; Key 0x41 -Ctrl; Text '2'; Key 0x0d; Capture '12-pdf-page-two'
    Size 540 640; Capture '13-narrow-pdf'
    $scale = [ReleaseSmokeNative]::GetDpiForWindow($window)/96.0
    $bounds = New-Object ReleaseSmokeNative+Rect; [void][ReleaseSmokeNative]::GetClientRect($window,[ref]$bounds)
    $point = New-Object ReleaseSmokeNative+Point; [void][ReleaseSmokeNative]::ClientToScreen($window,[ref]$point)
    [void][ReleaseSmokeNative]::SetCursorPos($point.X+$bounds.Right-[int](40*$scale),$point.Y+[int](24*$scale))
    Assert-Target; [ReleaseSmokeNative]::mouse_event(2,0,0,0,[UIntPtr]::Zero); [ReleaseSmokeNative]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
    Start-Sleep -Milliseconds 500; Capture '14-settings-top'
    [void][ReleaseSmokeNative]::SetCursorPos($point.X+[int](300*$scale),$point.Y+[int](340*$scale))
    for ($i=0;$i -lt 16;$i++) { Assert-Target; [ReleaseSmokeNative]::Wheel(-120); Start-Sleep -Milliseconds 80 }
    Capture '15-settings-dictionaries'; Key 0x1b
    Key 0x57 -Ctrl; Capture '16-populated-library'
    Key 0x73 -Alt
    if (-not $process.WaitForExit(8000) -or $process.ExitCode -ne 0) { throw 'Normal close did not exit cleanly.' }
    $library = Get-Content -Raw -LiteralPath (Join-Path $profile 'simPl/library.json') | ConvertFrom-Json
    if ($library.entries.Count -ne 5) { throw "Expected five imported formats, got $($library.entries.Count)." }
    $formats = @($library.entries | ForEach-Object { if ($_.PSObject.Properties['source_kind']) { $_.source_kind } else { $_.document.kind } })
    if (-not ($formats -contains 'Text') -or -not ($formats -contains 'Markdown')) { throw 'TXT/Markdown source labels were not retained.' }
    $evidence = [ordered]@{ exe_sha256=(Get-FileHash $ExePath -Algorithm SHA256).Hash.ToLowerInvariant(); dpi_scale=$scale; profile=$profile; formats=$formats; result='passed'; assertions='native picker, keyboard navigation, persisted five imports/source labels, clean exit; screenshots require visual review' }
    $evidence | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 -LiteralPath (Join-Path $EvidenceDirectory 'result.json')
    Write-Host 'Normal-reader native smoke passed; inspect all screenshots.'
} finally {
    if ($null -ne $process -and -not $process.HasExited) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
    $env:LOCALAPPDATA = $previousLocal
    if ($previousForeground -ne [IntPtr]::Zero) { [void][ReleaseSmokeNative]::SetForegroundWindow($previousForeground) }
    if ($previousDpi -ne [IntPtr]::Zero) { [void][ReleaseSmokeNative]::SetThreadDpiAwarenessContext($previousDpi) }
}
