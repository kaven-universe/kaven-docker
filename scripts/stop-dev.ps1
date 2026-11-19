$ErrorActionPreference = "Stop"
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$targetExecutable = Join-Path $workspaceRoot "src-tauri\target\debug\kaven-docker.exe"
$manifestPath = Join-Path $workspaceRoot "src-tauri\Cargo.toml"

$applicationProcesses = Get-CimInstance Win32_Process | Where-Object {
    $_.ExecutablePath -and $_.ExecutablePath.Equals($targetExecutable, [StringComparison]::OrdinalIgnoreCase)
}
foreach ($process in $applicationProcesses) {
    $nativeProcess = Get-Process -Id $process.ProcessId -ErrorAction SilentlyContinue
    if ($nativeProcess) {
        $closed = $nativeProcess.CloseMainWindow()
        if (-not $closed -or -not $nativeProcess.WaitForExit(3000)) {
            Stop-Process -Id $process.ProcessId -Force -ErrorAction SilentlyContinue
        }
    }
}

$processes = Get-CimInstance Win32_Process
$cargoProcesses = $processes | Where-Object {
    $_.Name -eq "cargo.exe" -and $_.CommandLine -like "*$manifestPath*"
}
foreach ($process in $cargoProcesses) {
    Stop-Process -Id $process.ProcessId -Force -ErrorAction SilentlyContinue
}

$viteProcesses = $processes | Where-Object {
    $_.Name -eq "node.exe" -and $_.CommandLine -like "*$workspaceRoot*" -and $_.CommandLine -match "vite"
}
foreach ($process in $viteProcesses) {
    Stop-Process -Id $process.ProcessId -Force -ErrorAction SilentlyContinue
}

Write-Host "Stopped Kaven Docker development processes."
