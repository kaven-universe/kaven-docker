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

if ($tauri.identifier -cne "xyz.kaven.app.docker") {
    throw "Tauri identifier must be xyz.kaven.app.docker."
}
if ($tauri.bundle.windows.nsis.installMode -cne "currentUser") {
    throw "The NSIS installer must use currentUser mode."
}
$templatePath = Join-Path (Join-Path $workspaceRoot "src-tauri") $tauri.bundle.windows.nsis.template
if (-not (Test-Path -LiteralPath $templatePath -PathType Leaf)) {
    throw "The configured NSIS template is missing: $templatePath"
}
$template = Get-Content -Raw -LiteralPath $templatePath
$tauriCliVersion = $package.devDependencies.'@tauri-apps/cli'
if ($tauriCliVersion -notmatch '^\d+\.\d+\.\d+$') {
    throw "@tauri-apps/cli must use an exact X.Y.Z version."
}
$expectedTemplateSource = "; Vendored from Tauri CLI $tauriCliVersion (Apache-2.0 OR MIT)."
if ([regex]::Matches($template, [regex]::Escape($expectedTemplateSource)).Count -ne 1) {
    throw "The vendored NSIS template must match @tauri-apps/cli $tauriCliVersion."
}
$expectedInstallPath = 'StrCpy $INSTDIR "$LOCALAPPDATA\Kaven\App\Kaven-Docker"'
if ([regex]::Matches($template, [regex]::Escape($expectedInstallPath)).Count -ne 1) {
    throw "The NSIS template must define the standard current-user installation path exactly once."
}

$releaseNotes = Join-Path $workspaceRoot "docs\releases\$Tag.md"
if (-not (Test-Path -LiteralPath $releaseNotes -PathType Leaf)) {
    throw "Reviewed release notes are missing: $releaseNotes"
}

Write-Host "Release identity is valid for $Tag."
