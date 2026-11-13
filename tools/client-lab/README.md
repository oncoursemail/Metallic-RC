# Native-client laboratory: transactional progress

This is an explicitly approved, exact-build experiment for Steam build **23191908**. It is not a general injector, a complete network sandbox, or a claim that original-game login works.

## Current state

The latest user-triggered run, `20261021T203537.110261Z`, reached attachment. Attachment canceled after **15.006 seconds**; the supervisor completed in **17.606 seconds**, closed the owned job, and checked the original files. No agent was loaded and the client was not resumed. This is a different result from the earlier receipt-write failure, but it is still **not a successful original-client connection**.

The unreliable progress-file replacement/mutex approach is now replaced with a **single-publisher SQLite WAL progress store**. The final code passed five consecutive 18-test suites, including 5,000 concurrent-reader updates and another 5,000 supervised-worker updates. The updated v3 manifest and existing desktop entrypoint are applied on Main. No new game run or Windows approval request was made during this repair.

## Live progress and final evidence

`progress_store.py` creates a new `receipt.json.progress.sqlite` in the private run directory. It refuses an existing database. One publisher commits complete JSON payloads with sequence numbers and SHA-256 hashes. Readers fetch committed snapshots in read-only mode, close their query before processing the content, and never read a JSON file that the worker is trying to replace.

The database schema/header is checked in one read transaction. An uncommitted initialization is treated as pending, not as corruption. Invalid record bounds, digest mismatches, unsupported schema, nonfinite JSON, oversized state, and generation exhaustion fail rather than silently becoming successful results.

The main row retains the latest full snapshot; a bounded history records generation/phase/status/size/hash metadata. Maximum payload size is 2 MiB and the generation limit is 4,096. Publication stops when the WAL is already above its 64 MiB stop threshold; one commit may cross that threshold. Busy waits are bounded to 200 ms. Local storage can still fail; this is not a guarantee against disk failure or all operating-system stalls.

After the worker and its owned children are gone, the supervisor writes `receipt.json` and `supervisor.json` **once**. Existing final JSON is not overwritten. The initial supervisor identity is saved separately as `supervisor.start.json`. Cleanup is independent of successful progress publication, and a persistence failure prevents a successful native-client result. When possible, a separate one-shot failure receipt preserves the cleanup result.

Do not copy a live SQLite database without its WAL state. Use the finalized JSON after completion, or SQLite-aware inspection while the private run is active. This store is test-run evidence, not the backend account database.

## Process and approval boundaries

The independent supervisor keeps the **90-second total worker budget**, **30-second startup cap**, **15-second attach cancellation**, and per-phase watchdog. Final cleanup/rehashing may add a few seconds; UAC waiting and preflight are not described as part of the worker budget. The PowerShell entrypoint bounds its wait for the exact elevated supervisor handle after approval.

The worker starts suspended, is assigned to a non-breakaway kill-on-close Windows Job Object, and verifies containment before launching a game. The new game must join the same job before attachment. Existing game processes are refused; cleanup never uses process-name-wide termination. The game is not resumed without verified agent/network-guard readiness.

The runner records requested, attached, agent-loaded, routing-ready, resumed, and discovery observations separately. It does not infer login or room joining from a zero exit code or a parsed discovery response. Read-only process timing/mitigation queries now run before attachment and before cleanup; denied queries remain explicit. They do not alter policy or establish why attachment fails.

## v3 input profile and desktop entrypoint

The private manifest pins **15 inputs**, including the new progress module. Legacy v1/v2 runtime profiles are not admitted by the v3 runner. Create a new profile from a still-matching reviewed predecessor; do not edit prior evidence:

```text
python tools/client-lab/prepare-manifest.py --previous C:\private\previous-manifest.json --output C:\private\run-manifest-v3.json
```

The default runner invocation remains preflight-only. `--run` and separately approved Windows elevation are required to execute the original-client test. The PC-specific desktop wrapper also pins the manifest, runner, and launch controller. The previous wrapper and manifests are preserved.

## Reproduce authored checks without launching Rec Room

```text
python -B tools/client-lab/test_progress_store.py
python -B tools/client-lab/test_supervision.py
node --test tools/client-lab/route-policy.test.js tools/client-lab/runtime-agent.test.js
```

There are **18 progress-store tests**, including one Windows supervisor case; **20 control/process tests**, including eight Windows-only cases; and **10 JavaScript policy/diagnostic tests**. Windows-only tests are explicitly skipped on Linux, not presented as Linux process containment proof.

`test_manifest_profile.py` performs 11 admission checks on an explicitly supplied private v3 manifest, without importing Frida or launching a game. `test_frida_control.py` checks the actual pinned instrumentation library on a new synthetic Python process. Synthetic attachment/guard success is not original-game acceptance.

## Runtime-routing scope and remaining gate

The unchanged agent uses temporary in-process URI routing and observations inside only the test process. This is not read-only game analysis. It does not replace installed binaries, alter global DNS/firewall/certificate settings, bypass authentication, disable certificate verification, or weaken process mitigation policy. The observed socket/DNS guards are not proof of coverage over every possible networking mechanism. The game may still write ordinary local preferences/cache data in an approved run.

The original game's attachment problem is still unresolved. No original-client discovery, successful login, room admission, gameplay, or voice result is established by this logging repair. See `docs/evidence/NATIVE_PROGRESS_STORE_2026-10-21.md` for the exact measured checkpoint and preserved latest-user-run evidence.
