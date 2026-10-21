# Metallic-RC

Metallic-RC starts from the owner's installed 2026 Rec Room client. The rejected Meadow prototype, its active world design, native build scripts and release links have been removed.

**Current compiled deliverable:** an original C# desktop launcher, read-only client inventory and verification, and a local CV3 creator website/service. **A rebuilt Rec Room executable and a compatible Metallic multiplayer/voice backend are not yet implemented.**

## Use the desktop tools

The complete desktop package contains the launcher, the local creator service and their .NET runtimes. Open `Metallic.RC.Launcher.exe` on Windows or `Metallic.RC.Launcher` on Linux.

1. **Find installations** or **Import installation** registers an existing client folder and records launch-file fingerprints.
2. **Verify files** detects changes; **Show launch plan** displays the precise executable and arguments before starting anything.
3. **Launch client** starts a supported local client. Installations requiring an external launcher use an explicitly registered executable and its own connection flow.
4. **Creator tools** opens the bundled local website. Create circuit drafts, connect typed nodes, edit their values, save revisions, validate and preview execution.

The launcher can inspect a Windows installation on Linux, but does not pretend that its Windows executable is a native Linux game. The current creator is a local authoring tool; it is not yet integrated into the compiled game or deployed at Metallic.club.

[Installed build and final validation](docs/evidence/FINAL_DESKTOP_HANDOFF_2026-10-20.md) · [Desktop usage](docs/DESKTOP_README.txt) · [Rework architecture and actual build evidence](docs/COMPILED_CLIENT_REWORK.md) · [Revival research](docs/REVIVAL_RESEARCH_2026-10-20.md) · [Recovered REA history](docs/evidence/REA_HISTORY_RECOVERY_2026-10-20.md)

## Build from source

Install the SDK pinned in `global.json` and use the included solution, `Metallic.RC.slnx`.

```powershell
# Windows: acceptance checks and a self-contained desktop package
.\scripts\build-desktop.ps1 -Runtime win-x64
```

```bash
# Linux: acceptance checks and a self-contained desktop package
bash scripts/build-desktop.sh linux-x64
```

Both scripts refuse to replace an existing output directory. Choose a new output path for another build. Packages include a file-hash manifest; source provenance and device acceptance belong in a dated validation record.

```bash
# Individual checks
 dotnet run --project tests/Metallic.RC.Core.Tests -c Release
 dotnet run --project tests/Metallic.RC.Platform.Tests -c Release

# Run the local creator directly
 dotnet run --project src/Metallic.RC.Platform -c Release -- --port 5187 --data-dir /absolute/path/to/local-state
```

The browser suite is `tests/browser/verify_creator.mjs`; it uses real local API requests. See the [platform README](src/Metallic.RC.Platform/README.md) for the API, graph schema and limits.

## Source layout

| Component | Purpose | Current boundary |
| --- | --- | --- |
| `src/Metallic.RC.Core` | Client discovery evidence, persisted registrations, SHA256 verification and reviewed launch plans | Does not reconstruct Unity source or implement Rec Room networking |
| `src/Metallic.RC.Launcher` | Avalonia desktop interface and CLI | Windows/Linux tooling; device-specific game support needs separate proof |
| `src/Metallic.RC.Creator` | Typed graph validation, stable compilation hashes and bounded execution | Eleven original preview node types; no in-game CV3 integration yet |
| `src/Metallic.RC.Platform` | Loopback API and creator website, persistent drafts and revision checks | Local authoring; multiplayer, voice and account migration capability flags remain false |
| `services/metallic-control` | Existing Go Atlas operations prototype | Capacity/control lab, not an executable game room host |
| `game/` | Retained creator, companion, teaching and watch contracts | Data/logic prototypes for later integration |

## Actual starting files and remaining integration

The current Steam and installed NexaNet copies share the same inspected Unity 6 core files. Campus2025 is a separate Unity 2021 installation. Exact paths, hashes, sizes and provenance are recorded in the [PC inventory](docs/evidence/pc-client-inventory-2026-10-20.json). The recovered REA audit adds prior complete-manifest and PE/resource analysis, with its limits explicitly retained.

The next game integration gate is a real modern-client connection to owned services, followed by two clients sharing room state, voice, reconnect and persistence. An HTTP room record, circuit preview or successful process launch is not evidence for that gate. Native Android/Quest and Linux game builds also need target-specific source, plug-ins, assets and device testing.

## Project identity and retained plans

**Metallic-RC** is the Rec Room project. **Metallic-MC**, **Metallic-RB** and **Metallic-PX** remain separate projects; the family domain is **Metallic.club**. This change does not deploy that domain or alter those other projects.

[Project identities](docs/PROJECT_IDENTITIES.md) · [Roadmap](docs/ROADMAP.md) · [Development and release gates](docs/DEVELOPMENT.md) · [Creator/learning/companions](docs/METALLIC_CREATOR_LEARNING_AI.md) · [Feature catalog](docs/FEATURE_CATALOG.md)

The older website and engine research remain historical references. The compiled-client rework and the owner's 20 October correction define the current direction. Progress issue counts are not gameplay or performance claims; measured results must name the build, hardware, workload and evidence.

## Owned backend checkpoint (21 October 2026)

The repository now includes a persistent C# owned backend with real HTTP and WebSocket integration tests, a live launcher probe, local backup/restore checks, and a bounded Windows local-service runner. The Main deployment is loopback-only on port 4187. **Original Rec Room client login/gameplay compatibility is not yet verified; unimplemented native routes return 501.**

See [the measured backend checkpoint](docs/evidence/OWNED_BACKEND_2026-10-21.md), [API scope](src/Metallic.RC.Backend/README.md), and [local operations](ops/backend/README.md). This is not a public game release.
