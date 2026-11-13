# Metallic-RC: deep REA analysis and partial C# reconstruction

Checkpoint: 21 October 2026 UTC. Target: the actual recovered Steam Rec Room build **23191908**, not a substitute game. This pass produced **26 genuine REA/Ghidra dossiers for 24 distinct methods**, a searchable analysis database, 31 cross-checked service-discovery bindings and two tested C# codec reconstructions. All 419 generated C# source files were freshly syntax-checked.

**This is not a complete recovered Unity project or a runnable Metallic-RC game.** Unknown method bodies, incomplete decompiler ranges and untested behavior remain explicit.

## Open the actual C# source

The preferred source folder on Main is:

```text
C:\Users\alexi\AppData\Local\Metallic-RC\rework-20261020\rea-steam-decoding\csharp-reconstruction\CSharp.Source
```

Start with `Assembly-CSharp.dll.partial.cs`, `RecNet.Runtime.dll.partial.cs`, `RecNet.Interfaces.dll.partial.cs` and `Circuits.V2.dll.partial.cs`. There are **419 files / 145,020,870 bytes**, including Unity and framework code. The fresh Roslyn check found **zero syntax errors**. Escaped identifiers are source-display aliases, not verified Unity serialization/reflection names. Syntax acceptance is not semantic compilation or a successful Unity build.

| Method status | Count |
|---|---:|
| Typed method declarations | 409,503 |
| Simple entries with reconstructed managed IL | 27,302 |
| Concrete bodies still explicitly throwing `NotSupportedException` | 354,441 |
| Abstract/external methods left bodyless | 27,760 |

The 27,302 entries are field reads, primitive writes and constants. Shared code means they represent **1,900 distinct native byte spans / 13,421 instruction bytes**, not 27,302 complex gameplay systems. Those leaf results come from the preceding reconstruction checkpoint and are not being counted as new REA dossiers.

`Metallic-RC-23191908-Partial-CSharp.zip` in this analysis folder contains the source, a limitations README and a per-file hash manifest. All archived source hashes and ZIP integrity passed. Size: **15,251,401 bytes**. SHA256: `ba765ecf200f8cb3ab5c4f362f4e4aa39f19dc4bdd4a09c2d553869e929e40f2`. It remains private on Main and contains no native game DLL.

## Byte coverage versus understanding

The prior core-file ledger accounts for every byte of the restored native image and normalized metadata: **340,306,360 bytes**, with no unaccounted ranges. That is file identity/coverage, not complete behavioral understanding or reconstruction of all assets.

The broad linear inventory has 771,556 compiler unwind ranges, 44,556,578 instruction records, 5,948,003 direct call/jump operand observations and 308,926 indirect-flow observations. It retains **14,763 invalid instruction records in 4,204 ranges**. A compiler unwind record is not necessarily a whole C# method; a decoded static operand is not proof of reachability.

This pass independently audited the complete selected source spans for **24 methods: 14,968 bytes, 3,342 instructions, zero invalid instructions**. Two Paintball bodies extend beyond their first compiler unwind record; their spans were expanded instead of truncated. Six selected procedures still have non-0xCC source bytes outside REA's represented body. These async/error-path omissions remain documented in `range-reconciliation.json` and `reconciled-selected-disassembly/summary.json`.

## Real REA analysis

The installed REA 6.5.0 MCP server, with Ghidra 12.1.4, produced 21 initial dossiers and five changed-input follow-ups. These cover startup/addressables, name-server response processing, room joining/failure handling, Paintball update/flag capture, circuit bytecode operations, voice pause/update and VR/haptics. All six analysis sessions exported canonical evidence bundles and closed successfully.

The targets are bounded analysis carriers made from exact, SHA256-checked source pages at their original virtual addresses. Headers and export labels are generated scaffolding; instruction bytes are not invented. This avoids the earlier whole-DLL import timeout, but absent callees/globals can still affect inferred signatures and control flow. No whole-program decompilation is claimed.

Canonical records, assembly and pseudocode remain private in `rea-dossiers/` and `followup-dossiers/`. `DEEP-ANALYSIS-SUMMARY.json` retains evidence IDs, hashes, warnings, metadata signatures and range comparisons.

## Concrete findings for Metallic-RC

