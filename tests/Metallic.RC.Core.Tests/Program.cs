using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text.Json.Nodes;
using Metallic.RC.Core;

var tests = new (string Name, Func<Task> Body)[]
{
    ("IL2CPP inventory, opaque metadata, and zero source writes", Inventory),
    ("Release layout and source-project markers", LayoutAndProject),
    ("Managed assembly evidence remains a caveat", ManagedEvidence),
    ("Catalog import, literal launch arguments, and duplicate replacement", ImportAndPlan),
    ("Missing and modified launch files invalidate verification", TamperAndMissing),
    ("New nested plugin binaries invalidate verification", AddedPlugin),
    ("Asset total changes are disclosed without inventing asset hashes", AssetChange),
    ("External launcher requires explicit path and receives no arguments", ExternalLauncher),
    ("NexaNet markers block direct client launch", Wrapper),
    ("Direct guard and injector entrypoints are rejected", GuardRejection),
    ("Malformed, duplicate-key, unknown-field, and missing-schema catalogs fail closed", CorruptCatalog),
    ("Manifest path traversal and alternate command targets are rejected", ManifestPaths),
    ("State inside source installation is rejected before any write", StateIsolation),
    ("Reparse-link roots, children, and state directories are rejected", Links),
    ("Concurrent imports preserve both registrations", ConcurrentImports),
    ("Canceled inspection cannot produce an import", Cancellation),
    ("A previously planned executable is reverified before launch", LaunchReverification),
    ("A reviewed plan cannot be redirected by changing the catalog root", PlanRootBinding)
};
var failed = 0;
foreach (var test in tests)
{
    try { await test.Body(); Console.WriteLine("PASS " + test.Name); }
    catch (Exception error) { failed++; Console.Error.WriteLine("FAIL " + test.Name + ": " + error); }
}
Console.WriteLine($"{tests.Length - failed}/{tests.Length} acceptance checks passed. Fixture PE headers are synthetic; no game, external launcher, guard, or injector was executed.");
return failed == 0 ? 0 : 1;

static void Check(bool condition, string message)
{
    if (!condition) throw new InvalidOperationException(message);
}

static async Task ExpectCode(Func<Task> operation, params string[] codes)
{
    try { await operation(); }
    catch (CatalogException error) when (codes.Contains(error.Code)) { return; }
    throw new InvalidOperationException("Expected CatalogException: " + string.Join(", ", codes));
}

static async Task Inventory()
{
    using var fixture = new Fixture();
    var before = fixture.Snapshot();
    var inventory = await new ClientInspector().InspectAsync(fixture.Game);
    Check(inventory.CanImport && inventory.IsRecognizedClient && inventory.IsComplete, "A complete synthetic layout should be recognized structurally.");
    Check(inventory.ScriptingBackend == ScriptingBackend.Il2Cpp, "IL2CPP marker was not detected.");
    Check(inventory.Fingerprints.Count == 5, "Expected executable, UnityPlayer, GameAssembly, and two metadata hashes.");
    Check(inventory.OpaqueMetadataFiles.Count == 2, "Opaque headers were not disclosed.");
    Check(inventory.Issues.Any(issue => issue.Code == "opaque_rr_container"), "Opaque metadata must be clearly labeled.");
    Check(inventory.FileCount == before.Count && inventory.TotalBytes == Directory.GetFiles(fixture.Game, "*", SearchOption.AllDirectories).Sum(path => new FileInfo(path).Length), "Inventory totals are not exact for regular files.");
    Check(before.OrderBy(pair => pair.Key).SequenceEqual(fixture.Snapshot().OrderBy(pair => pair.Key)), "Inspection wrote to the source installation.");
}

