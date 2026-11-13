> **Later result, 21:25 UTC:** stable runtime metadata and native-code captures are now recovered. The normalized metadata passed an independent parser. See [the current recovery report](STEAM_RUNTIME_RECOVERY_2026-10-20.md). The earlier unsuccessful attempts below remain historical evidence.

# Steam Rec Room decoding with REA

Date: 2026-10-20. Status: **runtime recovery prepared; decoded metadata and usable game code not yet recovered**.

## Exact target

- Installation: `C:\Program Files (x86)\Steam\steamapps\common\RecRoom`.
- Steam build ID: `23191908`; Unity `6000.0.27f1`; Windows x64 IL2CPP.
- GameAssembly SHA-256: `a6d90770cdfda4043eadb278a16198ed0907c3ba2ac09bc37d06cd4dd22a1d84`.
- Metadata: 52,403,096 bytes, all `0x52`, SHA-256 `01ab66ca50a00960c555abe9a646401b7f37838c67bb654f9831f7bdc34ec62e`.

## Work actually performed

Located the existing `rea-agents@6.5.0` installation used by Codex. Ran REA against the installed Steam binaries and retained canonical evidence. Installed official portable Ghidra 12.1.4 and Temurin full JDK 21.0.12.1+1 in the Metallic-RC tooling folder after verifying official release sizes and SHA-256 digests. REA's Ghidra provider checks pass, including its native Windows controls. Provider settings are process-local; unrelated MCP registrations were not edited.

REA inspected PE resources, opened RecRoom.exe in Ghidra, searched startup strings, and analyzed its PE entry point. Ghidra reported 303 procedure candidates and 205 strings. **These are not 303 recovered game functions.** The entry-point output contains bad-instruction and overlapping-instruction warnings; its static output is not accepted as usable code. RecRoom.exe's .text entropy is about 7.991 bits/byte. GameAssembly's sampled .text and il2cpp sections are about 7.9998 bits/byte. Runtime decryption is still needed.

Evidence IDs:

- GameAssembly PE resources: `ev_27874338961a835108ca2e297e1f16136186a487bcd8b7a66bac7e7b91893360`.
- RecRoom.exe PE resources: `ev_5204bbff0e788c3e40103dd09d6bac93c8b10d4d00228272a744115b96bbae2a`.
- Ghidra overview: `ev_bb5fafe622aec32c6598285a16b5bf363c68398dd16969bcb38ac5a62378c3b2`.
- Startup string search: `ev_c2c713c58a27bbd6bb4e67b74a402101baabcc4264c8c32615d4d96e290523c8`.
- Unvalidated entry dossier: `ev_b4a0359aa2296b0c0a3d256e063415990abaaa69528771fe72dbebab531d4461`.

## Actual startup attempts

| Attempt | Observation | Recovery |
|---|---|---|
| Normal Steam launch, 18:03:50 UTC | Steam started Recroom_Release.exe. Referee reported missing type-0 blobs, status -99962, then configuration failure -99986. Five service-start retries failed. | No GameAssembly module or metadata candidate captured. |
| Direct original RecRoom.exe, 18:06:36 UTC | Referee.Game.log reported session initialization status -99903. The process exited with 0xC0000409. | No GameAssembly module or metadata candidate captured. |

The capture windows were 45 and 25 seconds. Both reports explicitly record zero complete metadata candidates and zero complete mapped images. Read-access errors seen during the second startup are retained; they are not treated as proof of a specific cause. The exact originals checked afterward still match their original hashes.

## Prepared next experiment

