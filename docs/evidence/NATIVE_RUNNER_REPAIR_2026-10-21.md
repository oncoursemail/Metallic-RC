# Metallic-RC native test: timeout, cleanup, and result-reporting repair

**21 October 2026. Base:** `5a3d55421ef7fc70b8f9994bef386253319a7753` (native-login profile checkpoint). Work is isolated on a repair branch; neither main nor the previous source checkout was overwritten.

**Result:** the known test-controller defects are repaired, tested, and applied to the existing desktop shortcut. **Successful attachment to the actual Rec Room client is not yet verified.** The original game's native discovery, login, room joining, gameplay and voice are not established by this checkpoint.

## What actually failed

The original run started at **17:24:21.190313 UTC** and ended at **17:27:42.006401 UTC**. It took **200.816088 seconds**, despite advertising a 90-second test, because that timer began only after attachment and script loading.

The retained error is `ProcessNotRespondingError`: the process either refused to load the instrumentation agent or terminated during injection. There were **zero agent events**. The original receipt's `runtimeRoutingHookUsed=true` was a preset intention, not proof that a hook had been installed. The game process exited with code 1; the temporary section handle closed, and both inspected original files still matched their hashes.

The original receipt was preserved byte-for-byte, SHA256 `8da39b3c41ed6f5f7a992ee29f31dee8e7b0061804cd47328f4b4f8592e8a904`. The error does **not** identify the precise root cause. Matching Application crash events were not found in the inspected time window, and additional security-log checks yielded no readable matching-window evidence. That absence is not proof that protection software, a loader fault, or another particular component caused or did not cause the failure.

[Original failure](native-runner-repair-20261021/original-failure.json) · [Passive diagnostics](native-runner-repair-20261021/passive-diagnostics.json).

## Implemented fixes

### 1. Time limits now cover startup

The launcher starts a separate supervised worker. Its **90-second total budget includes startup and observation**. Startup is limited to 30 seconds. The attach call has a 15-second cancellation deadline using the pinned Frida library's actual `Cancellable` context. Spawn, script creation/loading, resume, status queries, and cleanup also have explicit bounds.

A separate parent checks phase and total deadlines. It can terminate the owned job when a blocking native call ignores cooperative cancellation. Per-phase watchdog checks permit one extra second for cancellation/scheduling. Child cleanup and final file rehashing may add a few seconds after the active worker deadline; UAC waiting and preflight checks are not misrepresented as part of the worker's 90-second runtime.

The PowerShell entrypoint also limits its wait for the exact elevated supervisor handle to 130 seconds after approval. There is no self-elevation, automatic approval, or automatic retry after cancellation.

### 2. Process ownership and cleanup are independent of the failed attach

The supervisor creates a Windows Job Object with kill-on-close enabled and breakaway disabled. It creates the worker suspended, assigns it before execution, and retains the process/job handles. The worker verifies its membership before launching the game; the game must also be a member before attachment.

The job is emptied after normal return, failure, and timeout. A new suspended client or a child crash helper cannot silently survive a successful controller exit. Tests exercise termination of a fixture worker and grandchild, successful worker exit with a lingering child, and owner-process death. Existing processes are not selected for termination, and an existing Rec Room session still prevents a test launch.

This is process-lifetime containment, **not a complete operating-system sandbox or network firewall**.

### 3. Receipts distinguish intent from observed progress

The controller records a phase before each potentially blocking operation. The parent records its own result even if the worker cannot finish writing a receipt. Attachment, agent load, routing-hook readiness, process resume, job membership, and parsed discovery are separate facts initialized to false.

A worker exit code of zero is insufficient: the parent requires actual recorded discovery prerequisites, unchanged original file hashes, an empty job, and no agent failure. A clean but unproven run is non-success. Native login and room-join flags are never inferred from discovery.

The agent's ordinary diagnostics stay capped. A failure still reaches the controller after the ordinary event buffer fills; repeated failures do not create an unlimited error stream. Missing guard/agent readiness prevents resuming the original client.

### 4. Input admission and the desktop entrypoint were updated

The v2 manifest pins **14 inputs**, including the exact executable/library, authored runner, supervision modules, agent, policy, identities, reviewed runtime helpers and vendor files, and PowerShell launch controller. Duplicate paths, missing pins, altered files, wrong build/root/port, and changed runner identity fail before execution.

The existing **Metallic-RC Native Client Test** desktop shortcut now goes through the repaired, hash-checked entrypoint. The previous wrapper is saved as `Start-NativeClientTest.before-v2.ps1`; the original manifest and old failed-run receipts remain intact. Updating the shortcut did not open it again.