static async Task LayoutAndProject()
{
    using var fixture = new Fixture(releaseLayout: true);
    Directory.CreateDirectory(Path.Combine(fixture.Game, "Assets"));
    Directory.CreateDirectory(Path.Combine(fixture.Game, "Packages"));
    Directory.CreateDirectory(Path.Combine(fixture.Game, "ProjectSettings"));
    await File.WriteAllTextAsync(Path.Combine(fixture.Game, "Packages", "manifest.json"), "{}");
    await File.WriteAllTextAsync(Path.Combine(fixture.Game, "ProjectSettings", "ProjectVersion.txt"), "m_EditorVersion: fixture-only");
    var result = await new ClientInspector().InspectAsync(fixture.Game);
    Check(result.ClientExecutableRelativePath == "Recroom_Release.exe" && result.ClientDataRelativePath == "Recroom_Release_Data", "Release executable/data pair was not detected.");
    Check(result.IsUnityProject && result.ProjectMarkers.Count == 3, "A source project requires all three markers.");
    File.Delete(Path.Combine(fixture.Game, "Packages", "manifest.json"));
    Check(!(await new ClientInspector().InspectAsync(fixture.Game)).IsUnityProject, "Partial project markers must not establish a project.");
}

static async Task ManagedEvidence()
{
    using var fixture = new Fixture();
    File.Delete(Path.Combine(fixture.Game, "GameAssembly.dll"));
    var managed = Path.Combine(fixture.Data, "Managed");
    Directory.CreateDirectory(managed);
    Fixture.WritePe(Path.Combine(managed, "Assembly-CSharp.dll"));
    var result = await new ClientInspector().InspectAsync(fixture.Game);
    Check(result.ScriptingBackend == ScriptingBackend.ManagedAssemblies, "Managed marker was not detected.");
    Check(result.Issues.Any(issue => issue.Code == "managed_marker_only"), "Dummy/reconstructed assembly caveat is required.");
    Fixture.WritePe(Path.Combine(fixture.Game, "GameAssembly.dll"));
    Check((await new ClientInspector().InspectAsync(fixture.Game)).ScriptingBackend == ScriptingBackend.MixedMarkers, "Mixed markers must remain explicit.");
}

static async Task ImportAndPlan()
{
    using var fixture = new Fixture();
    var before = fixture.Snapshot();
    var catalog = new LauncherCatalog(fixture.State);
    Check((await catalog.ListAsync()).Count == 0 && !Directory.Exists(fixture.State), "Listing an absent catalog should not write.");
    var imported = await catalog.ImportAsync(new(fixture.Game, "Local 2026 build"));
    var screen = await catalog.CreateLaunchPlanAsync(imported.Id);
    var vr = await catalog.CreateLaunchPlanAsync(imported.Id, LaunchMode.Vr);
    Check(screen.Arguments.SequenceEqual(["+forcemode:screen"]) && vr.Arguments.SequenceEqual(["+forcemode:vr"]), "Arguments must be literal individual values.");
    Check(screen.ExecutablePath == Path.Combine(fixture.Game, "RecRoom.exe") && screen.WorkingDirectory == fixture.Game, "Executable and working directory must match the imported layout.");
    var processInfo = screen.ToProcessStartInfo();
    Check(!processInfo.UseShellExecute && processInfo.ArgumentList.Count == 1 && processInfo.Arguments.Length == 0, "Shell command construction is forbidden.");
    if (!OperatingSystem.IsWindows())
    {
        Check(!screen.CanExecuteOnCurrentPlatform && screen.BlockReason is not null, "A Windows entrypoint must be blocked on this platform.");
        await ExpectCode(() => catalog.LaunchAsync(screen), "platform_not_supported");
    }
    await ExpectCode(() => catalog.ImportAsync(new(fixture.Game, "Duplicate")), "duplicate_installation");
    var replaced = await catalog.ImportAsync(new(fixture.Game, "Renamed", ReplaceExisting: true));
    Check(replaced.Id == imported.Id && (await catalog.ListAsync()).Count == 1, "Explicit replacement must preserve identity.");
    Check(before.OrderBy(pair => pair.Key).SequenceEqual(fixture.Snapshot().OrderBy(pair => pair.Key)), "Import changed the source installation.");
    Check(!Directory.GetFiles(fixture.State, "*.tmp").Any(), "Atomic writes left temporary files behind.");
}

