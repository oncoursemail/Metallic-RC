# Metallic-RC: transactional progress repair completed

**21 October 2026.** Based on `f27c589549a7040a83fe8e84b969efeb90321732`, in an isolated new worktree. The desktop test entrypoint now selects the verified v3 transactional-progress runner. Earlier failed/candidate files and original run receipts remain intact.

**Completed here:** replace the unreliable live JSON-replacement/mutex mechanism, verify repeated reader/writer stress and interruption handling, apply a newly pinned manual entrypoint, and add read-only process observations for the unresolved attachment problem.

**Not completed:** actual Rec Room agent attachment, original-client discovery/login, room joining, gameplay, or voice. The last actual user run still timed out during attachment. This report does not convert a successful test-harness repair into game/backend acceptance.

## The latest actual user run changed the diagnosis

Run **`20261021T203537.110261Z`** started at **20:35:37.175696 UTC** and ended at **20:35:54.781362 UTC**. It occurred before this transactional repair. Unlike the earlier receipt-publication failure, it completed device acquisition and spawned the exact game process. Attachment then reached its cancellation deadline after **15.006 seconds**; total supervisor duration was **17.606 seconds**.

No instrumentation agent was loaded, the original client was not resumed, and no native discovery/login result was recorded. The owned process job ended with **zero active processes**, the temporary section handle was closed, and the original executable/library hashes matched. Exit code 124 records the controlled timeout/termination, not a proven spontaneous game crash.

This establishes that the prior timeout/cleanup controls worked in that run. It does not make the old logging design reliable under contention, nor establish why this game fails to attach. The record is preserved with source-file digests in [latest-user-run.json](native-progress-store-20261021/latest-user-run.json).

## The logging design was replaced, not given another mutex retry

The preceding approach repeatedly replaced `receipt.json` while another process polled it. The old implementation reproduced Windows access/sharing errors, and successive mutex/reader-priority candidates failed repeated tests. Those failures were not overwritten or counted as passing results.

The new `progress_store.py` uses Python's standard-library SQLite implementation in WAL mode. Each run owns a new private database; an existing database is refused. One writer commits complete state snapshots. Readers retrieve a committed snapshot and release the query before parsing/display work. Live progress never renames, replaces, truncates, or opens `receipt.json`.

Every snapshot has a generation number and SHA-256 checksum. The latest full state is retained alongside bounded phase/status/hash history. Header/schema checks use a single read transaction so initialization cannot be observed halfway through. Invalid checksum/schema/bounds, nonfinite payloads, oversized snapshots, generation exhaustion, and publication failures are not accepted as successful client evidence.

The payload limit is **2 MiB**, the generation limit is **4,096**, and SQLite busy waits are bounded to **200 ms**. A **64 MiB WAL stop threshold** prevents further publication when already exceeded; one transaction can cross the threshold. These are bounded local test logs, not an unlimited event archive or protection against every disk/OS fault.

After the worker and its owned children have exited, the supervisor creates final `receipt.json` and `supervisor.json` **once**. The starting supervisor identity has a separate filename. Existing final reports are not replaced. Cleanup uses best-effort publication and an explicit failure flag; a storage failure cannot turn an unproven run into success or prevent the supervisor from emptying the job.

