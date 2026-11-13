[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$ManifestPath,
    [Parameter(Mandatory=$true)][string]$ExpectedManifestHash,
    [Parameter(Mandatory=$true)][string]$RunnerPath,
    [Parameter(Mandatory=$true)][string]$ExpectedRunnerHash,
    [Parameter(Mandatory=$true)][string]$PythonPath,
    [switch]$PauseAtEnd
)
$ErrorActionPreference='Stop'
$exitCode=1
try {
    foreach($path in @($ManifestPath,$RunnerPath,$PythonPath)) {
        if(-not [IO.Path]::IsPathRooted($path) -or $path.Contains('"')) { throw 'Absolute reviewed paths without quotes are required.' }
    }
    if((Get-FileHash -LiteralPath $RunnerPath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $ExpectedRunnerHash) { throw 'Reviewed runner hash changed; no launch.' }
    if((Get-FileHash -LiteralPath $ManifestPath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $ExpectedManifestHash) { throw 'Reviewed manifest hash changed; no launch.' }
    $manifest=Get-Content -LiteralPath $ManifestPath -Raw|ConvertFrom-Json
    if($manifest.schemaVersion -ne 3 -or $manifest.steamBuildId -ne '23191908') { throw 'Reviewed v3 progress-store manifest required.' }
    & $PythonPath -I -B $RunnerPath --manifest $ManifestPath
    if($LASTEXITCODE -ne 0) { throw 'Preflight failed. No original-client launch was requested.' }
    Write-Host 'Metallic-RC native test: transactional progress; 90-second TOTAL worker budget.'
    Write-Host 'Startup limit: 30 seconds. Attachment limit: 15 seconds. Cleanup is supervised separately.'
    Write-Host 'Normal Windows approval is required. No password or Rec Room credential entry is needed.'
    Write-Host 'Only the newly created test process tree is controlled. No game-file or system security changes.'
    $receipt=Join-Path $manifest.labRoot ('manual-v2-request-'+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')+'.json')
    $record=[ordered]@{requestedUtc=[DateTime]::UtcNow.ToString('o');status='awaiting_windows_uac';workerBudgetSeconds=90;attachLimitSeconds=15;supervisorWaitSeconds=130}
    $record|ConvertTo-Json|Set-Content -LiteralPath $receipt -Encoding UTF8
    $process=$null
    try {
        $arguments='-I -B "'+$RunnerPath+'" --manifest "'+$ManifestPath+'" --run --seconds 90'
        $process=Start-Process -FilePath $PythonPath -ArgumentList $arguments -Verb RunAs -PassThru
        $null=$process.Handle # Retain identity and the real exit status.
        if(-not $process.WaitForExit(130000)) {
            # This is the exact returned supervisor handle, not a PID/name search.
            # Closing that process closes its kill-on-close child job.
            $process.Kill();$process.WaitForExit(5000)|Out-Null
            $record.status='supervisor_deadline_exceeded';$exitCode=124
        } else {
            $exitCode=[int]$process.ExitCode
            $record.status=if($exitCode -eq 0){'discovery_test_reported_success'}elseif($exitCode -eq 124){'test_deadline_reached'}else{'test_failed_or_no_native_discovery'}
        }
        $record.exitCode=$exitCode
    } catch { $record.status='approval_or_launch_failed';$record.error=$_.Exception.Message;$exitCode=1 }
    finally {
        if($null -ne $process) { $process.Dispose() }
        $record.finishedUtc=[DateTime]::UtcNow.ToString('o');$record|ConvertTo-Json|Set-Content -LiteralPath $receipt -Encoding UTF8
    }
    $record|ConvertTo-Json
    Write-Host ('Detailed phase receipts: '+(Join-Path $manifest.labRoot 'runs'))
    Write-Host 'Successful native login, room joining and gameplay are separate acceptance gates.'
} catch { Write-Host $_.Exception.Message -ForegroundColor Red }
if($PauseAtEnd) { Read-Host 'Press Enter to close'|Out-Null }
exit $exitCode