static async Task TamperAndMissing()
{
    using var fixture = new Fixture();
    var catalog = new LauncherCatalog(fixture.State);
    var imported = await catalog.ImportAsync(new(fixture.Game, "Tamper test"));
    var path = Path.Combine(fixture.Game, "GameAssembly.dll");
    var bytes = await File.ReadAllBytesAsync(path);
    bytes[^1] ^= 0xff;
    await File.WriteAllBytesAsync(path, bytes);
    var modified = await catalog.VerifyAsync(imported.Id);
    Check(!modified.IsValid && modified.Issues.Any(issue => issue.Code == "file_changed"), "A changed hash must invalidate the installation.");
    await ExpectCode(() => catalog.CreateLaunchPlanAsync(imported.Id), "verification_failed");
    File.Delete(Path.Combine(fixture.Game, "UnityPlayer.dll"));
    var missing = await catalog.VerifyAsync(imported.Id);
    Check(!missing.IsValid && missing.Issues.Any(issue => issue.Code == "file_missing"), "A missing binary must invalidate the installation.");
}

static async Task AddedPlugin()
{
    using var fixture = new Fixture();
    var catalog = new LauncherCatalog(fixture.State);
    var imported = await catalog.ImportAsync(new(fixture.Game, "Plugin test"));
    Directory.CreateDirectory(Path.Combine(fixture.Data, "Plugins", "x86_64"));
    Fixture.WritePe(Path.Combine(fixture.Data, "Plugins", "x86_64", "NewPlugin.dll"));
    var result = await catalog.VerifyAsync(imported.Id);
    Check(!result.IsValid && result.Issues.Any(issue => issue.Code == "file_added"), "New native plugins must not silently inherit the imported baseline.");
}

static async Task AssetChange()
{
    using var fixture = new Fixture();
    var catalog = new LauncherCatalog(fixture.State);
    var imported = await catalog.ImportAsync(new(fixture.Game, "Asset totals test"));
    await File.WriteAllTextAsync(Path.Combine(fixture.Data, "room.assetbundle"), "changed asset payload");
    var result = await catalog.VerifyAsync(imported.Id);
    Check(result.IsValid && result.Issues.Any(issue => issue.Code == "inventory_totals_changed"), "Asset total drift should be disclosed separately from key-file verification.");
}

static async Task ExternalLauncher()
{
    using var fixture = new Fixture();
    var launcherDirectory = Path.Combine(fixture.Root, "Campus space & dollar$(literal)");
    Directory.CreateDirectory(launcherDirectory);
    var launcher = Path.Combine(launcherDirectory, "Campus RecLauncher.exe");
    Fixture.WritePe(launcher);
    var catalog = new LauncherCatalog(fixture.State);
    var imported = await catalog.ImportAsync(new(fixture.Game, "Campus test", launcher));
    await ExpectCode(() => catalog.CreateLaunchPlanAsync(imported.Id, LaunchMode.Vr, LaunchTarget.ExternalLauncher), "external_launcher_confirmation_required");
    var plan = await catalog.CreateLaunchPlanAsync(imported.Id, LaunchMode.Vr, LaunchTarget.ExternalLauncher, launcher);
    Check(plan.Arguments.Count == 0 && plan.WorkingDirectory == launcherDirectory && plan.ExecutablePath == launcher, "External launcher must receive no inferred command-line arguments.");
    var other = Path.Combine(launcherDirectory, "Other.exe");
    Fixture.WritePe(other);
    await ExpectCode(() => catalog.CreateLaunchPlanAsync(imported.Id, target: LaunchTarget.ExternalLauncher, approvedExternalLauncherPath: other), "external_launcher_confirmation_required");
    var data = await File.ReadAllBytesAsync(launcher);
    data[^1] ^= 1;
    await File.WriteAllBytesAsync(launcher, data);
    Check(!(await catalog.VerifyAsync(imported.Id)).IsValid, "External launcher tampering must invalidate verification.");
}

