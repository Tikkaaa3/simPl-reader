# Windows PowerShell 5.1+. Cargo runner for Android test executables: copy one to
# the connected device or emulator and run it there with the test arguments.
# Set up by `scripts\android.ps1 test`; native libraries the tests load by name
# (libpdfium.so) are pushed beside the executables beforehand.
param(
    [Parameter(Mandatory = $true, Position = 0)][string]$Executable,
    [Parameter(ValueFromRemainingArguments = $true)][string[]]$TestArguments
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$adb = $env:SIMPL_ADB
if (-not $adb) { throw 'SIMPL_ADB is not set; run tests through scripts\android.ps1 test.' }
$remote = '/data/local/tmp/simpl-test'
$name = Split-Path -Leaf $Executable

function Quote([string]$Value) { "'" + $Value.Replace("'", "'\''") + "'" }

$ErrorActionPreference = 'Continue'
& $adb push $Executable "$remote/$name" | Out-Null
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$arguments = @($TestArguments | Where-Object { $null -ne $_ } | ForEach-Object { Quote $_ }) -join ' '
# Tests write scratch files below TMPDIR (std::env::temp_dir) and find libraries beside themselves.
$command = "cd $remote && chmod 755 $name && mkdir -p tmp && " +
    "TMPDIR=$remote/tmp LD_LIBRARY_PATH=$remote ./$name $arguments; status=`$?; rm -f $name; exit `$status"
& $adb shell $command
exit $LASTEXITCODE
