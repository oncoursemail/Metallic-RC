# Metallic-RC: revival research, 20 October 2026

## Decision and evidence standard

Start with the owner's installed clients and build Metallic's launcher, compatibility records and services around the versions actually present. The 2026 Steam/Nexa installation is the principal compatibility target; Campus is a separate older installation. The previous Meadow direction does not define this rework. See [Compiled client rework](COMPILED_CLIENT_REWORK.md) for the measured files and implementation gates.

This addendum updates the named-project comparison in [RESEARCH.md](RESEARCH.md). It records public evidence and engineering decisions, not a ranking of untested services. No third-party binaries were downloaded or executed for this research, and no proprietary source was copied into Metallic's implementation.

| Label | Meaning | What it establishes |
| --- | --- | --- |
| M — measured | Owner-authorized local inventory or repository acceptance run | The specific observed files or tested behavior |
| P — primary | A directly retrieved project-owned page, repository or official vendor document | What its author publishes; gameplay and capacity remain unmeasured |
| I — indexed | Search-indexed primary content whose direct page was unavailable or different | A dated claim requiring a fresh artifact or owner confirmation |
| U — unresolved | No adequately identified primary artifact | An open research item, not absence of a project |
| D — design | An original Metallic proposal | Work to implement and pass through an acceptance gate |

## Named projects and relevant alternatives

