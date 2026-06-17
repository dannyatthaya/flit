<#
.SYNOPSIS
  Build, sign, and (optionally) publish a Flit release — locally, using the
  signing keys stored in .env.

.DESCRIPTION
  Loads TAURI_SIGNING_PRIVATE_KEY / TAURI_SIGNING_PRIVATE_KEY_PASSWORD from .env,
  optionally bumps the version across the manifests, runs a signed `tauri build`,
  and (with -Publish) creates a draft GitHub release with the installers, their
  .sig files, and a generated latest.json for the auto-updater.

  For a full multi-OS release (Windows + Linux + macOS) signed via the CI
  secrets, push a `vX.Y.Z` git tag instead — that triggers .github/workflows/publish.yml.

.PARAMETER Version
  Optional new version (e.g. 0.2.0). Updates package.json, src-tauri/tauri.conf.json,
  and src-tauri/Cargo.toml before building.

.PARAMETER Publish
  After building, create a draft GitHub release (tag vX.Y.Z) and upload the
  Windows installers, their signatures, and latest.json. Requires the `gh` CLI.

.EXAMPLE
  pwsh scripts/release.ps1
  pwsh scripts/release.ps1 -Version 0.2.0 -Publish
#>
param(
  [string]$Version,
  [switch]$Publish
)

$ErrorActionPreference = 'Stop'
$RepoSlug = 'dannyatthaya/flit'
$root = Split-Path -Parent $PSScriptRoot

function Read-DotEnv($path) {
  if (-not (Test-Path $path)) { throw ".env not found at $path" }
  Get-Content $path | ForEach-Object {
    $line = $_.Trim()
    if ($line -and -not $line.StartsWith('#') -and $line -match '^([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$') {
      $name = $matches[1]
      $val = $matches[2].Trim().Trim('"')
      Set-Item -Path "Env:$name" -Value $val
    }
  }
}

function Set-Version($newVersion) {
  Write-Host "Setting version to $newVersion" -ForegroundColor Cyan
  # package.json
  $pkgPath = Join-Path $root 'package.json'
  $pkg = Get-Content $pkgPath -Raw
  $pkg = $pkg -replace '("version"\s*:\s*")[^"]*(")', "`${1}$newVersion`${2}"
  Set-Content $pkgPath $pkg -NoNewline -Encoding utf8
  # tauri.conf.json
  $confPath = Join-Path $root 'src-tauri/tauri.conf.json'
  $conf = Get-Content $confPath -Raw
  $conf = $conf -replace '("version"\s*:\s*")[^"]*(")', "`${1}$newVersion`${2}"
  Set-Content $confPath $conf -NoNewline -Encoding utf8
  # Cargo.toml (only the [package] version — first occurrence)
  $cargoPath = Join-Path $root 'src-tauri/Cargo.toml'
  $cargo = Get-Content $cargoPath -Raw
  $cargo = [regex]::Replace($cargo, '(?m)^version\s*=\s*"[^"]*"', "version = `"$newVersion`"", 1)
  Set-Content $cargoPath $cargo -NoNewline -Encoding utf8
}

function Get-CurrentVersion {
  $conf = Get-Content (Join-Path $root 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
  return $conf.version
}

# 1. Load signing keys from .env
Read-DotEnv (Join-Path $root '.env')
if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
  throw 'TAURI_SIGNING_PRIVATE_KEY missing from .env'
}
Write-Host 'Loaded signing key from .env' -ForegroundColor Green

# 2. Optional version bump
if ($Version) { Set-Version $Version }
$ver = Get-CurrentVersion
$tag = "v$ver"
Write-Host "Releasing Flit $tag" -ForegroundColor Cyan

# 3. Build (signed)
Push-Location $root
try {
  bun install
  bun run tauri build
} finally {
  Pop-Location
}

$bundleDir = Join-Path $root 'src-tauri/target/release/bundle'
$installer = Get-ChildItem -Path $bundleDir -Recurse -Filter '*-setup.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $installer) {
  $installer = Get-ChildItem -Path $bundleDir -Recurse -Filter '*.msi' -ErrorAction SilentlyContinue | Select-Object -First 1
}
if (-not $installer) { throw "No installer found under $bundleDir" }
$sig = Get-Item "$($installer.FullName).sig" -ErrorAction SilentlyContinue
Write-Host "Built: $($installer.Name)" -ForegroundColor Green

if (-not $Publish) {
  Write-Host "Done. Artifacts in $bundleDir" -ForegroundColor Green
  Write-Host "Re-run with -Publish to create a GitHub release, or push tag $tag to run CI." -ForegroundColor Yellow
  return
}

if (-not $sig) { throw "Signature file not found: $($installer.FullName).sig (build did not sign)" }

# 4. Generate latest.json for the updater (windows-x86_64)
$downloadUrl = "https://github.com/$RepoSlug/releases/download/$tag/$($installer.Name)"
$latest = [ordered]@{
  version   = $ver
  notes     = "Flit $tag"
  pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
  platforms = [ordered]@{
    'windows-x86_64' = [ordered]@{
      signature = (Get-Content $sig.FullName -Raw).Trim()
      url       = $downloadUrl
    }
  }
}
$latestPath = Join-Path $bundleDir 'latest.json'
$latest | ConvertTo-Json -Depth 6 | Set-Content $latestPath -Encoding utf8
Write-Host "Wrote $latestPath" -ForegroundColor Green

# 5. Create the draft GitHub release and upload assets
$assets = @($installer.FullName, $sig.FullName, $latestPath)
Write-Host "Creating draft release $tag on $RepoSlug" -ForegroundColor Cyan
gh release create $tag $assets --repo $RepoSlug --draft --title "Flit $tag" --notes "Flit $tag"
Write-Host "Draft release created. Review and publish it on GitHub (the updater only serves published, non-draft releases)." -ForegroundColor Yellow
