$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location $repo
$cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
if (-not $cargo) { $cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
if (-not (Test-Path -LiteralPath $cargo)) { throw 'Cargo is required to build the package.' }
& $cargo build --release -p first-light --locked
if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
$commit = (& git rev-parse --short=12 HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Git build ID unavailable.' }
$dirty = if ((& git status --porcelain).Count -gt 0) { 'yes' } else { 'no' }
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
$destination = Join-Path $repo "target\dist\the-passage-$commit-windows-x86_64"
if (Test-Path -LiteralPath $destination) { throw "Package already exists: $destination" }
New-Item -ItemType Directory -Path $destination -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $repo 'target\release\first-light.exe') -Destination (Join-Path $destination 'ThePassage.exe')
Copy-Item -LiteralPath (Join-Path $repo 'docs\guides\the-passage.md') -Destination (Join-Path $destination 'PLAY.md')
Copy-Item -LiteralPath (Join-Path $repo 'LICENSE-MIT') -Destination $destination
Copy-Item -LiteralPath (Join-Path $repo 'LICENSE-APACHE') -Destination $destination
Set-Content -LiteralPath (Join-Path $destination 'PLAY.cmd') -Value '@echo off', '"%~dp0ThePassage.exe" --passage' -Encoding ascii
$lockHash = (Get-FileHash -LiteralPath (Join-Path $repo 'Cargo.lock') -Algorithm SHA256).Hash
$exeHash = (Get-FileHash -LiteralPath (Join-Path $destination 'ThePassage.exe') -Algorithm SHA256).Hash
Set-Content -LiteralPath (Join-Path $destination 'BUILD.txt') -Value "Genesis The Passage experimental build`nGit commit: $commit`nWorking tree modified at build: $dirty`nExecutable SHA256: $exeHash`nCargo.lock SHA256: $lockHash`nTarget: Windows x86_64`nBuild profile: release" -Encoding utf8
$notices = @('Windows runtime dependency license metadata from Cargo metadata.', 'Review each upstream package for the complete license text.', '')
$notices += $packages | ForEach-Object { "$($_.name) $($_.version) | $($_.license) | $($_.repository)" }
Set-Content -LiteralPath (Join-Path $destination 'THIRD-PARTY-NOTICES.txt') -Value $notices -Encoding utf8
Write-Output $destination
