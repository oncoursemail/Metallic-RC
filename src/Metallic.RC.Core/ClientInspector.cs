using System.Buffers.Binary;
using System.Diagnostics;
using System.Security.Cryptography;

namespace Metallic.RC.Core;

/// <summary>Reads installation evidence without loading assemblies, executing tools, or writing to the installation.</summary>
public sealed class ClientInspector
{
    internal const int MaximumKeyFiles = 4096;
    private const int MaximumFiles = 2_000_000;
    private const int MaximumDepth = 64;
    private const int MaximumIssues = 256;
    private static readonly string[] WrapperNames =
        ["NexaNetLauncher.exe", "nexanet.dll", "CampusLauncher.exe", "RecLauncher.exe", "Campus.exe",
         "BepInEx", "MelonLoader", "version.dll", "winhttp.dll", "dobby.dll", "doorstop_config.ini"];

    public Task<ClientInventory> InspectAsync(string installRoot, CancellationToken cancellationToken = default)
        => Task.Run(() => InspectCoreAsync(installRoot, cancellationToken), cancellationToken);

    private static async Task<ClientInventory> InspectCoreAsync(string installRoot, CancellationToken cancellationToken)
    {
        var root = PathRules.Absolute(installRoot);
        PathRules.RejectLinks(root, allowMissingLeaf: true);
        var issues = new List<AuditIssue>();
        var files = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        var directories = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var candidates = new List<(string Relative, string Absolute)>();
        var wrapperMarkers = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        long fileCount = 0, totalBytes = 0, skippedLinks = 0;
        void AddIssue(IssueSeverity severity, string code, string message, string? relative = null)
        {
            if (issues.Count < MaximumIssues) issues.Add(new(severity, code, message, relative));
            else if (issues.Count == MaximumIssues)
                issues.Add(new(IssueSeverity.Error, "too_many_issues", "Further inspection issues were truncated; the inventory is incomplete."));
        }

        if (!Directory.Exists(root))
        {
            return new ClientInventory
            {
                RootPath = root, CapturedAtUtc = DateTimeOffset.UtcNow,
                Issues = [new(IssueSeverity.Error, "directory_missing", "The installation directory does not exist or is inaccessible.")]
            };
        }

        var pending = new Stack<(string Path, int Depth)>();
        pending.Push((root, 0));
        while (pending.TryPop(out var directory))
        {
            cancellationToken.ThrowIfCancellationRequested();
            try
            {
                PathRules.RejectLinks(directory.Path);
                foreach (var path in Directory.EnumerateFileSystemEntries(directory.Path))
                {
                    cancellationToken.ThrowIfCancellationRequested();
                    var relative = Path.GetRelativePath(root, path).Replace(Path.DirectorySeparatorChar, '/');
                    try
                    {
                        var attributes = File.GetAttributes(path);
                        if ((attributes & FileAttributes.ReparsePoint) != 0)
                        {
                            skippedLinks++;
                            AddIssue(IssueSeverity.Error, "link_skipped", "A symbolic link or reparse point was skipped; its target was not inspected.", relative);
                            continue;
                        }
                        if (WrapperNames.Contains(Path.GetFileName(path), StringComparer.OrdinalIgnoreCase)) wrapperMarkers.Add(relative);
                        if ((attributes & FileAttributes.Directory) != 0)
                        {
                            if (!directories.Add(relative))
                                AddIssue(IssueSeverity.Error, "ambiguous_case", "Multiple directories differ only by filename case.", relative);
                            if (directories.Count > 100_000)
                            {
                                AddIssue(IssueSeverity.Error, "directory_limit", "The maximum number of directories was reached.");
                                pending.Clear();
                                break;
                            }
                            if (directory.Depth >= MaximumDepth)
                                AddIssue(IssueSeverity.Error, "depth_limit", "The maximum inspection depth was reached.", relative);
                            else pending.Push((path, directory.Depth + 1));
                            continue;
                        }
                        if (++fileCount > MaximumFiles)
                        {
                            AddIssue(IssueSeverity.Error, "file_limit", "The maximum number of files was reached.");
                            pending.Clear();
                            break;
                        }
                        var fileInfo = new FileInfo(path);
                        totalBytes = checked(totalBytes + fileInfo.Length);
                        // Only retain names used for source/client detection and the bounded fingerprint set.
                        if (IsEvidenceFile(relative))
                        {
                            if (!files.TryAdd(relative, path))
                                AddIssue(IssueSeverity.Error, "ambiguous_case", "Multiple evidence files differ only by filename case.", relative);
                        }
                        if (ShouldFingerprint(relative))
                        {
                            if (candidates.Count >= MaximumKeyFiles)
                                AddIssue(IssueSeverity.Error, "fingerprint_limit", "There are too many launch-related files for a bounded import.", relative);
                            else candidates.Add((relative, path));
                        }
                    }
                    catch (Exception error) when (IsFileError(error))
                    { AddIssue(IssueSeverity.Error, "entry_unreadable", error.Message, relative); }
                }
            }
            catch (Exception error) when (IsFileError(error))
            { AddIssue(IssueSeverity.Error, "directory_unreadable", error.Message, Path.GetRelativePath(root, directory.Path)); }
        }

        string? executable = null, dataDirectory = null;
        foreach (var pair in new[] { ("RecRoom.exe", "RecRoom_Data"), ("Recroom_Release.exe", "Recroom_Release_Data") })
        {
            if (files.TryGetValue(pair.Item1, out var executablePath) && directories.Contains(pair.Item2))
            {
                if (executable is not null)
                {
                    AddIssue(IssueSeverity.Error, "ambiguous_client", "Both supported client layouts are present; import a directory containing one client.");
                    continue;
                }
                executable = Path.GetRelativePath(root, executablePath).Replace(Path.DirectorySeparatorChar, '/');
                dataDirectory = directories.Single(path => path.Equals(pair.Item2, StringComparison.OrdinalIgnoreCase));
            }
        }
        if (executable is null)
            AddIssue(IssueSeverity.Warning, "client_not_detected", "No supported RecRoom executable and matching data directory were found.");

        var fingerprints = new List<FileFingerprint>();
        var opaqueFiles = new List<string>();
        foreach (var candidate in candidates.OrderBy(candidate => candidate.Relative, StringComparer.Ordinal))
        {
            cancellationToken.ThrowIfCancellationRequested();
            try
            {
                var read = await ReadFingerprintAsync(candidate.Absolute, candidate.Relative, cancellationToken);
                fingerprints.Add(read.Fingerprint);
                if (read.OpaqueHeader)
                {
                    opaqueFiles.Add(candidate.Relative);
                    AddIssue(IssueSeverity.Information, "opaque_rr_container", "Header 0000525200005252 was observed. This is an opaque container, not parsed or decrypted metadata.", candidate.Relative);
                }
            }
            catch (Exception error) when (IsFileError(error))
            { AddIssue(IssueSeverity.Error, "fingerprint_failed", error.Message, candidate.Relative); }
        }

        string? unityVersion = null;
        if (executable is not null)
        {
            var executableHash = fingerprints.SingleOrDefault(file => file.RelativePath.Equals(executable, StringComparison.OrdinalIgnoreCase));
            if (executableHash?.IsWindowsPe != true)
                AddIssue(IssueSeverity.Error, "invalid_client_binary", "The selected client executable does not have a readable Windows PE header.", executable);
            if (!files.TryGetValue("UnityPlayer.dll", out var unityPlayer))
                AddIssue(IssueSeverity.Error, "unity_player_missing", "UnityPlayer.dll is missing from the client directory.");
            else
            {
                if (fingerprints.SingleOrDefault(file => file.RelativePath.Equals("UnityPlayer.dll", StringComparison.OrdinalIgnoreCase))?.IsWindowsPe != true)
                    AddIssue(IssueSeverity.Error, "invalid_unity_player", "UnityPlayer.dll does not have a readable Windows PE header.", "UnityPlayer.dll");
                try
                {
                    var version = FileVersionInfo.GetVersionInfo(unityPlayer);
                    unityVersion = version.ProductVersion ?? version.FileVersion;
                    if (string.IsNullOrWhiteSpace(unityVersion)) unityVersion = null;
                }
                catch (Exception error) when (IsFileError(error) || error is ArgumentException)
                { AddIssue(IssueSeverity.Warning, "unity_version_unavailable", "The UnityPlayer PE version resource could not be read: " + error.Message, "UnityPlayer.dll"); }
            }
        }

        var hasIl2Cpp = files.ContainsKey("GameAssembly.dll");
        var hasManaged = dataDirectory is not null && files.ContainsKey(dataDirectory + "/Managed/Assembly-CSharp.dll");
        var backend = (hasIl2Cpp, hasManaged) switch
        {
            (true, true) => ScriptingBackend.MixedMarkers,
            (true, false) => ScriptingBackend.Il2Cpp,
            (false, true) => ScriptingBackend.ManagedAssemblies,
            _ => ScriptingBackend.Unknown
        };
        if (hasManaged)
            AddIssue(IssueSeverity.Warning, "managed_marker_only", "Assembly-CSharp.dll is a managed-assembly marker. It may be a dummy or reconstructed assembly; this does not prove editable source or a rebuildable project.");
        if (hasIl2Cpp)
            AddIssue(IssueSeverity.Information, "il2cpp_marker", "GameAssembly.dll indicates an IL2CPP player layout; its presence does not provide the original C# source.");

        var projectMarkers = new List<string>();
        if (directories.Contains("Assets")) projectMarkers.Add("Assets/");
        if (files.ContainsKey("Packages/manifest.json")) projectMarkers.Add("Packages/manifest.json");
        if (files.ContainsKey("ProjectSettings/ProjectVersion.txt")) projectMarkers.Add("ProjectSettings/ProjectVersion.txt");

        return new ClientInventory
        {
            RootPath = root, CapturedAtUtc = DateTimeOffset.UtcNow,
            ClientExecutableRelativePath = executable, ClientDataRelativePath = dataDirectory,
            UnityVersion = unityVersion, ScriptingBackend = backend,
            IsUnityProject = projectMarkers.Count == 3, ProjectMarkers = projectMarkers.AsReadOnly(),
            ExternalModificationsDetected = wrapperMarkers.Count != 0,
            ExternalModificationMarkers = wrapperMarkers.Order(StringComparer.Ordinal).ToArray(),
            OpaqueMetadataFiles = opaqueFiles.AsReadOnly(), Fingerprints = fingerprints.AsReadOnly(),
            FileCount = fileCount, TotalBytes = totalBytes, SkippedLinkCount = skippedLinks, Issues = issues.AsReadOnly()
        };
    }

