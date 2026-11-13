# Metallic-RC: native login profile and original-client test gate

Checkpoint: **21 October 2026 UTC**, exact recovered Steam build **23191908**, based on owned-backend commit `24b5fb88dda5e174079fa1a1199147ec3ba574d4`.

**The original-client run did not start.** The reviewed preflight passed, but Windows reported that its normal administrator-approval request was canceled. No automatic retry followed that cancellation. There is no native-client discovery, login, or room-join success to report from this attempt.

Work continued without elevation: ten further successful REA dossiers, a compiled and bounded native login-response profile, native-instruction comparisons, diagnostic CLI support, and a deployed **fail-closed native login error envelope**. That error endpoint is not an authentication implementation.

## Exact wire-format findings

The recovered generated formatter is `Utf8Json.Formatters.RecNet.KKMVTPWPBIG`, methods 126718/126719/126720. Its constructor registers **16 exact aliases** for eight fields. Its canonical output order is:

```text
Error, error_description, access_token, refresh_token,
Key, code_hint, login_context, hcc
```

The capitalized **Error** and **Key** are intentional observations, not normalization choices. The compiled `LoginResponseContract` preserves the observed raw-name aliases, field order, null behavior, last-recognized-value wins behavior, and unknown-property skipping. Root `null` and `{}` produce no response. A nonempty unknown-only object is different: it produces an object whose fields are null.

The initial verified profile admits **only absent or null `code_hint`**. Non-null nested hint objects are explicitly outside scope and rejected by the authored profile. They are not silently discarded and are not falsely described as rejected by the original parser. Seven string/null fields are supported. Arbitrary original-parser behavior for escaped property names, malformed byte encodings, nested hint formatters, and all escaping rules is not proved.

The API's classification result deliberately stops at `RequiresTokenAndAccountValidation`; there is no `LoggedIn` result. Its branch model distinguishes security-code continuation, code retry, remote authorization pending, additional challenge work, generic rejection, and missing response. These classification self-tests are **managed model checks based on observed branches**, not a claim of complete native asynchronous-state-machine equivalence.

## Why parsing a response is not native login

The native manual-password path assembles a password grant, then the request builder uses the relative path `connect/token` and adds platform/build/session context. The successful-response path passes the access token, refresh token and key into a separate consumer, then initiates an account request. The inspected consumer performs token/claim-related processing and passes a non-null key to Base64 decoding.

Consequently, replacing this with the existing owned backend's opaque session string would be an unverified protocol substitution. No such substitution or fake successful login was made. A complete native token/key/account contract, asynchronous error/retry behavior, and live original-client acceptance are still required. Shipped production client-secret values encountered in private analysis were **not** reused, published, or sent to a production service.

## Native versus compiled C# verification

The comparison evaluates exact captured constructor/read/write instructions in an isolated Unicorn x64 guest. Runtime allocation, metadata resolution, GC barriers, JSON token access, dictionary lookup, string encoding and null-only nested interface dispatch are explicitly modeled. Original game binaries are not loaded as executable host code.

| Verification | Result |
|---|---:|
| Total synthetic fixture observations compared | **1,428** |
| Read / write fixtures | **1,171 / 257** |
| In-profile or modeled type-error cases | **1,410**, zero differences |
| Deliberately unsupported nested-hint cases | **18**, explicitly stopped outside profile |
| Observed guest instructions across the fixture run | **656,666** |
| Incorrect blanket-casefold mutation detected | **8 cases** |
| Incorrect first-value-wins mutation detected | **779 cases** |
| Incorrect empty-object mutation detected | **1 case** |
| Authored profile/classification self-tests | **106 checks passed** |
| Full backend transport regression | **92 checks passed** |
| Existing Core / Platform checks | **18/18 / 101 assertions passed** |
| Existing discovery Compatibility tests | **570 cases / 4,996 assertions passed** |
| New launcher diagnostic checks | **12 passed** |
| Pure temporary-route policy tests | **7 passed** |
| Full local locked .NET solution build | **Passed** |

Writer comparisons use ordered JSON property/value semantics, not a claim that every emitted escape sequence is byte-identical to the original serializer. The earlier 27,302 reconstructed simple method-entry count is unchanged; neither this authored component nor the test fixtures inflate it.

