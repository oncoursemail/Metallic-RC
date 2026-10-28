# Native runner output fingerprints

Source candidate, 2026-10-28. No native client, Frida device, game, model or service was run for this change.

`tools/client-lab/run-native-client.py` subscribes to the pinned device's `output` signal before spawning.
It admits a PID only after the retained process image matches and Windows Job membership is verified.
Earlier events increment a count without retaining their PID, file descriptor or payload. Coverage therefore begins after ownership admission; this cannot reconstruct output already dropped during spawn/admission.

For that admitted PID, exact integer file descriptors 1 and 2 and exact `bytes` payloads are accepted.
The callback maintains independent stream byte/event counts, EOF state and SHA-256 fingerprints of at most the first 65,536 bytes per stream.
Additional accepted bytes increase the count and truncation flag without extending the fingerprint. An empty byte string marks EOF; subsequent events on that stream are counted separately and ignored.
The implementation stores no raw output buffer, text, body snippet or new output log. It does not classify or diagnose the startup failure.
Fingerprints remain private diagnostic data: hashes are not a privacy guarantee, especially for predictable short output.
Existing private runner/player logs are outside this change.

A lock protects callback updates and detached JSON-safe snapshots. The callback performs no ledger, disk or SDK calls.
`startupOutputBeforeAttach` records a snapshot before the attachment attempt.
After process/instrumentation cleanup, capture seals before unsubscribing outside the lock; `startupOutput` records the final accepted-data snapshot and subscription status.
Unsubscription uses the existing cancellation helper with at most one second of the remaining total deadline.
Timeout/failure appears under cleanup diagnostics without replacing the original outcome. Sealing prevents late callbacks from changing accepted data even if unsubscription cannot finish.
Repeated/reentrant cleanup cannot report a pending unsubscribe as complete, and removes only the exact owned callback.

The vendored Frida 17.24.3 Python source declares `DeviceOutputCallback = Callable[[int, int, bytes], None]` and device `on/off("output", callback)` overloads.
Only that local API declaration/implementation was inspected; no native instrumentation was imported in the tests.

Offline verification:

```powershell
python -B tools/client-lab/test_startup_output.py
```

Fourteen new unittest cases passed. They import the actual runner and exercise synthetic events, per-stream bounds/hash correctness, strict identities/types, EOF, snapshot independence, concurrent callbacks, reentrant/pending cleanup and subscription failures.
Fake-worker cases verify subscription-before-spawn, binding-after-admission, pre-attach/final receipts, image/job refusal, rejected library version and preservation of the original attach failure on unsubscribe timeout.
Four unchanged cancellation-helper cases also passed: normal return, preservation of the original error, cancellation of a synthetic wait and refusal before calling with zero budget.
These checks establish source behavior under injected conditions, not successful original-client startup, attachment, login or gameplay.

Existing manifests, pinned profiles, budgets and network/security policies are unchanged.
Because the runner is hash-pinned, an old manifest must still refuse the changed runner. A later native attempt requires the existing separate review/admission process; this note grants no new authority.