SQLite WAL permits committed reads alongside a writer; the locally measured tests below verify this use rather than relying on that general capability alone. See [SQLite WAL](https://www.sqlite.org/wal.html) and [snapshot isolation](https://www.sqlite.org/isolation.html).

## Repeated checks on the exact deployed code

The final 18-test progress suite passed **five consecutive runs**. Each suite included a separate reader observing 1,000 updates, a real Windows supervisor observing another 1,000 updates, a long-lived read transaction, a held legacy JSON handle, an interrupted transaction, a forcibly stopped fixture writer, corruption/bounds rejection, and create-only final export behavior.

Across those final repetitions, **5,000 concurrent-reader updates and another 5,000 supervised-worker updates** completed without the old publication/reader failure. The five suite durations were **3.867, 4.391, 3.751, 4.026, and 4.625 seconds**. These totals count the specified stress updates, not every small test snapshot. The tests use new authored fixtures, not game processes.

An early test-fixture teardown left its own SQLite connection open and failed directory removal. That fixture handle was closed explicitly, and the failed log was retained privately. No failed candidate run is included in the five final passing repeats.

| Verification | Observed result |
|---|---|
| Final progress-store suite | **18 tests × 5 consecutive passing runs** |
| Concurrent-reader stress updates | **5,000 passed** |
| Real-supervisor stress updates | **5,000 passed** |
| Deadline, ownership, result, and diagnostic tests | **20 passed** |
| JavaScript routing/diagnostic tests | **10 passed** |
| Private v3 admission checks | **11 passed** |
| Real pinned Frida synthetic-child test | Attach, script loading, cancellation and network guards passed |
| Full locked .NET solution | Passed, zero warnings/errors |
| Existing Core regression | **18/18 passed** |
| Existing Platform regression | **101 assertions passed** |
| Existing Compatibility regression | **570 cases / 4,996 assertions passed** |
| Authored login-profile regression | **106 checks passed** |
| Owned-backend transport regression | **92 checks passed** |
| Installed and fresh original game files | **4 hashes unchanged** |

The synthetic instrumentation test is not original-client attachment. The backend regression uses authored HTTP/WebSocket clients, not the recovered Rec Room game. The CI workflow now includes progress-store tests; Windows-only process tests are explicitly skipped on Linux rather than represented as Linux containment proof.

[Repeated results and source identities](native-progress-store-20261021/repeated-tests.json) · [Progress repeat 1](native-progress-store-20261021/progress-repeat-1.txt) · [Progress repeat 5](native-progress-store-20261021/progress-repeat-5.txt) · [Controller tests](native-progress-store-20261021/supervision.txt) · [Actual instrumentation fixture](native-progress-store-20261021/instrumentation-fixture.json) · [Backend/regression results](native-progress-store-20261021/regressions.json).

## Applied to the existing desktop shortcut

The active private profile is now **`run-manifest-v3-progress-reviewed.json`**, with **15 verified input pins**, including the new progress module. Preflight passed against the same running owned backend. The existing **Metallic-RC Native Client Test** shortcut still selects the reviewed wrapper; that wrapper now selects the v3 runner. The previous wrapper is preserved as `Start-NativeClientTest.before-progress-store.ps1`.

This update did not open the shortcut, request Windows approval, start another original-game process, change the backend deployment, or modify system DNS/firewall/security settings. The same explicit approval requirement, 90-second worker budget, 30-second startup cap, 15-second attachment limit, and job ownership remain.

The next authorized attempt will also record read-only process timing and raw mitigation-query results before attachment and before cleanup. Query denial remains an explicit error, not an assumption that a policy is off. No mitigation policy is changed, and these observations are not yet an explanation of the attachment failure. See Microsoft's [GetProcessTimes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes) and [GetProcessMitigationPolicy](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocessmitigationpolicy).

[Desktop deployment and pins](native-progress-store-20261021/desktop-deployment.json) · [Admission checks](native-progress-store-20261021/manifest-admission.json).

## What remains

The original client's instrumentation attachment remains the next unresolved integration gate. Its latest observed attempt timed out before agent load/resume. No new original-client run was performed during this repair, and no native discovery, authentication, room admission, gameplay, or voice success is claimed.

The unchanged agent is temporary runtime routing, not read-only game analysis or a full operating-system network sandbox. The repair contains no authentication-success patch, certificate-verification bypass, security-policy relaxation, or automatic elevation. The recorded executable/library fingerprints and the live backend's deployed-assembly fingerprint still match.

The backend remained healthy at **127.0.0.1:4187**, with instance **`212d47d08e104ef58c747a4cdf10ece1`**, without a restart by this work. Existing native source-reconstruction totals are unchanged. Only authored source/tests, documentation, and aggregate evidence are published; captured game code, private run databases, game logs, and credentials remain on Main.

Source worktree: `%LOCALAPPDATA%\Metallic-RC\client-progress-store-20261021`.
Private test evidence: `%LOCALAPPDATA%\Metallic-RC\client-progress-evidence-20261021`.

[Preservation checks](native-progress-store-20261021/preservation.json) · [Aggregate summary](native-progress-store-20261021/summary.json) · [Operational boundaries](../../tools/client-lab/README.md).