[Native comparison](native-login-20261021/formatter-comparison.json) · [Self-tests](native-login-20261021/login-self-tests.json) · [Regressions](native-login-20261021/regression-tests.json) · [Handoff/CLI checks](native-login-20261021/handoff-tests.json).

## Real backend change, deliberately no fake success

`POST /recnet/auth/connect/token` now returns HTTP **501** in the observed eight-field LoginResponse envelope. It returns a nonempty `Error`, no access/refresh token, and no key. It does not inspect/store submitted credentials, authenticate against a production service, or assert that native login is implemented. Other unmapped native service routes retain their explicit unsupported responses.

The endpoint was exercised over real local HTTP. Its actual response was then consumed by the captured formatter instructions under the stated modeled boundaries: all eight field values matched. Integration checks verify that it cannot be mistaken for completed login and does not echo synthetic submitted credentials.

This small error-profile update was deployed to the already-owned loopback service at `127.0.0.1:4187` after a verified online backup. The old deployment manifest/release were retained, the server stopped gracefully, the new release passed health/capability checks, and the backup hash remained unchanged. No database-schema migration or elevation prompt was involved. The service still advertises **nativeLogin=false**.

[Deployment](native-login-20261021/deployment.json). This is a native failure-path compatibility improvement, **not** successful original-client authentication.

## Launcher diagnostic

The rebuilt source-tree launcher supports:

```text
check-login-response RESPONSE.json --http-status 200 --build-id 23191908
```

It reports only whether the bounded profile parsed and the pre-account disposition. It does not print tokens, keys, status details or response-body contents, start a game, contact an endpoint, or write a client cache/state directory. A zero exit means the response profile parsed, not that login succeeded. The installed older GUI package was not silently replaced.

## REA results and retained limitations

Ten dossiers were retained for request construction, request sending, response parsing/handling, token consumption, the manual password request, and generated response formatting. Two initial selections did not produce dossiers: the grouped serializer entry and an account-fetch selection. A different focused serializer carrier recovered that entry; the account-fetch selection remains unresolved rather than labeled successful.

The focused writer's **1,232-byte selected extent** was independently decoded into **288 instructions with zero invalid instructions**. Several asynchronous routines have noncontiguous decompiler bodies and unresolved globals/dependencies. Those warnings and range mismatches are preserved in the aggregate records. A function dossier or readable pseudocode is not a complete source project or runtime compatibility proof.

[REA observations](native-login-20261021/rea-observations.json) · [Sanitized contract findings](native-login-20261021/native-login-findings.json) · [Exact alias profile](native-login-20261021/response-profile.json).

## Prepared original-client test

The `tools/client-lab` harness admits the exact original Steam build and pinned tooling, refuses an existing Rec Room session, and confines the observation to its own retained process. The planned observation is 90 seconds. It uses temporary in-process URI routing and observed WinSock/DNS guards; it does **not** patch installed binaries or change machine-wide DNS, firewall, certificate trust, or authentication-success logic.

These guards are not a complete OS network sandbox. Their pure-policy and synthetic owned-process tests do not establish how every networking path of the original game behaves. The normal original client's local preferences/cache writes may still occur during an approved run. No credentials need to be entered for the initial discovery observation.

The actual request in this turn was canceled before elevation, and the private run-receipt directory has no original-client run from that request. Approval was not bypassed or retried automatically. [Approval outcome](native-login-20261021/original-client-attempt.json).

On Main the reviewed script is `%LOCALAPPDATA%\Metallic-RC\native-client-lab-20261021\Start-NativeClientTest.ps1`. It rechecks its manifest and runner hashes before requesting Windows approval. A completed script is not by itself successful login: the resulting receipt must prove the native discovery result and subsequent server observations separately.

## Preservation and next acceptance gate

Installed and fresh `RecRoom.exe` and `GameAssembly.dll` hashes remain unchanged. The backend remains loopback-only and healthy. This pass did not start the original game, request approval again after cancellation, collect account credentials, contact production game services, or publish captured game code/binaries.

Only authored scripts, bounded C# components/tests, and aggregate evidence are published. The original runtime inputs, full native listings, complete fixture outputs, vendor binaries and PC-specific approval manifest remain private on Main.

The immediate gate is an approved, observed original-client discovery run. Native token/key and account integration, then actual room admission/gameplay and voice, remain separate unfinished gates. This checkpoint does not claim that Metallic-RC is playable against the owned backend yet.
