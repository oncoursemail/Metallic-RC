# Startup reconstruction and bounded differential verification

Exact target: privately retained Steam build 23191908. These are analysis tools and small authored C# behavior models, not game source, a launcher update, a production backend or a runnable replacement client.

## C# projects

- `BindingProbe` contains the observed 31-service dispatch model and a JSONL test harness. Input: synthetic values and a callback-stop position. Output: the callback trace and whether it completed. The optional `sorted-mutant` argument intentionally uses the wrong ordering as a negative control.
- `ManagedProbe` contains the separately reconstructed two-operation byte codec and a JSONL test harness. The optional `upfront-mutant` introduces an intentionally incorrect up-front bounds check.
- `ObservedServices` contains the local model of URI spelling/cache-change handling and its synthetic tests. It takes a new output-report path. It does not claim original-runtime or native differential equivalence.

All three projects target .NET 10 and have no NuGet package dependencies. The installed SDK used in the evidence is 10.0.402. Rebuild with `dotnet build <project-path> -c Release`. The committed publication-validation report records builds and test runs of these exact publication copies, not only their earlier workspace counterparts.

The input/output paths are explicit. Output files use create-new semantics where the probe accepts a JSONL destination. Do not use a game directory for outputs. Original assemblies are never loaded by these managed harnesses.

## Native verification

`verify_binding_sequence.py` evaluates the exact 726-byte source dispatch slice inside Unicorn, using synthetic response memory and a modeled callback. The callback records arguments and either returns or stops at the selected call position; the service updater itself is not emulated. Caller-saved registers are deliberately clobbered on returns. Native error unwinding and the entire asynchronous method are outside the test.

`codec-verification/differential_codec.py` evaluates two exact codec routines and two helper spans. Bounds/null helper entry stops evaluation. That observes output values and buffer mutation up to the error boundary, not the original runtime exception class or unwind implementation.

The emulator is pinned to Unicorn 2.1.4, wheel SHA256 `d7107500c64ce5c168fbff6bef9485b5db1350050036f4cea568650cf8bdbdf5`. It is installed only under the private verification workspace. The acquisition helper checks the package digest and a synthetic emulator fixture. The package and native game bytes are not bundled with this repository.

## Private workspace layout

The Python orchestrators are retained as executed and depend on the existing private evidence layout on Main:

```text
rea-steam-decoding/
  runs/recovery-/20261020T212503.738773Z/  # pinned metadata/native evidence
  csharp-reconstruction/                # typed indexes and registered parser output
  deep-rea-20261021/                     # read-only atlas, earlier REA evidence
  verification-pass-20261021/            # isolated emulator, codec/service probes
  startup-followthrough-20261021/        # this pass's scripts and outputs
```

The matching root is `C:\Users\alexi\AppData\Local\Metallic-RC\rework-20261020\rea-steam-decoding`. Do not expect an arbitrary source-only checkout to contain the client or captured data. Do not copy captured payloads into the repository to satisfy these paths. The orchestrators create new output directories; use an explicitly separate analysis destination for reruns rather than deleting prior evidence.

`prepare_scope.py` derives the four-function scoped carrier builder from the previous reviewed builder. The carriers use unmodified source pages, not replacement executable code. `run_startup_rea.py` invokes the actual installed REA MCP tool catalog and stores full evidence locally. `audit_startup_ranges.py` checks selected source ranges with Iced. `resolve_cache_evidence.py` checks the reverse field map; `resolve_callsite_metadata.py` separates concrete tagged generic contexts from canonical shared native bodies.

The source report and aggregate verification JSON are under `docs/evidence/REA_STARTUP_VALIDATION_2026-10-21.md` and `docs/evidence/rea-startup-validation-20261021`. Full native listings, source-page carriers, pseudocode, the SQLite atlas, generated game source, and emulation fixture/result files remain private on Main.

## Required interpretation boundaries

Source hashes establish identity. Linear instruction decoding establishes byte accounting. Controlled emulation supports only the declared synthetic state and modeled boundaries. Neither successful C# parsing nor these small probes establishes an original Unity project, complete startup compatibility, authentication, room sharing, VR readiness, or game performance.
