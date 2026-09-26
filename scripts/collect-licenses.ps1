# Copy notices for the shipped Windows x64 release binary of the simPl reader.
# Notices come from the crate package itself when it ships one, otherwise from the exact
# upstream revision recorded in the package provenance; fetched texts are cached under
# target/native/rust-licenses so later runs (including -Offline) reuse them.
param(
    [Parameter(Mandatory = $true)][string]$Destination,
    [switch]$Offline
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$target = 'x86_64-pc-windows-msvc'
$cacheRoot = Join-Path $root 'target\native\rust-licenses'
$candidateNames = @(
    'LICENSE', 'LICENCE', 'LICENSE.md', 'LICENSE.txt', 'LICENCE.md',
    'LICENSE-MIT', 'LICENSE-MIT.md', 'LICENSE-APACHE', 'LICENSE-APACHE.md', 'LICENSE-APACHE-2.0',
    'COPYING', 'COPYING.md', 'COPYRIGHT', 'NOTICE'
)
# Legal texts are files named LICENSE*/LICENCE*/COPYING*/COPYRIGHT*/NOTICE*/AUTHORS*/PATENTS*.
function Test-LegalFileName([string]$Name) {
    return $Name -match '^(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE|AUTHORS|PATENTS)([-._].*)?$'
}

# Narrow, explicit mapping for shipped crates that publish no license file at all. Each entry
# names the declared license option being exercised, the canonical text source for it, and the
# substrings that prove the fetched text really is that license. This never weakens the general
# detection path: crates absent from this table still fail when no text can be collected.
$declaredLicenseSources = @{
    'mac|0.1.1' = [pscustomobject]@{
        License    = 'Apache-2.0'
        TextName   = 'Apache-2.0.txt'
        TextUrl    = 'https://www.apache.org/licenses/LICENSE-2.0.txt'
        MustContain = @('Apache License', 'Version 2.0, January 2004')
        Note       = 'The crate declares MIT/Apache-2.0 and ships no license file in the crate ' +
                     'package or in its repository; the Apache License 2.0 option is exercised here.'
    }
}

function Get-Field($Object, [string]$Name) {
    if ($null -eq $Object) { return $null }
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) { return $null }
    return $property.Value
}

# Quote the upstream declaration and attribution verbatim, so the canonical license text is not
# presented as if the crate had shipped it, and no copyright details are invented.
function Write-LicenseDeclaration([string]$PackageSource, [string]$Name, [string]$Version, [string]$Declared, $Mapping, [string]$Path) {
    $manifest = Join-Path $PackageSource 'Cargo.toml'
    $readme = Join-Path $PackageSource 'README.md'
    $lines = New-Object 'System.Collections.Generic.List[string]'
    $lines.Add("Verbatim license declaration and attribution from the $Name $Version crate package.")
    $lines.Add('')
    $lines.Add("Declared license identifiers: $Declared")
    $lines.Add("License text included beside this file: $($Mapping.TextName), unmodified, from $($Mapping.TextUrl)")
    $lines.Add($Mapping.Note)
    $lines.Add('That text is not attributed to the crate, and no copyright year is asserted here beyond')
    $lines.Add('what the crate metadata states.')
    $lines.Add('')
    if (Test-Path -LiteralPath $manifest -PathType Leaf) {
        $hash = (Get-FileHash -LiteralPath $manifest -Algorithm SHA256).Hash
        $lines.Add("From the crate package Cargo.toml (sha256 $hash), verbatim:")
        foreach ($line in @(Get-Content -LiteralPath $manifest | Where-Object { $_ -match '^\s*(license|license-file|authors)\s*=' })) {
            $lines.Add('    ' + $line.Trim())
        }
        $lines.Add('')
    }
    if (Test-Path -LiteralPath $readme -PathType Leaf) {
        $hash = (Get-FileHash -LiteralPath $readme -Algorithm SHA256).Hash
        $content = @(Get-Content -LiteralPath $readme)
        $start = -1
        for ($i = 0; $i -lt $content.Count; $i++) {
            if ($content[$i] -match '^#{1,6}\s*licen[cs]e\b') { $start = $i; break }
        }
        if ($start -ge 0) {
            $end = $content.Count
            for ($i = $start + 1; $i -lt $content.Count; $i++) {
                if ($content[$i] -match '^#{1,6}\s') { $end = $i; break }
            }
            $lines.Add("From the crate package README.md (sha256 $hash), verbatim section:")
            foreach ($line in $content[$start..($end - 1)]) { $lines.Add('    ' + $line) }
        }
    }
    while ($lines.Count -gt 0 -and -not $lines[$lines.Count - 1].Trim()) { $lines.RemoveAt($lines.Count - 1) }
    [IO.File]::WriteAllLines($Path, $lines, (New-Object Text.UTF8Encoding($false)))
}

