# Metallic-RC: owned backend deployed and tested

Checkpoint: **21 October 2026 UTC**. Built on startup-contract commit `54764a8c409382a40675bde6ec33eb63f1e2c08f`, in a separate worktree and branch. The new service is deployed on Main at **`http://127.0.0.1:4187`**. Its running release was checked against the authored source fingerprint and deployed file hashes.

**Working now:** the owned C# service, persistent Metallic accounts/profiles/room records, revocable sessions, authenticated real-time room presence/pose connections, real HTTP discovery responses, live launcher diagnostics, consistent backups, and measured process-crash recovery.

**Not yet working/verified:** the original 2026 Rec Room client logging into this backend, its native account/session protocol, native room admission/gameplay transport, voice, or a fully reconstructed game. The 31 discovery addresses identify owned endpoint namespaces; they do **not** mean 31 complete native services exist. Unimplemented `/recnet/` requests deliberately return **501**, not fabricated success.

## What was implemented

`src/Metallic.RC.Backend` is an ASP.NET C# project in the main solution. It references the recovered `Metallic.RC.Compatibility` discovery contract and uses locked SQLite dependencies. The existing authoring service on port 4186 was not replaced.

The owned `/metallic/v1` API supports administrator-controlled account creation, password verification, one-hour opaque sessions, logout/revocation, own-profile reads and revision-checked updates, persistent public/private room metadata, and authenticated room WebSocket connections. Passwords are hashed; raw bearer tokens are not stored in the session table. The data directory is restricted to its owning user and a process lock prevents multiple hosts from opening it as owners.

Two independently authenticated test clients joined the same owned room and received each other's server-attributed pose events over actual WebSockets. The tests also exercised private-room denial, room capacity, stale sequence rejection, coordinate bounds, actor-identity spoof rejection, logout disconnect, and reconnect snapshots. This is the newly authored **metallic-room-v1 presence/pose protocol**, not original Rec Room gameplay or full authoritative physics.

The host binds only 127.0.0.1. Host/origin checks, strict bounded JSON, duplicate/unknown-field rejection, rate limits, bounded connection/message queues, and sanitized diagnostics are implemented. These are local-development protections, not a claim of a production security audit, Internet DDoS protection, or zero exploitable defects.

## The discovery connection is live, not a fixture-only claim

The deployed `/nameserver?buildId=23191908` endpoint emits the recovered 32 canonical fields, including a null `RecNetStatus` and 31 owned loopback service addresses. Unsupported build IDs are rejected.

The launcher now has `probe-backend http://127.0.0.1:4187/`. It performed a **real health/discovery/capability handshake**, read the live JSON with the compatibility library, and checked that every service address remained on the owned origin. External origins, user-info URLs, extra paths, and unsupported build IDs were rejected. The installed historical GUI package was not silently replaced; this command is in the rebuilt source-tree launcher.

The actual **1,613-byte live HTTP body** was also checked against the captured generated formatter instruction ranges in an isolated Unicorn guest. Its constructor, reader, and writer ran 1,393, 1,701, and 716 guest instructions respectively. All 32 ordered field values matched, and a wrong-type negative control was rejected. JSON tokenization, metadata resolution, allocation, dictionary lookup, GC barriers, and string encoding remain modeled callbacks. **This proves the stated formatter profile for that real response; it does not prove original-client transport or login acceptance.**

[Live native-formatter profile evidence](owned-backend-20261021/live-native-discovery.json).

## Persistence, backups, and measured recovery

SQLite stores accounts, password hashes, hashed sessions, expiry, profile revisions, and room metadata. The integration tests stopped and restarted the server and verified that profiles, sessions, rooms, and revocations survived. They also restored a verified backup into a separate stopped test database and checked account/session/room recovery. Live pose membership is transient and receives a new room epoch after a server restart.

The running service schedules a consistent online backup every three hours and retains eight verified snapshots. Immediate backup creation, SQLite integrity, SHA-256 verification, and preservation across deployment restarts were tested. These remain **local backups**; they are not immutable, off-machine, or a guarantee against a compromised user deleting both live and backup data.

The deployment has a **Metallic-RC-Backend** logon task running with **Limited** privilege for the current user. It starts a hidden, bounded supervisor; it is not an elevated system service and does not start before user login or power the computer on.

The recovery work found and fixed two real problems:

1. Scheduler-only failure restart was configured but did **not** recover the forced-exit test within 110 seconds. A manual safety restart restored the owned service. That failed attempt remains recorded, and the scheduler restart policy was removed rather than advertised as verified.
2. The first supervisor used a `Start-Process` result that lost its exit status on this Windows PowerShell run. A graceful zero exit was misclassified as failure. The final runner retains a direct `System.Diagnostics.Process` handle, copies stdout/stderr asynchronously, and reads the actual exit code.

