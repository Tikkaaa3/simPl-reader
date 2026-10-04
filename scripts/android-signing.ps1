# Keep the distribution identity outside the checkout. Local credentials use Windows DPAPI.
param(
    [ValidateSet('initialize', 'load', 'github')][string]$Command = 'load',
    [string]$Directory = (Join-Path $env:LOCALAPPDATA 'simPl\android-signing')
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot)).TrimEnd('\') + '\'
$Directory = [IO.Path]::GetFullPath($Directory)
if (($Directory.TrimEnd('\') + '\').StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'The release keystore must be outside the repository.'
}
$keystore = Join-Path $Directory 'release.jks'
$credentials = Join-Path $Directory 'release-credentials.xml'
if ($Command -eq 'initialize') {
    if ((Test-Path -LiteralPath $keystore) -or (Test-Path -LiteralPath $credentials)) {
        throw 'Signing identity already exists. Load it instead of replacing the update key.'
    }
    if (-not $env:JAVA_HOME) { throw 'Set JAVA_HOME to JDK 21.' }
    New-Item -ItemType Directory -Path $Directory -Force | Out-Null
    $random = New-Object byte[] 48
    $generator = [Security.Cryptography.RandomNumberGenerator]::Create()
    try { $generator.GetBytes($random) } finally { $generator.Dispose() }
    $password = [Convert]::ToBase64String($random)
    $env:SIMPL_ANDROID_STORE_PASSWORD = $password
    $env:SIMPL_ANDROID_KEY_PASSWORD = $password
    $env:SIMPL_ANDROID_KEY_ALIAS = 'simpl'
    & (Join-Path $env:JAVA_HOME 'bin\keytool.exe') -genkeypair -keystore $keystore -storetype PKCS12 `
        -storepass:env SIMPL_ANDROID_STORE_PASSWORD -keypass:env SIMPL_ANDROID_KEY_PASSWORD `
        -alias simpl -keyalg RSA -keysize 4096 -validity 10000 -dname 'CN=simPl Reader, O=Tikkaaa3'
    if ($LASTEXITCODE -ne 0) { throw 'Could not create the distribution keystore.' }
    New-Object Management.Automation.PSCredential('simpl', (ConvertTo-SecureString $password -AsPlainText -Force)) |
        Export-Clixml -LiteralPath $credentials
    Write-Host "Created Android signing identity outside the repository: $Directory"
} else {
    $names = @('SIMPL_ANDROID_KEYSTORE', 'SIMPL_ANDROID_STORE_PASSWORD', 'SIMPL_ANDROID_KEY_ALIAS', 'SIMPL_ANDROID_KEY_PASSWORD')
    $provided = @($names | Where-Object { [Environment]::GetEnvironmentVariable($_, 'Process') })
    if ($provided.Count -eq 4) {
        $keystore = [IO.Path]::GetFullPath($env:SIMPL_ANDROID_KEYSTORE)
        if ($keystore.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) { throw 'The release keystore must be outside the repository.' }
        if (-not (Test-Path -LiteralPath $keystore -PathType Leaf)) { throw 'The provided keystore does not exist.' }
        if ($Command -eq 'load') { return }
    } else {
        if ($provided.Count -ne 0) { throw 'Supply all four SIMPL_ANDROID signing variables together.' }
        if (-not (Test-Path -LiteralPath $keystore -PathType Leaf) -or -not (Test-Path -LiteralPath $credentials -PathType Leaf)) {
            throw 'No local signing identity. Run scripts/android-signing.ps1 initialize or supply the signing environment variables.'
        }
        $identity = Import-Clixml -LiteralPath $credentials
        $env:SIMPL_ANDROID_STORE_PASSWORD = $identity.GetNetworkCredential().Password
        $env:SIMPL_ANDROID_KEY_PASSWORD = $env:SIMPL_ANDROID_STORE_PASSWORD
        $env:SIMPL_ANDROID_KEY_ALIAS = $identity.UserName
    }
}
$env:SIMPL_ANDROID_KEYSTORE = $keystore
if ($Command -eq 'github') {
    Push-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
    try {
        $repository = & gh repo view --json nameWithOwner --jq .nameWithOwner
        if ($LASTEXITCODE -ne 0 -or -not $repository) { throw 'Sign in with gh auth login for this repository.' }
        $secrets = @{
            SIMPL_ANDROID_KEYSTORE_BASE64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($keystore))
            SIMPL_ANDROID_STORE_PASSWORD = $env:SIMPL_ANDROID_STORE_PASSWORD
            SIMPL_ANDROID_KEY_ALIAS = $env:SIMPL_ANDROID_KEY_ALIAS
            SIMPL_ANDROID_KEY_PASSWORD = $env:SIMPL_ANDROID_KEY_PASSWORD
        }
        foreach ($name in $secrets.Keys) {
            $secrets[$name] | & gh secret set $name --repo $repository
            if ($LASTEXITCODE -ne 0) { throw "Could not configure GitHub secret $name." }
        }
        Write-Host "Configured the four Android signing secrets for $repository."
    } finally { Pop-Location }
}