function Get-UpstreamText([string]$Url) {
    try {
        $response = Invoke-WebRequest -Uri $Url -UseBasicParsing -Headers @{ 'User-Agent' = 'simPl-license-collector' }
    } catch {
        $status = $null
        $webResponse = Get-Field $_.Exception 'Response'
        if ($webResponse) { $status = [int]$webResponse.StatusCode }
        # A missing candidate file is expected while probing; anything else is a real failure.
        if ($status -eq 404) { return $null }
        throw "Could not fetch $Url (status $status): $($_.Exception.Message)"
    }
    if ($response.StatusCode -ne 200) { return $null }
    return [string]$response.Content
}

# Owner/repo slug and pinned revision for the exact source that was compiled.
function Resolve-Upstream([string]$Repository, [string]$Version, $VcsInfo) {
    $normalized = $Repository -replace '^git\+', '' -replace '\.git$', '' -replace '/$', ''
    if ($normalized -notmatch '^https?://github\.com/(?<owner>[^/]+)/(?<repo>[^/]+)$') {
        return [pscustomobject]@{ Slug = $null; Revision = $null; Note = $null }
    }
    $slug = $Matches['owner'] + '/' + $Matches['repo']
    $revision = $null
    $note = $null
    $git = Get-Field $VcsInfo 'git'
    if ($git) {
        $revision = Get-Field $git 'sha1'
        if ($revision) { $note = "github.com/$slug commit $revision (crate package provenance)" }
    }
    if (-not $revision) {
        # Old packages predate .cargo_vcs_info.json; the release tag for this version is the
        # closest pinned provenance and is recorded together with its resolved commit.
        $tags = Get-UpstreamText "https://api.github.com/repos/$slug/tags?per_page=100"
        $tagList = @()
        if ($tags) { $tagList = @($tags | ConvertFrom-Json) }
        foreach ($tag in $tagList) {
            $name = Get-Field $tag 'name'
            if ($name -eq $Version -or $name -eq ('v' + $Version)) {
                $revision = Get-Field (Get-Field $tag 'commit') 'sha'
                $note = "github.com/$slug tag $name ($revision)"
                break
            }
        }
    }
    return [pscustomobject]@{ Slug = $slug; Revision = $revision; Note = $note }
}

Push-Location -LiteralPath $root
try {
    # Resolve through rustup from the repository root, where rust-toolchain.toml is in force.
    $sysrootLines = @(& rustc --print sysroot)
    if ($LASTEXITCODE -ne 0 -or $sysrootLines.Count -ne 1) {
        throw 'Could not resolve the pinned Rust toolchain sysroot.'
    }
    $sysroot = $sysrootLines[0].Trim()
    $toolchainInfo = @(& rustc --version --verbose)
    if ($LASTEXITCODE -ne 0 -or $toolchainInfo.Count -eq 0) {
        throw 'Could not identify the pinned Rust toolchain version.'
    }
    $pinFile = Join-Path $root 'rust-toolchain.toml'
    if (-not (Test-Path -LiteralPath $pinFile -PathType Leaf)) { throw "Missing Rust toolchain pin: $pinFile" }
    $pin = Get-Content -LiteralPath $pinFile -Raw
    if ($pin -notmatch '(?m)^\s*channel\s*=\s*"([^"]+)"\s*$') { throw 'Rust toolchain pin has no channel.' }
    $channel = $Matches[1]
    if ((Split-Path -Leaf $sysroot) -ne $channel) {
        throw "Rust sysroot $sysroot does not match pinned channel $channel."
    }
    if (-not (@($toolchainInfo | Where-Object { $_ -eq "host: $target" }).Count -gt 0) -or
        -not (@($toolchainInfo | Where-Object { $_ -match '^commit-hash: [0-9a-f]{40}$' }).Count -eq 1)) {
        throw "Rust toolchain is missing the $target host or commit provenance."
    }
    $json = & cargo metadata --offline --locked --format-version 1 | Out-String
    if ($LASTEXITCODE -ne 0 -or -not $json) { throw 'Cargo metadata failed; build or fetch dependencies before packaging.' }
    $metadata = $json | ConvertFrom-Json
    # Cargo metadata's resolve graph includes optional dependencies that the release features
    # do not activate, so the shipped package set comes from the actual feature-resolved tree.
    $treeLines = @(& cargo tree -p iced-shell --offline --locked --target $target -e normal,build --prefix none --format '{p}`{l}`{r}')
    if ($LASTEXITCODE -ne 0) { throw 'Cargo tree failed; cannot determine the shipped dependency set.' }
} finally {
    Pop-Location
}

