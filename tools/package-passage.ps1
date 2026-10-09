$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location $repo
$cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
if (-not $cargo) { $cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
if (-not (Test-Path -LiteralPath $cargo)) { throw 'Cargo is required to build the package.' }
$previousFlags = $env:RUSTFLAGS
try {
    $env:RUSTFLAGS = (($previousFlags, '-C target-feature=+crt-static') | Where-Object { $_ }) -join ' '
    & $cargo build --release -p first-light --locked
    if ($LASTEXITCODE -ne 0) { throw 'Static-CRT release build failed.' }
} finally {
    $env:RUSTFLAGS = $previousFlags
}
$commit = (& git rev-parse --short=12 HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Git build ID unavailable.' }
$dirty = if ((& git status --porcelain).Count -gt 0) { 'yes' } else { 'no' }
$buildId = if ($dirty -eq 'yes') { "$commit-working" } else { $commit }
$metadata = & $cargo metadata --locked --filter-platform x86_64-pc-windows-msvc --format-version 1
if ($LASTEXITCODE -ne 0) { throw 'Cannot produce dependency notices.' }
$metadata = $metadata | ConvertFrom-Json
$root = ($metadata.packages | Where-Object { $_.name -eq 'first-light' } | Select-Object -First 1).id
$nodes = @{}
foreach ($node in $metadata.resolve.nodes) { $nodes[$node.id] = $node }
$seen = [System.Collections.Generic.HashSet[string]]::new()
$queue = [System.Collections.Generic.Queue[string]]::new()
$queue.Enqueue($root)
while ($queue.Count -gt 0) {
    $id = $queue.Dequeue()
    if (-not $seen.Add($id)) { continue }
    foreach ($dep in $nodes[$id].deps) {
        if ($dep.dep_kinds | Where-Object { $_.kind -ne 'dev' }) { $queue.Enqueue($dep.pkg) }
    }
}
$packages = $metadata.packages | Where-Object { $_.source -and $seen.Contains($_.id) } | Sort-Object name, version -Unique
$dist = Join-Path $repo 'target\dist'
$destination = Join-Path $dist "the-passage-$buildId-windows-x86_64"
$archive = "$destination.zip"
if (Test-Path -LiteralPath $destination) { throw "Package already exists: $destination" }
if (Test-Path -LiteralPath $archive) { throw "Package already exists: $archive" }
New-Item -ItemType Directory -Path $destination -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $repo 'target\release\first-light.exe') -Destination (Join-Path $destination 'ThePassage.exe')
Copy-Item -LiteralPath (Join-Path $repo 'docs\guides\playtest-card.md') -Destination (Join-Path $destination 'PLAY.md')
Copy-Item -LiteralPath (Join-Path $repo 'docs\guides\the-passage.md') -Destination (Join-Path $destination 'GUIDE.md')
Copy-Item -LiteralPath (Join-Path $repo 'docs\guides\playtest-feedback.md') -Destination (Join-Path $destination 'FEEDBACK.md')
Copy-Item -LiteralPath (Join-Path $repo 'LICENSE-MIT') -Destination $destination
Copy-Item -LiteralPath (Join-Path $repo 'LICENSE-APACHE') -Destination $destination
Set-Content -LiteralPath (Join-Path $destination 'PLAY.cmd') -Value '@echo off', '"%~dp0ThePassage.exe" --passage', 'if errorlevel 1 (', '  echo The Passage could not start. Update the graphics driver and check for Vulkan or DirectX 12 support.', '  pause', '  exit /b 1', ')' -Encoding ascii
$lockHash = (Get-FileHash -LiteralPath (Join-Path $repo 'Cargo.lock') -Algorithm SHA256).Hash
$exeHash = (Get-FileHash -LiteralPath (Join-Path $destination 'ThePassage.exe') -Algorithm SHA256).Hash
Set-Content -LiteralPath (Join-Path $destination 'BUILD.txt') -Value "Genesis The Passage experimental build`nGit commit: $commit`nWorking tree modified at build: $dirty`nExecutable SHA256: $exeHash`nCargo.lock SHA256: $lockHash`nTarget: Windows x86_64 MSVC`nBuild profile: release; static CRT" -Encoding utf8
$licenseRoot = Join-Path $destination 'THIRD-PARTY-LICENSES'
New-Item -ItemType Directory -Path $licenseRoot | Out-Null
$notices = @('Windows runtime dependency closure from Cargo metadata. Project licenses are in the package root.', 'Each directory below contains upstream license files, except clearly named SPDX fallback copies.', '')
foreach ($package in $packages) {
    $name = "$($package.name)-$($package.version)"
    $licenseDir = Join-Path $licenseRoot $name
    New-Item -ItemType Directory -Path $licenseDir | Out-Null
    $sourceDir = Split-Path $package.manifest_path
    $files = @(Get-ChildItem -LiteralPath $sourceDir -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE)([-._].*)?$' })
    if ($files.Count -eq 0) {
        if ($package.license -notmatch 'Apache-2\.0') { throw "Missing distributable license text for $name ($($package.license))" }
        Copy-Item -LiteralPath (Join-Path $repo 'LICENSE-APACHE') -Destination (Join-Path $licenseDir 'SPDX-Apache-2.0.txt')
        $notices += "$name | $($package.license) | SPDX Apache-2.0 text; no license file in published crate | $($package.repository)"
    } else {
        foreach ($file in $files) { Copy-Item -LiteralPath $file.FullName -Destination $licenseDir }
        $notices += "$name | $($package.license) | $($files.Name -join ', ') | $($package.repository)"
    }
}
Set-Content -LiteralPath (Join-Path $destination 'THIRD-PARTY-NOTICES.txt') -Value $notices -Encoding utf8
$zipTime = [datetime]'1980-01-01T00:00:00'
Get-ChildItem -LiteralPath $destination -Recurse -Force | ForEach-Object { $_.LastWriteTime = $zipTime }
(Get-Item -LiteralPath $destination).LastWriteTime = $zipTime
Compress-Archive -LiteralPath $destination -DestinationPath $archive -CompressionLevel Optimal
$check = Join-Path $env:TEMP "Genesis package check $buildId"
if (Test-Path -LiteralPath $check) { throw "Verification directory already exists: $check" }
Expand-Archive -LiteralPath $archive -DestinationPath $check
$extracted = Join-Path $check (Split-Path $destination -Leaf)
if ((Get-FileHash -LiteralPath (Join-Path $extracted 'ThePassage.exe') -Algorithm SHA256).Hash -ne $exeHash) { throw 'ZIP executable hash mismatch.' }
if (-not (Test-Path -LiteralPath (Join-Path $extracted 'PLAY.cmd'))) { throw 'ZIP launcher missing.' }
if (@(Get-ChildItem -LiteralPath $extracted -Recurse -File).Count -ne @(Get-ChildItem -LiteralPath $destination -Recurse -File).Count) { throw 'ZIP file count mismatch.' }
Write-Output "Package: $archive"
Write-Output "Verified extraction: $extracted"
