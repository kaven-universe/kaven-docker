param(
    [Parameter(Mandatory = $true)]
    [string]$Tag
)

$ErrorActionPreference = "Stop"

if ($Tag -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
    throw "Release tag must use the exact X.Y.Z format: $Tag"
}

$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$package = Get-Content -Raw -LiteralPath (Join-Path $workspaceRoot "package.json") |
    ConvertFrom-Json
$packageLock = Get-Content -Raw -LiteralPath (Join-Path $workspaceRoot "package-lock.json")
$tauri = Get-Content -Raw -LiteralPath (Join-Path $workspaceRoot "src-tauri\tauri.conf.json") |
    ConvertFrom-Json
$cargoToml = Get-Content -Raw -LiteralPath (Join-Path $workspaceRoot "src-tauri\Cargo.toml")
$cargoLock = Get-Content -Raw -LiteralPath (Join-Path $workspaceRoot "src-tauri\Cargo.lock")
$cargoVersion = [regex]::Match(
    $cargoToml,
    '(?m)^version\s*=\s*"([^"]+)"'
).Groups[1].Value
$cargoLockVersion = [regex]::Match(
    $cargoLock,
    '(?ms)\[\[package\]\]\s+name\s*=\s*"kaven-docker"\s+version\s*=\s*"([^"]+)"'
).Groups[1].Value
$packageLockVersions = [regex]::Matches(
    $packageLock,
    '(?m)^\s*"version"\s*:\s*"([^"]+)"'
)
if ($packageLockVersions.Count -lt 2) {
    throw "package-lock.json does not contain the expected root versions."
}
$packageLockVersion = $packageLockVersions[0].Groups[1].Value
$rootLockVersion = $packageLockVersions[1].Groups[1].Value
$versions = [ordered]@{
    "package.json" = $package.version
    "package-lock.json" = $packageLockVersion
    "package-lock.json root package" = $rootLockVersion
    "src-tauri/Cargo.toml" = $cargoVersion
    "src-tauri/Cargo.lock" = $cargoLockVersion
    "src-tauri/tauri.conf.json" = $tauri.version
}

foreach ($entry in $versions.GetEnumerator()) {
    if ($entry.Value -cne $Tag) {
        throw "$($entry.Key) version '$($entry.Value)' does not match release tag '$Tag'."
    }
}

$releaseNotes = Join-Path $workspaceRoot "docs\releases\$Tag.md"
if (-not (Test-Path -LiteralPath $releaseNotes -PathType Leaf)) {
    throw "Reviewed release notes are missing: $releaseNotes"
}

Write-Host "Release identity is valid for $Tag."