static async Task Wrapper()
{
    using var fixture = new Fixture();
    Fixture.WritePe(Path.Combine(fixture.Game, "nexanet.dll"));
    var catalog = new LauncherCatalog(fixture.State);
    var imported = await catalog.ImportAsync(new(fixture.Game, "Wrapper test"));
    Check(imported.Inventory.ExternalModificationsDetected, "NexaNet marker must be explicit.");
    await ExpectCode(() => catalog.CreateLaunchPlanAsync(imported.Id), "external_launcher_required");
}

static async Task GuardRejection()
{
    using var fixture = new Fixture();
    var catalog = new LauncherCatalog(fixture.State);
    foreach (var name in new[] { "NexaGuard.exe", "DllInjector.exe", "AntiCheat.exe", "RecRoom.exe" })
    {
        var external = Path.Combine(fixture.Root, name);
        Fixture.WritePe(external);
        await ExpectCode(() => catalog.ImportAsync(new(fixture.Game, "Rejected", external)), "invalid_external_launcher");
    }
    Check(!Directory.Exists(fixture.State), "Rejected external imports must not create a catalog.");
}

static async Task CorruptCatalog()
{
    using var fixture = new Fixture();
    var catalog = new LauncherCatalog(fixture.State);
    await catalog.ImportAsync(new(fixture.Game, "Schema test"));
    var original = await File.ReadAllTextAsync(catalog.RegistryPath);
    foreach (var invalid in new[] { "{broken", "{}", original.Replace("\"schemaVersion\": 1", "\"schemaVersion\": 1, \"schemaVersion\": 1"), original.Replace("\"schemaVersion\": 1", "\"schemaVersion\": 1, \"command\": \"anything.exe\"") })
    {
        await File.WriteAllTextAsync(catalog.RegistryPath, invalid);
        await ExpectCode(() => catalog.ListAsync(), "catalog_corrupt");
        await ExpectCode(() => catalog.ImportAsync(new(fixture.Game, "Do not replace", ReplaceExisting: true)), "catalog_corrupt");
        Check(await File.ReadAllTextAsync(catalog.RegistryPath) == invalid, "A corrupt registry must never be silently reset or overwritten.");
    }
}

static async Task ManifestPaths()
{
    using var fixture = new Fixture();
    var catalog = new LauncherCatalog(fixture.State);
    var imported = await catalog.ImportAsync(new(fixture.Game, "Path test"));
    var original = await File.ReadAllTextAsync(catalog.RegistryPath);
    var json = JsonNode.Parse(original)!;
    json["installations"]![0]!["inventory"]!["fingerprints"]![0]!["relativePath"] = "../../escape.exe";
    await File.WriteAllTextAsync(catalog.RegistryPath, json.ToJsonString());
    await ExpectCode(() => catalog.ListAsync(), "invalid_relative_path", "catalog_corrupt");
    json = JsonNode.Parse(original)!;
    json["installations"]![0]!["inventory"]!["clientExecutableRelativePath"] = "UntrustedCommand.exe";
    await File.WriteAllTextAsync(catalog.RegistryPath, json.ToJsonString());
    await ExpectCode(() => catalog.ListAsync(), "catalog_corrupt");
    await ExpectCode(() => catalog.VerifyAsync("../../outside"), "invalid_id");
    await File.WriteAllTextAsync(catalog.RegistryPath, original);
    Check((await catalog.VerifyAsync(imported.Id)).IsValid, "The original valid catalog should still verify.");
}

