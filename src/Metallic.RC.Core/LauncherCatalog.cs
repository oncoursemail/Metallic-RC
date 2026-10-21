using System.Diagnostics;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Metallic.RC.Core;

/// <summary>Owns only a bounded JSON catalog in the caller-supplied state directory.</summary>
public sealed class LauncherCatalog
{
    private const int MaximumRegistryBytes = 16 * 1024 * 1024;
    private const int MaximumInstallations = 100;
    private readonly ClientInspector inspector = new();
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        PropertyNameCaseInsensitive = false,
        WriteIndented = true,
        IgnoreReadOnlyProperties = true,
        UnmappedMemberHandling = JsonUnmappedMemberHandling.Disallow,
        MaxDepth = 32,
        Converters = { new JsonStringEnumConverter(JsonNamingPolicy.CamelCase, allowIntegerValues: false) }
    };

    public LauncherCatalog(string stateDirectory)
    {
        StateDirectory = PathRules.Absolute(stateDirectory);
        PathRules.RejectLinks(StateDirectory, allowMissingLeaf: true);
        RegistryPath = Path.Combine(StateDirectory, "installations.json");
    }

    public string StateDirectory { get; }
    public string RegistryPath { get; }

    public async Task<IReadOnlyList<RegisteredInstallation>> ListAsync(CancellationToken cancellationToken = default)
        => (await ReadDocumentAsync(cancellationToken)).Installations.AsReadOnly();

    public async Task<RegisteredInstallation> ImportAsync(ImportRequest request, CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(request);
        var installRoot = PathRules.Absolute(request.InstallRoot);
        var displayName = PathRules.DisplayName(request.DisplayName);
        EnsureStateOutside(installRoot);
        var inventory = await inspector.InspectAsync(installRoot, cancellationToken);
        if (!inventory.CanImport)
            throw new CatalogException("inspection_failed", "The installation cannot be imported: "
                + string.Join(" ", inventory.Issues.Where(issue => issue.Severity != IssueSeverity.Information).Take(5).Select(issue => issue.Message)));

        RegisteredExternalLauncher? external = null;
        if (request.ExternalLauncherPath is not null)
        {
            var externalPath = ValidateExternalPath(request.ExternalLauncherPath);
            var workingDirectory = Path.GetDirectoryName(externalPath)!;
            EnsureStateOutside(workingDirectory);
            var read = await ClientInspector.ReadFingerprintAsync(externalPath, Path.GetFileName(externalPath), cancellationToken);
            if (!read.Fingerprint.IsWindowsPe)
                throw new CatalogException("invalid_external_launcher", "An external launcher must be a Windows PE executable.");
            external = new(externalPath, workingDirectory, read.Fingerprint.Length, read.Fingerprint.Sha256, true);
        }

        await using var catalogLock = await AcquireWriteLockAsync(cancellationToken);
        var document = await ReadDocumentAsync(cancellationToken);
        var existing = document.Installations.SingleOrDefault(item => PathRules.PathComparer.Equals(item.InstallRoot, installRoot));
        if (existing is not null && !request.ReplaceExisting)
            throw new CatalogException("duplicate_installation", "This installation is already imported. Explicit replacement is required to accept a new fingerprint baseline.");
        if (existing is null && document.Installations.Count >= MaximumInstallations)
            throw new CatalogException("catalog_full", "The catalog has reached its installation limit.");
        var entry = new RegisteredInstallation
        {
            Id = existing?.Id ?? Guid.NewGuid().ToString("N"), DisplayName = displayName, InstallRoot = installRoot,
            ImportedAtUtc = DateTimeOffset.UtcNow, Inventory = inventory, ExternalLauncher = external
        };
        if (existing is not null) document.Installations.Remove(existing);
        document.Installations.Add(entry);
        await WriteDocumentAsync(document with { Revision = Guid.NewGuid().ToString("N"), WrittenAtUtc = DateTimeOffset.UtcNow }, cancellationToken);
        return entry;
    }

    public async Task<VerificationResult> VerifyAsync(string installationId, CancellationToken cancellationToken = default)
    {
        PathRules.Id(installationId);
        var document = await ReadDocumentAsync(cancellationToken);
        var installation = document.Installations.SingleOrDefault(item => item.Id == installationId)
            ?? throw new CatalogException("installation_not_found", "No installation with that ID is in this catalog.");
        EnsureStateOutside(installation.InstallRoot);
        var current = await inspector.InspectAsync(installation.InstallRoot, cancellationToken);
        var issues = current.Issues.ToList();
        if (!current.CanImport)
            issues.Add(new(IssueSeverity.Error, "client_unavailable", "The imported client is missing, incomplete, or no longer a supported layout."));
        if (!string.Equals(current.ClientExecutableRelativePath, installation.Inventory.ClientExecutableRelativePath, StringComparison.Ordinal)
            || !string.Equals(current.ClientDataRelativePath, installation.Inventory.ClientDataRelativePath, StringComparison.Ordinal))
            issues.Add(new(IssueSeverity.Error, "client_layout_changed", "The client executable or data-directory selection has changed since import."));

        var expected = installation.Inventory.Fingerprints.ToDictionary(file => file.RelativePath, StringComparer.OrdinalIgnoreCase);
        var actual = current.Fingerprints.ToDictionary(file => file.RelativePath, StringComparer.OrdinalIgnoreCase);
        foreach (var (path, file) in expected)
        {
            if (!actual.TryGetValue(path, out var observed))
                issues.Add(new(IssueSeverity.Error, "file_missing", "An imported launch-related file is missing or unreadable.", path));
            else if (file.Length != observed.Length || !file.Sha256.Equals(observed.Sha256, StringComparison.OrdinalIgnoreCase)
                || file.IsWindowsPe != observed.IsWindowsPe)
                issues.Add(new(IssueSeverity.Error, "file_changed", "A launch-related file no longer matches its imported SHA-256 fingerprint.", path));
        }
        foreach (var path in actual.Keys.Except(expected.Keys, StringComparer.OrdinalIgnoreCase))
            issues.Add(new(IssueSeverity.Error, "file_added", "A new launch-related file was added after import; review and explicitly replace the baseline before launching.", path));
        if (current.ExternalModificationsDetected != installation.Inventory.ExternalModificationsDetected
            || !current.ExternalModificationMarkers.Order(StringComparer.Ordinal).SequenceEqual(installation.Inventory.ExternalModificationMarkers.Order(StringComparer.Ordinal)))
            issues.Add(new(IssueSeverity.Error, "wrapper_markers_changed", "External launcher or modification markers have changed since import."));
        if (current.FileCount != installation.Inventory.FileCount || current.TotalBytes != installation.Inventory.TotalBytes)
            issues.Add(new(IssueSeverity.Warning, "inventory_totals_changed", "The file count or total size differs from import. Asset bundles are counted but are not all fingerprinted."));

        if (installation.ExternalLauncher is { } external)
        {
            try
            {
                ValidateExternalPath(external.ExecutablePath);
                var read = await ClientInspector.ReadFingerprintAsync(external.ExecutablePath, Path.GetFileName(external.ExecutablePath), cancellationToken);
                if (read.Fingerprint.Length != external.Length || !read.Fingerprint.Sha256.Equals(external.Sha256, StringComparison.OrdinalIgnoreCase)
                    || !read.Fingerprint.IsWindowsPe)
                    issues.Add(new(IssueSeverity.Error, "external_launcher_changed", "The separately registered launcher no longer matches its imported fingerprint."));
            }
            catch (Exception error) when (ClientInspector.IsFileError(error))
            { issues.Add(new(IssueSeverity.Error, "external_launcher_unavailable", error.Message)); }
        }
        return new(installation, current, issues.AsReadOnly());
    }

    public async Task<LaunchPlan> CreateLaunchPlanAsync(string installationId, LaunchMode mode = LaunchMode.Screen,
        LaunchTarget target = LaunchTarget.Client, string? approvedExternalLauncherPath = null,
        CancellationToken cancellationToken = default)
    {
        if (!Enum.IsDefined(mode) || !Enum.IsDefined(target))
            throw new CatalogException("invalid_launch_intent", "The requested launch mode or target is unsupported.");
        var verified = await VerifyAsync(installationId, cancellationToken);
        if (!verified.IsValid)
            throw new CatalogException("verification_failed", "Launch planning is blocked: "
                + string.Join(" ", verified.Issues.Where(issue => issue.Severity == IssueSeverity.Error).Take(5).Select(issue => issue.Message)));
        var installation = verified.Installation;
        string executable, workingDirectory;
        string[] arguments;
        if (target == LaunchTarget.ExternalLauncher)
        {
            var external = installation.ExternalLauncher
                ?? throw new CatalogException("external_launcher_not_registered", "Explicitly register the intended external launcher during import first.");
            if (approvedExternalLauncherPath is null
                || !PathRules.PathComparer.Equals(ValidateExternalPath(approvedExternalLauncherPath), external.ExecutablePath))
                throw new CatalogException("external_launcher_confirmation_required", "Supply the exact registered external launcher path as explicit launch input; a catalog entry alone cannot select an arbitrary executable.");
            executable = external.ExecutablePath;
            workingDirectory = external.WorkingDirectory;
            arguments = []; // Campus and other external launchers own their screen/VR selection.
        }
        else
        {
            if (approvedExternalLauncherPath is not null)
                throw new CatalogException("invalid_launch_intent", "An external launcher path is only valid for an external-launcher target.");
            if (verified.CurrentInventory.ExternalModificationsDetected)
                throw new CatalogException("external_launcher_required", "This installation contains external launcher or modification markers. Direct game launch would bypass that launcher, so select its explicitly registered entrypoint.");
            executable = PathRules.Combine(installation.InstallRoot, installation.Inventory.ClientExecutableRelativePath!);
            workingDirectory = installation.InstallRoot;
            arguments = [mode == LaunchMode.Screen ? "+forcemode:screen" : "+forcemode:vr"];
        }
        var canExecute = OperatingSystem.IsWindows();
        return new(installation.Id, installation.InstallRoot, mode, target, executable, workingDirectory,
            arguments, canExecute,
            canExecute ? null : "This is a Windows PE entrypoint. Inspection and launch planning work here; execution requires Windows. No Wine/Proton adapter is configured.",
            approvedExternalLauncherPath);
    }

    public async Task<LaunchResult> LaunchAsync(LaunchPlan plan, CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(plan);
        // Never deserialize or execute a command line from a registry. Recreate and reverify the typed intent.
        var fresh = await CreateLaunchPlanAsync(plan.InstallationId, plan.Mode, plan.Target,
            plan.ApprovedExternalLauncherPath, cancellationToken);
        if (!PathRules.PathComparer.Equals(fresh.InstallRoot, plan.InstallRoot)
            || !PathRules.PathComparer.Equals(fresh.ExecutablePath, plan.ExecutablePath)
            || !PathRules.PathComparer.Equals(fresh.WorkingDirectory, plan.WorkingDirectory)
            || !fresh.Arguments.SequenceEqual(plan.Arguments))
            throw new CatalogException("launch_plan_changed", "The installation or entrypoint changed after planning. Create a new plan.");
        if (!fresh.CanExecuteOnCurrentPlatform)
            throw new CatalogException("platform_not_supported", fresh.BlockReason!);
        PathRules.RejectLinks(fresh.ExecutablePath);
        PathRules.RejectLinks(fresh.WorkingDirectory);
        cancellationToken.ThrowIfCancellationRequested();
        using var process = Process.Start(fresh.ToProcessStartInfo())
            ?? throw new CatalogException("process_start_failed", "The operating system did not create the requested process.");
        return new(process.Id, DateTimeOffset.UtcNow, fresh);
    }

    private void EnsureStateOutside(string installationRoot)
    {
        if (PathRules.IsWithin(installationRoot, StateDirectory))
            throw new CatalogException("state_inside_installation", "The catalog state directory must be outside the imported game and external launcher directories.");
    }

    private static string ValidateExternalPath(string path)
    {
        var absolute = PathRules.Absolute(path);
        var name = Path.GetFileName(absolute);
        if (!Path.GetExtension(name).Equals(".exe", StringComparison.OrdinalIgnoreCase)
            || name.Equals("RecRoom.exe", StringComparison.OrdinalIgnoreCase)
            || name.Equals("Recroom_Release.exe", StringComparison.OrdinalIgnoreCase)
            || name.Contains("guard", StringComparison.OrdinalIgnoreCase)
            || name.Contains("inject", StringComparison.OrdinalIgnoreCase)
            || name.Contains("anticheat", StringComparison.OrdinalIgnoreCase)
            || name.Contains("anti-cheat", StringComparison.OrdinalIgnoreCase)
            || name.Contains("bypass", StringComparison.OrdinalIgnoreCase))
            throw new CatalogException("invalid_external_launcher", "Select an actual external launcher EXE. Direct game, guard, anti-cheat, injector, and bypass executables are not external launcher entrypoints.");
        PathRules.RejectLinks(absolute);
        return absolute;
    }

    private async Task<RegistryDocument> ReadDocumentAsync(CancellationToken cancellationToken)
    {
        PathRules.RejectLinks(StateDirectory, allowMissingLeaf: true);
        PathRules.RejectLinks(RegistryPath, allowMissingLeaf: true);
        if (!File.Exists(RegistryPath)) return new();
        try
        {
            await using var stream = new FileStream(RegistryPath, FileMode.Open, FileAccess.Read, FileShare.Read | FileShare.Delete,
                64 * 1024, FileOptions.Asynchronous | FileOptions.SequentialScan);
            if (stream.Length > MaximumRegistryBytes || stream.Length == 0)
                throw new CatalogException("catalog_corrupt", "The catalog is empty or exceeds its 16 MiB size limit; it was not reset.");
            var bytes = new byte[(int)stream.Length];
            await stream.ReadExactlyAsync(bytes, cancellationToken);
            if (stream.ReadByte() != -1)
                throw new CatalogException("catalog_corrupt", "The catalog changed while it was being read.");
            using (var json = JsonDocument.Parse(bytes, new JsonDocumentOptions { MaxDepth = 32 })) RejectDuplicateJsonProperties(json.RootElement);
            var document = JsonSerializer.Deserialize<RegistryDocument>(bytes, JsonOptions)
                ?? throw new CatalogException("catalog_corrupt", "The catalog does not contain an object.");
            ValidateDocument(document);
            return document;
        }
        catch (Exception error) when (error is JsonException or NotSupportedException or ArgumentException or OverflowException)
        { throw new CatalogException("catalog_corrupt", "The catalog failed strict schema validation and was not reset. " + error.Message, error); }
    }

    private async Task<FileStream> AcquireWriteLockAsync(CancellationToken cancellationToken)
    {
        PathRules.RejectLinks(StateDirectory, allowMissingLeaf: true);
        Directory.CreateDirectory(StateDirectory);
        PathRules.RejectLinks(StateDirectory);
        var lockPath = Path.Combine(StateDirectory, "installations.lock");
        var deadline = DateTimeOffset.UtcNow.AddSeconds(3);
        while (true)
        {
            cancellationToken.ThrowIfCancellationRequested();
            PathRules.RejectLinks(lockPath, allowMissingLeaf: true);
            try { return new FileStream(lockPath, FileMode.OpenOrCreate, FileAccess.ReadWrite, FileShare.None); }
            catch (IOException error)
            {
                if (DateTimeOffset.UtcNow >= deadline)
                    throw new CatalogException("catalog_busy", "Another process is updating the catalog. Retry after it finishes.", error);
                await Task.Delay(50, cancellationToken);
            }
        }
    }

    private async Task WriteDocumentAsync(RegistryDocument document, CancellationToken cancellationToken)
    {
        ValidateDocument(document);
        var bytes = JsonSerializer.SerializeToUtf8Bytes(document, JsonOptions);
        if (bytes.Length > MaximumRegistryBytes)
            throw new CatalogException("catalog_full", "The catalog would exceed its 16 MiB limit.");
        PathRules.RejectLinks(RegistryPath, allowMissingLeaf: true);
        var temporary = Path.Combine(StateDirectory, ".installations-" + Guid.NewGuid().ToString("N") + ".tmp");
        try
        {
            await using (var stream = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None,
                64 * 1024, FileOptions.Asynchronous | FileOptions.WriteThrough))
            {
                await stream.WriteAsync(bytes, cancellationToken);
                await stream.FlushAsync(cancellationToken);
                stream.Flush(flushToDisk: true);
            }
            cancellationToken.ThrowIfCancellationRequested();
            PathRules.RejectLinks(StateDirectory);
            PathRules.RejectLinks(RegistryPath, allowMissingLeaf: true);
            File.Move(temporary, RegistryPath, overwrite: true);
        }
        finally
        {
            if (File.Exists(temporary)) File.Delete(temporary);
        }
    }

    private static void RejectDuplicateJsonProperties(JsonElement element)
    {
        if (element.ValueKind == JsonValueKind.Object)
        {
            var names = new HashSet<string>(StringComparer.Ordinal);
            foreach (var property in element.EnumerateObject())
            {
                if (!names.Add(property.Name)) throw new JsonException("Duplicate JSON property: " + property.Name);
                RejectDuplicateJsonProperties(property.Value);
            }
        }
        else if (element.ValueKind == JsonValueKind.Array)
            foreach (var item in element.EnumerateArray()) RejectDuplicateJsonProperties(item);
    }

    private static void ValidateDocument(RegistryDocument document)
    {
        static void Require(bool valid, string message)
        { if (!valid) throw new CatalogException("catalog_corrupt", message); }
        Require(document.SchemaVersion == 1, "Unsupported catalog schema version.");
        Require(document.Revision is not null && Guid.TryParseExact(document.Revision, "N", out _), "Invalid catalog revision.");
        Require(document.Installations is not null && document.Installations.Count <= MaximumInstallations, "Invalid catalog installation list.");
        var ids = new HashSet<string>(StringComparer.Ordinal);
        var roots = new HashSet<string>(PathRules.PathComparer);
        foreach (var installation in document.Installations!)
        {
            Require(installation is not null, "Null installation entry.");
            PathRules.Id(installation!.Id);
            Require(ids.Add(installation.Id), "Duplicate installation identifier.");
            Require(PathRules.Absolute(installation.InstallRoot) == installation.InstallRoot && roots.Add(installation.InstallRoot), "Invalid or duplicated installation root.");
            Require(PathRules.DisplayName(installation.DisplayName) == installation.DisplayName, "Invalid display name.");
            var inventory = installation.Inventory;
            Require(inventory is not null, "Missing installation inventory.");
            Require(inventory!.RootPath == installation.InstallRoot, "Inventory and installation paths disagree.");
            Require(inventory.Fingerprints is not null && inventory.Fingerprints.Count is > 0 and <= ClientInspector.MaximumKeyFiles, "Invalid fingerprint list.");
            Require(inventory.FileCount >= inventory.Fingerprints!.Count && inventory.FileCount <= 2_000_000 && inventory.TotalBytes >= 0 && inventory.SkippedLinkCount == 0, "Invalid inventory totals.");
            Require(Enum.IsDefined(inventory.ScriptingBackend), "Invalid scripting backend.");
            Require(inventory.UnityVersion is null || inventory.UnityVersion.Length <= 256, "Invalid Unity version text.");
            Require(inventory.ClientExecutableRelativePath is not null && inventory.ClientDataRelativePath is not null, "Missing client layout.");
            var firstLayout = inventory.ClientExecutableRelativePath!.Equals("RecRoom.exe", StringComparison.OrdinalIgnoreCase)
                && inventory.ClientDataRelativePath!.Equals("RecRoom_Data", StringComparison.OrdinalIgnoreCase);
            var secondLayout = inventory.ClientExecutableRelativePath.Equals("Recroom_Release.exe", StringComparison.OrdinalIgnoreCase)
                && inventory.ClientDataRelativePath!.Equals("Recroom_Release_Data", StringComparison.OrdinalIgnoreCase);
            Require(firstLayout || secondLayout, "Only recognized RecRoom executable/data-directory pairs may be registered.");
            var paths = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            long keyBytes = 0;
            foreach (var file in inventory.Fingerprints)
            {
                Require(file is not null, "Null fingerprint entry.");
                PathRules.Relative(file!.RelativePath);
                Require(ClientInspector.ShouldFingerprint(file.RelativePath) && paths.Add(file.RelativePath), "Invalid or duplicate fingerprint path.");
                Require(file.Length >= 0 && PathRules.IsSha256(file.Sha256), "Invalid file size or SHA-256.");
                keyBytes = checked(keyBytes + file.Length);
            }
            Require(keyBytes <= inventory.TotalBytes, "Fingerprint sizes exceed the inventory total.");
            Require(paths.Contains(inventory.ClientExecutableRelativePath) && paths.Contains("UnityPlayer.dll"), "Required client fingerprints are missing.");
            Require(inventory.Issues is not null && inventory.Issues.Count <= 257, "Invalid inventory issue list.");
            foreach (var issue in inventory.Issues!)
            {
                Require(issue is not null && Enum.IsDefined(issue.Severity) && issue.Severity != IssueSeverity.Error
                    && issue.Code is { Length: > 0 and <= 100 } && issue.Message is { Length: > 0 and <= 8192 }, "Invalid or incomplete imported inventory.");
                if (issue!.RelativePath is not null) PathRules.Relative(issue.RelativePath);
            }
            ValidatePathList(inventory.ProjectMarkers, 16, allowTrailingSlash: true);
            ValidatePathList(inventory.ExternalModificationMarkers, ClientInspector.MaximumKeyFiles);
            ValidatePathList(inventory.OpaqueMetadataFiles, ClientInspector.MaximumKeyFiles);
            Require(inventory.ExternalModificationsDetected == (inventory.ExternalModificationMarkers.Count > 0), "Wrapper detection fields disagree.");
            if (installation.ExternalLauncher is { } external)
            {
                Require(PathRules.Absolute(external.ExecutablePath) == external.ExecutablePath, "Invalid external launcher path.");
                Require(PathRules.Absolute(external.WorkingDirectory) == external.WorkingDirectory
                    && Path.GetDirectoryName(external.ExecutablePath) == external.WorkingDirectory, "External launcher working directory must be its containing directory.");
                Require(external.Length > 0 && PathRules.IsSha256(external.Sha256) && external.IsWindowsPe, "Invalid external launcher fingerprint.");
            }
        }
    }

    private static void ValidatePathList(IReadOnlyList<string> paths, int maximum, bool allowTrailingSlash = false)
    {
        if (paths is null || paths.Count > maximum) throw new CatalogException("catalog_corrupt", "Invalid inventory marker list.");
        var unique = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var path in paths)
        {
            if (path is null) throw new CatalogException("catalog_corrupt", "Null inventory marker.");
            PathRules.Relative(allowTrailingSlash ? path.TrimEnd('/') : path);
            if (!unique.Add(path)) throw new CatalogException("catalog_corrupt", "Duplicate inventory marker.");
        }
    }

    private sealed record RegistryDocument
    {
        [JsonRequired] public int SchemaVersion { get; init; } = 1;
        [JsonRequired] public string Revision { get; init; } = Guid.NewGuid().ToString("N");
        [JsonRequired] public DateTimeOffset WrittenAtUtc { get; init; } = DateTimeOffset.UtcNow;
        [JsonRequired] public List<RegisteredInstallation> Installations { get; init; } = [];
    }
}