The final retest verified intentional graceful shutdown and task restart, then forcibly terminated **only the identity-checked backend child**. The supervisor recovered the service with a new instance in **6.071 seconds**, without a manual start. The verified backup remained unchanged. The task runner, other services, Steam client, and PC were not terminated.

The runner retries after 2, 4, 8, 16, and 30 seconds, with at most five retries in a ten-minute failure window. It rechecks the release hashes before each attempt and keeps intentional zero-exit shutdowns stopped. A process that hangs without exiting is not yet covered by a health watchdog. A physical reboot/power-loss test was not performed.

[Passing deployment/recovery test](owned-backend-20261021/deployment-recovery.json) · [Retained scheduler-only failure](owned-backend-20261021/scheduler-only-failure.json) · [Retained exit-classification failure](owned-backend-20261021/runner-exit-failure.json) · [Deployment identity](owned-backend-20261021/deployment.json).

## Verification results

| Check | Observed result |
|---|---|
| Full .NET solution, locked dependency mode | Passed; zero warnings/errors in local build |
| Owned-backend integration assertions | **88 passed**, using **82 real HTTP requests** plus WebSocket traffic |
| Accepted test WebSocket connections | **4**, covering two simultaneous accounts, reconnect, and restart |
| Concurrent profile writes | Exactly one accepted update; stale revision rejected |
| Privacy, authorization, malformed input, capacity, revocation, and rate limits | Negative checks passed |
| Real SQLite restart and separate backup restore | Passed |
| Existing Core acceptance | **18/18 passed** |
| Existing Platform/Creator regression | **101 assertions passed** |
| Existing Compatibility regression | **570 cases / 4,996 assertions passed** |
| Live launcher connection checks | **5 passed**, including four rejection cases |
| Live discovery through captured formatter instruction profile | All **32 field values** matched; negative control rejected |
| Graceful deployed stop and task start | Passed |
| Verified-child forced-exit recovery | Passed in **6.071 seconds** |
| Installed and fresh original executables | Four hashes unchanged |

The assertion count is not a count of complete game systems. The temporary integration accounts were removed with their test directories; they were not installed as production demo users.

[Local suite evidence](owned-backend-20261021/validation.json). The new workflow defines Windows and Linux authored-service checks. **This checkpoint does not claim hosted CI or Linux runtime success.**

## Original-client routing investigation

Eight additional REA/Ghidra dossiers were retained across the environment parser, Core getters, concrete release-provider getters, and their loader. The provider getters supply the strings consumed by Core initialization. In the inspected loader instruction path, the environment is assigned the `prod` literal and the nameserver override is assigned `String.Empty`.

An override parser exists and can parse a URI, but **that alone is not evidence that this release accepts a user-supplied launch flag or configuration-file override**. No such live routing entry point was verified in this pass. The loader's pseudocode has truncation/PIC warnings and spans more than its first unwind range; those limitations are preserved. This observation is about the inspected provider path, not proof that every possible path lacks an override.

No guessed switch, global DNS redirection, security change, binary patch, or fabricated native login was used to claim progress. The next native acceptance gate is a verified, isolated client-side routing/integration mechanism, followed by the actual account/session and room transport contracts. The existing method-reconstruction counts were not inflated by the new backend code or dossiers.

[Routing observations and REA evidence IDs](owned-backend-20261021/native-client-routing.json).

## Where it is and what stayed unchanged

The deployed backend is under `%LOCALAPPDATA%\Metallic-RC\backend`. Its operations scripts provide status, graceful stop, and local account creation. The separate implementation worktree is `%LOCALAPPDATA%\Metallic-RC\backend-work-20261021`. See [backend API notes](../../src/Metallic.RC.Backend/README.md) and [operations instructions](../../ops/backend/README.md).

Only authored code, tests, operation scripts, package lock files, and aggregate evidence are intended for publication. Captured game binaries, full decompiled game source, canonical native instruction/pseudocode files, SQLite user state, and administrator credentials remain private on Main.

The installed and freshly downloaded `RecRoom.exe` and `GameAssembly.dll` still match their earlier hashes. No original game process was launched by these tests. No public listener, firewall rule, DNS entry, certificate trust change, or public-domain deployment was made. No merge or public release is claimed.

[Preservation checks](owned-backend-20261021/preservation.json) · [Authored file identities](owned-backend-20261021/authored-file-manifest.json).