    private static bool IsEvidenceFile(string relative) => ShouldFingerprint(relative)
        || relative.Equals("Packages/manifest.json", StringComparison.OrdinalIgnoreCase)
        || relative.Equals("ProjectSettings/ProjectVersion.txt", StringComparison.OrdinalIgnoreCase);

    internal static bool ShouldFingerprint(string relative)
    {
        var name = relative[(relative.LastIndexOf('/') + 1)..];
        var extension = Path.GetExtension(name);
        // Assemblies and native plugins can affect the launched process even when nested under a loader.
        if (extension.Equals(".exe", StringComparison.OrdinalIgnoreCase)
            || extension.Equals(".dll", StringComparison.OrdinalIgnoreCase)
            || extension.Equals(".so", StringComparison.OrdinalIgnoreCase)
            || extension.Equals(".dylib", StringComparison.OrdinalIgnoreCase)) return true;
        return name.Equals("global-metadata.dat", StringComparison.OrdinalIgnoreCase)
            || name.Equals("globalgamemanagers", StringComparison.OrdinalIgnoreCase)
            || name.Equals("ScriptingAssemblies.json", StringComparison.OrdinalIgnoreCase)
            || name.Equals("boot.config", StringComparison.OrdinalIgnoreCase)
            || name.Equals("doorstop_config.ini", StringComparison.OrdinalIgnoreCase)
            || name.Equals("steam_appid.txt", StringComparison.OrdinalIgnoreCase);
    }

