@echo off
title Metallic-RC REA Steam Capture
echo Metallic-RC: bounded Steam Rec Room runtime capture
echo This requests Windows administrator approval for the reviewed capture job.
echo It holds a temporary shared memory section, launches the original Steam game,
echo observes its game module and metadata for 45 seconds, and cleans up its own process.
echo Output: C:\Users\alexi\AppData\Local\Metallic-RC\rework-20261020\rea-steam-decoding\runs
powershell.exe -NoProfile -Command "$ErrorActionPreference='Stop'; $job='C:\Users\alexi\AppData\Local\Metallic-RC\rework-20261020\rea-steam-decoding\run_steam_recovery.py'; $capture='C:\Users\alexi\AppData\Local\Metallic-RC\rework-20261020\rea-steam-decoding\capture_steam_recroom.py'; if ((Get-FileHash -Algorithm SHA256 -LiteralPath $job).Hash.ToLower() -ne 'ce5f715474db5c0f5aa021ba3aaba9fa9384764bb5a86e7b6ab6e3a85c785856') { throw 'Recovery job hash changed. Re-review required.' }; if ((Get-FileHash -Algorithm SHA256 -LiteralPath $capture).Hash.ToLower() -ne '37ace729f9ee7ce45b4189c6bc54e4f21a1b9f3063b6ee56a1570f4e4ddb5179') { throw 'Capture helper hash changed. Re-review required.' }; $argsList=@($job,'--output-parent','C:\Users\alexi\AppData\Local\Metallic-RC\rework-20261020\rea-steam-decoding\runs','--seconds','45'); $p=Start-Process -FilePath 'C:\Python315\python.exe' -ArgumentList $argsList -Verb RunAs -Wait -PassThru; exit $p.ExitCode"
set captureResult=%errorlevel%
echo Recovery job returned %captureResult%. Detailed results are in the output folder.
pause
exit /b %captureResult%
