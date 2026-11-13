# Metallic-RC: reworking the installed compiled clients

**Decision date:** 20 October 2026. **Scope:** preserve the owner's actual client baseline, compile original C# tooling and services, and advance compatibility through explicit gates. This record does not declare a rebuilt Rec Room client or a working replacement game service.

## Measured starting point

Source: the owner-authorized, read-only [Main-PC inventory](evidence/pc-client-inventory-2026-10-20.json) recorded in this work session. Paths below use user-relative templates. File totals identify that snapshot and do not establish authenticity or complete content hashing.

| Installation | Observed location | Files / bytes | Engine and interpretation |
| --- | --- | --- | --- |
| Steam Rec Room | `%ProgramFiles(x86)%\Steam\steamapps\common\RecRoom` | 3,298 / 6,781,713,784 | Unity `6000.0.27f1`, revision `9f66c1164388`; primary modern baseline |
| NexaNet | `%USERPROFILE%\Downloads\NexaNet\NexaNet` | 3,310 / 6,784,356,614 | Same Unity engine and four matching core binary hashes below; additional wrapper files differ |
| Campus2025 | `%USERPROFILE%\OneDrive\Documents\Campus\Campus2025` | 3,272 / 6,721,315,947 | Unity `2021.3.41f1`, revision `6c5a9e20c022`; separate compatibility target |

Steam records App ID `471710`, build ID `23191908` and manifest `8867414017562658247`. The precise date-form game version has **not** been confirmed. Neither `Campus2025` nor an engine version substitutes for that evidence.

| Core file | SHA256, identical in the inspected Steam and Nexa folders |
| --- | --- |
| `RecRoom.exe` | `3bfddbd82167a7fd3042abaddd39b6ecaf2b7149407df606eb6af0e798a57a70` |
| `Recroom_Release.exe` | `0e5b80fd7483c00001fbc899b5c3dce3f3102d0b8c2b9a8f5bd04d052a7bda39` |
| `UnityPlayer.dll` | `30f4a99c49566b2c5045469b5a39572dd2298adc31f7fe2bc39d8bb8fc9d75bb` |
| `GameAssembly.dll` | `a6d90770cdfda4043eadb278a16198ed0907c3ba2ac09bc37d06cd4dd22a1d84` |

The modern metadata file is 52,403,096 bytes. An independent full-file check confirms `global-metadata.dat` contains only `0x52`, with SHA256 `01ab66ca50a00960c555abe9a646401b7f37838c67bb654f9831f7bdc34ec62e`, consistent with the recovered `/REA` audit. The `0000525200005252` prefix belongs to `globalgamemanagers`/`ScriptingAssemblies.json`, not that metadata file. These observations do not identify an encryption algorithm or prove source recoverability. Campus metadata is 47,962,260 bytes, with a `Recroom_Release.exe`/`Recroom_Release_Data` layout. No complete Unity project markers were found in the inspected targets.

The desktop `Campus.exe` reports `1.22`; `Downloads\RecLauncher.exe` reports `2.5.0.0`. Nexa's existing `RecVR.bat` references a missing `Injector.exe`, while `RecScreen.bat` uses `nexanet.exe`. The required external route remains a compatibility issue. The rework did not replace, emulate or bypass its guard. `C:\Meow.Net` exists but was not fully inventoried. The requested `/REA` [history was recovered](evidence/REA_HISTORY_RECOVERY_2026-10-20.md) at `%USERPROFILE%\Documents\Codex\2026-10-18\vi-bot-acceptance\outputs\PaintballPreservation\audit\NEXANET_BUILD_AUDIT_20261017.md`; this audit is evidence of earlier inspection, not a recovered Unity source project. Its claimed game-date version still requires independent confirmation.

## What recompilation actually requires

