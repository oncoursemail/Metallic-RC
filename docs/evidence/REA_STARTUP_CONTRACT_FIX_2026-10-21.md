# Metallic-RC: implemented startup discovery contract

Checkpoint: 21 October 2026 UTC. Exact reference: recovered Steam build **23191908**. This change adds a **compiled C# compatibility library, tests, and a working offline launcher command**. It fixes concrete gaps in the startup-response reconstruction; it does not claim that the original game now logs into a Metallic server.

## Implemented code

`src/Metallic.RC.Compatibility` is now part of `Metallic.RC.slnx`. It contains an authored discovery-response reader/writer and the observed synchronous service-update/cache-change behavior. Mapping views are read-only. Negative-control mutations used during verification are not included in this library.

The launcher now references the library and supports:

```text
Metallic.RC.Launcher check-discovery response.json --build-id 23191908
```

The command validates the bounded response profile and all 31 service addresses. It returns exit code 0 for the supported profile and 2 for a rejected response/build. It does not start the game, contact any endpoint, write a cache, or create a launcher state directory. It does not echo supplied addresses or status-message contents; the privacy fixture checked this explicitly. A successful check is not a login or live-client compatibility result.

The rebuilt launcher is in the project worktree's `src/Metallic.RC.Launcher/bin/Release/net10.0` folder on Main. The previously installed GUI package was not silently replaced, and no Steam files were overwritten.

## Response-format rules recovered and implemented

The actual generated formatter is `Utf8Json.Formatters.RecNet.GXJICYTLDFZ`. Its constructor, Serialize and Deserialize entries were traced back to the exact recovered native bytes and normalized metadata. The constructor registers **76 property-name aliases** for **32 canonical fields**: RecNetStatus plus the 31 service-address fields.

The reader preserves these observed formatter decisions within the declared input profile:

- A root `null` or empty object `{}` produces no response, rather than an initialized successful response.
- Registered names map to the correct field. The implementation does not substitute unrestricted case-insensitive matching for the observed alias table.
- Repeated recognized fields use the last value, including an explicit null. Unknown properties are skipped; a nonempty object containing only unknown fields still produces an object with null fields.
- The writer emits all 32 canonical fields in the recovered order, including null values. Semantic JSON property/value order is checked; identical escaping for every string is not claimed.

A separate startup check identified the status convention: the processing routine calls `string.IsNullOrEmpty` on RecNetStatus. **A nonempty status is an error; even `"OK"` is not a success marker in this path.** The implementation preserves that distinction.

The managed adapter uses System.Text.Json, with explicit 1 MiB and depth-64 limits. These are authored guardrails, not a claim that the original parser has identical limits. Arbitrary escaped property-name matching, malformed UTF-8, and every corner of the original Utf8Json fork remain outside the proven profile.

## Cache transitions and failure ordering

The implementation preserves the prior verified service sequence, including WWW before Strings and StringsCDN. Initial responses populate the service map and produce a reverse cache response. Unchanged loaded-cache responses are not resaved or invalidated. Changed addresses preserve OriginalString comparison semantics in the model, invalidate the cache, and signal that reconnection is required.

Earlier successful updates remain visible if a later required address is missing; the code does not silently turn the native sequence into an all-or-nothing transaction. Missing responses, nonempty statuses, invalid URI values and missing required addresses are distinguished by the validator. The recovered cache namespace is `CachedNameServerResponse_` plus the caller-supplied partition.

This is the synchronous response-application stage. It does not replace the original network transport, retry policy, asynchronous state machine, platform preferences provider or original exception messages. Modern .NET URI behavior is not asserted to match the original runtime for all possible strings.

## The previously omitted cleanup block

The tagged metadata at the cleanup call resolves to **`List<Action>.Enumerator.Dispose`**. Its shared native leaf is the exact three-byte `RET 0` encoding, not an invented replacement. The formerly omitted initialization block calls that cleanup first, then either returns normally or forwards the original saved exception pointer to the native propagation boundary.

The new bounded cleanup model and tests preserve this order and pointer identity. The complete native Windows exception dispatcher, language-specific handler data and OS unwinding are **not** reimplemented. Testing starts at the observed cleanup block with declared synthetic stack state and stops before the propagation implementation.

## Verification results