static async Task StateIsolation()
{
    using var fixture = new Fixture();
    var before = fixture.Snapshot();
    var state = Path.Combine(fixture.Game, "catalog-state");
    var catalog = new LauncherCatalog(state);
    await ExpectCode(() => catalog.ImportAsync(new(fixture.Game, "Do not write")), "state_inside_installation");
    Check(!Directory.Exists(state) && before.OrderBy(pair => pair.Key).SequenceEqual(fixture.Snapshot().OrderBy(pair => pair.Key)), "State isolation must happen before any source write.");
    await ExpectCode(() => Task.FromResult(new LauncherCatalog("relative/path")), "invalid_path");
}

static async Task Links()
{
    using var fixture = new Fixture();
    var sourceLink = Path.Combine(fixture.Root, "linked-game");
    await CreateDirectoryLinkAsync(sourceLink, fixture.Game);
    await ExpectCode(() => new ClientInspector().InspectAsync(sourceLink), "link_not_allowed");
    var outside = Path.Combine(fixture.Root, "external-tree");
    Directory.CreateDirectory(outside);
    await File.WriteAllTextAsync(Path.Combine(outside, "must-not-inspect.txt"), "outside");
    await CreateDirectoryLinkAsync(Path.Combine(fixture.Game, "linked-assets"), outside);
    var result = await new ClientInspector().InspectAsync(fixture.Game);
    Check(result.SkippedLinkCount == 1 && !result.CanImport && result.FileCount == 6, "A child link must be skipped without inspecting its target.");
    await CreateDirectoryLinkAsync(fixture.State, outside);
    await ExpectCode(() => Task.FromResult(new LauncherCatalog(fixture.State)), "link_not_allowed");
}

static async Task CreateDirectoryLinkAsync(string link, string target)
{
    if (!OperatingSystem.IsWindows())
    {
        Directory.CreateSymbolicLink(link, target);
        return;
    }
    // Exercise the Windows reparse-point boundary without requiring Developer Mode.
    var info = new System.Diagnostics.ProcessStartInfo(Path.Combine(Environment.SystemDirectory, "cmd.exe"))
    { UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true };
    foreach (var arg in new[] { "/d", "/c", "mklink", "/J", link, target }) info.ArgumentList.Add(arg);
    using var process = System.Diagnostics.Process.Start(info) ?? throw new InvalidOperationException("Could not create a test junction.");
    var stdout = process.StandardOutput.ReadToEndAsync();
    var stderr = process.StandardError.ReadToEndAsync();
    await process.WaitForExitAsync();
    Check(process.ExitCode == 0, $"Test junction creation failed: {await stdout} {await stderr}");
    Check((File.GetAttributes(link) & FileAttributes.ReparsePoint) != 0, "Fixture must be an actual Windows reparse point.");
}

static async Task ConcurrentImports()
{
    using var fixture = new Fixture();
    using var second = new Fixture();
    var firstCatalog = new LauncherCatalog(fixture.State);
    var secondCatalog = new LauncherCatalog(fixture.State);
    await Task.WhenAll(firstCatalog.ImportAsync(new(fixture.Game, "First")), secondCatalog.ImportAsync(new(second.Game, "Second")));
    Check((await firstCatalog.ListAsync()).Count == 2, "Concurrent writes lost an installation.");
}

static async Task Cancellation()
{
    using var fixture = new Fixture();
    using var cancellation = new CancellationTokenSource();
    cancellation.Cancel();
    try { await new LauncherCatalog(fixture.State).ImportAsync(new(fixture.Game, "Canceled"), cancellation.Token); }
    catch (OperationCanceledException)
    {
        Check(!Directory.Exists(fixture.State), "Canceled inspection must not write registry state.");
        return;
    }
    throw new InvalidOperationException("Canceled import did not propagate cancellation.");
}