$packages = @{}
foreach ($package in $metadata.packages) {
    $key = $package.name + '|' + $package.version
    if (-not $packages.ContainsKey($key)) { $packages[$key] = New-Object 'System.Collections.Generic.List[object]' }
    $packages[$key].Add($package)
}
$workspace = @{}
foreach ($id in $metadata.workspace_members) { $workspace[$id] = $true }

$shipped = [ordered]@{}
foreach ($line in $treeLines) {
    if (-not $line) { continue }
    $fields = $line -split '`'
    if ($fields.Count -lt 3) { throw "Unrecognized cargo tree line: $line" }
    # A trailing " (*)" marks a package already shown earlier in the tree output.
    for ($i = 0; $i -lt $fields.Count; $i++) { $fields[$i] = $fields[$i] -replace ' \(\*\)$', '' }
    $spec = $fields[0]
    if ($spec -notmatch '^(?<name>\S+) v(?<version>\S+?)(?: \([^)]*\))?$') { throw "Unrecognized cargo tree package: $spec" }
    $key = $Matches['name'] + '|' + $Matches['version']
    $shipped[$key] = [pscustomobject]@{
        Name       = $Matches['name']
        Version    = $Matches['version']
        License    = $fields[1]
        Repository = $fields[2]
    }
}
if ($shipped.Count -eq 0) { throw 'Cargo tree returned no shipped dependencies.' }

$rust = Join-Path $Destination 'rust'
New-Item -ItemType Directory -Path $rust -Force | Out-Null
$inventory = New-Object 'System.Collections.Generic.List[string]'
$inventory.Add('Third-party Rust notices for the Windows x64 release build of the simPl reader.')
$inventory.Add('Generated from cargo tree -p iced-shell -e normal,build --target ' + $target + ' (locked, offline).')
$inventory.Add('The application license itself is unspecified and is not asserted by this file.')
$inventory.Add('Legal texts are in rust/<crate>-<version>/. Fields: <crate> <version> | declared license | source | notice provenance.')
$inventory.Add('Linked Rust standard-library notices: rust-standard-library/COPYRIGHT-library.html;')
$inventory.Add('its pinned toolchain commit and source hash are in rust-standard-library/TOOLCHAIN-PROVENANCE.txt.')
$inventory.Add('Provenance values: "crate package" (text shipped inside the crate archive),')
$inventory.Add('"upstream <source>" (text pinned from the exact compiled revision or from the declared-license')
$inventory.Add('source recorded in scripts/collect-licenses.ps1 and cached in target/native/rust-licenses for')
$inventory.Add('offline reuse), or "local provisioning" (owner-supplied files).')
$workspaceNames = [string[]]@($metadata.packages | Where-Object { $workspace.ContainsKey($_.id) } |
    ForEach-Object { $_.name + ' ' + $_.version })
# Ordinal ordering keeps the inventory byte-identical across PowerShell/.NET hosts.
[Array]::Sort($workspaceNames, [System.StringComparer]::Ordinal)
$inventory.Add('Project-authored workspace packages (not third-party): ' + ($workspaceNames -join ', '))
$inventory.Add('')
$collected = 0
$unresolved = New-Object 'System.Collections.Generic.List[string]'
$orderedKeys = [string[]]@($shipped.Keys)
[Array]::Sort($orderedKeys, [System.StringComparer]::Ordinal)

