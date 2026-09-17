$ErrorActionPreference = "Stop"
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$listener = Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1

if ($listener) {
    $processInfo = Get-CimInstance Win32_Process -Filter "ProcessId = $($listener.OwningProcess)"
    $isWorkspaceVite = $processInfo.CommandLine -like "*$workspaceRoot*" -and $processInfo.CommandLine -match "vite"
    if (-not $isWorkspaceVite) {
        throw "Port 1420 is used by another application (PID $($listener.OwningProcess))."
    }

    Write-Host "Local: http://localhost:1420/ (reusing workspace Vite server)"
    while (Get-Process -Id $listener.OwningProcess -ErrorAction SilentlyContinue) {
        Start-Sleep -Seconds 2
    }
    exit 0
}

Set-Location $workspaceRoot
npm run dev