static async Task LaunchReverification()
{
    using var fixture = new Fixture();
    var catalog = new LauncherCatalog(fixture.State);
    var imported = await catalog.ImportAsync(new(fixture.Game, "Reverification"));
    var plan = await catalog.CreateLaunchPlanAsync(imported.Id);
    File.Delete(Path.Combine(fixture.Game, "RecRoom.exe"));
    await ExpectCode(() => catalog.LaunchAsync(plan), "verification_failed");
}

static async Task PlanRootBinding()
{
    using var fixture = new Fixture();
    using var second = new Fixture();
    var catalog = new LauncherCatalog(fixture.State);
    var imported = await catalog.ImportAsync(new(fixture.Game, "Reviewed installation"));
    var plan = await catalog.CreateLaunchPlanAsync(imported.Id);
    await catalog.ImportAsync(new(second.Game, "Different installation"));
    var json = JsonNode.Parse(await File.ReadAllTextAsync(catalog.RegistryPath))!;
    var redirected = json["installations"]![1]!.DeepClone();
    redirected["id"] = imported.Id;
    json["installations"] = new JsonArray(redirected);
    await File.WriteAllTextAsync(catalog.RegistryPath, json.ToJsonString());
    await ExpectCode(() => catalog.LaunchAsync(plan), "launch_plan_changed");
}

sealed class Fixture : IDisposable
{
    public Fixture(bool releaseLayout = false)
    {
        Root = Path.Combine(AppContext.BaseDirectory, "acceptance-" + Guid.NewGuid().ToString("N"));
        Game = Path.Combine(Root, "client space & dollar$(literal)");
        State = Path.Combine(Root, "catalog-state");
        Data = Path.Combine(Game, releaseLayout ? "Recroom_Release_Data" : "RecRoom_Data");
        Directory.CreateDirectory(Data);
        WritePe(Path.Combine(Game, releaseLayout ? "Recroom_Release.exe" : "RecRoom.exe"));
        WritePe(Path.Combine(Game, "UnityPlayer.dll"));
        WritePe(Path.Combine(Game, "GameAssembly.dll"));
        File.WriteAllBytes(Path.Combine(Data, "globalgamemanagers"), [0, 0, 0x52, 0x52, 0, 0, 0x52, 0x52, 99]);
        File.WriteAllBytes(Path.Combine(Data, "ScriptingAssemblies.json"), [0, 0, 0x52, 0x52, 0, 0, 0x52, 0x52, 88]);
        File.WriteAllText(Path.Combine(Data, "room.assetbundle"), "asset");
    }
    public string Root { get; }
    public string Game { get; }
    public string Data { get; }
    public string State { get; }
    public static void WritePe(string path)
    {
        // Minimal format-recognition fixture, deliberately not a runnable executable or real Unity file.
        var bytes = new byte[128];
        bytes[0] = (byte)'M'; bytes[1] = (byte)'Z';
        BinaryPrimitives.WriteInt32LittleEndian(bytes.AsSpan(0x3c, 4), 64);
        bytes[64] = (byte)'P'; bytes[65] = (byte)'E';
        bytes[^1] = 42;
        File.WriteAllBytes(path, bytes);
    }
    public Dictionary<string, string> Snapshot() => Directory.GetFiles(Game, "*", SearchOption.AllDirectories)
        .ToDictionary(path => Path.GetRelativePath(Game, path), path => Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(path))));
    public void Dispose()
    {
        if (!Directory.Exists(Root)) return;
        // Remove fixture links before deleting their targets; dangling junctions
        // can fail during recursive directory cleanup on Windows.
        var pending = new Stack<DirectoryInfo>();
        pending.Push(new DirectoryInfo(Root));
        while (pending.TryPop(out var directory))
        {
            foreach (var item in directory.EnumerateFileSystemInfos())
            {
                if ((item.Attributes & FileAttributes.ReparsePoint) != 0) item.Delete();
                else if (item is DirectoryInfo child) pending.Push(child);
            }
        }
        Directory.Delete(Root, recursive: true);
    }
}
