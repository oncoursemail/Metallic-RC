# Metallic-RC launcher core

Original C#/.NET 10 code for inspecting and registering an existing local Rec Room client, verifying its imported files, and producing a typed launch plan. There are no NuGet dependencies and no third-party launcher or game code in this project.

## What the core does

- Recognizes `RecRoom.exe` with `RecRoom_Data`, or `Recroom_Release.exe` with `Recroom_Release_Data`, and checks the executable and `UnityPlayer.dll` for PE signatures.
- Reads UnityPlayer's version resource when available. `GameAssembly.dll` is reported as IL2CPP evidence; a managed `Assembly-CSharp.dll` is reported with a dummy/reconstructed-assembly caveat. A Unity project requires `Assets/`, `Packages/manifest.json`, and `ProjectSettings/ProjectVersion.txt` together.
- Enumerates regular files off the caller's UI thread and hashes launch-related files asynchronously with streaming SHA-256. It fingerprints EXE/DLL/native-plugin files, boot configuration, relevant Unity metadata, and known loader configuration. Other content contributes file-count and byte totals, not an invented content-integrity guarantee.
- Reports the observed `0000525200005252` header as an opaque container. It never decrypts the container, loads an assembly, treats it as JSON, or claims the original source has been recovered.
- Refuses symbolic links and reparse points, including linked ancestors. Traversal and file/issue/key-file counts are bounded. Inaccessible or changing files make the inventory incomplete and block import.
- Writes only a catalog and lock file in the caller's separate state directory. Import never copies, patches, renames, or writes into the original client or registered external-launcher directories.
- Uses a bounded, versioned JSON schema with unknown-property and duplicate-key rejection. Invalid catalogs are surfaced and preserved. Updates use a cross-process lock and a flushed temporary file followed by atomic replacement.
- Rechecks every stored key-file fingerprint, added/missing launch files, layout, wrapper markers, and separately registered external-launcher EXE before planning or starting a process.

## Public entrypoints

```csharp
using Metallic.RC.Core;

var inventory = await new ClientInspector().InspectAsync(absoluteInstallPath, cancellationToken);
var catalog = new LauncherCatalog(absoluteStateDirectory);

var imported = await catalog.ImportAsync(
    new ImportRequest(absoluteInstallPath, "My installed 2026 client"), cancellationToken);

var registrations = await catalog.ListAsync(cancellationToken);
var verification = await catalog.VerifyAsync(imported.Id, cancellationToken);
var plan = await catalog.CreateLaunchPlanAsync(
    imported.Id, LaunchMode.Screen, cancellationToken: cancellationToken);

// Present the concrete plan to the caller/player before requesting this operation.
// This method re-verifies the catalog and files and checks the platform again.
var started = await catalog.LaunchAsync(plan, cancellationToken);
```

Inspection, registration, verification, and planning run on Windows or Linux. The recognized clients are Windows PE entrypoints; execution is blocked on Linux. No Wine/Proton adapter is supplied, and this is not a Linux client port.

Direct client plans contain exactly one literal argument: `+forcemode:screen` or `+forcemode:vr`. These intents reflect the owner's existing launch scripts; they do not establish headset support or a successful game session. Execution uses `ProcessStartInfo.ArgumentList` and `UseShellExecute = false`.

## Campus and other external launchers

Register an external launcher only by passing its full EXE path explicitly:

```csharp
var imported = await catalog.ImportAsync(new ImportRequest(
    absoluteInstallPath, "Campus installation", absoluteCampusLauncherPath));

var plan = await catalog.CreateLaunchPlanAsync(
    imported.Id,
    mode: LaunchMode.Screen,
    target: LaunchTarget.ExternalLauncher,
    approvedExternalLauncherPath: absoluteCampusLauncherPath);
```

The external executable has its own fingerprint, its containing directory is its working directory, and it receives no arguments: its own UI selects screen/VR mode. The caller must provide that exact path again when creating the plan. A catalog file cannot supply command-line arguments or implicitly choose an arbitrary external executable. Guard, injector, anti-cheat, bypass, and direct game executable names are rejected as external entrypoints.

Known NexaNet and loader markers make direct game launch unavailable so the core does not bypass an existing launcher's required route. Detection is evidence, not a full analysis of the wrapper. Registering a launcher does not establish that its dependencies or behavior have been audited, and this core does not start it during import, inspection, verification, or planning.

## Integrity boundaries

An imported SHA-256 baseline identifies the files the user selected and detects subsequent changes. It does **not** establish authorship, authenticity, permissions, or malware safety. The catalog is local JSON, not a signed trust authority. A player-facing launcher must present the selected installation and entrypoint concretely, and must only call `LaunchAsync` in response to an explicit launch action. Treat a catalog received from another person as untrusted data; do not auto-launch it.

`LaunchPlan` objects are constructed in code after verification, never deserialized from a manifest. A changed installation root, executable, working directory, or argument list invalidates a previously reviewed plan. Reverification is a point-in-time filesystem check, not an immutable snapshot or protection against a privileged local process racing every filesystem operation. Keep the original client idle during import and verification.

Asset bundles are not all hashed, and external-launcher dependencies outside the imported client are not comprehensively audited. File-count or total-size drift is disclosed; same-size changes to non-fingerprinted content are outside the integrity check. A recognized layout is not evidence of a working server, complete source project, executable compatibility, or tested gameplay.

## Acceptance checks

Run from the repository root with the pinned .NET SDK:

```sh
dotnet run --project tests/Metallic.RC.Core.Tests/Metallic.RC.Core.Tests.csproj
```

The dependency-free console harness checks opaque metadata, source-project markers, zero source writes, external-launcher intent, literal arguments, missing/tampered/added binary files, corrupt schemas, traversal, linked paths, concurrent imports, cancellation, and launch-plan reverification. Its tiny PE fixtures test format recognition only. They are not runnable games and are never launched. A passing harness does not count as a real client, Windows, VR, or server playtest.

On Windows, the symbolic-link check requires Developer Mode or appropriate symlink privileges; it fails explicitly if unavailable instead of reporting an unexecuted check as passing.
