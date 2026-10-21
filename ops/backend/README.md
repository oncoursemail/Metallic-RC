# Local backend operations

These controls operate the **owned loopback backend**, not Rec Room's servers or another Metallic project. Default address: `http://127.0.0.1:4187/` on the hosting PC. No Internet listener, DNS change, firewall opening, public release, or native-client login is implied.

## Create a deployment

```powershell
python ops/backend/install_local_backend.py --dotnet C:\path\to\dotnet.exe --autostart
```

The installer is Windows-specific. It publishes the locked C# backend into a new timestamped release directory under `%LOCALAPPDATA%\Metallic-RC\backend`, records file hashes, copies operations scripts, and creates **Metallic-RC-Backend** as a limited-current-user logon task. It refuses an existing deployment root, a pre-existing same-name task, or an occupied target port. It never silently upgrades an unrelated service or replaces game files. Review upgrades separately, preserve the database, and obtain a verified backup before changing an existing release.

The running server creates owner-restricted storage and an administrator key. That key is not printed, passed as a process argument, or published. Local state and backups must never be committed.

## Check, start, and stop the installed instance

```powershell
powershell -NoProfile -ExecutionPolicy RemoteSigned -File "$env:LOCALAPPDATA\Metallic-RC\backend\ops\Get-LocalBackendStatus.ps1"
Start-ScheduledTask -TaskName 'Metallic-RC-Backend'
powershell -NoProfile -ExecutionPolicy RemoteSigned -File "$env:LOCALAPPDATA\Metallic-RC\backend\ops\Stop-LocalBackend.ps1"
```

The scripts use the recorded deployment identity and private administrator key for a graceful stop. To stop and disable future logon starts:

```powershell
powershell -NoProfile -ExecutionPolicy RemoteSigned -File "$env:LOCALAPPDATA\Metallic-RC\backend\ops\Stop-LocalBackend.ps1" -DisableAutoStart
```

Re-enable intentionally with `Enable-ScheduledTask -TaskName 'Metallic-RC-Backend'` and then start it. The per-process PowerShell execution-policy argument does not modify the machine's saved execution-policy configuration.

## Create an actual Metallic-only account

```powershell
powershell -NoProfile -ExecutionPolicy RemoteSigned -File "$env:LOCALAPPDATA\Metallic-RC\backend\ops\New-LocalAccount.ps1" -Username creator -DisplayName Creator
```

This prompts for a new password. Do not reuse a Rec Room password or provide a third-party session. The deployed database is not populated with the integration suite's temporary test users. This creates an owned backend account; it does not make the original game accept that account yet.

## Recovery behavior

The logon task starts a bounded supervisor. A failed backend child is retried after exponential delays of 2, 4, 8, 16, and 30 seconds, with at most five retries in a ten-minute failure window. The runner verifies the deployed file hashes again before each attempt. Repeated failures eventually stop rather than causing an unbounded restart loop. A normal zero exit, including authenticated shutdown, stops intentionally.

An earlier scheduler-only restart setting did **not** recover the forced-exit test within 110 seconds on Main. It was replaced by the explicit child supervisor and removed from the task definition. The report retains that failure and the subsequent measured retest; no claim relies on an untested checkbox.

The task is **at user logon**, not an elevated system service that starts before login. It has no mechanism to power the PC on. A hung process that never exits is not yet automatically restarted by a health watchdog. After an outage, actual recovery depends on Windows and this user session becoming available. Network partition handling and original-client reconnect are separate work.

## Storage and backups

`data/backend.sqlite` holds owned account/profile/room/session data. `data/admin.key` is private. `data/runtime.json` contains nonsecret instance/process information. `deployment.json` binds the selected release and its hashes. `logs/backend.log`, `logs/backend.stderr.log`, and `logs/supervisor.log` retain bounded operational records.

Backups run every three hours while the backend runs, retaining eight online SQLite snapshots after integrity verification. An administrator can request one immediately with `POST /metallic/v1/admin/backups`. Keep the administrator header out of terminal history and logs; scripts can read its protected local file directly. Local backups do not protect against losing the entire PC or a user-level attacker who can delete both live and backup files.

Restore was tested against an isolated, stopped database and a new data directory. Do not replace a running SQLite database or mix a restored database with stale WAL/SHM sidecars. No automatic destructive rollback is performed.

## Reproduce the checks

```powershell
python ops/backend/verify_backend.py --dotnet C:\path\to\dotnet.exe --output C:\new\verification-directory
```

This builds in locked mode and runs Core, Platform, Compatibility, and owned-backend real transport tests. Test reports distinguish authored clients from original-game acceptance.

`test_deployment_recovery.py` exercises an installed instance and includes an **opt-in disruptive crash test** (`--allow-crash-test`). It verifies the recorded live instance, executable path, and exact backend command before terminating only the owned server child. Do not run the crash test during real user activity. The task runner, Steam client, other services, and PC are not terminated. Failure evidence remains explicit and the helper attempts to bring the owned task back up after a failed check.