| Verification | Result |
|---|---:|
| Exact generated-formatter control-flow comparisons | **2,474 cases, zero mismatches** |
| Read / write cases in that comparison | **2,222 / 252** |
| Observed guest instructions in formatter verification | **1,249,734** |
| Wrong empty-object mutation detected | **1 case** |
| Wrong blanket-casefold mutation detected | **305 cases** |
| Wrong first-value-wins mutation detected | **320 cases** |
| Omitted cleanup-block comparisons | **2,048 cases, zero mismatches** |
| Cleanup normal-return / propagation-boundary cases | **1,024 / 1,024** |
| Wrong skip-cleanup / drop-error mutations detected | **2,048 / 1,024 cases** |
| Integrated managed component self-tests | **570 cases, 4,996 assertions passed** |
| Real loopback HTTP fixture | **4 requests passed; listener stopped** |
| Launcher CLI checks | **10 passed** |
| Existing Core acceptance checks | **18/18 passed** |
| Existing Platform/Creator assertions | **101 passed** |
| Full .NET solution build | **Passed with zero warnings/errors** |

The formatter comparison evaluates the original constructor/read/write instruction ranges in an isolated Unicorn CPU guest. Metadata resolution, allocation, GC barriers, JSON token reading, dictionary lookup and string encoding are explicit modeled callbacks. This checks generated formatter decisions under those boundaries, **not the complete original JSON parser/runtime**. Error categories at modeled callbacks are not original OS exception-class proofs.

The final reusable library, with test mutations removed and read-only mapping views, was rebuilt and run against the same 2,474 native-side fixtures with zero differences. Its loopback HTTP fixture exercises successful data, an empty response, malformed JSON, HTTP failure and cancellation behavior using authored code. **The actual Rec Room client was not connected to this fixture.**

The first regression run encountered a Windows junction-fixture path-length failure beneath the deep worktree. No test was disabled: the exact compiled regression outputs were copied to a short task-owned directory, all copied hashes were checked, and all original Core and Platform checks then passed. The first incorrect locked-build command and the rejected raw-x64 provider attempt remain recorded rather than relabeled as successful.

## REA limitations retained

The first combined carrier caused the Serialize entry to be misidentified. A focused PE carrier recovered a separate dossier for the exact method range. Windows REA 6.5's Ghidra adapter rejected the attempted raw-x64 route, so that attempt was not counted as successful analysis.

The grouped and isolated REA records retain all warnings, source-range identities and evidence IDs. Some pseudocode still has absent-callee or tail-path warnings; the independent instruction-level tests are the stated behavioral evidence, not a claim of complete decompilation. Canonical native listings and game pseudocode remain private on Main.

## Reproduce without the game

```text
dotnet build Metallic.RC.slnx -c Release -p:RestoreLockedMode=true
dotnet run --project tests/Metallic.RC.Compatibility.Tests -c Release -- --self-test result.json
dotnet run --project src/Metallic.RC.Launcher -c Release -- check-discovery tests/Metallic.RC.Compatibility.Tests/fixtures/discovery.valid.json
```

The fixture addresses are deliberately non-service `.invalid` names and are never contacted. These commands build/test authored code; native verification separately needs the exact private captured inputs.

[Integrated verification](rea-startup-contract-20261021/integrated-validation.json) · [Formatter comparison](rea-startup-contract-20261021/formatter-comparison.json) · [Cleanup comparison](rea-startup-contract-20261021/cleanup-comparison.json) · [Resolved cleanup context](rea-startup-contract-20261021/cleanup-context.json) · [REA observations](rea-startup-contract-20261021/rea-observations.json) · [Response-name map](rea-startup-contract-20261021/response-contract.json).

## Preservation and remaining acceptance gate

Installed and freshly downloaded RecRoom.exe and GameAssembly.dll were rehashed and still match the original baselines. No game patch, DNS/security change, UAC capture, account login or external game-service request was made. Only bounded native guest snippets, authored C# tests and a temporary loopback fixture were evaluated.

The remaining end-to-end gate is an isolated actual-client startup against a correctly implemented owned discovery/transport service, including asynchronous errors and cache-provider behavior. Login, room joining, two-client state, voice, VR/device support and a full rebuilt game remain unverified. This patch does not increase or relabel the earlier whole-game method-recovery count.