The public exact-build [RecRoom dumping notes](https://github.com/VT-0xFF/RecRoom_Info/blob/main/DUMPING.md), blob `7fa2048c397de165400391cb13e4a2a174b5af9e`, describe a missing global shared-memory prerequisite for the observed -99903 startup failure. They specify `Global\KjMpQrStUvWxYzAb`, 66,060,456 bytes. The same notes identify this build's exact all-R metadata length. This is an external research hypothesis; the current work has not validated that the proposed section restores this client.

`run_steam_recovery.py` implements one bounded experiment. It requires an already elevated Windows process, verifies the exact original/client/helper hashes, refuses existing Rec Room processes, refuses an existing named section, holds a newly created zero-initialized pagefile-backed section, arms the capture helper, launches the original RecRoom.exe, and gives the helper the exact owned PID. The capture interval is 45 seconds. Cleanup closes the section and only acts on retained handles for the processes the job created. Source review fixed PID selection and cleanup on interruption before handoff.

The job does not write installed payload files, change registry or security settings, adjust privileges, install persistence, inject code, or take a whole-process dump. The original game may create its normal logs and child processes. Captured candidates remain unvalidated until offline checks complete.

The non-elevated preflight passes: exact files/helpers match, no existing Rec Room process was present, and the wrapper reports `preflight_passed_elevation_required`. This connection has no elevated token and does not hold SeCreateGlobalPrivilege. Windows administrator approval is required for the proposed next run.

### Run on Main

Open the desktop shortcut **Metallic-RC REA Capture**, or run `Run-REA-Steam-Capture.cmd` in this folder, and approve Windows UAC. The launcher checks the reviewed wrapper/helper hashes before requesting elevation. The wrapper itself never self-elevates.

Read-only preflight can be repeated with:

```powershell
& 'C:\Python315\python.exe' '.\run_steam_recovery.py' --output-parent '.\runs' --seconds 45 --check-only
```

Results are written into timestamped `runs/recovery-/...` directories, including `operation-receipt.json`, `capture/capture-report.json`, coverage sidecars, artifact hashes, and startup logs. Opening the launcher is an experiment, not a promise that decoding has succeeded.

## Acceptance gates after capture

1. Require a fully readable metadata candidate of the exact validated length. Preserve holes and incomplete captures explicitly.
2. Solve the individually permuted 64-integer header and identify all 31 tables using record layouts, names, tokens, bounds, cross-table counts, and an independent IL2CPP parser. A successful tiling or a familiar string alone is insufficient.
3. Require coherent decrypted .text and il2cpp bytes. Reconstruct code sections using runtime bytes and original disk data sections, accounting for relocations and runtime-only data. Do not feed an arbitrary flat memory image to a static IL2CPP parser.
4. Validate registration layout, module ordering and method pointers for this exact build. Do not transplant the older 2025 offsets or counts.
5. Keep decoded metadata/native pseudocode separate from claims of original C# source, a complete Unity project, a compatible server, or a buildable game.

`inspect_metadata_candidate.py` is a bounded offline inspector. It deliberately reports candidates and unresolved mappings, makes no automatic repairs, and never marks a file decoded. The installed all-R file was used as a negative control and correctly rejected as a placeholder.

## Validation and reviewed scripts

- Seven capture-helper tests passed on Windows, covering PE bounds, read coverage, holes, deadlines and placeholder screening.
- Seven metadata-inspector tests passed on Windows, including shuffled individual integers, incorrect endpoints, search truncation, string bounds and partial capture rejection.
- Both transferred script hashes were verified against the reviewed versions.
- Wrapper SHA-256: `ce5f715474db5c0f5aa021ba3aaba9fa9384764bb5a86e7b6ab6e3a85c785856`.
- Capture-helper SHA-256: `37ace729f9ee7ce45b4189c6bc54e4f21a1b9f3063b6ee56a1570f4e4ddb5179`.

## Primary references

- [REA 6.5.0 Windows Ghidra boundary](https://github.com/morluto/rea/blob/rea-agents-6.5.0/docs/windows-ghidra-p0.md).
- [REA 6.5.0 CLI and evidence](https://github.com/morluto/rea/blob/rea-agents-6.5.0/docs/cli.md).
- [Exact-build metadata/runtime recovery notes](https://github.com/VT-0xFF/RecRoom_Info/blob/main/DUMPING.md).
- [RecFlare build 23191908 observations](https://github.com/recflare/skills/blob/main/.claude/skills/rr-client-bringup/references/referee-protected-build-23191908.md).
- [IL2CPP metadata structures](https://github.com/Perfare/Il2CppDumper/blob/master/Il2CppDumper/Il2Cpp/MetadataClass.cs).

REA evidence and the reports above establish the scope of this checkpoint. **Full decoding remains unfinished.**