| Project / identity | Evidence and confidence | Useful comparison | Remaining uncertainty |
| --- | --- | --- | --- |
| **Vanilla / VanillaNet** | P: [homepage](https://vanillarec.net/) and [download page](https://vanillarec.net/download/) present Windows screen/VR and Quest. | A joined-up rooms/events/download experience and a discoverable headset route. | Apex download page shows PC loading text and a Quest link; the [indexed www page](https://www.vanillarec.net/download/) instead includes Android and an upcoming 2021 build. Resolve domain/version differences before asserting current Android support. Website population or room claims are not audited telemetry. |
| **NexaNet / nerlz.com** | I: [project download page](https://nerlz.com/download) lists Windows, Quest and Studio requiring Unity `6000.0.27f1`; direct retrieval was unavailable. M: a Nexa installation exists on Main PC. | Its Studio requirement matches the engine version measured in the owner's Steam/Nexa binaries. This is a concrete modern-client comparison. | Flat Android, iOS, consoles and Avatar Studio are marked in development in the indexed page. A Quest APK does not establish flat Android support. Server behavior, source availability and performance were not independently tested. |
| **Studio 87 / recroom.network** | P: [homepage](https://recroom.network/) explicitly identifies a community tribute by Studio 87; [download page](https://recroom.network/download) presents several platform labels. | Community identity, room discovery and creator-hub presentation. | Platform navigation is not proof of downloadable or functioning builds. Keep this identity distinct from NexaNet unless an operator confirms a relationship. |
| **MeowNet / meow.net** | P: [Meta Quest listing](https://www.meta.com/experiences/meownet/1196298696890609/) is retrievable and marked Early Access. M: `C:\Meow.Net` exists but was not fully inventoried. | A real platform-store presence is stronger distribution evidence than a landing page alone. | Exact current client generation, PC launcher, service availability and frame performance need build-specific verification. An unrelated [archived fan launcher](https://github.com/hollyntt/MeowNet-Launcher) cannot establish the state of the current service. |
| **Phoenix Studios** | I: [indexed project site](https://phoenixstudiosrr.netlify.app/) describes an April 2023 experience, inventions, interface work and a CV3 roadmap. Direct fetch returned 404. | Useful requirements for creator workflows and a modernized interface on an older base. | Indexed October 1–10 launch targets do not demonstrate a release. CV3 roadmap wording is not a shipped compiler or multiplayer conformance result. Obtain a canonical current site and versioned artifact. |
| **Campus** | M: the owner's `Campus.exe` reports version `1.22`; its configuration identifies `Campus2025`, measured as Unity `2021.3.41f1`. | A concrete installed external-launcher workflow, distinct from the Unity 6 base. | Folder naming does not prove the exact game release. Public operator/source identity and runtime connectivity remain unresolved. |
| **RecLauncher** | M: the owner's `Downloads\RecLauncher.exe` reports file version `2.5.0.0`, as recorded in the [machine inventory](evidence/pc-client-inventory-2026-10-20.json). | Existing launcher behavior can inform explicit installation selection and handoff. | The filename/version alone does not identify its upstream repository, supported clients or current server contract. |
| **“8bd studios or wtv”** | U: the supplied spelling did not resolve to a sufficiently corroborated primary identity. | Preserve the intended comparison as an open item. | Studio 87 is separately identified above; do not silently equate the two names. |
| **RecFlare** | P: [server repository](https://github.com/recflare/server) describes a Node/Hono backend designed for Cloudflare; its compatibility table labels `20230414` default, `20250718.01` beta and `20260323.14` alpha. | Version-specific support tables, service contracts and deployable component boundaries are useful reference patterns. | An alpha version check is not full 2026 parity. Its own [service inventory](https://github.com/recflare/server/blob/main/SERVICES.md) identifies stubs and missing workers. No independent scalability result was established. |
| **neorec** | P: [repository](https://github.com/therock64/neorec) documents a modern Unity 6/IL2CPP client and explicitly limits itself to research and a bootstrap reference. | A narrowly scoped modern-client protocol research reference. | The author states it is not a working private server. Treat its protocol/security observations as author findings, not facts about every installed build. |
| **RestoRoom** | P: [repository](https://github.com/RestoRoom/RestoRoom) describes early research with nothing playable yet. | Openly stated gaps and a service inventory make progress reviewable. | Authentication, room hosting and client integration are not established by its README. |

## Findings that change the implementation plan

**A newer executable is not a newer complete server.** RecFlare's explicit default/beta/alpha split is useful evidence that compatibility has to be per build. Its [deployment guide](https://github.com/recflare/server/blob/main/DEPLOYING.md) distinguishes 2023 Photon Realtime/Voice from a 2025 Realtime plus Tachyon voice dependency. These dependencies must be independently proven for Metallic's exact target; the 2025 description does not establish the 2026 transport.

**Advertised services can remain unimplemented.** RecFlare's [service inventory](https://github.com/recflare/server/blob/main/SERVICES.md) documents several placeholders and a moderation routing caveat. Metallic should publish observed capability states and end-to-end acceptance evidence. An HTTP 200, a nameserver response or a successful room-list request cannot count as a multiplayer room session.

**Hosted HTTP does not supply game or media transport automatically.** The [RecFlare deployment guide](https://github.com/recflare/server/blob/main/DEPLOYING.md) separates its Cloudflare storage/routing dependencies from realtime and voice prerequisites. Metallic's inference is to budget control API, authoritative room simulation, media and asset delivery separately. No unlimited capacity or zero-cost hosting claim follows from a microservice design.

**The original service cannot be the continuity assumption.** Rec Room's [official Steam announcements](https://steamcommunity.com/app/471710/announcements/) scheduled the June 1, 2026 shutdown and loss of login and Studio online services. The April 6 export announcement describes Steam-only underlying creation data, not working room copies. A locally stored export may be useful input to a versioned importer, but it is not a ready server or recovered Unity project.

**CV3 is an original Metallic workstream.** The same [official announcements](https://steamcommunity.com/app/471710/announcements/) describe March 2026 fixes to synchronized CV2 variables, state machines and Studio properties. That history motivates explicit synchronization and migration fixtures. It does not establish an official CV3 runtime or make Metallic's current preview compatible with Rec Room circuits.

## Architecture and language conclusions

Use C#/.NET 10 for the launcher core, local platform and CV3 authoring implementation. The code already compiles and shares a typed model without a language bridge. Microsoft's [.NET policy](https://dotnet.microsoft.com/en-us/platform/support/policy/dotnet-core) lists .NET 10 as active LTS through 14 November 2028; the verified toolchain is SDK `10.0.402` with runtime `10.0.13`. Keep servicing versions pinned and review updates deliberately.

The desktop project currently pins Avalonia `11.3.22`. [Avalonia's platform documentation](https://docs.avaloniaui.net/docs/supported-platforms) supports the choice of one C# desktop shell, subject to package-version and device testing. A cross-platform UI framework does not convert the Windows game into a Linux or Android client. Preserve the existing Go Atlas work; replacing working infrastructure solely to standardize its language adds no measured benefit.

Rust remains an option for a separately measured native bottleneck or isolated worker. A Rust rewrite of the whole game is not supported by the current evidence: source recovery, scene/asset reconstruction, input, XR and network compatibility are still distinct engineering problems. If scripting later grows beyond the typed node catalog, evaluate fuel limits and deterministic host imports against [Wasmtime's determinism guidance](https://docs.wasmtime.dev/examples-deterministic-wasm-execution.html) before adopting a general-purpose runtime.

## How Metallic can demonstrate better quality

These are proposed comparative methods, not current performance claims. Use the same scene, device, resolution, refresh rate, player count, route and measurement duration. Record software versions, warm/cold cache, packet conditions, raw captures and a reproducible procedure. Keep unknown competitor cells blank rather than assigning scores.

| Dimension | Evidence to collect | Proposed acceptance basis |
| --- | --- | --- |
| Launch and repair | Process-start failures, changed-file handling, interrupted update/rollback records | A selected installation has a reproducible launch route and cannot silently switch binaries |
| Frame pacing | CPU/GPU frame-time distributions, p95/p99, missed/reprojected frames, memory | Compare identical workloads; each declared headset refresh rate has an explicit measured budget |
| Room lifecycle | Cold/cached join p50/p95, reconnect results, host-restart recovery | Two real clients complete join, shared interaction, disconnect and reconnect |
| Creator reliability | Save/reload/revision conflicts, undo and import diagnostics | No lost acknowledged edits; unsupported fields remain visible |
| CV3 correctness | Typed fixtures, deterministic state hashes, quota failures, synchronization traces | Preview conformance first, authoritative multiplayer conformance as a separate gate |
| Operations | Peak bandwidth per player, tick backlog, storage latency, backup restoration | Publish measured capacity for specified hardware and workload |
| Voice and moderation | Microphone permissions, spatial playback, mute/block/report flows | Real audio paths and persisted report outcomes, without simulated success |
| Interface quality | Owner-reviewed launcher/editor captures, keyboard/controller flows, DPI and accessibility | Evaluate actual installed workflows and record remaining issues |

For the compiled Windows baseline, [PresentMon](https://github.com/GameTechDev/PresentMon) provides an external frame-event measurement route. Full [Unity Profiler attachment](https://docs.unity3d.com/2022.2/Documentation/Manual/profiler-profiling-applications.html) depends on an appropriate development build; do not presume that the supplied release binaries expose those instrumentation paths. Headset compositor and thermal captures are additional measurements, not substitutes for desktop FPS.

## Immediate research backlog

1. Identify the exact game-date version behind the measured Steam build ID and retain a complete redacted inventory; do not substitute Unity's engine version for the game version.
2. Reconcile the recovered `/REA` [build audit](evidence/REA_HISTORY_RECOVERY_2026-10-20.md) with the current inventory. It provides concrete prior build evidence, but its earlier assistant claim of game version `20260323.14` still needs independent manifest confirmation; a complete Unity source project has not been established.
3. Confirm Campus/RecLauncher upstream identities and all external-launcher dependencies through owner-selected artifacts; preserve existing launcher routes.
4. Obtain versioned, project-owned manifests for Vanilla, MeowNet and Phoenix; resolve the two Vanilla domain presentations and Phoenix's unavailable direct page.
5. Establish one reproducible, permitted modern-client bootstrap experiment with explicit protocol gaps before building a broad compatibility backend.
6. Run the first real two-client room and device frame-time measurements only after the relevant integration gate is implemented.

Sources were checked on 20 October 2026. Live pages, default branches and indexed claims may change; future comparisons must retain a dated artifact or commit and repeat the relevant behavior check.
