$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'interaction-trace.ps1')
$path = Join-Path $env:TEMP ('w14-validator-' + [guid]::NewGuid().ToString('N'))
function Reject([scriptblock]$operation) {
    $rejected = $false
    try { & $operation } catch { $rejected = $true }
    if (-not $rejected) { throw 'invalid input was accepted' }
}
try {
    Reject { Read-W14Trace $path }
    'w14/v1,10000,app,qpc' | Set-Content $path
    Reject { Read-W14Trace $path }
    @('w14/v1,10000,app,qpc','wheel,100,0.000,0','scroll,100,20.000,3') | Set-Content $path
    Reject { Read-W14Trace $path }
    @('w14/v1,10000,app,qpc','present,100,0.000,0') | Set-Content $path
    Reject { Read-W14Trace $path }
    @('w14/v1,10000,app,qpc','wheel,100,0.000,0','scroll,110,500.000,2','view,120,0.000,8','view,170,0.000,9') | Set-Content $path
    $trace = Read-W14Trace $path
    if ($trace.records.Count -ne 4 -or (Get-W14Quantile @(2,5,9,11,40) 95) -ne 40 -or (Get-W14Quantile @() 50)) { throw 'validator/nearest-rank mismatch' }
    Reject { Assert-W14Scenario $trace scroll 99 171 800 }
    Reject { Assert-W14Scenario $trace resize 99 171 480 }
    Reject { Assert-W14Scenario $trace idle 99 171 800 }
    $validWidth = [pscustomobject]@{records=@([pscustomobject]@{event='width';ticks=90;value=800})}
    Assert-W14Width $validWidth 800 172
    Reject { Assert-W14Width $validWidth 480 172 }
    $scrollEvents=@([pscustomobject]@{event='scroll';ticks=100;value=0},[pscustomobject]@{event='maximum';ticks=101;value=90000})
    foreach ($i in 1..12) { $scrollEvents+=@([pscustomobject]@{event='wheel';ticks=(100+$i*10);value=0},[pscustomobject]@{event='scroll';ticks=(101+$i*10);value=($i*500)},[pscustomobject]@{event='maximum';ticks=(102+$i*10);value=90000}) }
    $scrollEvents+=@([pscustomobject]@{event='scroll';ticks=300;value=89990},[pscustomobject]@{event='maximum';ticks=301;value=90000},[pscustomobject]@{event='scroll';ticks=310;value=88000},[pscustomobject]@{event='maximum';ticks=311;value=90000})
    Reject { Assert-W14Scenario ([pscustomobject]@{records=$scrollEvents}) scroll 90 400 800 }
    $returned=$scrollEvents+@([pscustomobject]@{event='scroll';ticks=320;value=0},[pscustomobject]@{event='maximum';ticks=321;value=90000})
    Assert-W14Scenario ([pscustomobject]@{records=$returned}) scroll 90 400 800
    $resizeEvents=@([pscustomobject]@{event='resize';ticks=100;value=720},[pscustomobject]@{event='viewport';ticks=101;value=563.8},[pscustomobject]@{event='resize';ticks=110;value=520},[pscustomobject]@{event='viewport';ticks=111;value=363.8})
    Reject { Assert-W14Scenario ([pscustomobject]@{records=$resizeEvents}) resize 90 200 800 }
    Assert-W14Scenario ([pscustomobject]@{records=($resizeEvents+@([pscustomobject]@{event='resize';ticks=120;value=900},[pscustomobject]@{event='viewport';ticks=121;value=600}))}) resize 90 200 800
    Reject { Assert-W14Scenario ([pscustomobject]@{records=@([pscustomobject]@{event='view';ticks=130;value=0})}) idle 100 200 800 }
    Assert-W14TargetOwner ([IntPtr]31) ([IntPtr]31)
    Reject { Assert-W14TargetOwner ([IntPtr]31) ([IntPtr]29) }
    Reject { Assert-W14TargetOwner ([IntPtr]31) ([IntPtr]::Zero) }
    Assert-W14CaptureOwner ([IntPtr]31) ([IntPtr]31)
    Reject { Assert-W14CaptureOwner ([IntPtr]31) ([IntPtr]::Zero) }
    Reject { Assert-W14CaptureOwner ([IntPtr]31) ([IntPtr]45) }
    $footer=[pscustomobject]@{overhead=[pscustomobject]@{count=3}}
    Reject { Assert-W14CleanExit $footer $false 0 }
    Reject { Assert-W14CleanExit $footer $true 101 }
    Assert-W14CleanExit $footer $true 0
    @('w14/v1,10000,app,qpc','wheel,100,0.000,0','scroll,110,0.000,2','maximum,111,90000.000,0','overhead,60,25,3') | Set-Content $path
    $trace=Read-W14Trace $path
    if ($trace.overhead.count -ne 3) { throw 'valid overhead footer lost' }
    @('w14/v1,10000,app,qpc','wheel,100,0.000,0','overhead,60,25,4') | Set-Content $path
    Reject { Read-W14Trace $path }
    @('w14/v1,10000,app,qpc','wheel,100,0.000,0','overhead,60,25,1','wheel,120,0.000,0') | Set-Content $path
    Reject { Read-W14Trace $path }
    Write-Host 'W14 invalid/missing/monotonic/quantile/scenario controls PASS'
} finally { Remove-Item $path -ErrorAction SilentlyContinue }
