$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'interaction-source.ps1')
$root=Join-Path $env:TEMP ('w14-source-test-' + [guid]::NewGuid().ToString('N'))
function Reject([scriptblock]$case) {
    $failed=$false
    try { & $case } catch { $failed=$true }
    if (-not $failed) { throw 'source identity negative case accepted' }
}
try {
    New-Item -ItemType Directory -Path $root | Out-Null
    'one' | Set-Content (Join-Path $root 'a.rs')
    'two' | Set-Content (Join-Path $root 'b.ps1')
    $first=Get-W14SourceIdentity $root @('a.rs','b.ps1')
    $again=Get-W14SourceIdentity $root @('b.ps1','a.rs')
    if ($first.sha256 -ne $again.sha256 -or $first.files.Count -ne 2) { throw 'unstable source identity' }
    Assert-W14SourceIdentity $first $again
    'changed' | Set-Content (Join-Path $root 'a.rs')
    Reject { Assert-W14SourceIdentity $first (Get-W14SourceIdentity $root @('a.rs','b.ps1')) }
    Reject { Get-W14SourceIdentity $root @('a.rs','missing.rs') }
    Reject { Get-W14SourceIdentity $root @('..\elsewhere.rs') }
    Write-Host 'W14 source manifest identity/changed/missing/outside controls PASS'
} finally { Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue }
