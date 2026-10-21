# Metallic-RC: startup, service binding and native/C# verification

Checkpoint: **21 October 2026 UTC**, actual recovered Steam build **23191908**. This continues the real-client reconstruction. It does not introduce another prototype game, replace Steam files, or claim a finished game/server.

## What changed in this pass

REA/Ghidra produced **four additional native-function dossiers** covering the service-related preference operation, reverse response construction, `RecNet.Core.Initialize`, and `RecNetNameServerResponseChangedException` construction. The canonical evidence bundle was exported and the analysis session closed successfully.

An independent Iced instruction pass accounted for **5,269 selected source bytes / 1,104 instruction records**, with **zero invalid instructions**. This is selected-range byte coverage, not proof that every branch is reachable or every game method is understood.

Most importantly, the C# checks now include comparisons with **the original instruction bytes evaluated inside an isolated CPU emulator**, rather than only checking generated source syntax or comparing two newly written implementations. The emulator never loads the original DLL into Windows as executable code. Each test has explicit entry/stop ranges and synthetic memory; unsupported paths stop rather than being invented.

## Native-versus-C# checks

| Check | Observed result | Boundary |
|---|---:|---|
| Service-binding dispatch | **2,048 cases; 0 mismatches** | A 726-byte native caller slice, with the service updater modeled as a callback |
| Callback continuation/stop | **64 completed, 1,984 stopped cases** | Stops injected at each of the 31 call positions; not native exception-unwind execution |
| Deliberately wrong sorted dispatch | **448 cases rejected** | Proves the check notices changes to the observed call order |
| Four-byte codec comparison | **5,912 cases; 0 mismatches** | Two native codec methods plus two real helper spans, in synthetic guest memory |
| Partial-write codec failures | **657 cases preserved** | Bytes written before later bounds-helper entry match the C# behavior |
| Deliberately wrong up-front bounds check | **657 cases rejected** | A superficially reasonable rewrite changes failure side effects |
| Service-address C# model | **2,037 cases / 6,045 assertions passed** | Managed model tests only; not original-runtime differential validation |

The service-binding test evaluated **210,560 guest instructions** and observed **33,728 callback invocations**. It also clobbers caller-saved registers on modeled callback returns, so the test does not accidentally rely on unrealistically preserved callback registers. The original response memory remained unchanged by the dispatch slice.

The codec recheck evaluated **159,391 guest instructions**. Its outcomes were 3,144 successful cases, 2,733 bounds-helper entries and 35 null-helper entries. Tests compare returned values, buffer contents and the observed success/error category. They stop at the original error-helper entry: they do **not** establish the original exception class, full unwind behavior, concurrency, Unity runtime integration, or non-Windows equivalence.

These limited tests are not added to the earlier **27,302 reconstructed leaf-method entries**. The existing generated-source inventory and its unresolved bodies remain unchanged.

## Concrete service-discovery findings

### 1. Response fields must not be dispatched in guessed order

The client calls its updater in this observed service-ID order:

```text
0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
16, 17, 18, 19, 20, 21, 22, 23, 24, 27, 25, 26, 28, 29, 30
```

In particular, **WWW (27) is visited before Strings (25) and StringsCDN (26)**. Sorting the entries numerically would change which updates have happened when a later callback fails. The new C# `ObservedServiceBindingSequence` preserves this order and the corresponding recovered response-property names. It is a bounded dispatch model, not the complete asynchronous response-processing method or a JSON decoder.

### 2. The reverse cache-related path uses the same 31-field map

Method **148091**, recovered name `RecNet.Core::VVSKWLPRHAJ`, reads service dictionary entries, calls `System.Uri.OriginalString`, and assigns the results into a response object. All **31 reverse assignments** match the field offsets independently observed in the incoming response-processing path.

That provides a checked relationship between response fields and stored service addresses in both directions. It does not yet establish the complete persistence format or which platform-specific preference provider is used.

### 3. Concrete generic contexts are now separated from shared native bodies

Five source-observed, tagged metadata slots were decoded against the pinned version-aware metadata reader and captured registration tables. They identify:

- The response allocation type as `RecNet.Core.NameServerResponse`.
- The generic string-returning conversion call's type argument as `RecNet.Core.NameServerResponse`.
- Dictionary getter/setter contexts as `Dictionary<RecNet.Service, System.Uri>`.
- The preference interface as `RecRoom.Preferences.STAKPLUPOSH`.

A validation check rejected an overly strong assumption: **the three concrete generic contexts do not each have a direct concrete-instantiation row in the recovered generic method table**. The code targets do match their generic method definitions through canonical shared bodies. The reports retain both facts instead of fabricating one-to-one native functions for every specialization.

The preference interface's observed slot 22 takes one string and returns a Boolean; slot 24 takes two strings and returns `SetResult`. These signatures and their call sites are recorded. A method's obfuscated name or signature alone is not sufficient to declare the behavior of its unknown storage implementation.

