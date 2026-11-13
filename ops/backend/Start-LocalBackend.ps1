[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$ManifestPath)
$ErrorActionPreference = 'Stop'
$manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
if ($manifest.service -ne 'metallic-rc-backend' -or $manifest.port -lt 1024 -or $manifest.port -gt 65535) { throw 'Invalid backend manifest.' }
foreach ($path in @($manifest.dotnet, $manifest.assembly, $manifest.dataDirectory, $manifest.logDirectory)) {
    if (-not [IO.Path]::IsPathRooted($path) -or $path.Contains('"')) { throw 'Manifest paths must be absolute and must not contain quotes.' }
}
if (-not (Test-Path -LiteralPath $manifest.dotnet -PathType Leaf)) { throw 'The configured .NET host is missing.' }
$release = Split-Path -Parent $manifest.assembly
[IO.Directory]::CreateDirectory($manifest.logDirectory) | Out-Null
$log = Join-Path $manifest.logDirectory 'backend.log'
$errorLog = Join-Path $manifest.logDirectory 'backend.stderr.log'
$supervisorLog = Join-Path $manifest.logDirectory 'supervisor.log'
function Rotate-Log([string]$Path) {
    if (Test-Path -LiteralPath $Path) {
        for ($i = 4; $i -ge 1; $i--) {
            $old = "$Path.$i"
            if (Test-Path -LiteralPath $old) { Move-Item -LiteralPath $old -Destination "$Path.$($i+1)" -Force }
        }
        Move-Item -LiteralPath $Path -Destination "$Path.1" -Force
    }
}
function Record-Event([string]$Event, [int]$Code, [int]$DelaySeconds) {
    if ((Test-Path -LiteralPath $supervisorLog) -and (Get-Item -LiteralPath $supervisorLog).Length -gt 1048576) {
        Move-Item -LiteralPath $supervisorLog -Destination "$supervisorLog.1" -Force
    }
    [pscustomobject]@{event=$Event; exitCode=$Code; retryDelaySeconds=$DelaySeconds; utc=[DateTime]::UtcNow.ToString('o')} |
        ConvertTo-Json -Compress | Add-Content -LiteralPath $supervisorLog -Encoding UTF8
}
$failures = @()
while ($true) {
    # Recheck the release on every attempt; an integrity failure is not retried.
    if ((Get-FileHash -LiteralPath $manifest.assembly -Algorithm SHA256).Hash.ToLowerInvariant() -ne $manifest.assemblySha256) {
        throw 'Backend assembly changed since deployment; refusing to start.'
    }
    foreach ($entry in $manifest.files) {
        if ($entry.name -match '[/\\:]' -or $entry.name -in @('.', '..')) { throw 'Invalid release file name.' }
        $file = Join-Path $release $entry.name
        if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.sha256) {
            throw 'A deployed release file failed integrity verification.'
        }
    }
    Rotate-Log $log; Rotate-Log $errorLog
    $arguments = '"' + $manifest.assembly + '" --port ' + $manifest.port + ' --data-dir "' + $manifest.dataDirectory + '"'
    $code = 1; $child = $null; $outStream = $null; $errStream = $null; $started = $false
    try {
        Record-Event 'child_starting' 0 0
        # Keep the actual Process handle. Start-Process's returned object lost its exit
        # status in the first Windows PowerShell test and misclassified graceful exits.
        $child = New-Object System.Diagnostics.Process
        $child.StartInfo.FileName = $manifest.dotnet
        $child.StartInfo.Arguments = $arguments
        $child.StartInfo.WorkingDirectory = $release
        $child.StartInfo.UseShellExecute = $false
        $child.StartInfo.CreateNoWindow = $true
        $child.StartInfo.RedirectStandardOutput = $true
        $child.StartInfo.RedirectStandardError = $true
        $outStream = [IO.File]::Open($log, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::Read)
        $errStream = [IO.File]::Open($errorLog, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::Read)
        $started = $child.Start()
        if (-not $started) { throw 'Backend process did not start.' }
        $outCopy = $child.StandardOutput.BaseStream.CopyToAsync($outStream)
        $errCopy = $child.StandardError.BaseStream.CopyToAsync($errStream)
        $child.WaitForExit()
        $code = [int]$child.ExitCode
        [Threading.Tasks.Task]::WaitAll([Threading.Tasks.Task[]]@($outCopy, $errCopy))
    } catch { Record-Event 'child_launch_failed' 1 0; $code = 1 }
    finally {
        if ($null -ne $child) {
            if ($started -and -not $child.HasExited) { $child.Kill(); $child.WaitForExit() }
            $child.Dispose()
        }
        if ($null -ne $outStream) { $outStream.Dispose() }
        if ($null -ne $errStream) { $errStream.Dispose() }
    }
    if ($code -eq 0) { Record-Event 'graceful_stop' 0 0; exit 0 }
    $now = [DateTime]::UtcNow
    $failures = @($failures | Where-Object { $_ -gt $now.AddMinutes(-10) }) + @($now)
    if ($failures.Count -gt 5) { Record-Event 'failure_budget_exhausted' $code 0; exit 1 }
    $delay = [int][Math]::Min(30, [Math]::Pow(2, $failures.Count))
    Record-Event 'child_failed_retry_scheduled' $code $delay
    Start-Sleep -Seconds $delay
}
