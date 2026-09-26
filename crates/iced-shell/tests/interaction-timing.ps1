param(
    [string]$ExePath = (Join-Path $PSScriptRoot '..\..\..\target\release\iced-shell.exe'),
    [string]$EvidenceDirectory = (Join-Path $env:TEMP ('iced-w14-' + [guid]::NewGuid().ToString('N'))),
    [string]$OnlyRun = ''
)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'interaction-trace.ps1')
. (Join-Path $PSScriptRoot 'interaction-source.ps1')
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\..'))
$ExePath = [IO.Path]::GetFullPath($ExePath)
$EvidenceDirectory = [IO.Path]::GetFullPath($EvidenceDirectory)
if (-not [Environment]::UserInteractive) { throw 'Interactive desktop required' }
if (Test-Path -LiteralPath $EvidenceDirectory) { throw 'Evidence directory must be fresh' }
if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) { throw 'Release executable missing' }
$overrides = @(Get-ChildItem Env: | Where-Object { $_.Name -match '^(ICED_|WGPU_|WINIT_|RUST_LOG$)' })
if ($overrides.Count) { throw "Inherited evidence/backend overrides: $($overrides.Name -join ',')" }
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class W14Native {
 [StructLayout(LayoutKind.Sequential)] public struct Point { public int X,Y; }
 [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
 [DllImport("kernel32.dll")] public static extern bool QueryPerformanceCounter(out long ticks);
 [DllImport("kernel32.dll")] public static extern bool QueryPerformanceFrequency(out long frequency);
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd,out uint pid);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
 [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hwnd);
 [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(Point point);
 [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr hwnd,uint flags);
 [StructLayout(LayoutKind.Sequential)] public struct GuiThreadInfo {
  public uint cbSize,flags; public IntPtr hwndActive,hwndFocus,hwndCapture,hwndMenuOwner,hwndMoveSize,hwndCaret; public Rect rcCaret;
 }
 [DllImport("user32.dll")] public static extern bool GetGUIThreadInfo(uint thread,ref GuiThreadInfo info);
 [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd,ref Rect rect);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd,ref Rect rect);
 [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd,ref Point point);
 [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd,IntPtr after,int x,int y,int width,int height,uint flags);
 [DllImport("user32.dll")] public static extern void mouse_event(uint flags,uint x,uint y,uint data,UIntPtr extra);
}
"@
function Tick { [long]$t=0; if (-not [W14Native]::QueryPerformanceCounter([ref]$t)) { throw 'QPC failed' }; return $t }
function Owned($p) {
 $p.Refresh(); [uint32]$pidOwner=0; $hwnd=$p.MainWindowHandle
 if ($p.HasExited -or $hwnd -eq 0 -or -not [W14Native]::IsWindowVisible($hwnd) -or [W14Native]::IsIconic($hwnd) -or
     [W14Native]::GetForegroundWindow() -ne $hwnd -or [W14Native]::GetWindowThreadProcessId($hwnd,[ref]$pidOwner) -eq 0 -or $pidOwner -ne [uint32]$p.Id) { throw 'wrong/occluded/unowned/minimized target' }
 $rect=New-Object W14Native+Rect
 if (-not [W14Native]::GetClientRect($hwnd,[ref]$rect)) { throw 'client geometry unavailable' }
 foreach ($xy in @(@([int]($rect.Right/2),[int]($rect.Bottom/2)),@([int]($rect.Right/3),[int]($rect.Bottom/3)),@([int](2*$rect.Right/3),[int](2*$rect.Bottom/3)))) {
  $point=New-Object W14Native+Point; $point.X=$xy[0]; $point.Y=$xy[1]
  if (-not [W14Native]::ClientToScreen($hwnd,[ref]$point) -or [W14Native]::GetAncestor([W14Native]::WindowFromPoint($point),2) -ne $hwnd) { throw 'occluded client sample' }
 }
 # Unsampled/transient overlay remains an acknowledged limit.
}
function Assert-InputPoint($p,[W14Native+Point]$point) {
 Owned $p
 $actual=[W14Native]::GetAncestor([W14Native]::WindowFromPoint($point),2)
 try { Assert-W14TargetOwner $p.MainWindowHandle $actual }
 catch { throw "actual input coordinate $($point.X),$($point.Y) is occluded or unowned (expected=$($p.MainWindowHandle), root=$actual)" }
}
function Assert-DragCapture($p,[W14Native+Point]$point) {
 Owned $p
 [uint32]$pidOwner=0
 $thread=[W14Native]::GetWindowThreadProcessId($p.MainWindowHandle,[ref]$pidOwner)
 $info=New-Object W14Native+GuiThreadInfo
 $info.cbSize=[Runtime.InteropServices.Marshal]::SizeOf($info)
 if (-not [W14Native]::GetGUIThreadInfo($thread,[ref]$info)) { throw 'drag capture query failed' }
 $capture=[W14Native]::GetAncestor($info.hwndCapture,2)
 $move=[W14Native]::GetAncestor($info.hwndMoveSize,2)
 if ($capture -ne $p.MainWindowHandle -and $move -ne $p.MainWindowHandle) { throw "drag destination $($point.X),$($point.Y) is not captured by owned HWND" }
}
function Assert-ThumbCapture($p,[W14Native+Point]$point) {
 Owned $p
 [uint32]$pidOwner=0
 $thread=[W14Native]::GetWindowThreadProcessId($p.MainWindowHandle,[ref]$pidOwner)
 $info=New-Object W14Native+GuiThreadInfo
 $info.cbSize=[Runtime.InteropServices.Marshal]::SizeOf($info)
 if (-not [W14Native]::GetGUIThreadInfo($thread,[ref]$info)) { throw 'thumb capture query failed' }
 $capture=[W14Native]::GetAncestor($info.hwndCapture,2)
 try { Assert-W14CaptureOwner $p.MainWindowHandle $capture }
 catch { throw "thumb movement $($point.X),$($point.Y) lost owned mouse capture (root=$capture)" }
}
function ClientPoint($p,[int]$x,[int]$y) {
 $point=New-Object W14Native+Point; $point.X=$x; $point.Y=$y
 if (-not [W14Native]::ClientToScreen($p.MainWindowHandle,[ref]$point)) { throw 'ClientToScreen failed' }
 return $point
}
function Capture($p,[string]$path) {
 Owned $p
 $rect=New-Object W14Native+Rect
 if (-not [W14Native]::GetClientRect($p.MainWindowHandle,[ref]$rect)) { throw 'GetClientRect failed' }
 $origin=ClientPoint $p 0 0
 # This thread is DPI-unaware PowerShell 5.1: client coordinates are virtualized.
 $scale=[W14Native]::GetDpiForWindow($p.MainWindowHandle)/96.0
 $bitmap=New-Object Drawing.Bitmap([int][Math]::Round($rect.Right*$scale),[int][Math]::Round($rect.Bottom*$scale))
 $graphics=[Drawing.Graphics]::FromImage($bitmap)
 try {
   $before=Tick
   $graphics.CopyFromScreen([int][Math]::Round($origin.X*$scale),[int][Math]::Round($origin.Y*$scale),0,0,$bitmap.Size)
   $after=Tick
   Owned $p
   $bitmap.Save($path,[Drawing.Imaging.ImageFormat]::Png)
   $cropRect=New-Object Drawing.Rectangle([int]($bitmap.Width*0.28),[int]($bitmap.Height*0.20),[int]($bitmap.Width*0.44),[int]($bitmap.Height*0.52))
   $crop=$bitmap.Clone($cropRect,[Drawing.Imaging.PixelFormat]::Format32bppArgb)
   $stream=New-Object IO.MemoryStream
   $sha=[Security.Cryptography.SHA256]::Create()
   try {
    $crop.Save($stream,[Drawing.Imaging.ImageFormat]::Png)
    $contentHash=([BitConverter]::ToString($sha.ComputeHash($stream.ToArray()))).Replace('-','').ToLowerInvariant()
   } finally { $crop.Dispose(); $stream.Dispose(); $sha.Dispose() }
   return [pscustomobject]@{start=$before;end=$after;sha256=(Get-FileHash $path -Algorithm SHA256).Hash.ToLowerInvariant();content_sha256=$contentHash;pixels="$($bitmap.Width)x$($bitmap.Height)"}
 } finally { $graphics.Dispose(); $bitmap.Dispose() }
}
function Wheel($p,[int]$delta) {
 Owned $p
 $rect=New-Object W14Native+Rect; if (-not [W14Native]::GetClientRect($p.MainWindowHandle,[ref]$rect)) { throw 'client rect failed' }
 $point=ClientPoint $p ([int]($rect.Right/2)) ([int]([Math]::Min($rect.Bottom-80,360)))
 Assert-InputPoint $p $point
 if (-not [W14Native]::SetCursorPos($point.X,$point.Y)) { throw 'cursor move failed' }
 Assert-InputPoint $p $point
 $before=Tick
 [W14Native]::mouse_event(0x0800,0,0,[BitConverter]::ToUInt32([BitConverter]::GetBytes([int32]$delta),0),[UIntPtr]::Zero)
 $after=Tick
 return [pscustomobject]@{source='driver';event='wheel_injected';ticks=$before;end=$after;delta=$delta}
}
function DragThumb($p,[bool]$toEnd) {
 Owned $p
 $rect=New-Object W14Native+Rect; if (-not [W14Native]::GetClientRect($p.MainWindowHandle,[ref]$rect)) { throw 'client rect failed' }
 $x=$rect.Right-30
 $from=if ($toEnd) { 145 } else { $rect.Bottom-27 }
 $to=if ($toEnd) { $rect.Bottom-8 } else { 145 }
 $point=ClientPoint $p $x $from; $destination=ClientPoint $p $x $to
 Assert-InputPoint $p $point
 if (-not [W14Native]::SetCursorPos($point.X,$point.Y)) { throw 'thumb position failed' }
 Start-Sleep -Milliseconds 130; Assert-InputPoint $p $point
 $start=Tick
 [W14Native]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
 try {
  Assert-ThumbCapture $p $point
  foreach ($step in 1..12) {
   $next=New-Object W14Native+Point; $next.X=$point.X; $next.Y=$point.Y+[int](($destination.Y-$point.Y)*$step/12)
   Assert-InputPoint $p $next
   Assert-ThumbCapture $p $next
   if (-not [W14Native]::SetCursorPos($next.X,$next.Y)) { throw 'thumb drag cursor failed' }
   Start-Sleep -Milliseconds 25
   Assert-InputPoint $p $next
   Assert-ThumbCapture $p $next
  }
 } finally { [W14Native]::mouse_event(4,0,0,0,[UIntPtr]::Zero) }
 return [pscustomobject]@{source='driver';event='thumb_drag';ticks=$start;end=(Tick);to_end=$toEnd}
}
function ResizeGesture($p,[int]$dy) {
 Owned $p
 $rect=New-Object W14Native+Rect; if (-not [W14Native]::GetWindowRect($p.MainWindowHandle,[ref]$rect)) { throw 'window rect failed' }
 $x=[int](($rect.Left+$rect.Right)/2); $y=$rect.Bottom-2
 $point=New-Object W14Native+Point; $point.X=$x; $point.Y=$y
 Assert-InputPoint $p $point
 if (-not [W14Native]::SetCursorPos($x,$y)) { throw 'resize cursor failed' }
 Start-Sleep -Milliseconds 120
 Assert-InputPoint $p $point
 $start=Tick
 [W14Native]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
 try {
   Assert-DragCapture $p $point
   foreach ($step in 1..8) {
    $next=New-Object W14Native+Point; $next.X=$x; $next.Y=$y+[int]($dy*$step/8)
    Assert-DragCapture $p $next
    if (-not [W14Native]::SetCursorPos($next.X,$next.Y)) { throw 'resize cursor step failed' }
    Start-Sleep -Milliseconds 30; Assert-DragCapture $p $next
   }
 } finally { [W14Native]::mouse_event(4,0,0,0,[UIntPtr]::Zero) }
 return [pscustomobject]@{source='driver';event='resize_gesture';ticks=$start;end=(Tick);delta=$dy}
}
function ClickWidth($p) {
 Owned $p; $point=ClientPoint $p 930 115
 Assert-InputPoint $p $point
 if (-not [W14Native]::SetCursorPos($point.X,$point.Y)) { throw 'width cursor move failed' }
 Start-Sleep -Milliseconds 120; Assert-InputPoint $p $point
 [W14Native]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
 try { Start-Sleep -Milliseconds 60 } finally { [W14Native]::mouse_event(4,0,0,0,[UIntPtr]::Zero) }
}
function WaitLoaded($p,[string]$tracePath) {
 $until=[DateTime]::UtcNow.AddSeconds(90)
 do {
   if ($p.HasExited) { throw "reader exited: $($p.ExitCode)" }
   if (Test-Path $tracePath) {
     $lines=@(Get-Content $tracePath -ErrorAction SilentlyContinue)
     if (@($lines | Where-Object { $_ -match '^view,' }).Count -ge 3) { return }
   }
   Start-Sleep -Milliseconds 150
 } while ([DateTime]::UtcNow -lt $until)
 throw 'reader view did not become ready'
}
function CloseOwned($p,[bool]$RequestClean) {
 if ($null -eq $p) { return [pscustomobject]@{intentional=$false;exit_code=$null} }
 $intentional=$false; $code=$null
 try {
  $p.Refresh()
  if ($RequestClean -and -not $p.HasExited) {
   Owned $p
   if ($p.CloseMainWindow()) {
    $intentional=$p.WaitForExit(5000)
   }
  }
  $p.Refresh()
  if (-not $p.HasExited) { $p.Kill(); [void]$p.WaitForExit(5000) }
  $code=$p.ExitCode
  return [pscustomobject]@{intentional=$intentional;exit_code=$code}
 } catch {
  $p.Refresh()
  if (-not $p.HasExited) { $p.Kill(); [void]$p.WaitForExit(5000) }
  return [pscustomobject]@{intentional=$false;exit_code=$p.ExitCode}
 } finally { $p.Dispose() }
}
function Probe-Adapter([string]$path) {
 $probePath=Join-Path $path 'adapter-probe.stderr.txt'
 $p=$null
 try {
  $info=New-Object Diagnostics.ProcessStartInfo
  $info.FileName=$ExePath; $info.Arguments='--reader-poc'; $info.WorkingDirectory=$root
  $info.UseShellExecute=$false; $info.RedirectStandardError=$true
  $info.EnvironmentVariables['ICED_SHELL_ADAPTER_DIAGNOSTICS']='1'
  $p=New-Object Diagnostics.Process; $p.StartInfo=$info
  if (-not $p.Start() -or -not $p.WaitForInputIdle(15000)) { throw 'adapter probe did not become input idle' }
  for ($attempt=0; $attempt -lt 8; $attempt++) {
   $p.Refresh()
   if ($p.MainWindowHandle -ne 0) { [void][W14Native]::SetForegroundWindow($p.MainWindowHandle) }
   Start-Sleep -Milliseconds 180
   if ([W14Native]::GetForegroundWindow() -eq $p.MainWindowHandle) { break }
  }
  Owned $p
  Start-Sleep -Milliseconds 1800
  if (-not $p.CloseMainWindow() -or -not $p.WaitForExit(5000)) { throw 'adapter probe failed clean close' }
  $code=$p.ExitCode
  $raw=$p.StandardError.ReadToEnd()
  [IO.File]::WriteAllText($probePath,$raw)
  $p.Dispose(); $p=$null
  if ($code -ne 0) { throw "adapter probe nonzero exit $code" }
  $pattern='(?s)\AICED_SHELL_ADAPTER_BEGIN target=iced_wgpu::window::compositor\r?\nSelected: AdapterInfo \{\s+name: "(?<name>[^"\r\n]+)",.*?\s+driver: "(?<driver>[^"\r\n]+)",\s+driver_info: "(?<version>[^"\r\n]+)",\s+backend: (?<backend>Dx12|Vulkan|Gl),\s+\}\r?\nICED_SHELL_ADAPTER_END\r?\n?\z'
  $m=[regex]::Match($raw,$pattern)
  if (-not $m.Success) { throw 'missing/invalid/duplicate adapter selection record' }
  return [ordered]@{name=$m.Groups['name'].Value;driver=$m.Groups['driver'].Value;driver_info=$m.Groups['version'].Value;backend=$m.Groups['backend'].Value;exit_code=$code;exe_sha256=(Get-FileHash $ExePath -Algorithm SHA256).Hash.ToLowerInvariant();mode='--reader-poc';gate='adapter only; interaction trace absent';attribution='separate diagnostic process on same binary/config; cannot prove per-run selection or its uninstrumented cost'}
 } finally {
  if ($null -ne $p) {
   if (-not $p.HasExited) { $p.Kill(); [void]$p.WaitForExit(5000) }
   $p.Dispose()
  }
 }
}
# Negative controls are executed before launching any child.
try { Owned ([Diagnostics.Process]::GetCurrentProcess()); throw 'unowned negative control accepted' } catch { if ($_.Exception.Message -eq 'unowned negative control accepted') { throw } }
$display=Get-CimInstance Win32_VideoController | Where-Object { $_.CurrentHorizontalResolution -gt 0 -and $_.CurrentRefreshRate -gt 0 } | Select-Object -First 1
if (-not $display) { throw 'display refresh unavailable' }
[long]$frequency=0; if (-not [W14Native]::QueryPerformanceFrequency([ref]$frequency)) { throw 'QPC frequency unavailable' }
New-Item -ItemType Directory -Path $EvidenceDirectory | Out-Null
$exeHash=(Get-FileHash $ExePath -Algorithm SHA256).Hash.ToLowerInvariant()
$sourceFiles=@(
 Get-ChildItem (Join-Path $root 'crates\iced-shell\src') -Recurse -File -Filter '*.rs' | ForEach-Object { $_.FullName.Substring($root.Length+1).Replace('\','/') }
 Get-ChildItem (Join-Path $root 'crates\reader-workload\src') -Recurse -File -Filter '*.rs' | ForEach-Object { $_.FullName.Substring($root.Length+1).Replace('\','/') }
 Get-ChildItem (Join-Path $root 'patches\cosmic-text-0.15.0\src') -Recurse -File -Filter '*.rs' | ForEach-Object { $_.FullName.Substring($root.Length+1).Replace('\','/') }
 'Cargo.toml'; 'Cargo.lock'; 'rust-toolchain.toml'; 'crates/iced-shell/Cargo.toml'; 'crates/reader-workload/Cargo.toml';
 'patches/cosmic-text-0.15.0/Cargo.toml'; 'fixtures/reader-workload/manifest.txt';
 'crates/iced-shell/tests/interaction-timing.ps1'; 'crates/iced-shell/tests/interaction-trace.ps1';
 'crates/iced-shell/tests/interaction-trace-tests.ps1'; 'crates/iced-shell/tests/interaction-source.ps1';
 'crates/iced-shell/tests/interaction-source-tests.ps1'; 'crates/iced-shell/tests/analyze-interaction.py';
 'crates/iced-shell/tests/analyze-interaction-tests.py'
)
$sourceBefore=Get-W14SourceIdentity $root $sourceFiles
$sourceBefore | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $EvidenceDirectory 'source-manifest.json')
$context=[ordered]@{
 schema='iced-w14/v1'; rustc=(rustc --version | Out-String).Trim(); endpoint='app callbacks + driver QPC + guarded post-gesture desktop pixels; no presented-frame endpoint';
 exe=$ExePath; exe_sha256=$exeHash; exe_bytes=(Get-Item $ExePath).Length;
 lock_sha256=(Get-FileHash (Join-Path $root 'Cargo.lock') -Algorithm SHA256).Hash.ToLowerInvariant();
 patch_shape_sha256=(Get-FileHash (Join-Path $root 'patches\cosmic-text-0.15.0\src\shape.rs') -Algorithm SHA256).Hash.ToLowerInvariant();
 fixture_manifest_sha256=(Get-FileHash (Join-Path $root 'fixtures\reader-workload\manifest.txt') -Algorithm SHA256).Hash.ToLowerInvariant();
 fixture_revision='reader-workload-fx-3'; source_identity_sha256=$sourceBefore.sha256; source_identity_file='source-manifest.json'; source_file_count=$sourceBefore.files.Count; os=[Environment]::OSVersion.VersionString;
 display="$($display.CurrentHorizontalResolution)x$($display.CurrentVerticalResolution)@$($display.CurrentRefreshRate)Hz (WMI reported)"; qpc_frequency=$frequency;
 gpu=@(Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion);
 power=(powercfg /getactivescheme | Out-String).Trim(); ac=(Get-CimInstance Win32_Battery -ErrorAction SilentlyContinue | Select-Object BatteryStatus,EstimatedChargeRemaining);
 actual_timing_backend='unobserved (private WGPU state); separate same-executable reader adapter probe is supporting context, not a per-run identity attestation';
 note='Adapter probe is separately launched with a diagnostic gate on the same release executable; measurement processes have only exact W14 trace. Actual per-run adapter selection cannot be proven from private WGPU state. Input coordinates and capture ownership are guarded. Resize-only setup parks a short viewport using SetWindowPos before timed real mouse resize gestures.'
}
$context.adapter_probe=Probe-Adapter $EvidenceDirectory
if ($context.adapter_probe.exe_sha256 -ne $exeHash) { throw 'adapter probe executable changed' }
$context | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $EvidenceDirectory 'manifest.json')
$report=[Collections.Generic.List[object]]::new()
$negativePointChecked=$false
try {
 foreach ($size in @(1000,10000)) {
  foreach ($kind in @('scroll','resize')) {
   $widths=if ($kind -eq 'scroll') { @(800,480) } else { @(800) }
   foreach ($width in $widths) {
    foreach ($run in 1..3) {
     $id="$size-$kind-$width-$run"
     if ($OnlyRun -and $id -ne $OnlyRun) { continue }
     $runDir=Join-Path $EvidenceDirectory $id
     New-Item -ItemType Directory $runDir | Out-Null
     $tracePath=Join-Path $runDir 'app.csv'
     $p=$null; $old=$env:ICED_SHELL_INTERACTION_TRACE
     $outcome=[ordered]@{id=$id;size=$size;kind=$kind;width=$width;status='INVALID';reason=$null; events=@(); capture=@(); started=(Tick); ended=$null; presented_frame='unavailable'; input_to_display='unavailable'}
     try {
      $env:ICED_SHELL_INTERACTION_TRACE=$tracePath
      $mode=if ($size -eq 1000) {'--reader-poc'} else {'--reader-poc-large'}
      $p=Start-Process -FilePath $ExePath -ArgumentList $mode -WorkingDirectory $root -PassThru
      if (-not $p.WaitForInputIdle(15000)) { throw 'reader not input idle' }
      $foreground=$false
      foreach ($attempt in 1..8) {
       $p.Refresh()
       if ($p.MainWindowHandle -ne 0) { [void][W14Native]::SetForegroundWindow($p.MainWindowHandle) }
       Start-Sleep -Milliseconds 180
       if ([W14Native]::GetForegroundWindow() -eq $p.MainWindowHandle) { $foreground=$true; break }
      }
      if (-not $foreground) { throw 'cannot foreground reader' }
      Owned $p
      if (-not $negativePointChecked) {
       $wrong=New-Object W14Native+Point; $wrong.X=-500; $wrong.Y=-500
       try { Assert-InputPoint $p $wrong; throw 'off-target input point was accepted' }
       catch { if ($_.Exception.Message -eq 'off-target input point was accepted') { throw } }
       $negativePointChecked=$true
      }
      WaitLoaded $p $tracePath
      Start-Sleep -Milliseconds 1800
      if ($width -eq 480) { ClickWidth $p; Start-Sleep -Milliseconds 600 }
      if ($kind -eq 'resize') {
       # Setup only: park a short viewport clear of the taskbar. Measured changes remain native mouse drags.
       $outer=New-Object W14Native+Rect
       if (-not [W14Native]::GetWindowRect($p.MainWindowHandle,[ref]$outer) -or
           -not [W14Native]::SetWindowPos($p.MainWindowHandle,[IntPtr]::Zero,100,10,$outer.Right-$outer.Left,540,0x14)) { throw 'resize setup failed' }
       Start-Sleep -Milliseconds 450; Owned $p
      }
      $outcome.pid=$p.Id; $outcome.hwnd=$p.MainWindowHandle.ToInt64(); $outcome.dpi=[W14Native]::GetDpiForWindow($p.MainWindowHandle)
      $before=Capture $p (Join-Path $runDir 'before.png'); $outcome.capture+=@($before)
      $outcome.start=(Tick)
      if ($kind -eq 'scroll') {
       foreach ($step in 1..32) { $outcome.events+=@(Wheel $p -120); Start-Sleep -Milliseconds 25 }
       Start-Sleep -Milliseconds 450
       # Native scrollbar drag tests the far end; no absolute row estimate substitutes for movement.
       $outcome.events+=@(DragThumb $p $true); Start-Sleep -Milliseconds 350
       foreach ($step in 1..48) { $outcome.events+=@(Wheel $p -1200); Start-Sleep -Milliseconds 25 }
       Start-Sleep -Milliseconds 350
       $far=Capture $p (Join-Path $runDir 'far.png'); $outcome.capture+=@($far)
       foreach ($step in 1..32) { $outcome.events+=@(Wheel $p 120); Start-Sleep -Milliseconds 25 }
       $outcome.events+=@(DragThumb $p $false)
       Start-Sleep -Milliseconds 200
       foreach ($step in 1..60) { $outcome.events+=@(Wheel $p 1200); Start-Sleep -Milliseconds 25 }
       Start-Sleep -Milliseconds 350
      } else {
       $outcome.events+=@(ResizeGesture $p 260); Start-Sleep -Milliseconds 500
       $capped=Capture $p (Join-Path $runDir 'capped.png'); $outcome.capture+=@($capped)
       $outcome.events+=@(ResizeGesture $p -260); Start-Sleep -Milliseconds 500
       $short=Capture $p (Join-Path $runDir 'short.png'); $outcome.capture+=@($short)
       $outcome.events+=@(ResizeGesture $p 260); Start-Sleep -Milliseconds 500
      }
      Start-Sleep -Milliseconds 650
      $outcome.end=(Tick)
      $after=Capture $p (Join-Path $runDir 'after.png'); $outcome.capture+=@($after)
      if ($kind -eq 'scroll' -and ($before.sha256 -eq $far.sha256 -or $far.sha256 -eq $after.sha256 -or $before.content_sha256 -ne $after.content_sha256)) { throw 'scroll content pixels did not return to same top region' }
      if ($kind -eq 'resize' -and ($capped.pixels -eq $short.pixels -or $capped.sha256 -eq $short.sha256 -or $capped.pixels -ne $after.pixels)) { throw 'short/capped resize pixels or geometry did not change' }
      $trace=Read-W14Trace $tracePath
      if ($trace.frequency -ne $frequency) { throw 'QPC domains disagree' }
      Assert-W14Width $trace $width $outcome.start
      Assert-W14Scenario $trace $kind $outcome.start $outcome.end $width
      if ($kind -eq 'scroll') {
       # Stillness is a separate control: no injected wheel, not a dropped-frame denominator.
       $idleStart=Tick; Start-Sleep -Milliseconds 1000; $idleEnd=Tick
       $idleTrace=Read-W14Trace $tracePath; Assert-W14Scenario $idleTrace idle $idleStart $idleEnd $width
       $outcome.idle=@{start=$idleStart;end=$idleEnd}
      }
      $outcome.app_event_counts=@($trace.records | Group-Object event | ForEach-Object { "$($_.Name)=$($_.Count)" })
      $outcome.status='VALID'
     } catch { $outcome.reason=$_.Exception.Message }
     finally {
      $env:ICED_SHELL_INTERACTION_TRACE=$old
      $closed=CloseOwned $p ($outcome.status -eq 'VALID')
      $outcome.close=$closed
      if ($outcome.status -eq 'VALID') {
       try {
        $closedTrace=Read-W14Trace $tracePath
        Assert-W14CleanExit $closedTrace $closed.intentional $closed.exit_code
        $outcome.writer_overhead=$closedTrace.overhead
       } catch { $outcome.status='INVALID'; $outcome.reason=$_.Exception.Message }
      }
      $outcome.ended=Tick
      $outcome | ConvertTo-Json -Depth 12 | Set-Content (Join-Path $runDir 'outcome.json')
      $report.Add($outcome)
     }
    }
   }
  }
 }
} finally {
 $finalHash=(Get-FileHash $ExePath -Algorithm SHA256).Hash.ToLowerInvariant()
 $sourceError=$null; $sourceAfter=$null
 try {
  $sourceAfter=Get-W14SourceIdentity $root $sourceFiles
  $currentFiles=@(
   Get-ChildItem (Join-Path $root 'crates\iced-shell\src') -Recurse -File -Filter '*.rs' | ForEach-Object { $_.FullName.Substring($root.Length+1).Replace('\','/') }
   Get-ChildItem (Join-Path $root 'crates\reader-workload\src') -Recurse -File -Filter '*.rs' | ForEach-Object { $_.FullName.Substring($root.Length+1).Replace('\','/') }
   Get-ChildItem (Join-Path $root 'patches\cosmic-text-0.15.0\src') -Recurse -File -Filter '*.rs' | ForEach-Object { $_.FullName.Substring($root.Length+1).Replace('\','/') }
   $sourceFiles | Where-Object { $_ -notmatch '^(crates/(iced-shell|reader-workload)|patches/cosmic-text-0\.15\.0)/src/' }
  )
  $sourceCurrent=Get-W14SourceIdentity $root $currentFiles
  Assert-W14SourceIdentity $sourceBefore $sourceAfter
  Assert-W14SourceIdentity $sourceBefore $sourceCurrent
 } catch { $sourceError=$_.Exception.Message }
 $series=[ordered]@{schema='iced-w14-series/v1';exe_before=$exeHash;exe_after=$finalHash;source_before=$sourceBefore.sha256;source_after=$(if ($sourceAfter) { $sourceAfter.sha256 } else { $null });source_error=$sourceError;run_count=$report.Count;valid=($exeHash -eq $finalHash -and -not $sourceError -and -not $OnlyRun -and $report.Count -eq 18 -and @($report | Where-Object status -ne 'VALID').Count -eq 0)}
 $series | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $EvidenceDirectory 'series-outcome.json')
 $report | ConvertTo-Json -Depth 10 | Set-Content (Join-Path $EvidenceDirectory 'report.json')
}
if ($sourceError) { throw "W14 source provenance failed: $sourceError" }
if ($finalHash -ne $exeHash) { throw 'release executable changed during series' }
$invalid=@($report | Where-Object status -ne VALID)
if ($invalid.Count) { throw "$($invalid.Count) W14 invalid runs retained in $EvidenceDirectory : $($invalid | ForEach-Object { "$($_.id):$($_.reason)" } | Out-String)" }
if (-not $OnlyRun -and $report.Count -ne 18) { throw "Incomplete W14 matrix: $($report.Count)" }
Write-Host "W14 $($report.Count) valid $(if ($OnlyRun) { 'diagnostic-only' } else { 'independent matrix' }) runs: $EvidenceDirectory"
