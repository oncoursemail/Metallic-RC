using System.Diagnostics;

namespace Metallic.RC.Core;

public enum IssueSeverity { Information, Warning, Error }
public enum ScriptingBackend { Unknown, Il2Cpp, ManagedAssemblies, MixedMarkers }
public enum LaunchMode { Screen, Vr }
public enum LaunchTarget { Client, ExternalLauncher }

public sealed record AuditIssue(IssueSeverity Severity, string Code, string Message, string? RelativePath = null);

public sealed record FileFingerprint(
    string RelativePath, long Length, DateTimeOffset LastWriteTimeUtc, string Sha256, bool IsWindowsPe);

public sealed record ClientInventory
{
    public required string RootPath { get; init; }
    public required DateTimeOffset CapturedAtUtc { get; init; }
    public string? ClientExecutableRelativePath { get; init; }
    public string? ClientDataRelativePath { get; init; }
    public string? UnityVersion { get; init; }
    public ScriptingBackend ScriptingBackend { get; init; }
    public bool IsUnityProject { get; init; }
    public IReadOnlyList<string> ProjectMarkers { get; init; } = [];
    public bool ExternalModificationsDetected { get; init; }
    public IReadOnlyList<string> ExternalModificationMarkers { get; init; } = [];
    public IReadOnlyList<string> OpaqueMetadataFiles { get; init; } = [];
    public IReadOnlyList<FileFingerprint> Fingerprints { get; init; } = [];
    public long FileCount { get; init; }
    public long TotalBytes { get; init; }
    public long SkippedLinkCount { get; init; }
    public IReadOnlyList<AuditIssue> Issues { get; init; } = [];
    public bool IsRecognizedClient => ClientExecutableRelativePath is not null && ClientDataRelativePath is not null;
    public bool IsComplete => !Issues.Any(issue => issue.Severity == IssueSeverity.Error);
    public bool CanImport => IsRecognizedClient && IsComplete;
}

public sealed record ImportRequest(
    string InstallRoot, string DisplayName, string? ExternalLauncherPath = null, bool ReplaceExisting = false);

public sealed record RegisteredExternalLauncher(
    string ExecutablePath, string WorkingDirectory, long Length, string Sha256, bool IsWindowsPe);

public sealed record RegisteredInstallation
{
    public required string Id { get; init; }
    public required string DisplayName { get; init; }
    public required string InstallRoot { get; init; }
    public required DateTimeOffset ImportedAtUtc { get; init; }
    public required ClientInventory Inventory { get; init; }
    public RegisteredExternalLauncher? ExternalLauncher { get; init; }
}

public sealed record VerificationResult(
    RegisteredInstallation Installation, ClientInventory CurrentInventory, IReadOnlyList<AuditIssue> Issues)
{
    public bool IsValid => !Issues.Any(issue => issue.Severity == IssueSeverity.Error);
}

/// <summary>A plan can only be created by the catalog after verification. It is never deserialized from a manifest.</summary>
public sealed class LaunchPlan
{
    internal LaunchPlan(string installationId, string installRoot, LaunchMode mode, LaunchTarget target,
        string executablePath, string workingDirectory, IReadOnlyList<string> arguments,
        bool canExecute, string? blockReason, string? approvedExternalLauncherPath)
    {
        InstallationId = installationId;
        InstallRoot = installRoot;
        Mode = mode;
        Target = target;
        ExecutablePath = executablePath;
        WorkingDirectory = workingDirectory;
        Arguments = Array.AsReadOnly(arguments.ToArray());
        CanExecuteOnCurrentPlatform = canExecute;
        BlockReason = blockReason;
        ApprovedExternalLauncherPath = approvedExternalLauncherPath;
    }

    public string InstallationId { get; }
    public string InstallRoot { get; }
    public LaunchMode Mode { get; }
    public LaunchTarget Target { get; }
    public string ExecutablePath { get; }
    public string WorkingDirectory { get; }
    public IReadOnlyList<string> Arguments { get; }
    public bool CanExecuteOnCurrentPlatform { get; }
    public string? BlockReason { get; }
    internal string? ApprovedExternalLauncherPath { get; }

    public ProcessStartInfo ToProcessStartInfo()
    {
        var info = new ProcessStartInfo(ExecutablePath) { WorkingDirectory = WorkingDirectory, UseShellExecute = false };
        foreach (var argument in Arguments) info.ArgumentList.Add(argument);
        return info;
    }
}

public sealed record LaunchResult(int ProcessId, DateTimeOffset StartedAtUtc, LaunchPlan Plan);

public sealed class CatalogException(string code, string message, Exception? innerException = null)
    : Exception(message, innerException)
{
    public string Code { get; } = code;
}
