# Build 23191908 recovery tools

This is an exact-build forensic checkpoint, not a generic decoder or runnable game.

The matching private workspace on Main is `C:\Users\alexi\AppData\Local\Metallic-RC\rework-20261020\rea-steam-decoding`.
The scripts are retained as executed source; they expect the private `runs/recovery-/20261020T212503.738773Z` artifacts adjacent to the scripts. Those artifacts, game bytes, full method/type indexes and native pseudocode are deliberately not committed. Use the existing private workspace rather than expecting this source-only checkout to contain Rec Room.

1. `fast-capture-v4` preserves the exact owned-PID, hash and Windows UAC boundaries of the previous reviewed capture. It caches only a qualified metadata allocation and preserves a provisional read before waiting for full-hash stability. Do not rerun capture unnecessarily; the matching validated artifacts already exist.
2. `normalize_metadata_23191908.py` checks exact input SHA256, all 31 table roles and reciprocal range constraints. It writes a derivative only, with complete module repair records; the raw capture is unchanged.
3. `independent-parser` contains unmodified Perfare/Il2CppDumper parser sources from pinned commit `4741d46ba9cd6159c5d853eb9d6fc48b4bfa2b1a`, their MIT license and source hashes. Only the project and Program.cs harness are ours. The harness reads a private memory copy and checks it was not mutated.
4. `test_metadata_recovery.py` rejects seven deliberately corrupted variants of the genuine captured metadata.
5. `reconstruct_native_analysis.py` restores captured code into an offline derivative while retaining original disk data sections. It is never loaded as code or used to replace the installed game.
6. `map_native_methods.py` locates code-generation modules by exact assembly name and matches tokens and counts, rather than assuming module array order.
7. The REA runners retain both the whole-image timeout and bounded native-analysis results. The page-carrier builders copy exact source pages and preserve virtual addresses. Generated PE headers/export labels are scaffolding, not original exports. Missing out-of-page globals/callees remain unknown.

All outputs are analysis evidence, not a complete source project. See the current report in `docs/evidence/STEAM_RUNTIME_RECOVERY_2026-10-20.md`.
