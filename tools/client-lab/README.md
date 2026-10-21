# Native-client test controller — repaired startup supervision

This is a single, explicitly approved test of the owner's exact Steam build **23191908** against the owned loopback backend. It is **not** a playable rebuilt game, a general process injector, a complete network sandbox, or evidence that native login works.

## Current result

The first actual-client attempt on 21 October 2026 failed while attaching the instrumentation agent. It recorded zero agent events and lasted 200.816 seconds because the previous 90-second observation timer did not cover startup. The installed executable/library hashes remained unchanged.

The controller defects have now been repaired and checked with authored fixtures and the actual pinned instrumentation library. The same instrumentation successfully attached to a new synthetic Python child, and its network guards allowed the owned service while rejecting tested nonadmitted destinations. **This does not prove it can attach to the actual Rec Room client.** The subsequent normal Windows approval request was canceled before the revised test started. No automatic retry followed. Native attachment, discovery, login, and room/gameplay acceptance remain unverified.

## Independent time and ownership boundaries

`run-native-client.py` now has a supervisor and a separate worker. The supervisor creates a non-breakaway Windows Job Object with kill-on-close semantics, starts the worker suspended, assigns it to the job, and only then lets it run. The worker verifies job membership before doing anything with the game. The game process must also be in that job before attachment.

`--seconds 90` is the **total worker lifetime budget**, including startup. It no longer means 90 seconds *after* an unlimited attach. Startup is capped at 30 seconds, attachment has a 15-second cooperative cancellation deadline, and the independent parent checks stalled phases even if a native call ignores cancellation. The parent allows a one-second scheduling margin on per-phase watchdog checks. Cleanup waits and final hash verification are reported separately and can add a few seconds to the wall-clock duration.

The PowerShell entrypoint waits at most 130 seconds for its exact elevated supervisor process after approval. UAC waiting and the preflight hash checks are not described as part of the worker's 90-second budget. There is no self-elevation or attempt to approve a Windows prompt on the user's behalf.

Only the new test job and retained handles are controlled. Existing Rec Room sessions are refused. The job is emptied on success, failure, timeout, and ordinary worker exit; an orphan child cannot be mistaken for completed cleanup. Fixture tests also verify cleanup after the supervising fixture exits unexpectedly. No process-name-wide termination, unrelated service stop, or persistent injection is used.

These controls are process-lifetime management, **not an operating-system network sandbox**.

## Truthful results and diagnostics

Phase changes are written before spawn, attach, script creation/loading, resume, observation, and cleanup. Each completed/failed phase records elapsed time. The supervisor writes its own independent receipt even if the worker cannot finish its receipt. A stalled attach therefore no longer leaves only an unexplained `preparing` record.

`runtimeRoutingRequested` records intent. `attached`, `agentLoaded`, `runtimeRoutingHookUsed`, `processResumed`, and `gameJobMembershipVerified` are separate observations; they start false. A zero worker exit without the required discovery evidence is converted to a non-success result. Native login and room-join status are not inferred from successful discovery.

Ordinary agent diagnostics remain bounded. A failure is still recorded after that buffer fills, and repeated failures cannot create an unlimited error stream. The game is never resumed when agent/network-guard readiness has not been established.

## Input verification

The v2 private manifest pins the original game executables, Python runtime helpers, authored runner/supervision modules, agent, route policy, method identities, launch controller, and reviewed Frida files. Missing required pins, duplicate paths, changed files, wrong build/port/root, or a changed runner identity fail before game execution.

Create a **new** v2 manifest from a still-matching reviewed v1 manifest:

```text
python tools/client-lab/prepare-manifest.py --previous C:\private\run-manifest.json --output C:\private\run-manifest-v2.json
```

This preserves the original manifest; it does not launch a game or request elevation. The owner-specific desktop entrypoint independently pins the new runner, manifest, and PowerShell controller. On Main, **Metallic-RC Native Client Test** now points through the repaired entrypoint. Its previous wrapper remains saved separately.

## Tests that do not run the game

```text
python -B tools/client-lab/test_supervision.py
node --test tools/client-lab/route-policy.test.js tools/client-lab/runtime-agent.test.js
```

The Python suite has 19 tests: 12 portable control/result tests and 7 Windows process-containment tests. The Windows suite includes an uncancellable attach-like stall that exercises the actual per-phase watchdog. Windows-only cases are explicitly skipped on other operating systems, not represented as passing native containment tests.

The JS suite contains 10 authored policy/diagnostic tests. Private-manifest admission tests are available through `test_manifest_profile.py`; they perform no network request or game launch. `test_frida_control.py` exercises cancellation, spawn, attach, script load, and network guards on a **new synthetic Python child** under job supervision, using an explicitly supplied reviewed vendor directory. It never selects a running game by name.

Hosted CI runs only the authored, non-game tests. The private original client, vendor binaries, analysis dumps, game credentials, and PC-specific approval manifests are not uploaded.

## Runtime-routing scope

`runtime-agent.js` changes temporary URI routing inside the newly created test process. It is not read-only analysis. It does not patch installed game files, change global DNS/firewall/certificate settings, replace authentication-success checks, or disable certificate verification. The game can still create its ordinary local preferences/cache files.

Observed WinSock/DNS guards reject tested nonadmitted destinations, including dynamically obtained ConnectEx pointers. They are not a proof that every possible networking mechanism is covered. The agent and runtime integration may fail on the real client even when synthetic tests pass. There is no stealth/anti-detection workaround in this repair.

See `docs/evidence/NATIVE_RUNNER_REPAIR_2026-10-21.md` for measured results, preserved failure evidence, and the remaining actual-client gate.