[Desktop deployment](native-runner-repair-20261021/desktop-deployment.json) · [Admission tests](native-runner-repair-20261021/manifest-checks.json).

## Tests completed

| Test | Result |
|---|---|
| Controller/deadline/result tests | **19 passed** on Main: 12 portable and 7 real Windows process/job tests |
| Uncancellable attach-like fixture | Independent phase watchdog terminated the owned wait and left the job empty |
| JavaScript route and agent diagnostic tests | **10 passed** |
| Private-manifest admission tests | **9 passed**, including rejection cases; no Frida import/game launch |
| Real pinned Frida cancellation | An actual native wait canceled in **0.210 seconds** |
| Real Frida attach/script/guard test | Passed on a **new synthetic Python child** inside the supervised job |
| Synthetic child's owned HTTP request | Reached the live Metallic backend |
| Synthetic child's nonadmitted destinations | Tested external IP/other local port/DNS requests were rejected |
| Full locked .NET solution build | Passed, **zero warnings/errors** |
| Existing Core regression | **18/18 passed** |
| Existing Platform regression | **101 assertions passed** |
| Existing Compatibility regression | **570 cases / 4,996 assertions passed** |
| Authored native-login profile | **106 checks passed** |
| Owned-backend real transport tests | **92 checks passed** |
| Original installed and fresh game files | **4 hashes unchanged** |

The fixture is not Rec Room, and fixture attachment success does not establish target-game compatibility. The native game was not launched during these tests. A first fixture cleanup exposed a Windows current-directory teardown race; the fixture now retains and waits for the relevant child handle rather than assuming an empty job means every filesystem reference has already been released. The final test run passed; the earlier failed log remains in the private evidence directory.

[Controller tests](native-runner-repair-20261021/supervision-tests.txt) · [JavaScript tests](native-runner-repair-20261021/javascript-tests.txt) · [Actual instrumentation fixture](native-runner-repair-20261021/instrumentation-fixture.json) · [Fixture supervisor](native-runner-repair-20261021/fixture-supervision.json) · [Backend/regression results](native-runner-repair-20261021/regressions.json).

The CI workflow now includes the portable and Windows process-control cases alongside route/agent tests and owned-backend checks. Windows-only tests are explicitly skipped on Linux rather than represented as native Linux containment proof. Hosted CI results are separate from the local results above.

## The controlled retry did not start

One normal approval request for the repaired test was made at **17:48:51.154938 UTC**. At **17:50:53.519313 UTC**, Windows reported **“The operation was canceled by the user.”** No v2 worker or original-game run directory was created. No second prompt or automatic retry followed.

Therefore this checkpoint **repairs the timer, cleanup, readiness checks, evidence reporting, and manual entrypoint**, but it cannot certify that the actual game's agent-loading problem is resolved. The earlier error remains the latest observed original-client attachment result. A successful new approved run is needed to measure the next client-specific failure or establish discovery progress; no bypass of the game's authentication, certificate checks, system security, or Windows approval is included.

## Preservation and remaining work

The owned backend remained healthy on `127.0.0.1:4187`, with the same instance throughout these checks. Its deployed assembly still matched the recorded hash; it was not restarted or reconfigured by the repair. Installed and fresh `RecRoom.exe` and `GameAssembly.dll` remained unchanged. No global DNS/firewall/security change or existing-game-process termination occurred.

The repaired test does not make the original client log in, admit it into a room, or produce a rebuilt Unity project. The next acceptance gate is successful, observed original-client attachment/agent readiness, followed by verified discovery and the actual native authentication/account and room transport contracts. Existing reconstructed-source totals are unchanged.

On Main, the active private profile is under `%LOCALAPPDATA%\Metallic-RC\native-client-lab-20261021`; the repair worktree is `%LOCALAPPDATA%\Metallic-RC\client-repair-20261021`. Detailed private test evidence is under `%LOCALAPPDATA%\Metallic-RC\client-repair-evidence-20261021`.

[Preservation and backend health](native-runner-repair-20261021/preservation.json) · [Aggregate summary](native-runner-repair-20261021/summary.json) · [Runner documentation](../../tools/client-lab/README.md).

## Primary implementation references

- Microsoft, [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects) and [extended job limits](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_extended_limit_information).
- Frida, [attach cancellation parameter](https://frida.re/docs/frida-core/method.Device.attach.html) and the locally pinned Python binding's `Cancellable` implementation. The installed binding's real cancellation path was tested rather than assumed from a different release's examples.
