# Exact-build C# reconstruction tooling

This checkpoint analyzes the owner's already captured Rec Room Steam build 23191908. It does not include game assets, captured game code, normalized game metadata, generated game DLLs, or the large reconstructed C# files. Those remain in the private workspace on Main.

## Provenance and inputs

The as-executed workspace is `C:\Users\alexi\AppData\Local\Metallic-RC\rework-20261020\rea-steam-decoding\csharp-reconstruction`. The private verified capture is the sibling `runs/recovery-/20261020T212503.738773Z` under `rea-steam-decoding`. The Python checkpoint scripts deliberately preserve these exact-build paths and hash guards. They are not generic decoders and refuse existing output directories.

Run `bootstrap-upstream.ps1` only to acquire the public analysis dependency alongside these projects. It pins Perfare/Il2CppDumper to commit `4741d46ba9cd6159c5d853eb9d6fc48b4bfa2b1a` and refuses to replace an existing different or dirty checkout. Upstream sources remain unmodified. NuGet package versions/content hashes are pinned in each project's lock file. The SDK is pinned to 10.0.402.

## Tool roles

- `recover_native_registration.py`: verifies native type/generic arrays and creates an offline-only registration derivative. This is never a replacement installed DLL.
- `RecoveryHost`: reads explicit validated registrations, repairs parser-only method-pointer values from the exact captured runtime table, then emits typed indexes and managed reference assemblies. Unrecovered concrete methods throw explicitly. No PE loader, LoadLibrary or game invocation is used.
- `analyze_leaf_bodies.py`: admits only exact typed getter/primitive-setter/constant leaf encodings. Complex calls/branches, constructors and unsupported generic cases are rejected.
- `LeafEmitter`: independently decodes source bytes with Iced, emits accepted managed leaf IL, preserves explicit unknown-method throws, and reads every generated method back. Synthetic dynamic methods test only our own Probe fixtures, not recovered game code.
- `CSharpExporter`: renders the partial managed assemblies with ILSpy, including its standard invalid-identifier escaping transform. Per-module limits remain explicit.
- `SyntaxCheck`: uses the installed SDK's Roslyn parser. A syntax pass is not semantic compilation or a successful Unity build.
- `NativeFlowAudit`: linearly decodes compiler-provided unwind ranges and writes static direct/indirect-flow indexes. Invalid instructions, unresolved indirect calls and the distinction between unwind ranges and C# methods remain explicit.
- `verify_coverage.py`: checks byte-range/hash coverage and every typed method against the previous captured pointer map; it does not label hashing as semantic understanding.

## Checks requiring no proprietary input

```powershell
python .\test_leaf_instruction_patterns.py
.\bootstrap-upstream.ps1
dotnet restore .\RecoveryHost\RecoveryHost.csproj --locked-mode
dotnet build .\RecoveryHost\RecoveryHost.csproj -c Release --no-restore
```

The portable matcher test is the exact instruction matcher extracted from the executed scan. It covers 35 positive/refusal cases without a game artifact. Other project builds can likewise use locked restore and do not execute game code.

## Observed result

The separate report records 419 syntax-checked C# files, 27,302 recovered simple managed method entries representing only 1,900 distinct native instruction spans, and explicit unresolved bodies. It also records the initial bad source export, its corrected identifier transform, and the full-range native instruction audit. These tools do not establish a runnable game or complete behavioral reconstruction.

Source dependencies: Unity IL2CPP documentation; pinned Perfare/Il2CppDumper (MIT, license in upstream checkout); Mono.Cecil 0.11.4; Iced 1.21.0; ICSharpCode.Decompiler 11.1.0.9782. No upstream or game payload is redistributed by this source-only tool directory.