**Service discovery:** all 31 service IDs agree between the native argument loads, REA's pseudocode projection, recovered response-field offsets and the `RecNet.Service` enum. This includes authentication, API, matchmaking, rooms, accounts, chat, moderation, content, studio and economy services. The updater also distinguishes absent values and changed URI strings. This establishes a client-side binding map, **not a tested wire schema or functioning replacement backend**. No live endpoint values, tokens or account payloads were acquired.

Evidence: `ev_3240650d079e5e63f4ca35ac67282c5e6290b2f5167c7c69bbbb476c5ec36973` and `ev_6d7938097f67e27791253bf43675962944b5b2a6a26e79131e613ce1ee75a9bc`. See `findings/name-server-bindings.json`.

**Room joining:** the inspected path distinguishes party/auto-follow scene requests from another indirect join path, and a separate handler records join-error messages. The actual owned-backend handshake and two-client join remain untested.

**Paintball:** the capture request reaches a validation routine before score/event operations. The inspected validation uses player team, spectator status, flag team, state and an own-flag-at-home condition in one mode. Obfuscated state fields and dynamic dependencies still need corroboration; this is not a verified authoritative-server implementation.

**Voice and VR:** the selected routines expose pause/resume coroutine handling, unscaled-time updates and a haptic command path. No physical headset, voice transport or end-to-end latency acceptance was performed.

**Circuit byte operations:** REA and the included helper routines show four-byte little-endian composition and sequential byte writes. The writer can partially modify an array before a later bounds failure. Replacing that with an up-front bounds check would change the observed ordering.

The new `RecoveredCodec` C# project models those two bounded operations. **33,029 synthetic checks passed**, covering 10,009 valid cases, 999 invalid cases and 297 partial-write cases, plus extreme indices and managed null behavior. Tests executed only this newly authored code, not original game assemblies/native code. Native exception/runtime/concurrency equivalence remains untested. These two models are separate from the earlier 27,302-leaf count.

## Searchable evidence atlas

`Metallic-RC.Analysis.sqlite` indexes all 409,503 methods, 63,648 types, 252,928 fields, native ranges, direct/indirect operands and leaf proofs. SQLite integrity passed. Its companion navigator opens the database read-only; all **12 navigator tests passed**, including parameterized searches, paging, invalid IDs and refusal to write.

```text
python query_atlas.py search NameServer --limit 20
python query_atlas.py method 148260
python query_atlas.py calls 148260 --limit 50
python query_atlas.py fields 22164
python query_atlas.py coverage
```

Run these from this analysis folder using the existing Python installation. The atlas is roughly 963 MB and remains on Main; it is not an online service. Static call operands and shared-body method candidates retain their interpretation limits.

## Language and remaining gates

**C# remains the reconstruction target:** it follows the recovered managed types and Unity interfaces. Rewriting everything in Rust would be an additional port, not a way to recover missing logic. Native components may be considered for measured requirements later; no language-based performance superiority is claimed.

Next work: recover and verify the omitted async/exception paths, resolve indirect dispatch and shared generic semantics, confirm startup service contracts, rebuild asset/project bindings and test the real client against owned services. Successful login, two clients sharing a room, voice/reconnect, in-game CV3, platform ports and measured performance are separate acceptance gates.

The installed and fresh `RecRoom.exe`/`GameAssembly.dll` hashes were rechecked and remain unchanged. This pass did not run the original game, request UAC, capture another process, patch payloads, or change DNS/security settings. Captured game binaries, complete generated source, the atlas, full native listings and pseudocode remain private on Main. Only our analysis tooling and aggregate evidence are intended for repository publication.

## Repository companions

[Previous C# reconstruction](CSHARP_RECONSTRUCTION_2026-10-21.md) · [Deep-analysis summary](rea-deep-analysis-20261021/summary.json) · [Selected-range audit](rea-deep-analysis-20261021/selected-range-audit.json) · [Service bindings](rea-deep-analysis-20261021/name-server-bindings.json) · [Navigator tests](rea-deep-analysis-20261021/atlas-tests.json) · [C# codec tests](rea-deep-analysis-20261021/codec-tests.json) · [Analysis tools](../../tools/rea/deep-analysis/README.md).

## Installed handoff and source-copy verification

The desktop shortcuts **Metallic-RC Code Analysis** and **Metallic-RC C# Source** were created and read back successfully; they open folders and do not execute game code. The new codec project was rebuilt from this exact publication worktree and all 33,029 checks passed again. [Handoff record](rea-deep-analysis-20261021/desktop-handoff.json) · [Publication-copy codec checks](rea-deep-analysis-20261021/publication-codec-tests.json).
