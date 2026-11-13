[CmdletBinding()]
param([string]$Root = (Join-Path $env:LOCALAPPDATA 'Metallic-RC\backend'))
$ErrorActionPreference = 'Stop'
$manifestPath = Join-Path $Root 'deployment.json'
if (-not (Test-Path -LiteralPath $manifestPath)) { throw 'No local Metallic-RC backend deployment exists.' }
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ($manifest.service -ne 'metallic-rc-backend' -or $manifest.port -lt 1024 -or $manifest.port -gt 65535) { throw 'Invalid deployment manifest.' }
$uri = 'http://127.0.0.1:' + $manifest.port
$task = Get-ScheduledTask -TaskName $manifest.taskName -ErrorAction SilentlyContinue
try {
    $health = Invoke-RestMethod -Uri "$uri/healthz" -TimeoutSec 5 -MaximumRedirection 0
    if ($health.service -ne 'metallic-rc-backend') { throw 'Unexpected service on backend port.' }
    $capabilities = Invoke-RestMethod -Uri "$uri/metallic/v1/capabilities" -TimeoutSec 5 -MaximumRedirection 0
    [pscustomobject]@{
        service=$health.service; status=$health.status; address=$uri; instanceId=$health.instanceId
        scheduledTaskState=if($null -eq $task){'absent'}else{[string]$task.State}
        actualGameClientVerified=$capabilities.compatibility.actualGameClientVerified
        protocol=$capabilities.protocol; dataDirectory=$manifest.dataDirectory
    } | ConvertTo-Json
} catch {
    [pscustomobject]@{service='metallic-rc-backend'; status='unreachable'; address=$uri
        scheduledTaskState=if($null -eq $task){'absent'}else{[string]$task.State}} | ConvertTo-Json
    exit 2
}
