# Metallic-RC: actual Steam-build C# reconstruction

Completed: 2026-10-21T00:16:50.867779+00:00. Exact Steam build **23191908**. This is a **partial reconstruction of the real client**, not a substitute game. The original capture and both Steam copies remain unchanged.

## Start here on Main

Open `CSharp.Source/` for one generated C# file per assembly. Start with `Assembly-CSharp.dll.partial.cs`, `RecNet.Runtime.dll.partial.cs`, and `Circuits.V2.dll.partial.cs`. The source folder contains **419 files totaling 145,020,870 bytes**. Framework and Unity assemblies are included, so these totals are not exclusively game-authored code.

All **419 generated files passed C# syntax parsing with zero errors**. **Syntax parsing is not semantic compilation or a successful Unity/game build.** Compiler-generated names were escaped for source display. Original metadata names, tokens, type IDs and native addresses remain in the typed indexes; aliases have not been validated against Unity serialization or reflection.

For original-name type/method/field details, use `typed-reference-v3/native-types.jsonl`, `method-signatures.jsonl` and `field-layouts.jsonl`. The large `CSharp.Declarations.reference.cs` is a raw declaration reference, not the preferred syntax-checked source presentation.

`partial-csharp/PartialAssemblies.NOT_A_GAME/` contains the managed analysis assemblies. **Do not run these or copy them into Steam.** Their purpose is browsing and reconstruction.

## What was recovered

| Layer | Verified result |
|---|---:|
| Managed images / assemblies | 419 |
| Defined types | 63,648 |
| Typed method declarations | 409,503 |
| Fields | 252,928 |
| Parameters | 373,383 |
| Native type descriptors | 201,385 |
| Generic class records | 78,758 |
| Generic instantiation records | 113,837 |
| Generic method specifications | 1,275,652 |
| Concrete field offsets / explicit sentinels | 252,884 / 44 |

Every typed method entry was cross-checked against the independently captured runtime method map. A check caught **313 invalid addresses** caused by treating runtime-only BSS as on-disk bytes. The exporter now uses the exact captured method-pointer tables, with all corrections recorded. Native data is parsed as bytes; it is not loaded as executable code.

## Actual method bodies, not just signatures

The exact instruction-pattern scan examined all **409,503 method definitions**. A second instruction decoder checked the admitted machine-code sequences and their operands. **27,302 simple method entries** were emitted into managed IL: **19,616 field reads, 4,984 primitive-field writes, and 2,702 constants**. Another 780 candidate writes were deliberately excluded because the managed fields are readonly/literal.

These entries share native code: they correspond to **1,900 distinct byte spans totaling 13,421 instruction bytes**. This is not 27,302 independently complex game systems or a percentage-complete claim.

Every generated managed method was read back. **354,441 concrete methods still throw an explicit `NotSupportedException`**, rather than silently pretending to work. **27,760 abstract/external methods remain bodyless.** Reference stores, complex branches/calls, constructors and unsupported generic bodies were not guessed.

The matcher passed **35 synthetic encoding/refusal tests**. There were **76 synthetic managed operation checks** covering primitive/reference field-operation families, including boundary values. Those tests executed only newly authored fixtures, not Rec Room or its recovered assemblies. They do not prove gameplay, cross-platform, exception-timing or concurrency equivalence for every recovered method.

## Byte coverage and full-range instruction audit

Every byte of the **287,903,264-byte restored native image** and **52,403,096-byte normalized metadata** was read and hashed: **340,306,360 bytes, no unaccounted ranges**. `verification/byte-range-ledger.json` records section/table ranges, chunk hashes and explicit interpretation limits. Reading and hashing every byte is not the same as understanding every byte's behavior.

A separate read-only instruction pass covered **771,556 compiler-recorded unwind ranges**, spanning **189,189,243 unique bytes**. It recorded **44,556,578 instruction records**, **5,768,162 direct-call sites**, **285,650 indirect calls**, and **3,952,607 conditional branches**. The CSV indexes are in `native-flow/`.

The pass also retained **14,763 invalid instruction records in 4,204 ranges**, rather than hiding them. Their causes have not been classified; linear disassembly can encounter data, padding or unreachable bytes. Indirect targets remain unresolved. Compiler unwind ranges are not one-to-one C# methods, and leaf functions without unwind entries require separate accounting.

## Export issues caught and fixed

The initial raw C# export was rejected by syntax checking: **210,579 errors in 203 files**, primarily literal compiler-generated identifiers. The final exporter applies ILSpy's standard `EscapeInvalidIdentifiers` syntax-tree transform. Old diagnostics are preserved for comparison, not presented as the final result.

The largest initial whole-assembly export also reached its 90-second limit. Its separately bounded retry completed, and the final exporter gives that one assembly an explicit larger limit. The earlier failures remain documented; no export timeout is being relabeled as successful gameplay.

## Language decision

**C# is the reconstruction target.** It follows the recovered managed type system and Unity scripting contracts. IL2CPP originally compiles managed code through C++ into native instructions; understanding those instructions remains necessary for method-body recovery. Moving to Rust would be a separate port and would not itself recover missing logic, assets, serialization or engine behavior. Native components can be considered later for measured needs; no language-based speed improvement is claimed.

## Evidence and reproducibility

`verification/reconstruction-summary.json` provides the aggregate result. The native registration validation, runtime-pointer corrections, exact-byte leaf proofs, output-method readback counts, per-file syntax reports and hashes remain on Main. Analysis tools use a pinned, unmodified Il2CppDumper source commit `4741d46ba9cd6159c5d853eb9d6fc48b4bfa2b1a`, Mono.Cecil 0.11.4, Iced 1.21.0 and ICSharpCode.Decompiler 11.1.0.9782. Package locks accompany the tools.

Only our tooling and aggregate verification records are candidates for the project repository. Recovered game source, full method/field indexes, native code, original assets and generated game DLLs remain local. No game, capture job, UAC elevation, DNS change, security change or original-file patch was performed during this offline reconstruction pass.

## Remaining work

The next priority is typed control-flow and call reconstruction for the real startup/service boundary, then deterministic tests for recovered code against recorded behavior. Shared generic bodies, object lifetimes, native engine calls, asset/project reconstruction and original-name binding must be resolved. A working owned backend and two actual clients sharing a room are later acceptance gates. This workspace is useful C# reconstruction evidence, **not a completed runnable Rec Room replacement**.

## Primary tool references

- Unity IL2CPP: https://docs.unity3d.com/6000.0/Documentation/Manual/scripting-backends-il2cpp.html
- Pinned metadata/native parser source: https://github.com/Perfare/Il2CppDumper/tree/4741d46ba9cd6159c5d853eb9d6fc48b4bfa2b1a
- Iced instruction decoder: https://github.com/icedland/iced
- ILSpy standard source identifier transform: https://github.com/icsharpcode/ILSpy/blob/master/ICSharpCode.Decompiler/CSharp/ProjectDecompiler/WholeProjectDecompiler.cs

## Repository evidence

[Aggregate result](csharp-reconstruction-20261021/summary.json) · [Final syntax checks](csharp-reconstruction-20261021/csharp-source-syntax-check.json) · [Byte-range coverage](csharp-reconstruction-20261021/coverage-summary.json) · [Native flow audit](csharp-reconstruction-20261021/native-flow-audit.json) · [Tool source](../../tools/rea/csharp-reconstruction/README.md). Recovered game payloads and full indexes are intentionally absent from this repository.
