$ErrorActionPreference = "Stop"

$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$devServerUrl = "http://localhost:1421"

function Test-KavenDockerDevServer {
    try {
        $response = Invoke-WebRequest -Uri $devServerUrl -UseBasicParsing -TimeoutSec 2
        return $response.StatusCode -eq 200 `
            -and $response.Content.Contains("<title>Kaven Docker</title>") `
            -and $response.Content.Contains('/src/main.ts')
    }
    catch {
        return $false
    }
}

function Wait-KavenDockerDevServer([int]$Attempts) {
    for ($attempt = 0; $attempt -lt $Attempts; $attempt++) {
        if (Test-KavenDockerDevServer) {
            return $true
        }
        Start-Sleep -Milliseconds 200
    }
    return $false
}

$listener = Get-NetTCPConnection -LocalPort 1421 -State Listen -ErrorAction SilentlyContinue |
    Select-Object -First 1
if ($listener) {
    if (Wait-KavenDockerDevServer 50) {
        Write-Host "Kaven Docker UI development server is already ready at $devServerUrl."
        exit 0
    }
    throw "Port 1421 is occupied by another process (PID $($listener.OwningProcess))."
}

$workspaceBytes = [Text.Encoding]::UTF8.GetBytes($workspaceRoot)
$sha256 = [Security.Cryptography.SHA256]::Create()
try {
    $hashBytes = $sha256.ComputeHash($workspaceBytes)
}
finally {
    $sha256.Dispose()
}
$workspaceHash = ([BitConverter]::ToString($hashBytes) -replace "-", "").Substring(0, 12).ToLowerInvariant()
$stdoutPath = Join-Path $env:TEMP "kaven-docker-vite-$workspaceHash.stdout.log"
$stderrPath = Join-Path $env:TEMP "kaven-docker-vite-$workspaceHash.stderr.log"
$npm = (Get-Command npm.cmd -ErrorAction Stop).Source
$process = Start-Process `
    -FilePath $npm `
    -ArgumentList @("run", "dev") `
    -WorkingDirectory $workspaceRoot `
    -WindowStyle Hidden `
    -RedirectStandardOutput $stdoutPath `
    -RedirectStandardError $stderrPath `
    -PassThru

for ($attempt = 0; $attempt -lt 150; $attempt++) {
    $process.Refresh()
    if ($process.HasExited) {
        break
    }
    if (Test-KavenDockerDevServer) {
        Write-Host "Kaven Docker UI development server is ready at $devServerUrl."
        exit 0
    }
    Start-Sleep -Milliseconds 200
}

if (-not $process.HasExited) {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
}
$viteProcesses = Get-CimInstance Win32_Process | Where-Object {
    $_.Name -eq "node.exe" `
        -and $_.CommandLine -like "*$workspaceRoot*" `
        -and $_.CommandLine -match "vite"
}
foreach ($viteProcess in $viteProcesses) {
    Stop-Process -Id $viteProcess.ProcessId -Force -ErrorAction SilentlyContinue
}

if (Test-Path -LiteralPath $stdoutPath) {
    Get-Content -LiteralPath $stdoutPath | Write-Host
}
if (Test-Path -LiteralPath $stderrPath) {
    Get-Content -LiteralPath $stderrPath | Write-Error
}
throw "Kaven Docker UI development server did not become ready at $devServerUrl."
