# Deterministic validation for W14 app trace and external observations.
function Read-W14Trace([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw 'missing trace' }
    $lines = @(Get-Content -LiteralPath $Path)
    if ($lines.Count -lt 2 -or $lines[0] -notmatch '^w14/v1,([0-9]+),app,qpc$') { throw 'missing/corrupt trace header or events' }
    $frequency = [long]$Matches[1]
    if ($frequency -le 0 -or $lines.Count -gt 200001) { throw 'invalid frequency or event bound' }
    $records = @(); $overhead=$null
    [long]$previous = 0
    foreach ($line in $lines[1..($lines.Count-1)]) {
        if ($line -match '^overhead,([0-9]+),([0-9]+),([0-9]+)$') {
            if ($line -ne $lines[-1] -or $overhead) { throw 'overhead footer not last or duplicated' }
            $overhead=[pscustomobject]@{total_ticks=[long]$Matches[1];peak_ticks=[long]$Matches[2];count=[long]$Matches[3]}
            continue
        }
        if ($overhead) { throw 'event after overhead footer' }
        if ($line -notmatch '^(wheel|scroll|maximum|viewport|width|resize|view|redraw),([0-9]+),(-?[0-9]+\.[0-9]{3}),([0-9]+)$') { throw "invalid trace event: $line" }
        $eventName = $Matches[1]; [long]$tick = [long]$Matches[2]
        $value = [double]::Parse($Matches[3], [Globalization.CultureInfo]::InvariantCulture)
        [long]$duration = [long]$Matches[4]
        if ($tick -le $previous -or $duration -gt $tick -or [double]::IsInfinity($value) -or [double]::IsNaN($value)) { throw 'repeated/backward QPC or invalid duration/value' }
        $records += [pscustomobject]@{ event=$eventName; ticks=$tick; value=$value; duration_ticks=$duration; source='app' }
        $previous = $tick
    }
    if ($overhead -and ($overhead.count -ne $records.Count -or $overhead.peak_ticks -gt $overhead.total_ticks)) { throw 'inconsistent writer overhead' }
    return [pscustomobject]@{ frequency=$frequency; records=$records; overhead=$overhead }
}
function Get-W14Quantile([long[]]$Values, [int]$Percentile) {
    if (-not $Values -or $Percentile -lt 1 -or $Percentile -gt 100) { return $null }
    $sorted = @($Values | Sort-Object)
    return $sorted[[int][Math]::Ceiling($Percentile * $sorted.Count / 100.0) - 1]
}
function Assert-W14Width($trace, [int]$Width, [long]$Before) {
    $observed=@($trace.records | Where-Object { $_.event -eq 'width' -and $_.ticks -lt $Before })
    if ($observed.Count -eq 0 -or $observed[-1].value -ne $Width) { throw 'intended content width not observed in app trace' }
}
function Assert-W14TargetOwner([IntPtr]$Expected, [IntPtr]$Actual) {
    if ($Expected -eq [IntPtr]::Zero -or $Actual -ne $Expected) { throw 'actual input coordinate is occluded or unowned' }
}
function Assert-W14CaptureOwner([IntPtr]$Expected, [IntPtr]$CaptureRoot) {
    if ($Expected -eq [IntPtr]::Zero -or $CaptureRoot -ne $Expected) { throw 'mouse capture lost or foreign during thumb drag' }
}
function Assert-W14CleanExit($trace, [bool]$IntentionalClose, [int]$ExitCode) {
    if (-not $IntentionalClose -or $ExitCode -ne 0 -or -not $trace.overhead) { throw 'missing intentional zero-exit and complete trace footer' }
}
function Assert-W14Scenario($trace, [string]$Kind, [long]$Start, [long]$End, [int]$Width) {
    if ($End -le $Start -or $Width -notin @(480,800)) { throw 'invalid scenario window/width' }
    $events = @($trace.records | Where-Object { $_.ticks -ge $Start -and $_.ticks -le $End })
    if ($Kind -eq 'scroll') {
        $wheels = @($events | Where-Object event -eq wheel)
        $offsets = @($events | Where-Object event -eq scroll | ForEach-Object value)
        $maxima = @($events | Where-Object event -eq maximum | ForEach-Object value)
        if ($wheels.Count -lt 10 -or $offsets.Count -lt 10 -or $maxima.Count -ne $offsets.Count -or (($offsets | Measure-Object -Maximum).Maximum - $offsets[0]) -lt 250) { throw 'no proven active scroll' }
        if (($offsets | Measure-Object -Maximum).Maximum - $offsets[-1] -lt 100) { throw 'no proven reverse scroll' }
        $reachedEnd=$false
        for ($i=0; $i -lt $offsets.Count; $i++) {
            if ($maxima[$i] -ge 50000 -and [Math]::Abs($maxima[$i] - $offsets[$i]) -le 150) { $reachedEnd=$true; break }
        }
        if (-not $reachedEnd) { throw 'end of document not proven' }
        if ($offsets[0] -gt 120 -or $offsets[-1] -gt 50) { throw 'native scroll did not return near top' }
    } elseif ($Kind -eq 'resize') {
        $sizes = @($events | Where-Object event -eq resize | ForEach-Object value)
        $viewports = @($events | Where-Object event -eq viewport | ForEach-Object value)
        if ($sizes.Count -lt 2 -or $viewports.Count -lt 2 -or ($viewports | Measure-Object -Minimum).Minimum -gt 500 -or
            -not @($viewports | Where-Object { $_ -ge 599.5 -and $_ -le 600.5 }).Count) { throw 'short and 600-DIP capped viewport not proven' }
    } elseif ($Kind -eq 'idle') {
        if (@($events | Where-Object { $_.event -in @('wheel','scroll','resize','view','redraw') }).Count) { throw 'idle received input or movement' }
    } else { throw 'unknown scenario' }
}