[Unity's IL2CPP documentation](https://docs.unity3d.com/Manual/il2cpp-introduction.html) describes C# managed assemblies being stripped, converted to C++ and compiled into target-native machine code. The measured `GameAssembly.dll` is evidence of that compiled distribution. Renaming the executable, creating a .NET solution or finding metadata does not reconstruct its original C# project, scene authoring state, package locks, native plug-ins or build pipeline.

Mono assemblies, if encountered elsewhere, require a different inspection path; readable managed IL still does not guarantee the original project and assets. Unity project evidence means `Assets/`, `Packages/manifest.json` and `ProjectSettings/ProjectVersion.txt` together, followed by an actual clean build. Dummy or reconstructed assemblies must never be labeled recovered production source. The [launcher core](../src/Metallic.RC.Core/README.md) reports these distinctions explicitly.

There are two gated ways to advance the client: a documented compatible integration around the existing binary, or a buildable source project with its dependencies and content pipeline. The selected installation remains the reference for either path. Neither path is completed by the current launcher or authoring API. Share CV3 schema and conformance fixtures across runtimes; a .NET 10 service DLL is not automatically compatible with Unity's scripting runtime.

## Current original C# implementation

| Component | Implemented boundary | What still needs proof |
| --- | --- | --- |
| `Metallic.RC.Core` | Bounded read-only inventory, opaque metadata detection, persisted registrations, launch-related SHA256 checks, explicit external-launcher intent and reverified launch plans | A process plan does not demonstrate game login or room connectivity |
| `Metallic.RC.Launcher` | Avalonia desktop shell around installed-client discovery and the core; source pins Avalonia `11.3.22` | Native desktop packaging, visual/device acceptance and each external launcher route have separate QA evidence |
| `Metallic.RC.Creator` | Original typed CV3 graph validation, compilation hash and bounded deterministic preview | No injection into the compiled game, CV2 migration or network-authoritative execution |
| `Metallic.RC.Platform` | Loopback API, private local session, durable room/circuit drafts, ETag revisions, capability discovery and the local editor host | No public account service, Rec Room protocol adapter, live matchmaking, room simulation or voice |
| Existing Go Atlas and website | Retained infrastructure and presentation work | Neither a capacity model nor a website proves a running game room |

At this record's source checkpoint, the Core harness has **18 checks** and the Platform/Creator executable passes **103 assertions**, including real HTTP delivery of the editor and script. These are source/integration checks, not gameplay tests. [Core acceptance](../tests/Metallic.RC.Core.Tests/Program.cs) and [Platform acceptance](../tests/Metallic.RC.Platform.Tests/Program.cs) define their exact coverage; native Windows, browser, headset and hosted CI results belong to their own dated records.

C# is chosen because the implemented core, editor model and HTTP service share types and already compile with the pinned .NET 10 toolchain. Core, Creator and Platform require no third-party packages; the desktop shell uses Avalonia. [Microsoft's support policy](https://dotnet.microsoft.com/en-us/platform/support/policy/dotnet-core) supplies the servicing horizon. Rust adoption requires a measured problem, a bounded interface and a comparison against the C# implementation; changing language alone does not improve the supplied game binary.

## Target architecture and server gates

The following boundaries are an implementation plan. The only implemented server in this rework is the local authoring service described above.

| Boundary | Responsibility | Acceptance gate |
| --- | --- | --- |
| Installation and version adapter | Exact executable/content profile, supported launch route, version-specific client/server contract | Re-inventory the target, reject changed launch files, document missing dependencies, and demonstrate a real controlled bootstrap without calling unknown support “working” |
| Identity and control API | Independent Metallic identities, profiles, room discovery and short-lived join authorization | Expired/replayed credentials fail; two test accounts use actual persistence and cannot access each other's private data |
| Authoritative room host | Room lifecycle, ownership, interaction state, validated inputs and per-room execution budgets | Two real clients observe one shared interaction; join, disconnect, reconnect and host restart are recorded; invalid inputs cannot alter another player's state |
| Creator publish boundary | Immutable validated graph/room revision, supported content dependencies, runtime-version declaration | A saved draft becomes a versioned artifact; a host accepts the intended revision and rejects unsupported nodes or missing dependencies |
| Voice transport | Microphone access, media routing, spatial playback and mute/block policy | Two devices exchange real audio with permissions, spatial movement, mute/block and loss/reconnect checks |
| Asset delivery | Platform-specific manifests, integrity checks, cache and interruption recovery | Supported assets load on each target; missing, corrupt or wrong-platform bundles fail with actionable diagnostics |
| Atlas operations | Launch/stop health, placement, telemetry, capacity and recovery around a proven room host | Health reflects real room progress; increasing load has measured tick latency, bandwidth and recovery results |
| Public website and distribution | Actual status, downloads, creator content and support | Display measured availability; signed updates reject tampering/downgrade and recover interrupted installs before public release |

Implement and prove one modern-client vertical path before expanding the service map. Bootstrap/config compatibility, an HTTP room record and a live simulation are separate milestones. RecFlare's [versioned support](https://github.com/recflare/server) and [transport prerequisites](https://github.com/recflare/server/blob/main/DEPLOYING.md) illustrate why a generic “Rec Room server” label is insufficient. Preserve existing Atlas boundaries and introduce persistent production infrastructure only when these contracts and recovery requirements are concrete.

## CV3 progression and acceptance

The current [CV3/API contract](../src/Metallic.RC.Platform/README.md) implements start, scalar constants, arithmetic, comparison, branching, preview logs and numeric state. It permits at most 256 nodes, 1,024 edges, 4,096 execution steps and data depth 64; all cycles are rejected. Graph hashes normalize ordering and finite numeric representations. Preview has no arbitrary code, network or filesystem capabilities.

1. **Authoring gate — implemented:** malformed/duplicate IDs, ports, types, cycles, overflows and budget violations produce useful issues; drafts survive restart and stale ETags cannot overwrite them. Unfinished graphs save but do not execute.
2. **Publishing gate — pending:** bind graph hash to runtime/catalog version, persist immutable revisions, round-trip supported content and report incompatible migrations. A preview hash is not a publisher signature.
3. **Runtime gate — pending:** specify tick order, event order, ownership and replicated-state semantics. Replay a fixed input trace with identical state hashes; late join and reconnect restore the correct state. Any future timer or loop has a deterministic quota.
4. **Client gate — pending:** a room using the compiled client actually loads and runs the supported graph artifact; two clients observe the same score/interaction. A browser preview is not evidence for this gate.
5. **Migration gate — pending:** use owner-provided export fixtures to map documented supported fields, retaining unknown fields and loss diagnostics. [Official exports](https://steamcommunity.com/app/471710/announcements/) do not supply working room copies or an automatic CV2-to-CV3 conversion.

## Platform-specific gates

| Target | Present evidence | Required before claiming support |
| --- | --- | --- |
| Windows screen | Actual Windows PE clients; original .NET launcher/core/service source | Packaged launcher on Main PC, import/verify/handoff, then a real game session and measured frame pacing |
| Windows PCVR | Existing VR launch intent; no headset result implied | Exact headset/runtime, head and controller tracking, recenter, comfort, audio, disconnect/reconnect and frame-budget captures |
| Linux desktop | Core inspection and local API are portable; the core deliberately blocks Windows-client execution on Linux | Native launcher UI/device QA; separately prove either a supported compatibility runtime or a native game build. Neither is currently supplied |
| Linux VR | No runtime acceptance evidence | A supported graphics/XR/runtime combination, working input and compositor measurements on real hardware |
| Quest standalone | A third-party project listing/download is research evidence only | Target-native Android ARM64 package, correct asset bundles, signing, on-device input/audio and sustained thermal/frame testing |
| Flat Android | No Metallic client package or device result | A separate touch/controller interface, graphics/memory profile, lifecycle behavior, connectivity tests and install/update path |

[Unity's OpenXR 1.16 documentation](https://docs.unity3d.com/Packages/com.unity.xr.openxr@1.16/manual/index.html) lists Windows PC runtimes and Quest's Android ARM64/Vulkan target; its compatibility starts at Unity 2022.3 LTS. It does not establish the inspected Campus Unity 2021 build's plugin compatibility or universal Linux VR support. [AssetBundles are target-specific](https://docs.unity3d.com/Manual/assetbundles-platforms.html), so copying the Windows distribution into an APK cannot meet the mobile gate.

## Performance measurement and optimization order

Record the baseline before changing behavior: exact client hashes, launcher/server revision, OS, CPU/GPU/RAM, graphics API, resolution, headset refresh rate, scene, player count and cache state. Use a repeatable camera/input route with at least three warmed runs plus a separate cold-start run; retain raw data and variability. The original and reworked runs must use comparable settings.

Measure frame-time p50/p95/p99, CPU and GPU contribution where available, missed/reprojected headset frames, working-set/VRAM peaks, loading times and crashes. Use [PresentMon](https://github.com/GameTechDev/PresentMon) for eligible Windows presentation measurements; use Unity profiling only where a suitable [development build](https://docs.unity3d.com/2022.2/Documentation/Manual/profiler-profiling-applications.html) exists. For 72/90/120 Hz, arithmetic frame intervals are approximately 13.89/11.11/8.33 ms; selecting a target does not prove the application sustains it.

Optimize the measured constraint: main-thread work and allocation, rendering/overdraw/shadows, memory/asset loading, or network/room scheduling. Source-dependent changes such as shader replacement, asset rebaking, job scheduling and code stripping remain gated on a buildable project. Supported launcher settings can be tested earlier, with explicit before/after captures and reversible profiles.

For the future server, measure tick p95/p99, queue backlog, allocations, bytes per player, join latency and voice bandwidth separately. Progress from two real clients to controlled larger loads; include latency, packet loss and process-restart experiments. Publish capacity only for the tested hardware, room and workload, and reserve operating headroom from measurements rather than an invented concurrent-user forecast.

## Release evidence rule

A release record must identify its exact source commit, SDK/dependency locks, produced binary hashes, operating-system/device matrix and executed checks. [Windows](../scripts/build-desktop.ps1) and [Linux](../scripts/build-desktop.sh) package scripts publish the launcher and platform with their runtimes and a file-hash manifest; usage belongs in the [desktop readme](DESKTOP_README.txt). Local compiler success cannot stand in for hosted CI or gameplay; an imported SHA256 baseline detects changes but is not a publisher signature. Keep capability flags false until the corresponding end-to-end gate passes, and link unresolved client dependencies, missing source material and untested platforms directly from the release record.
