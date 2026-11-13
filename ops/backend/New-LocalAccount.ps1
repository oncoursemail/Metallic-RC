[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$Username,
      [Parameter(Mandatory=$true)][string]$DisplayName,
      [string]$Root = (Join-Path $env:LOCALAPPDATA 'Metallic-RC\backend'))
$ErrorActionPreference = 'Stop'
$manifest = Get-Content -LiteralPath (Join-Path $Root 'deployment.json') -Raw | ConvertFrom-Json
if ($manifest.service -ne 'metallic-rc-backend' -or $manifest.port -lt 1024 -or $manifest.port -gt 65535) { throw 'Invalid deployment manifest.' }
$uri = 'http://127.0.0.1:' + $manifest.port
$health = Invoke-RestMethod -Uri "$uri/healthz" -TimeoutSec 5 -MaximumRedirection 0
$runtime = Get-Content -LiteralPath (Join-Path $manifest.dataDirectory 'runtime.json') -Raw | ConvertFrom-Json
if ($health.service -ne 'metallic-rc-backend' -or $runtime.instanceId -ne $health.instanceId) { throw 'Local backend identity did not match.' }
$secure = Read-Host 'New Metallic password (12-256 characters; not a Rec Room password)' -AsSecureString
$pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
$key = $null; $password = $null; $body = $null
try {
    $password = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer)
    $key = (Get-Content -LiteralPath (Join-Path $manifest.dataDirectory 'admin.key') -Raw).Trim()
    $body = @{username=$Username; displayName=$DisplayName; password=$password} | ConvertTo-Json -Compress
    Invoke-RestMethod -Uri "$uri/metallic/v1/auth/register" -Method Post -ContentType 'application/json; charset=utf-8' -Body ([Text.Encoding]::UTF8.GetBytes($body)) -Headers @{'X-Metallic-Admin'=$key} -TimeoutSec 10 -MaximumRedirection 0 | ConvertTo-Json
} finally {
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer)
    $secure.Dispose(); $password=$null; $body=$null; $key=$null
}