foreach ($key in $orderedKeys) {
    $entry = $shipped[$key]
    $candidates = $packages[$key].ToArray()
    # A [patch] path override and the upstream crate can share name and version; the patched
    # local source is the one that is compiled and therefore the one whose notices are shipped.
    $local = @($candidates | Where-Object { -not (Get-Field $_ 'source') })
    $package = if ($local.Count -eq 1) { $local[0] } elseif ($candidates.Count -eq 1) { $candidates[0] } else {
        throw "Ambiguous dependency package for notices: $($entry.Name) $($entry.Version)"
    }
    # Project-authored code is covered (or not) by the application license, not by third-party notices.
    if ($workspace.ContainsKey($package.id)) { continue }
    $collected++
    $source = Split-Path -Parent $package.manifest_path
    if (-not (Test-Path -LiteralPath $source -PathType Container)) {
        throw "Dependency sources are not extracted locally, cannot collect notices: $source"
    }
    $licenseId = Get-Field $package 'license'
    $licenseFile = Get-Field $package 'license_file'
    $legal = Join-Path $rust ($entry.Name + '-' + $entry.Version)
    $files = [ordered]@{}
    foreach ($file in @(Get-ChildItem -LiteralPath $source -File)) {
        if (Test-LegalFileName $file.Name) { $files[$file.FullName] = $file.Name }
    }
    if ($licenseFile) {
        $declared = Join-Path $source $licenseFile
        if (-not (Test-Path -LiteralPath $declared -PathType Leaf)) { throw "Missing declared license file: $declared" }
        if (-not $files.Contains($declared)) { $files[$declared] = [IO.Path]::GetFileName($declared) }
    }
    $provenance = 'crate package'
    $extraDirs = @('LICENSES', 'licenses')

    if ($files.Count -eq 0 -and -not (Test-Path -LiteralPath (Join-Path $source 'LICENSES'))) {
        # The crate package ships no notice text.
        $repository = Get-Field $package 'repository'
        if (-not $repository) { $repository = $entry.Repository }
        $cache = Join-Path $cacheRoot ($entry.Name + '-' + $entry.Version)
        $mapping = if ($declaredLicenseSources.ContainsKey($key)) { $declaredLicenseSources[$key] } else { $null }
        $declaration = Join-Path $cache 'LICENSE-DECLARATION.txt'
        $cacheFiles = @()
        if (Test-Path -LiteralPath $cache -PathType Container) {
            # The generated declaration is recreated below, so it is not treated as cached text.
            $cacheFiles = @(Get-ChildItem -LiteralPath $cache -File |
                Where-Object { (Test-LegalFileName $_.Name) -and $_.Name -ne 'LICENSE-DECLARATION.txt' })
        }
        $canonicalPresent = $true
        if ($mapping) {
            # The declared-license text is named after the mapping, so it needs its own check.
            $cachedCanonical = Join-Path $cache $mapping.TextName
            if (Test-Path -LiteralPath $cachedCanonical -PathType Leaf) {
                $cacheFiles += Get-Item -LiteralPath $cachedCanonical
            } else {
                $canonicalPresent = $false
            }
        }
        if ($cacheFiles.Count -gt 0) {
            if (-not $canonicalPresent) {
                $unresolved.Add("$($entry.Name) $($entry.Version): declared-license text $($mapping.TextName) is not cached (expected in target/native/rust-licenses/$($entry.Name)-$($entry.Version)/$($mapping.TextName))")
            } else {
                # The declaration is regenerated from the crate package, so it always matches the build.
                if ($mapping) {
                    Write-LicenseDeclaration $source $entry.Name $entry.Version $licenseId $mapping $declaration
                    $cacheFiles += Get-Item -LiteralPath $declaration
                }
                # Label provenance from the cache manifest so online and offline runs agree.
                $provenanceFile = Join-Path $cache 'PROVENANCE.txt'
                $sourceLines = @()
                if (Test-Path -LiteralPath $provenanceFile -PathType Leaf) {
                    $sourceLines = @(Get-Content -LiteralPath $provenanceFile | Where-Object { $_.StartsWith('# Source: ') })
                }
                $provenance = if ($sourceLines.Count -gt 0) {
                    'upstream ' + $sourceLines[0].Substring('# Source: '.Length)
                } else {
                    'local provisioning (target/native/rust-licenses/' + $entry.Name + '-' + $entry.Version + ')'
                }
            }
        } elseif ($Offline) {
            # Offline mode never touches the network: provision the text or fail loudly.
            $expected = "target/native/rust-licenses/$($entry.Name)-$($entry.Version)/"
            if ($mapping) { $expected += " including $($mapping.TextName)" }
            $unresolved.Add("$($entry.Name) $($entry.Version): notices are not cached for offline use (expected in $expected)")
        } else {
            $vcsFile = Join-Path $source '.cargo_vcs_info.json'
            $vcsInfo = if (Test-Path -LiteralPath $vcsFile -PathType Leaf) { (Get-Content -LiteralPath $vcsFile -Raw) | ConvertFrom-Json } else { $null }
            $upstream = Resolve-Upstream $repository $entry.Version $vcsInfo
            $fetched = [ordered]@{}
            $sourceNote = $null
            if ($upstream.Slug -and $upstream.Revision) {
                $prefixes = @('')
                $pathInVcs = Get-Field $vcsInfo 'path_in_vcs'
                if ($pathInVcs) { $prefixes += ($pathInVcs.Trim('/') + '/') }
                foreach ($prefix in $prefixes) {
                    foreach ($name in $candidateNames) {
                        $url = "https://raw.githubusercontent.com/$($upstream.Slug)/$($upstream.Revision)/$prefix$name"
                        $text = Get-UpstreamText $url
                        if (-not $text) { continue }
                        if ($text.Length -lt 100 -or $text -match '(?i)<html') { throw "Suspicious notice content from $url" }
                        $fetched[$name] = [pscustomobject]@{ Url = $url; Text = $text }
                    }
                    # Notices normally live at the repository root; only search the crate
                    # subdirectory when the root holds nothing.
                    if ($fetched.Count -gt 0) { break }
                }
                if ($fetched.Count -gt 0) { $sourceNote = $upstream.Note }
            } elseif (-not $mapping) {
                $unresolved.Add("$($entry.Name) $($entry.Version): no upstream repository or revision could be resolved (repository: $repository)")
            }
            if ($fetched.Count -eq 0 -and $mapping) {
                # The declared-license fallback: canonical text for the declared option, plus the
                # verbatim upstream declaration and attribution quoted from the crate package.
                $text = Get-UpstreamText $mapping.TextUrl
                if (-not $text) { throw "Could not fetch the declared license text $($mapping.TextUrl) for $($entry.Name) $($entry.Version)." }
                foreach ($required in $mapping.MustContain) {
                    if ($text -notlike ('*' + $required + '*')) { throw "Fetched text from $($mapping.TextUrl) is not $($mapping.License): missing '$required'." }
                }
                $fetched[$mapping.TextName] = [pscustomobject]@{ Url = $mapping.TextUrl; Text = $text }
                $sourceNote = "$($mapping.License) text from $($mapping.TextUrl) (crate declares $licenseId and ships no license file)"
            } elseif ($fetched.Count -eq 0 -and $upstream.Slug -and $upstream.Revision) {
                $unresolved.Add("$($entry.Name) $($entry.Version): no license file exists at $($upstream.Note)")
            }
            if ($fetched.Count -gt 0) {
                New-Item -ItemType Directory -Path $cache -Force | Out-Null
                $manifest = New-Object 'System.Collections.Generic.List[string]'
                $manifest.Add("# Notices for $($entry.Name) $($entry.Version) (declared: $licenseId)")
                $manifest.Add("# Source: $sourceNote")
                foreach ($name in $fetched.Keys) {
                    $cachePath = Join-Path $cache $name
                    [IO.File]::WriteAllText($cachePath, $fetched[$name].Text, (New-Object Text.UTF8Encoding($false)))
                    $hash = (Get-FileHash -LiteralPath $cachePath -Algorithm SHA256).Hash
                    $manifest.Add("$name <- $($fetched[$name].Url) sha256 $hash")
                    $cacheFiles += Get-Item -LiteralPath $cachePath
                }
                if ($mapping) {
                    Write-LicenseDeclaration $source $entry.Name $entry.Version $licenseId $mapping $declaration
                    $manifest.Add("LICENSE-DECLARATION.txt <- quoted from the $($entry.Name) $($entry.Version) crate package")
                    $cacheFiles += Get-Item -LiteralPath $declaration
                }
                [IO.File]::WriteAllLines((Join-Path $cache 'PROVENANCE.txt'), $manifest, (New-Object Text.UTF8Encoding($false)))
                $provenance = "upstream $sourceNote"
            }
        }
        foreach ($file in $cacheFiles) { $files[$file.FullName] = $file.Name }
    }

    if ($files.Count -gt 0) { New-Item -ItemType Directory -Path $legal -Force | Out-Null }
    foreach ($file in $files.Keys) { Copy-Item -LiteralPath $file -Destination (Join-Path $legal $files[$file]) }
    foreach ($directory in $extraDirs) {
        $original = Join-Path $source $directory
        if (Test-Path -LiteralPath $original -PathType Container) {
            New-Item -ItemType Directory -Path $legal -Force | Out-Null
            Copy-Item -LiteralPath $original -Destination (Join-Path $legal $directory) -Recurse
        }
    }
    # A dependency with neither a license identifier nor any collected text is a hard error.
    if ($files.Count -eq 0 -and -not (Test-Path -LiteralPath $legal) -and -not $licenseId -and -not $licenseFile) {
        throw "No license identifier or legal file for shipped dependency: $($entry.Name) $($entry.Version)"
    }
    if ($files.Count -eq 0) { $provenance = 'NOT COLLECTED' }
    $license = if ($entry.License) { $entry.License } elseif ($licenseId) { $licenseId } elseif ($licenseFile) { 'SEE FILE' } else { 'SEE INCLUDED FILES' }
    $origin = $entry.Repository
    if (-not $origin) { $origin = Get-Field $package 'repository' }
    if (-not $origin) { $origin = Get-Field $package 'source' }
    if (-not $origin) { $origin = 'local path dependency' }
    $inventory.Add("$($entry.Name) $($entry.Version) | $license | $origin | $provenance")
}

