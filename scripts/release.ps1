<#
.SYNOPSIS
  Build, sign, and (optionally) publish a Flit release — locally, using the
  signing keys stored in .env.

.DESCRIPTION
  Loads TAURI_SIGNING_PRIVATE_KEY / TAURI_SIGNING_PRIVATE_KEY_PASSWORD from .env,
  optionally bumps the version across the manifests, runs a signed `tauri build`,
  and (with -Publish) creates a draft GitHub release with the installer, its
  .sig file, and a generated latest.json for the auto-updater.

  For a full multi-OS release (Windows + Linux + macOS) signed via the CI
  secrets, push a `vX.Y.Z` git tag instead — that triggers .github/workflows/publish.yml.

.PARAMETER Version
  Optional new version (e.g. 0.2.0). Updates package.json, src-tauri/tauri.conf.json,
  and src-tauri/Cargo.toml before building. Cannot be combined with -Publish:
  commit and push the bump first so the release tag points at the code that
  was built.

.PARAMETER Publish
  After building, create a draft GitHub release (tag vX.Y.Z, at the current
  commit) and upload the Windows installer, its signature, and latest.json.
  Requires the `gh` CLI, a clean working tree, and the commit pushed.

  The generated latest.json only lists Windows. Once published, Windows users
  update to it; Linux and macOS installs see no build for their platform and
  stay on their version until a full release from CI.

.EXAMPLE
  pwsh scripts/release.ps1 -Version 0.2.0   # bump + test build; then commit & push
  pwsh scripts/release.ps1 -Publish         # build the committed version and draft the release
#>
param(
  [string]$Version,
  [switch]$Publish
)

$ErrorActionPreference = 'Stop'
$RepoSlug = 'dannyatthaya/flit'
$root = Split-Path -Parent $PSScriptRoot
$Utf8NoBom = New-Object System.Text.UTF8Encoding $false

# External programs don't throw on failure in PowerShell; check every exit code.
function Invoke-Checked {
  param([string]$What, [scriptblock]$Command)
  & $Command
  if ($LASTEXITCODE -ne 0) { throw "$What failed (exit code $LASTEXITCODE)" }
}

function Write-TextFile($path, $text) {
  # Windows PowerShell 5.1's `-Encoding utf8` writes a BOM, which breaks JSON/TOML tools.
  [System.IO.File]::WriteAllText($path, $text, $Utf8NoBom)
}

function Read-DotEnv($path) {
  if (-not (Test-Path $path)) { throw ".env not found at $path" }
  Get-Content $path | ForEach-Object {
    $line = $_.Trim()
    if ($line -and -not $line.StartsWith('#') -and $line -match '^(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$') {
      $name = $matches[1]
      $val = $matches[2].Trim()
      if ($val.Length -ge 2 -and (($val[0] -eq '"' -and $val[-1] -eq '"') -or ($val[0] -eq "'" -and $val[-1] -eq "'"))) {
        $val = $val.Substring(1, $val.Length - 2)
      }
      Set-Item -Path "Env:$name" -Value $val
    }
  }
}