    internal static bool IsFileError(Exception error) => error is IOException or UnauthorizedAccessException
        or CatalogException or OverflowException or System.Security.SecurityException;

    internal static async Task<(FileFingerprint Fingerprint, bool OpaqueHeader)> ReadFingerprintAsync(
        string absolutePath, string relativePath, CancellationToken cancellationToken)
    {
        PathRules.RejectLinks(absolutePath);
        var before = new FileInfo(absolutePath);
        var length = before.Length;
        var lastWrite = before.LastWriteTimeUtc;
        await using var stream = new FileStream(absolutePath, FileMode.Open, FileAccess.Read, FileShare.Read,
            128 * 1024, FileOptions.Asynchronous | FileOptions.SequentialScan);
        var header = new byte[64];
        var read = await stream.ReadAtLeastAsync(header, header.Length, throwOnEndOfStream: false, cancellationToken);
        var opaque = read >= 8 && header.AsSpan(0, 8).SequenceEqual(new byte[] { 0, 0, 0x52, 0x52, 0, 0, 0x52, 0x52 });
        var isPe = false;
        if (read == 64 && header[0] == 'M' && header[1] == 'Z')
        {
            var offset = BinaryPrimitives.ReadInt32LittleEndian(header.AsSpan(0x3c, 4));
            if (offset >= 64 && offset <= stream.Length - 24 && offset <= 1024 * 1024)
            {
                stream.Position = offset;
                var signature = new byte[4];
                await stream.ReadExactlyAsync(signature, cancellationToken);
                isPe = signature.AsSpan().SequenceEqual(new byte[] { (byte)'P', (byte)'E', 0, 0 });
            }
        }
        stream.Position = 0;
        var sha = Convert.ToHexStringLower(await SHA256.HashDataAsync(stream, cancellationToken));
        before.Refresh();
        if (!before.Exists || before.Length != length || before.LastWriteTimeUtc != lastWrite || stream.Length != length)
            throw new CatalogException("file_changed_during_read", "A file changed while it was being fingerprinted; retry after the installation is idle.");
        return (new(relativePath, length, new DateTimeOffset(lastWrite, TimeSpan.Zero), sha, isPe), opaque);
    }
}