$inventory.Add('')
if ($unresolved.Count -gt 0) {
    $inventory.Add('UNRESOLVED NOTICES (' + $unresolved.Count + ' of ' + $collected + ' shipped dependencies):')
    foreach ($item in $unresolved) { $inventory.Add('  ' + $item) }
} else {
    $inventory.Add('All ' + $collected + ' shipped dependencies contributed their legal text.')
}
[IO.File]::WriteAllLines((Join-Path $Destination 'RUST-DEPENDENCIES.txt'), $inventory, (New-Object Text.UTF8Encoding($false)))

if ($unresolved.Count -gt 0) {
    $message = 'Third-party notices are incomplete for ' + $unresolved.Count + ' shipped dependency/ies:'
    foreach ($item in $unresolved) { $message += "`n  " + $item }
    $message += "`nSupply the missing text (target/native/rust-licenses/<crate>-<version>/), add a declared-license" +
        " source entry in scripts/collect-licenses.ps1 when upstream publishes none, or remove the dependency."
    throw $message
}

# The linked Rust standard library is not a crates.io package in cargo tree. Use only the
# matching toolchain's library-specific notice file, not unrelated compiler documentation.
$libraryCopyright = Join-Path $sysroot 'share\doc\rust\COPYRIGHT-library.html'
if (-not (Test-Path -LiteralPath $libraryCopyright -PathType Leaf)) {
    throw "Pinned Rust toolchain lacks share/doc/rust/COPYRIGHT-library.html: $sysroot"
}
$libraryNotices = Join-Path $Destination 'rust-standard-library'
New-Item -ItemType Directory -Path $libraryNotices -Force | Out-Null
$copyrightCopy = Join-Path $libraryNotices 'COPYRIGHT-library.html'
Copy-Item -LiteralPath $libraryCopyright -Destination $copyrightCopy -Force
$copyrightHash = (Get-FileHash -LiteralPath $libraryCopyright -Algorithm SHA256).Hash
if ((Get-FileHash -LiteralPath $copyrightCopy -Algorithm SHA256).Hash -ne $copyrightHash) {
    throw 'Copied Rust standard-library notices did not match the toolchain source.'
}
$provenance = New-Object 'System.Collections.Generic.List[string]'
$provenance.Add("Toolchain selected by rust-toolchain.toml: $channel")
$provenance.Add('Source within selected rustc --print sysroot: share/doc/rust/COPYRIGHT-library.html')
$provenance.Add("Source SHA256: $copyrightHash")
$provenance.Add('rustc --version --verbose (run from the repository root):')
foreach ($line in $toolchainInfo) { $provenance.Add($line) }
[IO.File]::WriteAllLines((Join-Path $libraryNotices 'TOOLCHAIN-PROVENANCE.txt'), $provenance, (New-Object Text.UTF8Encoding($false)))
Write-Host ("Rust notices collected for {0} shipped dependencies." -f $collected)
