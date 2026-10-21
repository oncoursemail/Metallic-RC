[CmdletBinding()]
param([string]$Root = (Join-Path $env:LOCALAPPDATA 'Metallic-RC\backend'), [switch]$DisableAutoStart)
$ErrorActionPreference = 'Stop'
$manifest = Get-Content -LiteralPath (Join-Path $Root 'deployment.json') -Raw | ConvertFrom-Json
if ($manifest.service -ne 'metallic-rc-backend' -or $manifest.port -lt 1024 -or $manifest.port -gt 65535) { throw 'Invalid deployment manifest.' }
$uri = 'http://127.0.0.1:' + $manifest.port
$health = Invoke-RestMethod -Uri "$uri/healthz" -TimeoutSec 5 -MaximumRedirection 0
if ($health.service -ne 'metallic-rc-backend') { throw 'Refusing to stop an unrelated service.' }
$runtime = Get-Content -LiteralPath (Join-Path $manifest.dataDirectory 'runtime.json') -Raw | ConvertFrom-Json
if ($runtime.instanceId -ne $health.instanceId) { throw 'Backend instance does not match local state.' }
if ($DisableAutoStart) {
    $task = Get-ScheduledTask -TaskName $manifest.taskName -ErrorAction Stop
    if ($task.Description -ne 'Metallic-RC owned loopback backend; local user only.') { throw 'Unexpected scheduled task owner.' }
    $task | Disable-ScheduledTask | Out-Null
}
$key = (Get-Content -LiteralPath (Join-Path $manifest.dataDirectory 'admin.key') -Raw).Trim()
try {
    Invoke-RestMethod -Uri "$uri/metallic/v1/admin/shutdown" -Method Post -Headers @{'X-Metallic-Admin'=$key} -TimeoutSec 5 -MaximumRedirection 0 | ConvertTo-Json
} finally { $key = $null }
