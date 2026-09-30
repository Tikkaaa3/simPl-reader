param()
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$run = Join-Path $root ('target/promotion/run-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
$fixtures = Join-Path $run 'fixtures'
$profile = Join-Path $run 'profile'
$frames = Join-Path $run 'full'
$variables = @('LOCALAPPDATA', 'SIMPL_PREVIEW_STORE', 'SIMPL_PROMOTION_FIXTURES', 'SIMPL_PREVIEW_OUTPUT', 'PYTHONUTF8')
$saved = @{}
foreach ($name in $variables) { $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
Push-Location $root
try {
    $env:PYTHONUTF8 = '1'
    python scripts/promotion-fixtures.py $fixtures
    if ($LASTEXITCODE -ne 0) { throw 'Demo fixture creation failed.' }
    $env:LOCALAPPDATA = $profile
    $env:SIMPL_PREVIEW_STORE = $profile
    $env:SIMPL_PROMOTION_FIXTURES = $fixtures
    $env:SIMPL_PREVIEW_OUTPUT = $frames
    cargo test -p iced-shell --release --offline render_promotion_gallery -- --ignored --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw 'Production gallery render failed.' }
    python scripts/promotion-export.py $frames (Join-Path $root 'docs/screenshots') (Join-Path $root 'target/promotion/simPl-website-media.zip')
    if ($LASTEXITCODE -ne 0) { throw 'Media export validation failed.' }
} finally {
    foreach ($name in $variables) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
    Pop-Location
}
