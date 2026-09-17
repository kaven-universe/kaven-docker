$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$listeners = Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue

foreach ($listener in $listeners) {
    $processInfo = Get-CimInstance Win32_Process -Filter "ProcessId = $($listener.OwningProcess)"
    $isWorkspaceVite = $processInfo.CommandLine -like "*$workspaceRoot*" -and $processInfo.CommandLine -match "vite"
    if ($isWorkspaceVite) {
        Stop-Process -Id $listener.OwningProcess -Force
    }
}