function Set-Version($newVersion) {
  if ($newVersion -notmatch '^\d+\.\d+\.\d+([-+][0-9A-Za-z.-]+)?$') {
    throw "Version '$newVersion' is not a valid semver version"
  }
  Write-Host "Setting version to $newVersion" -ForegroundColor Cyan
  # package.json and tauri.conf.json: only the top-level "version" key (first match).
  foreach ($rel in @('package.json', 'src-tauri/tauri.conf.json')) {
    $path = Join-Path $root $rel
    $text = [System.IO.File]::ReadAllText($path)
    $text = [regex]::Replace($text, '("version"\s*:\s*")[^"]*(")', "`${1}$newVersion`${2}", 1)
    Write-TextFile $path $text
  }
  # Cargo.toml (only the [package] version — first occurrence)
  $cargoPath = Join-Path $root 'src-tauri/Cargo.toml'
  $cargo = [System.IO.File]::ReadAllText($cargoPath)
  $cargo = [regex]::Replace($cargo, '(?m)^version\s*=\s*"[^"]*"', "version = `"$newVersion`"", 1)
  Write-TextFile $cargoPath $cargo
}

function Get-CurrentVersion {
  $conf = Get-Content (Join-Path $root 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
  return $conf.version
}

if ($Version -and $Publish) {
  throw 'Run with -Version first, commit and push the bump (including Cargo.lock), then run with -Publish.'
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

if ($Publish) {
  Push-Location $root
  try {
    $dirty = git status --porcelain
    if ($LASTEXITCODE -ne 0) { throw 'git status failed' }
    if ($dirty) { throw "Working tree is not clean; commit or stash first:`n$dirty" }
    $commit = (git rev-parse HEAD).Trim()
    # Refresh remote-tracking refs so a commit pushed from elsewhere counts.
    git fetch --quiet origin
    if ($LASTEXITCODE -ne 0) { throw 'git fetch failed' }
    $remoteBranches = git branch -r --contains $commit
    if (-not $remoteBranches) { throw "Commit $commit is not pushed; push it before publishing." }
    # A missing release makes `gh` write to stderr; don't let that stop the script.
    $ErrorActionPreference = 'Continue'
    gh release view $tag --repo $RepoSlug *> $null
    $exists = $LASTEXITCODE -eq 0
    $ErrorActionPreference = 'Stop'
    if ($exists) { throw "Release $tag already exists on $RepoSlug." }
  } finally {
    Pop-Location
  }
}

# 3. Build (signed)
$buildStart = Get-Date
Push-Location $root
try {
  Invoke-Checked 'bun install' { bun install --frozen-lockfile }
  Invoke-Checked 'tauri build' { bun run tauri build }
} finally {
  Pop-Location
}

# Pick the installer for *this* version from *this* build — never a leftover
# from an older build still sitting in the bundle directory.
$bundleDir = Join-Path $root 'src-tauri/target/release/bundle'
$escapedVer = [regex]::Escape($ver)
$candidates = Get-ChildItem -Path $bundleDir -Recurse -File -ErrorAction SilentlyContinue |
  Where-Object {
    ($_.Name -match "_${escapedVer}_.*-setup\.exe$" -or $_.Name -match "_${escapedVer}_.*\.msi$") -and
    $_.LastWriteTime -ge $buildStart
  } |
  Sort-Object @{ Expression = { $_.Extension -eq '.exe' }; Descending = $true }, LastWriteTime -Descending
$installer = $candidates | Select-Object -First 1
if (-not $installer) { throw "No installer for version $ver was produced under $bundleDir" }
$sig = Get-Item "$($installer.FullName).sig" -ErrorAction SilentlyContinue
Write-Host "Built: $($installer.Name)" -ForegroundColor Green

if (-not $Publish) {
  Write-Host "Done. Artifacts in $bundleDir" -ForegroundColor Green
  Write-Host "Commit and push, then re-run with -Publish to create a GitHub release, or push tag $tag to run CI." -ForegroundColor Yellow
  return
}

if (-not $sig -or $sig.LastWriteTime -lt $buildStart) {
  throw "Signature file not found or stale: $($installer.FullName).sig (build did not sign)"
}

# 4. Generate latest.json for the updater (windows-x86_64)
$downloadUrl = "https://github.com/$RepoSlug/releases/download/$tag/$([uri]::EscapeDataString($installer.Name))"
$latest = [ordered]@{
  version   = $ver
  notes     = "Flit $tag"
  pub_date  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss'Z'")
  platforms = [ordered]@{
    'windows-x86_64' = [ordered]@{
      signature = (Get-Content $sig.FullName -Raw).Trim()
      url       = $downloadUrl
    }
  }
}
$latestPath = Join-Path $bundleDir 'latest.json'
Write-TextFile $latestPath ($latest | ConvertTo-Json -Depth 6)
Write-Host "Wrote $latestPath" -ForegroundColor Green

# 5. Create the draft GitHub release (tag at the built commit) and upload assets
$assets = @($installer.FullName, $sig.FullName, $latestPath)
Write-Host "Creating draft release $tag on $RepoSlug at $commit" -ForegroundColor Cyan
Invoke-Checked 'gh release create' {
  gh release create $tag @assets --repo $RepoSlug --target $commit --draft --title "Flit $tag" --notes "Flit $tag"
}
Write-Host "Draft release created. Review and publish it on GitHub (the updater only serves published, non-draft releases)." -ForegroundColor Yellow