### 4. Address spelling and failure ordering matter

The separate service-address model preserves the observed behavior of appending a trailing character when absent, constructing a URI, comparing `OriginalString` values when cached data was loaded, and retaining a previously set “changed” flag. Its tests include missing values, missing cached keys, invalid URIs and differently spelled but semantically equal URI values.

Those are tests of authored C# modeled from native branches and recovered fields. Modern .NET URI behavior has not been proved equivalent to the original client runtime for every input. No live service URL, account token or server response was acquired or used.

### 5. Startup and endpoint-change handling are distinct paths

The new initialization dossier exposes guarded initialization, typed request-queue fields and calls through the recovered cancellation/token infrastructure. The endpoint-change path uses a specifically typed `RecNetNameServerResponseChangedException` rather than an unnamed failure.

This does not retroactively establish the cause of every reported crash. The earlier recorded DNS failure followed by `Application.quit` remains a separate observation. No startup, account login, or room connection has been demonstrated against Metallic-owned services in this checkpoint.

## What is still unresolved in these functions

The independent full-range listing retains **35 bytes omitted from REA's represented body of `RecNet.Core.Initialize`, including 33 non-0xCC bytes**. They include an additional disposal-like call and native handler path. Their complete exception/runtime meaning is not established. Two other selected methods each have one omitted 0xCC byte; those bytes are recorded, not automatically declared harmless.

The complete instruction listings and canonical REA pseudocode remain private on Main. Generated analysis PE headers and export labels are scaffolding around exact source pages; absent dependencies/globals still limit decompiler conclusions. No whole-program or original-source recovery claim follows from these dossiers.

## Evidence and authored code

[Aggregate checkpoint](rea-startup-validation-20261021/summary.json) · [REA records](rea-startup-validation-20261021/rea-dossiers.json) · [Selected-range audit](rea-startup-validation-20261021/selected-range-audit.json) · [Dispatch comparison](rea-startup-validation-20261021/binding-differential.json) · [Codec comparison](rea-startup-validation-20261021/codec-differential.json) · [Reverse cache bindings](rea-startup-validation-20261021/cache-findings.json) · [Tagged contexts](rea-startup-validation-20261021/callsite-contexts.json).

Authored probes, reconstruction models and orchestration source are under `tools/rea/startup-validation`. The native emulator is **Unicorn 2.1.4**, installed only in the private verification workspace from a SHA256-pinned wheel. The new models use the already installed .NET SDK. Native binaries, metadata, the database, full native listings and game pseudocode are not committed here.

New REA evidence IDs:

| Method index | Selected routine | Evidence ID |
|---:|---|---|
| 148090 | Core preference/key operation | `ev_e9f8ec4f42b9f821ee6672416943d8b696ec4f7ee8d5b9b45281016efe1660e3` |
| 148091 | Reverse response construction | `ev_a38acfc633b193a5644a57cad16ffe0c4cb05a2970dbff1c4bd42b7df36db4ef` |
| 148096 | Core.Initialize | `ev_69fd8947e185f97d0a8bd587ca0d4ae9cb1fa745166ca693b0989b47a95837fb` |
| 145417 | Endpoint-change exception construction | `ev_7d33461c1d21d3d1e096e948e98f561292dced7fec5d7925c54b1b4af74bb172` |

## Where to inspect this on Main

```text
C:\Users\alexi\AppData\Local\Metallic-RC\rework-20261020\rea-steam-decoding\startup-followthrough-20261021
```

Use `CHECKPOINT.json`, `BindingProbe/ObservedServiceBindingSequence.cs`, `binding-differential/summary.json`, `codec-recheck/summary.json`, `cache-findings.json`, `callsite-metadata.json`, and `rea-dossiers/startup-cache/`. The older full C# reconstruction remains in the sibling `csharp-reconstruction/CSharp.Source` directory.

The existing **Metallic-RC Code Analysis** folder also contains `CURRENT-STARTUP-CHECKPOINT.md` for navigation. Neither the new probes nor analysis derivatives should be copied over the Steam installation.

## Preservation and next acceptance gate

Both installed and fresh copies of `RecRoom.exe` and `GameAssembly.dll` were rehashed after this work and still match their original baselines. This pass did not launch the game, request administrator elevation, patch its files, modify DNS/security settings, or contact game services. Only bounded guest instruction snippets and authored managed probes were evaluated.

The next compatibility gate is the **complete startup response contract**, including serialization names, cached-response transitions and remaining async/error paths. A local contract fixture must then be checked against the actual client before claiming owned-backend login or multiplayer. This remains a draft analysis/reconstruction checkpoint, not a merged release or finished game.

The exact publication copies of all three C# probe projects were rebuilt and rerun before commit: **7,960 native-comparison cases, zero mismatches**, plus the separate **6,045 service-model assertions**. [Publication validation](rea-startup-validation-20261021/publication-validation.json).
