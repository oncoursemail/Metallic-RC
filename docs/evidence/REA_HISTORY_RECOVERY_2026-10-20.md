# Recovered REA and Codex evidence

Recorded 20 October 2026 UTC. This record separates the owner's earlier instructions, a recovered audit report, and independently checked files. It does not imply that an installed game is a complete source project.

## The original instruction was found

The relevant Codex session is:

`C:\Users\alexi\.codex\sessions\2026\09\08\rollout-2026-09-08T13-53-24-01a0825e-2914-7102-a2d7-c868593f8253.jsonl`

The session recorded Codex Desktop 0.153.4 and working directory `C:\Users\alexi\Documents\Codex\2026-09-08\connect-to-my-roblox-studio-we`.

On **18 October 2026 at 03:42:00.897 UTC**, the owner wrote:

> Yeah, fully checked the PC build and make sure and compare the files inside of it using REA

The preceding owner messages requested the PC 2026 build instead of only the Quest build, and discussed VR for the Rec Room port in Roblox. That historical request belongs to the **Roblox port context**. The current instruction explicitly makes the compiled PC client the starting point for **Metallic-RC**. These are related evidence sources but separate projects; no Roblox project files were changed during this rework.

The bounded history search examined 289 JSONL files, approximately 10.2 GB and 241,425 lines. It skipped 404 oversized lines. The exact relevant user instruction and 16 nearby relevant messages were recovered; this is not a claim that every possible chat attachment or oversized tool result was searched.

## The audit exists on Main

Recovered report:

`C:\Users\alexi\Documents\Codex\2026-10-18\vi-bot-acceptance\outputs\PaintballPreservation\audit\NEXANET_BUILD_AUDIT_20261017.md`

The report is dated 17 October and includes subsequent REA work. Reading the existing report establishes what that audit recorded. It does not rerun every earlier analysis. The report records:

| Prior audit result | Scope and interpretation |
| --- | --- |
| 3,298 Steam files, 6,781,713,784 bytes; every file matched the preserved manifest by SHA256 and size | A complete file comparison of that installation against its preservation manifest |
| 3,297 identical files in Nexa, 12 additional files, and `RecRoomLog.bat` omitted | Nexa's package retained the inspected game base and added its own launch/support files |
| 26 of 26 native executable/DLL images examined as PE32+ x64; 26 resource trees, 236 resource entries, 96 resource payloads, zero reported failures | Native container and resource inspection; the report supplies REA evidence identifiers |
| Three additional Nexa images inspected | Wrapper/support program inspection; not proof of working game services |
| All 16,338 Quest catalog keys shared with the PC catalog; 16,346 PC keys, eight additional export/settings keys | Catalog comparison in the Paintball preservation investigation; no extra Paintball content established by those eight keys |
| 2,787 UnityFS bundles, 1,625,851,018 bytes, and 114 numbered scenes | Asset/package inventory, not an editable Unity project |
| Quest main weapon bundle 80,410,141 bytes versus PC 302,344,158 bytes | Target-specific bundle differences; copying the Windows build does not produce an Android build |
| VR manifest actions for poses, skeletal hands and haptics, with Touch, Index, Vive, WMR and Cosmos bindings | Evidence of included VR support data, not a completed current headset test |
| Ghidra/IDA provider was unconfigured | **Procedure-level decompilation was not available in that audit** |

The report states that no game, guard, injector or third-party wrapper executable was run during its analysis. Its archive-date interpretation and full prior comparisons remain attributed to that report rather than being silently presented as new measurements.

## Supporting manifests

Under the same `PaintballPreservation` directory:

| File | Recovered data |
| --- | --- |
| `audit/LOCAL_STEAM_BUILD_MANIFEST.json` | Steam path; acquisition through the user's Steam installation; build ID `23191908`; depot `471711`; manifest `8867414017562658247`; 3,298 files and 6,781,713,784 bytes. No date-form game version field was present. |
| `SOURCE_BUILD_MANIFEST.json` | Status `EIGHT_QUEST_MAPS_RECOVERED_ROBLOX_PORT_INCOMPLETE`; primary map source Quest Android arm64 build `2026032301`, Unity `6000.0.27f1`; eight scenes; `mixedBuildCanonicalPort: false`; `playablePortComplete: false`. |
| `audit/QUEST_2026_BUILD_MANIFEST.json` | Quest build `2026032301`; two archives; 15,668 files; 173 joined Unity files; source archive recorded unmodified and archive hashes verified by the earlier acquisition workflow. |

The prior assistant called the PC target `20260323.14`. The current independent Steam metadata confirms **build ID 23191908**, not that date-form version string. Keep the latter as an attributed historical claim until a suitable build marker independently establishes it.

## Independently confirmed in this rework

The [PC inventory](pc-client-inventory-2026-10-20.json) records current inspection of Steam, Nexa, Campus and the installed launchers. Steam and Nexa matched for the four recorded core executable/library hashes. Steam uses Unity `6000.0.27f1`; Campus2025 uses Unity `2021.3.41f1`.

The current Steam `RecRoom_Data\il2cpp_data\Metadata\global-metadata.dat` is **52,403,096 bytes, entirely byte `0x52`**. This was checked across the whole file, not inferred from its name or its first bytes. SHA256:

`01ab66ca50a00960c555abe9a646401b7f37838c67bb654f9831f7bdc34ec62e`

This is distinct from the `0000525200005252` prefix observed in `globalgamemanagers` and `ScriptingAssemblies.json`. Do not report the latter prefix as the metadata file's prefix. No metadata decoder or source recovery is claimed.

No complete `Assets` + `Packages` + `ProjectSettings` tree, C# solution/project, or `Assembly-CSharp.dll` was established in the inspected installed client roots. The exact folder referred to as “Recroom (newish)” has not been independently identified as a full source tree. A compiled IL2CPP build does not preserve the original Unity authoring project simply because its native binaries and asset bundles exist. See [Unity's IL2CPP documentation](https://docs.unity3d.com/Manual/il2cpp-introduction.html).

## What follows from the evidence

The compiled original C# deliverable can safely inventory and register these installations, verify launch-file baselines, show process plans, hand off to an explicitly registered launcher, and run Metallic's own local creator service. Those are implemented independently of Rec Room's internals.

A rebuilt Rec Room executable, modern-client protocol adapter, two-player room simulation, voice, in-game CV3, native Linux game and Android/Quest game remain separate implementation and device-acceptance work. Prior REA resource inspection, a process launch, an HTTP room record, and a successful C# compile do not satisfy those gates. No bypass of Nexa guards or the missing VR injector was implemented.

The existing preservation workspace and other Metallic projects were left intact. Only task-owned Meadow files and the replacement launcher/platform source were changed.
