# Metallic-RC deep REA analysis tools

These tools were executed against the private recovered build 23191908 on Main. They do not contain Rec Room binaries, metadata, full generated C# source, native listings or pseudocode. The canonical game-derived artifacts stay in the private workspace.

## Private workspace

`C:\Users\alexi\AppData\Local\Metallic-RC\rework-20261020\rea-steam-decoding\deep-rea-20261021`

The scripts preserve the executed workspace layout: the sibling `csharp-reconstruction` directory supplies typed indexes, and the sibling `runs/recovery-/20261020T212503.738773Z` supplies the hash-pinned capture. Copying the source-only repository does not supply those inputs. Existing outputs are generally treated as immutable; use a fresh workspace for reproduction rather than overwriting evidence.

## Components

- `rea_session.py`: actual installed REA MCP client, canonical schema discovery, bounded requests, explicit close and retained canonical evidence. REA 6.5.0 / Ghidra 12.1.4 / full JDK21 were used. No registration or security settings are modified.
- `build_atlas.py`, `query_atlas.py`, `test_query_atlas.py`: SQLite evidence index and read-only navigation. Search accepts quoted literal prefix text. Twelve navigator tests pass.
- `build_scoped_targets.py`, `run_deep_rea.py`, `prepare_followup.py`: bounded exact-source-page carriers and real function dossiers. Generated PE headers/labels are scaffolding; source instruction bytes and original virtual addresses are preserved. Omitted globals and callees remain unresolved. Never execute a generated carrier.
- `extract_service_bindings.py`: correlates REA assembly argument loads, its pseudocode projection, field offsets and the recovered service enum. No online endpoints or credentials are contacted.
- `reconcile_selected_ranges.py`, `SelectedInstructionAudit`: account for every byte in selected union spans, including code beyond first unwind records and decompiler omissions. Iced 1.21.0 is pinned. The method ranges are not a proof of reachability.
- `RecoveredCodec`: two newly authored C# behavioral models with a deterministic synthetic test harness. It is separate from the recovered game assemblies and does not claim native runtime equivalence.

## Standalone codec check

```text
dotnet run --project RecoveredCodec/RecoveredCodec.csproj -c Release -- NEW-test-result.json
```

The output file must not already exist. The codec project needs no game artifacts or external packages. The result reports 33,029 assertions covering valid data, invalid indices, partial writes and managed null behavior. Only new code executes.

## What is not claimed

The 419 source files have passed syntax parsing, not a full semantic Unity/game build. The 354,441 unresolved concrete bodies still throw explicitly. This analysis does not establish a working owned backend, two-client gameplay, a complete asset project, physical VR/device support or a completed release.

See `docs/evidence/REA_DEEP_ANALYSIS_2026-10-21.md` for results, exact counts and retained limitations. This change is stacked on the existing C# reconstruction branch to preserve that work without modifying its checkout or main.
