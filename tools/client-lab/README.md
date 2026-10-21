# Original-client integration laboratory — not yet accepted

This folder contains the authored harness for a **single, bounded original Steam client test** against the owned loopback backend. The reference build is 23191908. It is not a reconstructed game executable, a general launcher replacement, or evidence that native login works.

## Current gate

The read-only preflight passed, including the exact game/tool file hashes and the live backend identity. The normal Windows UAC request for the 90-second original-client run was canceled before the elevated runner started. The harness was not retried automatically after cancellation. No original-client discovery, login, or room-join result exists from that request.

The private reviewed launch script and manifest on Main are under `%LOCALAPPDATA%\Metallic-RC\native-client-lab-20261021`. They are not committed because they contain PC-specific paths, pinned private analysis dependencies, and local run context. The manual launcher verifies its pinned runner and manifest again, performs preflight, and requests normal Windows approval. Do not enter real Rec Room credentials during the routing observation.

## Boundaries

`run-native-client.py` refuses an already running Rec Room session. It admits only the exact verified executable/library hashes, binds to its own spawned process and retained process handle, and has a bounded observation deadline. It does not attach to arbitrary user processes. A normal approval is required; it neither self-elevates nor attempts to suppress Windows approval.

`runtime-agent.js` uses temporary in-process instrumentation to route the observed URI constructors to the owned loopback origin and to observe the native discovery formatter result. **This changes the test process's runtime behavior; it is not described as read-only process analysis.** It does not patch installed game files, alter global DNS/firewall/certificate settings, replace an authentication-success check, or disable certificate verification. The original process may still perform its ordinary local preferences/cache writes.

The network guards reject nonadmitted destinations at the observed WinSock/DNS interfaces and include dynamically obtained ConnectEx pointers. These guards are not a Windows firewall or proof that every possible network mechanism has been covered. Their synthetic owned-process test is separate from the original-client run. Original runtime behavior remains unverified until that run is approved and observed.

`route-policy.js` is pure authored policy. `route-policy.test.js` tests local routing, malformed/ambiguous input, destination containment, and redacted path-only diagnostics. It preserves required queries for the actual local request but does not print query, fragment, credential, or response-body values in diagnostics.

## Test without starting a game

```text
node --test tools/client-lab/route-policy.test.js
```

The native runner's default invocation is preflight-only; `--run` is explicit, and elevation is independently checked. The pinned manifest and private dependencies must be present before using it. A zero runner exit does not establish native login or gameplay: inspect the separate receipt fields and actual server observations.

The prepared test must not be described as completed based on synthetic routing tests, static method analysis, or a healthy backend. The current compiled login-response profile and the live fail-closed error endpoint are documented in `docs/evidence/NATIVE_LOGIN_PROFILE_2026-10-21.md`.
