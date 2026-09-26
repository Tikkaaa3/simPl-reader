# W14 reproducible source-file identity. Bytes are captured, not build attestation.
function Get-W14SourceIdentity([string]$Root, [string[]]$RelativePaths) {
    if (-not $RelativePaths -or -not (Test-Path -LiteralPath $Root -PathType Container)) { throw 'missing source root or file set' }
    $base=[IO.Path]::GetFullPath($Root)
    $files=@()
    foreach ($path in @($RelativePaths | ForEach-Object { $_.Replace('\','/') } | Sort-Object -Unique)) {
        if ([IO.Path]::IsPathRooted($path) -or $path -match '(?:^|/)\.\.(?:/|$)' -or $path -match '(^|/)\.(?:/|$)' -or $path -match '^[A-Za-z]:') {
            throw "unsafe source path: $path"
        }
        $absolute=[IO.Path]::GetFullPath((Join-Path $base $path))
        if (-not $absolute.StartsWith($base + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase) -or
            -not (Test-Path -LiteralPath $absolute -PathType Leaf)) { throw "missing/outside source file: $path" }
        $files+=@([pscustomobject]@{path=$path;sha256=(Get-FileHash -LiteralPath $absolute -Algorithm SHA256).Hash.ToLowerInvariant();bytes=(Get-Item -LiteralPath $absolute).Length})
    }
    if (-not $files.Count) { throw 'empty source file set' }
    $canonical=(($files | ForEach-Object { "$($_.path) $($_.sha256)`n" }) -join '')
    $sha=[Security.Cryptography.SHA256]::Create()
    try { $digest=([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($canonical)))).Replace('-','').ToLowerInvariant() }
    finally { $sha.Dispose() }
    return [pscustomobject]@{schema='iced-w14-source/v1';sha256=$digest;files=$files}
}
function Assert-W14SourceIdentity($Before,$After) {
    if ($Before.schema -ne 'iced-w14-source/v1' -or $After.schema -ne $Before.schema -or
        $Before.files.Count -ne $After.files.Count -or $Before.sha256 -ne $After.sha256) { throw 'source bytes/file set changed during W14 series' }
}
